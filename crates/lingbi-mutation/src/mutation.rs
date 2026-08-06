use chrono::Utc;
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::{Candidate, CandidateStatus};
use lingbi_security::ProjectPathGuard;
use lingbi_storage::{
    AtomicFileStore, CandidateRepository, DiskAtomicFileStore, DocumentRepository,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    pub id: Uuid,
    pub candidate_id: Uuid,
    pub actor: String,
    pub approved_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitIntent {
    pub id: Uuid,
    pub candidate_id: Uuid,
    pub approval_id: Uuid,
    pub target_path: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitReceipt {
    pub id: Uuid,
    pub candidate_id: Uuid,
    pub target_path: String,
    pub after_revision: u64,
    pub after_content_hash: String,
    pub committed_at: chrono::DateTime<chrono::Utc>,
    pub idempotency_key: String,
}

pub struct ApprovalRepository {
    root: PathBuf,
    store: DiskAtomicFileStore,
}

impl ApprovalRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            store: DiskAtomicFileStore,
        }
    }

    pub fn write(&self, approval: &Approval) -> Result<(), AppError> {
        let path = self.approval_path(approval.id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io_error)?;
        }
        let bytes = serde_json::to_vec(approval).map_err(parse_error)?;
        self.store.write_atomic(&path, &bytes, None)?;
        Ok(())
    }

    pub fn read(&self, id: Uuid) -> Result<Option<Approval>, AppError> {
        let path = self.approval_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path).map_err(io_error)?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(parse_error)
    }

    fn approval_path(&self, id: Uuid) -> PathBuf {
        self.root
            .join(".lingbi/approvals")
            .join(format!("{id}.json"))
    }
}

pub struct IntentRepository {
    root: PathBuf,
    store: DiskAtomicFileStore,
}

impl IntentRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            store: DiskAtomicFileStore,
        }
    }

    pub fn write(&self, intent: &CommitIntent) -> Result<(), AppError> {
        let path = self.intent_path(intent.id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io_error)?;
        }
        let bytes = serde_json::to_vec(intent).map_err(parse_error)?;
        self.store.write_atomic(&path, &bytes, None)?;
        Ok(())
    }

    fn intent_path(&self, id: Uuid) -> PathBuf {
        self.root.join(".lingbi/intents").join(format!("{id}.json"))
    }
}

pub struct ReceiptRepository {
    root: PathBuf,
    store: DiskAtomicFileStore,
}

impl ReceiptRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            store: DiskAtomicFileStore,
        }
    }

    pub fn write(&self, receipt: &CommitReceipt) -> Result<(), AppError> {
        let path = self.receipt_path(receipt.candidate_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io_error)?;
        }
        let bytes = serde_json::to_vec(receipt).map_err(parse_error)?;
        self.store.write_atomic(&path, &bytes, None)?;
        Ok(())
    }

    pub fn read(&self, candidate_id: Uuid) -> Result<Option<CommitReceipt>, AppError> {
        let path = self.receipt_path(candidate_id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path).map_err(io_error)?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(parse_error)
    }

    pub fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<CommitReceipt>, AppError> {
        let dir = self.root.join(".lingbi/receipts");
        if !dir.exists() {
            return Ok(None);
        }
        for entry in std::fs::read_dir(&dir).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let bytes = std::fs::read(&path).map_err(io_error)?;
            let receipt: CommitReceipt = serde_json::from_slice(&bytes).map_err(parse_error)?;
            if receipt.idempotency_key == idempotency_key {
                return Ok(Some(receipt));
            }
        }
        Ok(None)
    }

    fn receipt_path(&self, candidate_id: Uuid) -> PathBuf {
        self.root
            .join(".lingbi/receipts")
            .join(format!("{candidate_id}.json"))
    }
}

pub struct MutationEngine {
    root: PathBuf,
    guard: ProjectPathGuard,
    store: DiskAtomicFileStore,
    candidates: CandidateRepository,
    approvals: ApprovalRepository,
    intents: IntentRepository,
    receipts: ReceiptRepository,
    documents: DocumentRepository,
}

