use chrono::Utc;
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::{Candidate, CandidateStatus};
use lingbi_security::ProjectPathGuard;
use lingbi_storage::{AtomicFileStore, CandidateRepository, DiskAtomicFileStore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
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
}

pub struct MutationEngine {
    root: PathBuf,
    guard: ProjectPathGuard,
    store: DiskAtomicFileStore,
    candidates: CandidateRepository,
    approvals: HashMap<Uuid, Approval>,
    receipts: HashMap<String, CommitReceipt>,
    revisions: HashMap<PathBuf, u64>,
}

impl MutationEngine {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let _ = std::fs::create_dir_all(&root);
        Self {
            guard: ProjectPathGuard::new(root.clone()),
            root: root.clone(),
            store: DiskAtomicFileStore,
            candidates: CandidateRepository::new(root),
            approvals: HashMap::new(),
            receipts: HashMap::new(),
            revisions: HashMap::new(),
        }
    }

    pub fn propose(&mut self, mut candidate: Candidate) -> Result<Candidate, AppError> {
        candidate.status = CandidateStatus::Pending;
        self.candidates.write(&candidate)?;
        Ok(candidate)
    }

    pub fn approve(
        &mut self,
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
        self.approvals.insert(approval.id, approval.clone());
        Ok(approval)
    }

    pub fn commit(&mut self, intent: CommitIntent) -> Result<CommitReceipt, AppError> {
        if let Some(existing) = self.receipts.get(&intent.idempotency_key) {
            return Ok(existing.clone());
        }

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

        let approval = self.approvals.get(&intent.approval_id).ok_or_else(|| {
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

        let target = self
            .guard
            .resolve(Path::new(&intent.target_path))
            .map_err(|_| {
                AppError::new(
                    ErrorCode::ProjectCorrupted,
                    "commit target escapes project boundary".to_owned(),
                    false,
                )
            })?;
        let current_revision = self.revisions.get(&target).copied().unwrap_or(0);
        if current_revision != intent.expected_revision {
            return Err(AppError::new(
                ErrorCode::MutationConflict,
                "revision conflict".to_owned(),
                false,
            ));
        }

        let payload = candidate.content.as_bytes();
        let content_hash = hex_sha256(payload);
        self.store.write_atomic(&target, payload, None)?;
        let verified = self.store.read(&target)?;
        if hex_sha256(&verified) != content_hash {
            return Err(AppError::new(
                ErrorCode::ProjectCorrupted,
                "verified content hash mismatch".to_owned(),
                false,
            ));
        }

        let after_revision = current_revision + 1;
        self.revisions.insert(target.clone(), after_revision);
        let receipt = CommitReceipt {
            id: Uuid::new_v4(),
            candidate_id: intent.candidate_id,
            target_path: intent.target_path.clone(),
            after_revision,
            after_content_hash: content_hash,
            committed_at: Utc::now(),
        };
        self.receipts
            .insert(intent.idempotency_key.clone(), receipt.clone());
        candidate.commit();
        self.candidates.write(&candidate)?;
        Ok(receipt)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn candidate(content: &str) -> Candidate {
        Candidate {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            document_id: Uuid::new_v4(),
            instruction: "write".to_owned(),
            base_revision: 0,
            base_content_hash: "before".to_owned(),
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

    #[tokio::test]
    async fn unapproved_commit_is_rejected() {
        let temp = TempDir::new().expect("temp dir");
        let mut engine = MutationEngine::new(temp.path().join("project"));
        let candidate = engine.propose(candidate("new content")).expect("propose");

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
        assert!(!temp.path().join("project/chapters/chapter.md").exists());
    }

    #[tokio::test]
    async fn revision_conflict_is_rejected() {
        let temp = TempDir::new().expect("temp dir");
        let mut engine = MutationEngine::new(temp.path().join("project"));
        let candidate = engine.propose(candidate("new content")).expect("propose");
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
    async fn approved_candidate_is_committed() {
        let temp = TempDir::new().expect("temp dir");
        let mut engine = MutationEngine::new(temp.path().join("project"));
        let candidate = engine.propose(candidate("new content")).expect("propose");
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
            fs::read_to_string(temp.path().join("project/chapters/chapter.md")).expect("read"),
            "new content"
        );
    }

    #[tokio::test]
    async fn same_idempotency_key_does_not_double_write() {
        let temp = TempDir::new().expect("temp dir");
        let mut engine = MutationEngine::new(temp.path().join("project"));
        let candidate = engine.propose(candidate("new content")).expect("propose");
        let approval = engine.approve(candidate.id, "user").expect("approve");
        let intent = CommitIntent {
            id: Uuid::new_v4(),
            candidate_id: candidate.id,
            approval_id: approval.id,
            target_path: "chapters/chapter.md".to_owned(),
            expected_revision: 0,
            idempotency_key: "key-1".to_owned(),
        };

        let first = engine.commit(intent.clone()).expect("first commit");
        let second = engine.commit(intent).expect("second commit");

        assert_eq!(first.id, second.id);
        assert_eq!(first.after_revision, second.after_revision);
    }

    #[tokio::test]
    async fn stale_temp_never_becomes_canonical() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("project");
        fs::create_dir_all(root.join("chapters")).expect("chapters");
        let path = root.join("chapters/chapter.md");
        fs::write(&path, "old content").expect("canonical");
        let stale = root.join("chapters/chapter.md.tmp-stale");
        fs::write(&stale, "stale").expect("stale");

        let mut engine = MutationEngine::new(&root);
        let candidate = engine.propose(candidate("new content")).expect("propose");
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
