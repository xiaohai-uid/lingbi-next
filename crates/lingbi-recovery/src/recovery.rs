use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::{CandidateStatus, Document};
use lingbi_mutation::{CommitIntent, CommitReceipt, IntentRepository, ReceiptRepository};
use lingbi_storage::{
    AtomicFileStore, CandidateRepository, DiskAtomicFileStore, DocumentRepository,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryIncidentKind {
    OrphanCandidate,
    ApprovedUncommitted,
    CommitIntentWithoutReceipt,
    ExternalBytesChanged,
    InvalidContentHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryIncident {
    pub id: Uuid,
    pub kind: RecoveryIncidentKind,
    pub path: Option<String>,
    pub candidate_id: Option<Uuid>,
    pub expected_hash: Option<String>,
    pub actual_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryAction {
    PreserveUserBytes,
    ArchiveCandidate,
    Recovered,
    MarkedFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryOutcome {
    pub incident_id: Uuid,
    pub action: RecoveryAction,
    pub preserved_path: Option<String>,
}

pub struct RecoveryService {
    root: PathBuf,
    store: DiskAtomicFileStore,
}

impl RecoveryService {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            store: DiskAtomicFileStore,
        }
    }

    pub fn scan(&self) -> Result<Vec<RecoveryIncident>, AppError> {
        let mut incidents = Vec::new();
        self.scan_candidates(&mut incidents)?;
        self.scan_intents(&mut incidents)?;
        self.scan_content_hashes(&mut incidents)?;
        Ok(incidents)
    }

    pub fn recover(&self, incident: &RecoveryIncident) -> Result<RecoveryOutcome, AppError> {
        match incident.kind {
            RecoveryIncidentKind::CommitIntentWithoutReceipt => {
                let Some(path) = incident.path.as_deref() else {
                    return Ok(RecoveryOutcome {
                        incident_id: incident.id,
                        action: RecoveryAction::MarkedFailed,
                        preserved_path: None,
                    });
                };
                let bytes = self.store.read(std::path::Path::new(path))?;
                let intent: CommitIntent = serde_json::from_slice(&bytes).map_err(parse_error)?;
                self.recover_intent(&intent)
            }
            RecoveryIncidentKind::ApprovedUncommitted => {
                if let Some(candidate_id) = incident.candidate_id {
                    let candidates = CandidateRepository::new(&self.root);
                    if let Some(mut candidate) = candidates.read(candidate_id)? {
                        candidate.mark_failed();
                        candidates.write(&candidate)?;
                    }
                }
                Ok(RecoveryOutcome {
                    incident_id: incident.id,
                    action: RecoveryAction::MarkedFailed,
                    preserved_path: incident.path.clone(),
                })
            }
            RecoveryIncidentKind::OrphanCandidate => Ok(RecoveryOutcome {
                incident_id: incident.id,
                action: RecoveryAction::ArchiveCandidate,
                preserved_path: incident.path.clone(),
            }),
            RecoveryIncidentKind::ExternalBytesChanged
            | RecoveryIncidentKind::InvalidContentHash => Ok(RecoveryOutcome {
                incident_id: incident.id,
                action: RecoveryAction::PreserveUserBytes,
                preserved_path: incident.path.clone(),
            }),
        }
    }

    pub fn recover_all(&self) -> Result<Vec<RecoveryOutcome>, AppError> {
        let mut outcomes = Vec::new();
        for intent in self.read_intents()? {
            outcomes.push(self.recover_intent(&intent)?);
        }
        Ok(outcomes)
    }

    fn recover_intent(&self, intent: &CommitIntent) -> Result<RecoveryOutcome, AppError> {
        let candidates = CandidateRepository::new(&self.root);
        let intents = IntentRepository::new(&self.root);
        let receipts = ReceiptRepository::new(&self.root);
        let documents = DocumentRepository::new(&self.root);

        if let Some(receipt) = receipts.read(intent.candidate_id)? {
            if let Some(mut candidate) = candidates.read(intent.candidate_id)? {
                candidate.commit();
                candidates.write(&candidate)?;
            }
            intents.delete(intent.id)?;
            return Ok(RecoveryOutcome {
                incident_id: intent.id,
                action: RecoveryAction::Recovered,
                preserved_path: Some(receipt.target_path),
            });
        }

        let Some(mut candidate) = candidates.read(intent.candidate_id)? else {
            return Ok(RecoveryOutcome {
                incident_id: intent.id,
                action: RecoveryAction::MarkedFailed,
                preserved_path: None,
            });
        };
        let Some(document) = documents.find(candidate.document_id)? else {
            candidate.mark_failed();
            candidates.write(&candidate)?;
            return Ok(RecoveryOutcome {
                incident_id: intent.id,
                action: RecoveryAction::MarkedFailed,
                preserved_path: None,
            });
        };
        let body_path = self.root.join(document.physical_path());
        if !body_path.exists() {
            candidate.mark_failed();
            candidates.write(&candidate)?;
            return Ok(RecoveryOutcome {
                incident_id: intent.id,
                action: RecoveryAction::MarkedFailed,
                preserved_path: Some(body_path.to_string_lossy().into_owned()),
            });
        }
        let body_hash = hex_sha256(&self.store.read(&body_path)?);
        let after_hash = candidate.content_hash.clone();
        let before_hash = candidate.base_content_hash.clone();
        let before_revision = candidate.base_revision;
        let after_revision = before_revision + 1;
        let body_is_after = body_hash.eq_ignore_ascii_case(&after_hash);
        let body_is_before = body_hash.eq_ignore_ascii_case(&before_hash);
        let metadata_before = document.revision == before_revision
            && document.content_hash.eq_ignore_ascii_case(&before_hash);
        let metadata_after = document.revision == after_revision
            && document.content_hash.eq_ignore_ascii_case(&after_hash);

        if body_is_after && metadata_before {
            self.write_metadata_and_receipt(&document, &candidate, intent)?;
            intents.delete(intent.id)?;
            return Ok(RecoveryOutcome {
                incident_id: intent.id,
                action: RecoveryAction::Recovered,
                preserved_path: Some(body_path.to_string_lossy().into_owned()),
            });
        }

        if body_is_after && metadata_after {
            self.write_receipt_and_commit(&document, &candidate, intent)?;
            intents.delete(intent.id)?;
            return Ok(RecoveryOutcome {
                incident_id: intent.id,
                action: RecoveryAction::Recovered,
                preserved_path: Some(body_path.to_string_lossy().into_owned()),
            });
        }

        if body_is_before && metadata_before {
            self.store.write_atomic(
                &body_path,
                candidate.content.as_bytes(),
                Some(&before_hash),
            )?;
            self.write_metadata_and_receipt(&document, &candidate, intent)?;
            intents.delete(intent.id)?;
            return Ok(RecoveryOutcome {
                incident_id: intent.id,
                action: RecoveryAction::Recovered,
                preserved_path: Some(body_path.to_string_lossy().into_owned()),
            });
        }

        candidate.mark_failed();
        candidates.write(&candidate)?;
        Ok(RecoveryOutcome {
            incident_id: intent.id,
            action: RecoveryAction::PreserveUserBytes,
            preserved_path: Some(body_path.to_string_lossy().into_owned()),
        })
    }

    fn write_metadata_and_receipt(
        &self,
        document: &Document,
        candidate: &lingbi_domain::Candidate,
        intent: &CommitIntent,
    ) -> Result<(), AppError> {
        let mut updated = document.clone();
        updated.revision = candidate.base_revision + 1;
        updated.content_hash = candidate.content_hash.clone();
        updated.updated_at = chrono::Utc::now();
        DocumentRepository::new(&self.root).update(&updated)?;
        self.write_receipt_and_commit(&updated, candidate, intent)
    }

    fn write_receipt_and_commit(
        &self,
        document: &Document,
        candidate: &lingbi_domain::Candidate,
        intent: &CommitIntent,
    ) -> Result<(), AppError> {
        let receipt = CommitReceipt {
            id: Uuid::new_v4(),
            candidate_id: candidate.id,
            target_path: document.physical_path().to_string_lossy().into_owned(),
            after_revision: document.revision,
            after_content_hash: document.content_hash.clone(),
            committed_at: chrono::Utc::now(),
            idempotency_key: intent.idempotency_key.clone(),
        };
        ReceiptRepository::new(&self.root).write(&receipt)?;
        let mut committed = candidate.clone();
        committed.commit();
        CandidateRepository::new(&self.root).write(&committed)
    }

    fn read_intents(&self) -> Result<Vec<CommitIntent>, AppError> {
        let intents_dir = self.root.join(".lingbi/intents");
        if !intents_dir.exists() {
            return Ok(Vec::new());
        }
        let mut intents = Vec::new();
        for entry in fs::read_dir(&intents_dir).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let bytes = self.store.read(&path)?;
            intents.push(serde_json::from_slice(&bytes).map_err(parse_error)?);
        }
        Ok(intents)
    }

    fn scan_candidates(&self, incidents: &mut Vec<RecoveryIncident>) -> Result<(), AppError> {
        let candidates = CandidateRepository::new(&self.root).list()?;
        for candidate in candidates {
            let path = self
                .root
                .join(".lingbi/candidates")
                .join(format!("{}.json", candidate.id));
            let receipt = self
                .root
                .join(".lingbi/receipts")
                .join(format!("{}.json", candidate.id));
            match candidate.status {
                CandidateStatus::Pending => incidents.push(RecoveryIncident {
                    id: Uuid::new_v4(),
                    kind: RecoveryIncidentKind::OrphanCandidate,
                    path: Some(path.to_string_lossy().into_owned()),
                    candidate_id: Some(candidate.id),
                    expected_hash: None,
                    actual_hash: None,
                }),
                CandidateStatus::Approved if !receipt.exists() => {
                    incidents.push(RecoveryIncident {
                        id: Uuid::new_v4(),
                        kind: RecoveryIncidentKind::ApprovedUncommitted,
                        path: Some(path.to_string_lossy().into_owned()),
                        candidate_id: Some(candidate.id),
                        expected_hash: None,
                        actual_hash: None,
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn scan_intents(&self, incidents: &mut Vec<RecoveryIncident>) -> Result<(), AppError> {
        let intents_dir = self.root.join(".lingbi/intents");
        if !intents_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&intents_dir).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let bytes = self.store.read(&path)?;
            let intent: CommitIntent = serde_json::from_slice(&bytes).map_err(parse_error)?;
            let receipt = self
                .root
                .join(".lingbi/receipts")
                .join(format!("{}.json", intent.candidate_id));
            if !receipt.exists() {
                incidents.push(RecoveryIncident {
                    id: Uuid::new_v4(),
                    kind: RecoveryIncidentKind::CommitIntentWithoutReceipt,
                    path: Some(path.to_string_lossy().into_owned()),
                    candidate_id: Some(intent.candidate_id),
                    expected_hash: None,
                    actual_hash: None,
                });
            }
        }
        Ok(())
    }

    fn scan_content_hashes(&self, incidents: &mut Vec<RecoveryIncident>) -> Result<(), AppError> {
        let index_path = self.root.join(".lingbi/documents.json");
        if !index_path.exists() {
            return Ok(());
        }
        let bytes = self.store.read(&index_path)?;
        let documents: Vec<Document> = serde_json::from_slice(&bytes).map_err(parse_error)?;

        for document in documents {
            let path = self.root.join(document.physical_path());
            let actual_hash = if path.exists() {
                Some(hex_sha256(&self.store.read(&path)?))
            } else {
                None
            };
            if actual_hash.as_deref() != Some(document.content_hash.as_str()) {
                incidents.push(RecoveryIncident {
                    id: Uuid::new_v4(),
                    kind: if actual_hash.is_some() {
                        RecoveryIncidentKind::ExternalBytesChanged
                    } else {
                        RecoveryIncidentKind::InvalidContentHash
                    },
                    path: Some(path.to_string_lossy().into_owned()),
                    candidate_id: Some(document.id),
                    expected_hash: Some(document.content_hash),
                    actual_hash,
                });
            }
        }
        Ok(())
    }
}

fn parse_error(error: serde_json::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("recovery metadata parse failed: {error}"),
        false,
    )
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("recovery scan failed: {error}"),
        false,
    )
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingbi_domain::Candidate;
    use lingbi_mutation::CommitIntent;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn write_json(path: &Path, value: &impl serde::Serialize) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(path, serde_json::to_vec(value).expect("json")).expect("write");
    }

    fn candidate(status: CandidateStatus) -> Candidate {
        Candidate {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            document_id: Uuid::new_v4(),
            instruction: "write".to_owned(),
            base_revision: 0,
            base_content_hash: "before".to_owned(),
            content: "candidate".to_owned(),
            content_hash: "after".to_owned(),
            provider_id: "fake".to_owned(),
            model_id: "fake-model".to_owned(),
            status,
            created_at: chrono::Utc::now(),
            approved_at: None,
            committed_at: None,
        }
    }

    fn recovery_fixture(root: &Path, phase: &str) -> (Document, Candidate, CommitIntent) {
        let now = chrono::Utc::now();
        let document = Document {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "第一章".to_owned(),
            order: 0,
            revision: 0,
            content_hash: hex_sha256(b"old content"),
            created_at: now,
            updated_at: now,
        };
        let candidate = Candidate {
            id: Uuid::new_v4(),
            project_id: document.project_id,
            document_id: document.id,
            instruction: "write".to_owned(),
            base_revision: 0,
            base_content_hash: document.content_hash.clone(),
            content: "new content".to_owned(),
            content_hash: hex_sha256(b"new content"),
            provider_id: "fake".to_owned(),
            model_id: "fake-model".to_owned(),
            status: CandidateStatus::Approved,
            created_at: now,
            approved_at: Some(now),
            committed_at: None,
        };
        let intent = CommitIntent {
            id: Uuid::new_v4(),
            candidate_id: candidate.id,
            approval_id: Uuid::new_v4(),
            target_path: "chapters/chapter.md".to_owned(),
            expected_revision: 0,
            idempotency_key: "key".to_owned(),
        };
        write_json(
            &root.join(".lingbi/documents.json"),
            &vec![document.clone()],
        );
        write_json(
            &root
                .join(".lingbi/candidates")
                .join(format!("{}.json", candidate.id)),
            &candidate,
        );
        write_json(
            &root
                .join(".lingbi/intents")
                .join(format!("{}.json", intent.id)),
            &intent,
        );
        fs::create_dir_all(root.join("chapters")).expect("chapters");
        let body = root.join(document.physical_path());
        let body_bytes: &[u8] = match phase {
            "AfterContentWrite" | "AfterMetadataWrite" | "BeforeReceipt" => b"new content",
            "External" => b"external",
            _ => b"old content",
        };
        fs::write(body, body_bytes).expect("body");
        if matches!(phase, "AfterMetadataWrite" | "BeforeReceipt") {
            let mut updated = document.clone();
            updated.revision = 1;
            updated.content_hash = candidate.content_hash.clone();
            write_json(&root.join(".lingbi/documents.json"), &vec![updated]);
        }
        (document, candidate, intent)
    }

    fn assert_recovered(root: &Path, candidate: &Candidate, intent: &CommitIntent) {
        assert_eq!(
            fs::read_to_string(
                root.join("chapters")
                    .join(format!("{}.md", candidate.document_id))
            )
            .expect("body"),
            "new content"
        );
        let bytes = fs::read(root.join(".lingbi/documents.json")).expect("index");
        let documents: Vec<Document> = serde_json::from_slice(&bytes).expect("documents");
        assert_eq!(documents[0].revision, 1);
        assert_eq!(documents[0].content_hash, candidate.content_hash);
        assert!(
            root.join(".lingbi/receipts")
                .join(format!("{}.json", candidate.id))
                .exists()
        );
        assert!(
            !root
                .join(".lingbi/intents")
                .join(format!("{}.json", intent.id))
                .exists()
        );
    }

    #[test]
    fn detects_orphan_candidate() {
        let temp = TempDir::new().expect("temp dir");
        let project = temp.path().join("project");
        write_json(
            &project.join(".lingbi/candidates/candidate.json"),
            &candidate(CandidateStatus::Pending),
        );
        let service = RecoveryService::new(&project);

        let incidents = service.scan().expect("scan");

        assert!(
            incidents
                .iter()
                .any(|incident| incident.kind == RecoveryIncidentKind::OrphanCandidate)
        );
    }

    #[test]
    fn detects_approved_uncommitted() {
        let temp = TempDir::new().expect("temp dir");
        let project = temp.path().join("project");
        write_json(
            &project.join(".lingbi/candidates/candidate.json"),
            &candidate(CandidateStatus::Approved),
        );
        let service = RecoveryService::new(&project);

        let incidents = service.scan().expect("scan");

        assert!(
            incidents
                .iter()
                .any(|incident| incident.kind == RecoveryIncidentKind::ApprovedUncommitted)
        );
    }

    #[test]
    fn detects_intent_without_receipt() {
        let temp = TempDir::new().expect("temp dir");
        let project = temp.path().join("project");
        write_json(
            &project.join(".lingbi/intents/intent.json"),
            &CommitIntent {
                id: Uuid::new_v4(),
                candidate_id: Uuid::new_v4(),
                approval_id: Uuid::new_v4(),
                target_path: "chapters/chapter.md".to_owned(),
                expected_revision: 0,
                idempotency_key: "key".to_owned(),
            },
        );
        let service = RecoveryService::new(&project);

        let incidents = service.scan().expect("scan");

        assert!(
            incidents
                .iter()
                .any(|incident| incident.kind == RecoveryIncidentKind::CommitIntentWithoutReceipt)
        );
    }

    #[test]
    fn detects_external_bytes_changed() {
        let temp = TempDir::new().expect("temp dir");
        let project = temp.path().join("project");
        let now = chrono::Utc::now();
        let document = Document {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "第一章".to_owned(),
            order: 0,
            revision: 0,
            content_hash: "expected".to_owned(),
            created_at: now,
            updated_at: now,
        };
        write_json(
            &project.join(".lingbi/documents.json"),
            &vec![document.clone()],
        );
        fs::create_dir_all(project.join("chapters")).expect("chapters");
        fs::write(project.join(document.physical_path()), "user bytes").expect("manuscript");
        let service = RecoveryService::new(&project);

        let incidents = service.scan().expect("scan");

        assert!(
            incidents
                .iter()
                .any(|incident| incident.kind == RecoveryIncidentKind::ExternalBytesChanged)
        );
    }

    #[test]
    fn recovers_after_intent() {
        let temp = TempDir::new().expect("temp dir");
        let project = temp.path().join("project");
        let (_, candidate, intent) = recovery_fixture(&project, "AfterIntent");
        let service = RecoveryService::new(&project);

        service.recover_all().expect("recover");

        assert_recovered(&project, &candidate, &intent);
    }

    #[test]
    fn recovers_after_content_write() {
        let temp = TempDir::new().expect("temp dir");
        let project = temp.path().join("project");
        let (_, candidate, intent) = recovery_fixture(&project, "AfterContentWrite");
        let service = RecoveryService::new(&project);

        service.recover_all().expect("recover");

        assert_recovered(&project, &candidate, &intent);
    }

    #[test]
    fn recovers_after_metadata_write() {
        let temp = TempDir::new().expect("temp dir");
        let project = temp.path().join("project");
        let (_, candidate, intent) = recovery_fixture(&project, "AfterMetadataWrite");
        let service = RecoveryService::new(&project);

        service.recover_all().expect("recover");

        assert_recovered(&project, &candidate, &intent);
    }

    #[test]
    fn recovers_before_receipt() {
        let temp = TempDir::new().expect("temp dir");
        let project = temp.path().join("project");
        let (_, candidate, intent) = recovery_fixture(&project, "BeforeReceipt");
        let service = RecoveryService::new(&project);

        service.recover_all().expect("recover");

        assert_recovered(&project, &candidate, &intent);
    }

    #[test]
    fn external_body_is_preserved_by_recovery() {
        let temp = TempDir::new().expect("temp dir");
        let project = temp.path().join("project");
        let (document, candidate, _) = recovery_fixture(&project, "External");
        let service = RecoveryService::new(&project);

        service.recover_all().expect("recover");

        assert_eq!(
            fs::read_to_string(project.join(document.physical_path())).expect("body"),
            "external"
        );
        let bytes = fs::read(project.join(".lingbi/documents.json")).expect("index");
        let documents: Vec<Document> = serde_json::from_slice(&bytes).expect("documents");
        assert_eq!(documents[0].revision, 0);
        let loaded: Candidate = serde_json::from_slice(
            &fs::read(
                project
                    .join(".lingbi/candidates")
                    .join(format!("{}.json", candidate.id)),
            )
            .expect("candidate"),
        )
        .expect("candidate json");
        assert_eq!(loaded.status, CandidateStatus::Failed);
    }

    #[test]
    fn recovery_prefers_preserving_user_bytes() {
        let temp = TempDir::new().expect("temp dir");
        let project = temp.path().join("project");
        let service = RecoveryService::new(&project);
        let incident = RecoveryIncident {
            id: Uuid::new_v4(),
            kind: RecoveryIncidentKind::ExternalBytesChanged,
            path: Some("chapters/chapter.md".to_owned()),
            candidate_id: None,
            expected_hash: Some("expected".to_owned()),
            actual_hash: Some("actual".to_owned()),
        };

        let outcome = service.recover(&incident).expect("recover");

        assert_eq!(outcome.action, RecoveryAction::PreserveUserBytes);
    }
}