impl MutationEngine {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let _ = std::fs::create_dir_all(&root);
        Self {
            guard: ProjectPathGuard::new(root.clone()),
            root: root.clone(),
            store: DiskAtomicFileStore,
            candidates: CandidateRepository::new(root.clone()),
            approvals: ApprovalRepository::new(root.clone()),
            intents: IntentRepository::new(root.clone()),
            receipts: ReceiptRepository::new(root.clone()),
            documents: DocumentRepository::new(root),
        }
    }

    pub fn propose(&self, mut candidate: Candidate) -> Result<Candidate, AppError> {
        candidate.status = CandidateStatus::Pending;
        self.candidates.write(&candidate)?;
        Ok(candidate)
    }

    pub fn approve(
        &self,
        candidate_id: Uuid,
        actor: impl Into<String>,
    ) -> Result<Approval, AppError> {
        let mut candidate = self.candidates.read(candidate_id)?.ok_or_else(|| {
            AppError::new(
                ErrorCode::DocumentNotFound,
                "candidate not found".to_owned(),
                false,
            )
        })?;
        if candidate.status != CandidateStatus::Pending {
            return Err(AppError::new(
                ErrorCode::MutationNotApproved,
                "candidate is not pending".to_owned(),
                false,
            ));
        }
        candidate.approve();
        self.candidates.write(&candidate)?;
        let approval = Approval {
            id: Uuid::new_v4(),
            candidate_id,
            actor: actor.into(),
            approved_at: Utc::now(),
        };
        self.approvals.write(&approval)?;
        Ok(approval)
    }

    pub fn commit(&self, intent: CommitIntent) -> Result<CommitReceipt, AppError> {
        if let Some(existing) = self
            .receipts
            .find_by_idempotency_key(&intent.idempotency_key)?
        {
            return Ok(existing);
        }
        self.intents.write(&intent)?;

        let mut candidate = self.candidates.read(intent.candidate_id)?.ok_or_else(|| {
            AppError::new(
                ErrorCode::DocumentNotFound,
                "candidate not found".to_owned(),
                false,
            )
        })?;
        if candidate.status != CandidateStatus::Approved {
            return Err(AppError::new(
                ErrorCode::MutationNotApproved,
                "candidate is not approved".to_owned(),
                false,
            ));
        }

        let approval = self.approvals.read(intent.approval_id)?.ok_or_else(|| {
            AppError::new(
                ErrorCode::MutationNotApproved,
                "approval not found".to_owned(),
                false,
            )
        })?;
        if approval.candidate_id != intent.candidate_id {
            return Err(AppError::new(
                ErrorCode::MutationNotApproved,
                "approval does not match candidate".to_owned(),
                false,
            ));
        }

        let document = self.documents.find(candidate.document_id)?.ok_or_else(|| {
            AppError::new(
                ErrorCode::DocumentNotFound,
                "document not found".to_owned(),
                false,
            )
        })?;
        if document.revision != intent.expected_revision {
            return Err(AppError::new(
                ErrorCode::MutationConflict,
                "revision conflict".to_owned(),
                false,
            ));
        }

        let target = self.guard.resolve(&document.physical_path()).map_err(|_| {
            AppError::new(
                ErrorCode::ProjectCorrupted,
                "commit target escapes project boundary".to_owned(),
                false,
            )
        })?;
        let payload = candidate.content.as_bytes();
        let content_hash = hex_sha256(payload);
        self.store
            .write_atomic(&target, payload, Some(&document.content_hash))?;
        let verified = self.store.read(&target)?;
        if hex_sha256(&verified) != content_hash {
            return Err(AppError::new(
                ErrorCode::ProjectCorrupted,
                "verified content hash mismatch".to_owned(),
                false,
            ));
        }

        let receipt = CommitReceipt {
            id: Uuid::new_v4(),
            candidate_id: intent.candidate_id,
            target_path: document.physical_path().to_string_lossy().into_owned(),
            after_revision: document.revision + 1,
            after_content_hash: content_hash,
            committed_at: Utc::now(),
            idempotency_key: intent.idempotency_key.clone(),
        };
        self.receipts.write(&receipt)?;
        candidate.commit();
        self.candidates.write(&candidate)?;
        Ok(receipt)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("mutation persistence failed: {error}"),
        false,
    )
}

