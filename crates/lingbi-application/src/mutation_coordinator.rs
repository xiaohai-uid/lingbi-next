use chrono::Utc;
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::{CandidateStatus, Document};
use lingbi_mutation::{CommitReceipt, ReceiptRepository};
use lingbi_storage::CandidateRepository;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use crate::DocumentApplicationService;

pub struct MutationCoordinator {
    documents: Arc<DocumentApplicationService>,
    candidates: CandidateRepository,
    receipts: ReceiptRepository,
}

impl MutationCoordinator {
    pub fn new(root: impl Into<PathBuf>, documents: Arc<DocumentApplicationService>) -> Self {
        let root = root.into();
        Self {
            documents,
            candidates: CandidateRepository::new(root.clone()),
            receipts: ReceiptRepository::new(root),
        }
    }

    pub async fn approve_and_commit(
        &self,
        candidate_id: Uuid,
    ) -> Result<(Document, CommitReceipt), AppError> {
        let mut candidate = self.candidates.read(candidate_id)?.ok_or_else(|| {
            AppError::new(
                ErrorCode::DocumentNotFound,
                format!("candidate not found: {candidate_id}"),
                false,
            )
        })?;

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

        let current = self.documents.get_document(candidate.document_id)?;
        if current.revision != candidate.base_revision
            || !current
                .content_hash
                .eq_ignore_ascii_case(&candidate.base_content_hash)
        {
            candidate.mark_stale();
            self.candidates.write(&candidate)?;
            return Err(AppError::new(
                ErrorCode::CandidateStale,
                "candidate is stale".to_owned(),
                false,
            ));
        }

        if candidate.status == CandidateStatus::Pending {
            candidate.approve();
            self.candidates.write(&candidate)?;
        }

        let document = self
            .documents
            .save_document(
                candidate.document_id,
                current.revision,
                candidate.content.clone(),
            )
            .await?;
        let receipt = CommitReceipt {
            id: Uuid::new_v4(),
            candidate_id,
            target_path: format!("chapters/{}.md", candidate.document_id),
            after_revision: document.revision,
            after_content_hash: document.content_hash.clone(),
            committed_at: Utc::now(),
            idempotency_key: format!("adopt-{candidate_id}"),
        };
        self.receipts.write(&receipt)?;
        candidate.commit();
        self.candidates.write(&candidate)?;
        Ok((document, receipt))
    }
}
