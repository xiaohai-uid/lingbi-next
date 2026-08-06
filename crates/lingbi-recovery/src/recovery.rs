use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::{CandidateStatus, Document};
use lingbi_mutation::CommitIntent;
use lingbi_storage::{AtomicFileStore, CandidateRepository, DiskAtomicFileStore};
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

    pub fn recover(&self, incident: &RecoveryIncident) -> RecoveryOutcome {
        let action = match incident.kind {
            RecoveryIncidentKind::OrphanCandidate => RecoveryAction::ArchiveCandidate,
            RecoveryIncidentKind::ApprovedUncommitted
            | RecoveryIncidentKind::CommitIntentWithoutReceipt
            | RecoveryIncidentKind::ExternalBytesChanged
            | RecoveryIncidentKind::InvalidContentHash => RecoveryAction::PreserveUserBytes,
        };
        RecoveryOutcome {
            incident_id: incident.id,
            action,
            preserved_path: incident.path.clone(),
        }
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

        let outcome = service.recover(&incident);

        assert_eq!(outcome.action, RecoveryAction::PreserveUserBytes);
    }
}