fn parse_error(error: serde_json::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("mutation metadata parse failed: {error}"),
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
    use lingbi_domain::Document;
    use std::fs;
    use tempfile::TempDir;

    fn candidate(document_id: Uuid, content: &str) -> Candidate {
        Candidate {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            document_id,
            instruction: "write".to_owned(),
            base_revision: 0,
            base_content_hash: hex_sha256(b"old content"),
            content: content.to_owned(),
            content_hash: hex_sha256(content.as_bytes()),
            provider_id: "fake".to_owned(),
            model_id: "fake-model".to_owned(),
            status: CandidateStatus::Pending,
            created_at: Utc::now(),
            approved_at: None,
            committed_at: None,
        }
    }

    fn setup_engine(temp: &TempDir) -> (MutationEngine, Uuid, Document) {
        let root = temp.path().join("project");
        fs::create_dir_all(root.join(".lingbi")).expect("lingbi");
        fs::create_dir_all(root.join("chapters")).expect("chapters");
        let now = Utc::now();
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
        fs::write(root.join(document.physical_path()), "old content").expect("canonical");
        fs::write(
            root.join(".lingbi/documents.json"),
            serde_json::to_vec(&vec![document.clone()]).expect("json"),
        )
        .expect("index");
        (MutationEngine::new(root), document.id, document)
    }

    #[tokio::test]
    async fn unapproved_commit_is_rejected() {
        let temp = TempDir::new().expect("temp dir");
        let (engine, document_id, _) = setup_engine(&temp);
        let candidate = engine
            .propose(candidate(document_id, "new content"))
            .expect("propose");

        let result = engine.commit(CommitIntent {
            id: Uuid::new_v4(),
            candidate_id: candidate.id,
            approval_id: Uuid::new_v4(),
            target_path: "chapters/chapter.md".to_owned(),
            expected_revision: 0,
            idempotency_key: "key-1".to_owned(),
        });

        assert!(matches!(
            result,
            Err(AppError {
                code: ErrorCode::MutationNotApproved,
                ..
            })
        ));
        assert_eq!(
            fs::read_to_string(
                temp.path()
                    .join("project/chapters")
                    .join(format!("{document_id}.md"))
            )
            .expect("read canonical"),
            "old content"
        );
    }

    #[tokio::test]
    async fn revision_conflict_is_rejected() {
        let temp = TempDir::new().expect("temp dir");
        let (engine, document_id, _) = setup_engine(&temp);
        let candidate = engine
            .propose(candidate(document_id, "new content"))
            .expect("propose");
        let approval = engine.approve(candidate.id, "user").expect("approve");

        let result = engine.commit(CommitIntent {
            id: Uuid::new_v4(),
            candidate_id: candidate.id,
            approval_id: approval.id,
            target_path: "chapters/chapter.md".to_owned(),
            expected_revision: 1,
            idempotency_key: "key-1".to_owned(),
        });

        assert!(matches!(
            result,
            Err(AppError {
                code: ErrorCode::MutationConflict,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn approved_candidate_is_committed_and_persisted() {
        let temp = TempDir::new().expect("temp dir");
        let (engine, document_id, _) = setup_engine(&temp);
        let candidate = engine
            .propose(candidate(document_id, "new content"))
            .expect("propose");
        let approval = engine.approve(candidate.id, "user").expect("approve");

        let receipt = engine
            .commit(CommitIntent {
                id: Uuid::new_v4(),
                candidate_id: candidate.id,
                approval_id: approval.id,
                target_path: "chapters/chapter.md".to_owned(),
                expected_revision: 0,
                idempotency_key: "key-1".to_owned(),
            })
            .expect("commit");

        assert_eq!(receipt.after_revision, 1);
        assert_eq!(receipt.after_content_hash.len(), 64);
        assert_eq!(
            fs::read_to_string(
                temp.path()
                    .join("project/chapters")
                    .join(format!("{document_id}.md"))
            )
            .expect("read"),
            "new content"
        );
        assert!(
            temp.path()
                .join("project/.lingbi/receipts")
                .join(format!("{}.json", candidate.id))
                .exists()
        );
        assert!(
            temp.path()
                .join("project/.lingbi/approvals")
                .join(format!("{}.json", approval.id))
                .exists()
        );
        assert!(
            temp.path()
                .join("project/.lingbi/intents")
                .read_dir()
                .expect("intents")
                .next()
                .is_some()
        );
    }

    #[tokio::test]
    async fn same_idempotency_key_survives_new_engine() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("project");
        let (first, document_id, _) = setup_engine(&temp);
        let candidate = first
            .propose(candidate(document_id, "new content"))
            .expect("propose");
        let approval = first.approve(candidate.id, "user").expect("approve");
        let intent = CommitIntent {
            id: Uuid::new_v4(),
            candidate_id: candidate.id,
            approval_id: approval.id,
            target_path: "chapters/chapter.md".to_owned(),
            expected_revision: 0,
            idempotency_key: "key-1".to_owned(),
        };
        let first_receipt = first.commit(intent.clone()).expect("first commit");
        let before_bytes = fs::read(
            temp.path()
                .join("project/chapters")
                .join(format!("{document_id}.md")),
        )
        .expect("read before");

        let second = MutationEngine::new(root);
        let second_receipt = second.commit(intent).expect("second commit");
        let after_bytes = fs::read(
            temp.path()
                .join("project/chapters")
                .join(format!("{document_id}.md")),
        )
        .expect("read after");

        assert_eq!(first_receipt.id, second_receipt.id);
        assert_eq!(first_receipt.after_revision, second_receipt.after_revision);
        assert_eq!(before_bytes, after_bytes);
    }

    #[tokio::test]
    async fn propose_and_approve_survive_new_engine() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("project");
        let (first, document_id, _) = setup_engine(&temp);
        let candidate = first
            .propose(candidate(document_id, "new content"))
            .expect("propose");
        let approval = first.approve(candidate.id, "user").expect("approve");

        let second = MutationEngine::new(root);
        let loaded_candidate = second
            .candidates
            .read(candidate.id)
            .expect("read candidate")
            .expect("candidate");
        let loaded_approval = second
            .approvals
            .read(approval.id)
            .expect("read approval")
            .expect("approval");

        assert_eq!(loaded_candidate.status, CandidateStatus::Approved);
        assert_eq!(loaded_approval.candidate_id, candidate.id);
    }

    #[tokio::test]
    async fn stale_temp_never_becomes_canonical() {
        let temp = TempDir::new().expect("temp dir");
        let (engine, document_id, _) = setup_engine(&temp);
        let path = temp
            .path()
            .join("project/chapters")
            .join(format!("{document_id}.md"));
        let stale = path.with_file_name(format!("{document_id}.md.tmp-stale"));
        fs::write(&stale, "stale").expect("stale");

        let candidate = engine
            .propose(candidate(document_id, "new content"))
            .expect("propose");
        let approval = engine.approve(candidate.id, "user").expect("approve");

        assert_eq!(
            fs::read_to_string(&path).expect("read canonical"),
            "old content"
        );
        engine
            .commit(CommitIntent {
                id: Uuid::new_v4(),
                candidate_id: candidate.id,
                approval_id: approval.id,
                target_path: "chapters/chapter.md".to_owned(),
                expected_revision: 0,
                idempotency_key: "key-1".to_owned(),
            })
            .expect("commit");

        assert_eq!(
            fs::read_to_string(&path).expect("read canonical"),
            "new content"
        );
        assert_eq!(fs::read_to_string(&stale).expect("read stale"), "stale");
    }
}
