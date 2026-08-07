use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::{CandidateStatus, Document};
use lingbi_mutation::{
    Approval, ApprovalRepository, CommitIntent, CommitReceipt, MutationEngine, ReceiptRepository,
};
use lingbi_storage::CandidateRepository;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::DocumentApplicationService;

/// Thin orchestration over the ONE production commit path
/// (`MutationEngine`). There is exactly one write path for adopting a
/// candidate:
///
/// ```text
/// Candidate
///   → 持久化 Approval
///   → 持久化 CommitIntent
///   → 正文写入 (atomic, expected-hash guarded)
///   → metadata 更新 (revision + content hash)
///   → 持久化 Receipt
///   → Candidate = Committed
/// ```
///
/// GenerationService generates candidates only; it never writes document
/// bodies itself. This coordinator only performs the pre-commit checks
/// (idempotency, staleness) and the approval step, then delegates the
/// commit to `MutationEngine::commit`.
pub struct MutationCoordinator {
    documents: Arc<DocumentApplicationService>,
    candidates: CandidateRepository,
    approvals: ApprovalRepository,
    receipts: ReceiptRepository,
    engine: MutationEngine,
}

impl MutationCoordinator {
    pub fn new(root: impl Into<PathBuf>, documents: Arc<DocumentApplicationService>) -> Self {
        let root = root.into();
        Self {
            documents: documents.clone(),
            candidates: CandidateRepository::new(root.clone()),
            approvals: ApprovalRepository::new(root.clone()),
            receipts: ReceiptRepository::new(root.clone()),
            engine: MutationEngine::new(root),
        }
    }

    pub async fn approve_and_commit(
        &self,
        candidate_id: Uuid,
    ) -> Result<(Document, CommitReceipt), AppError> {
        let candidate = self.candidates.read(candidate_id)?.ok_or_else(|| {
            AppError::new(
                ErrorCode::DocumentNotFound,
                format!("candidate not found: {candidate_id}"),
                false,
            )
        })?;

        // Idempotent: an already committed candidate returns its receipt.
        if let Some(receipt) = self.receipts.read(candidate_id)?
            && candidate.status == CandidateStatus::Committed
        {
            let document = self.documents.get_document(candidate.document_id)?;
            return Ok((document, receipt));
        }

        if !matches!(
            candidate.status,
            CandidateStatus::Pending | CandidateStatus::Approved
        ) {
            return Err(AppError::new(
                ErrorCode::MutationNotApproved,
                "candidate is not approved".to_owned(),
                false,
            ));
        }

        // Staleness: the user may have edited the document since the
        // candidate was generated. Never overwrite user edits.
        let current = self.documents.get_document(candidate.document_id)?;
        if current.revision != candidate.base_revision
            || !current
                .content_hash
                .eq_ignore_ascii_case(&candidate.base_content_hash)
        {
            if let Some(mut stale) = self.candidates.read(candidate_id)? {
                stale.mark_stale();
                self.candidates.write(&stale)?;
            }
            return Err(AppError::new(
                ErrorCode::CandidateStale,
                "candidate is stale".to_owned(),
                false,
            ));
        }

        // 1. Approval (persisted; survives restart between this and commit).
        let approval: Approval = match self.approvals.find_by_candidate(candidate_id)? {
            Some(existing) => existing,
            None => {
                let mut to_approve = candidate.clone();
                to_approve.approve();
                self.candidates.write(&to_approve)?;
                let approval = to_approve
                    .approved_at
                    .map(|_| Approval {
                        id: Uuid::new_v4(),
                        candidate_id,
                        actor: "user".to_owned(),
                        approved_at: chrono::Utc::now(),
                    })
                    .expect("approved candidate has approval time");
                self.approvals.write(&approval)?;
                approval
            }
        };

        // 2-7. The single production commit path.
        let intent = CommitIntent {
            id: Uuid::new_v4(),
            candidate_id,
            approval_id: approval.id,
            target_path: current.physical_path().to_string_lossy().into_owned(),
            expected_revision: current.revision,
            idempotency_key: format!("adopt-{candidate_id}"),
        };
        let receipt = self.engine.commit(intent)?;
        let document = self.documents.get_document(candidate.document_id)?;
        Ok((document, receipt))
    }
}
