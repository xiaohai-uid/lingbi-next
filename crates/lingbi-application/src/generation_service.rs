use crate::DocumentApplicationService;
use chrono::Utc;
use futures_util::StreamExt;
use lingbi_ai::{AiEvent, AiProvider, ChatMessage, ChatRequest};
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::{Candidate, CandidateStatus, Document};
use lingbi_storage::CandidateRepository;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

pub struct GenerationService {
    provider: Arc<dyn AiProvider>,
    documents: Arc<DocumentApplicationService>,
    candidates: CandidateRepository,
}

impl GenerationService {
    pub fn new(
        root: impl Into<PathBuf>,
        provider: Arc<dyn AiProvider>,
        documents: Arc<DocumentApplicationService>,
    ) -> Self {
        let root = root.into();
        Self {
            provider,
            documents,
            candidates: CandidateRepository::new(root),
        }
    }

    pub async fn generate(
        &self,
        chapter_id: Uuid,
        instruction: impl Into<String>,
    ) -> Result<Candidate, AppError> {
        let document = self.documents.get_document(chapter_id)?;
        let instruction = instruction.into();
        let request = ChatRequest {
            messages: vec![
                ChatMessage {
                    role: "system".to_owned(),
                    content: "你是中文小说写作助手。".to_owned(),
                },
                ChatMessage {
                    role: "user".to_owned(),
                    content: instruction.clone(),
                },
            ],
            temperature: 0.7,
            max_tokens: 2048,
        };

        let mut stream = self.provider.stream_chat(request);
        let mut content = String::new();
        while let Some(event) = stream.next().await {
            match event.map_err(ai_error)? {
                AiEvent::ContentDelta(delta) => content.push_str(&delta),
                AiEvent::Completed => break,
                _ => {}
            }
        }

        if content.trim().is_empty() {
            return Err(AppError::new(
                ErrorCode::AiInvalidResponse,
                "model returned empty content".to_owned(),
                false,
            ));
        }

        let content_hash = hex_sha256(content.as_bytes());
        let candidate = Candidate {
            id: Uuid::new_v4(),
            project_id: document.project_id,
            document_id: chapter_id,
            instruction,
            base_revision: document.revision,
            base_content_hash: document.content_hash.clone(),
            content,
            content_hash,
            provider_id: self.provider.provider_id().to_owned(),
            model_id: self.provider.model_id().to_owned(),
            status: CandidateStatus::Pending,
            created_at: Utc::now(),
            approved_at: None,
            committed_at: None,
        };
        self.write_candidate(&candidate)?;
        Ok(candidate)
    }

    pub fn list(&self, document_id: Uuid) -> Result<Vec<Candidate>, AppError> {
        Ok(self
            .candidates
            .list()?
            .into_iter()
            .filter(|candidate| candidate.document_id == document_id)
            .collect())
    }

    pub async fn adopt(
        &self,
        candidate_id: Uuid,
        expected_revision: u64,
    ) -> Result<Document, AppError> {
        let mut candidate = self.read_candidate(candidate_id)?;
        if candidate.status != CandidateStatus::Pending {
            return Err(AppError::new(
                ErrorCode::MutationNotApproved,
                "candidate is not pending".to_owned(),
                false,
            ));
        }

        let document = self
            .documents
            .save_document(
                candidate.document_id,
                expected_revision,
                candidate.content.clone(),
            )
            .await?;
        candidate.commit();
        self.write_candidate(&candidate)?;
        Ok(document)
    }

    pub fn reject(&self, candidate_id: Uuid) -> Result<(), AppError> {
        let mut candidate = self.read_candidate(candidate_id)?;
        candidate.reject();
        self.write_candidate(&candidate)
    }

    fn read_candidate(&self, candidate_id: Uuid) -> Result<Candidate, AppError> {
        self.candidates.read(candidate_id)?.ok_or_else(|| {
            AppError::new(
                ErrorCode::DocumentNotFound,
                format!("candidate not found: {candidate_id}"),
                false,
            )
        })
    }

    fn write_candidate(&self, candidate: &Candidate) -> Result<(), AppError> {
        self.candidates.write(candidate)
    }
}

fn ai_error(error: lingbi_ai::AiError) -> AppError {
    let (code, retryable) = match error {
        lingbi_ai::AiError::NoApiKey => (ErrorCode::AiNoApiKey, false),
        lingbi_ai::AiError::AuthFailed => (ErrorCode::AiAuthFailed, false),
        lingbi_ai::AiError::RateLimited => (ErrorCode::AiRateLimited, true),
        lingbi_ai::AiError::Timeout => (ErrorCode::AiTimeout, true),
        lingbi_ai::AiError::Network => (ErrorCode::AiNetworkError, true),
        lingbi_ai::AiError::Server(_) => (ErrorCode::AiServerError, true),
        lingbi_ai::AiError::InvalidResponse => (ErrorCode::AiInvalidResponse, false),
    };
    AppError::new(code, error.to_string(), retryable)
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CreateProjectRequest, DocumentApplicationService, ProjectApplicationService};
    use lingbi_ai::{AiError, FakeProvider};
    use tempfile::TempDir;

    async fn setup() -> (
        TempDir,
        crate::ProjectSessionSnapshot,
        Arc<DocumentApplicationService>,
    ) {
        let temp = TempDir::new().expect("temp dir");
        let service = ProjectApplicationService::new();
        let snapshot = service
            .create_project(CreateProjectRequest {
                name: "测试小说".to_owned(),
                root: temp.path().join("novel"),
            })
            .await
            .expect("create");
        let documents = Arc::new(DocumentApplicationService::new(temp.path().join("novel")));
        (temp, snapshot, documents)
    }

    #[tokio::test]
    async fn fake_provider_creates_candidate_without_canonical_write() {
        let (temp, snapshot, documents) = setup().await;
        let provider = Arc::new(FakeProvider::new("第一章正文：雨夜。"));
        let generation = GenerationService::new(temp.path().join("novel"), provider, documents);

        let candidate = generation
            .generate(snapshot.current_document.id, "写一个雨夜开场")
            .await
            .expect("generate");

        assert_eq!(candidate.status, CandidateStatus::Pending);
        assert_eq!(candidate.content, "第一章正文：雨夜。");
        assert_eq!(
            generation
                .list(snapshot.current_document.id)
                .expect("list")
                .len(),
            1
        );
        assert!(
            temp.path()
                .join("novel/chapters")
                .join(format!("{}.md", snapshot.current_document.id))
                .exists()
        );
    }

    #[tokio::test]
    async fn provider_error_creates_no_candidate() {
        let (temp, snapshot, documents) = setup().await;
        let provider = Arc::new(FakeProvider::with_error(AiError::AuthFailed));
        let generation = GenerationService::new(temp.path().join("novel"), provider, documents);

        let result = generation
            .generate(snapshot.current_document.id, "写")
            .await;

        assert!(result.is_err());
        assert!(
            generation
                .list(snapshot.current_document.id)
                .expect("list")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn adopt_updates_canonical_content_and_survives_restart() {
        let (temp, snapshot, documents) = setup().await;
        let provider = Arc::new(FakeProvider::new("第一章正文：雨夜。"));
        let generation =
            GenerationService::new(temp.path().join("novel"), provider, documents.clone());
        let candidate = generation
            .generate(snapshot.current_document.id, "写")
            .await
            .expect("generate");

        let adopted = generation.adopt(candidate.id, 0).await.expect("adopt");

        assert_eq!(adopted.revision, 1);
        assert_eq!(
            documents
                .read_document(snapshot.current_document.id)
                .await
                .expect("read"),
            "第一章正文：雨夜。"
        );
        assert_eq!(
            generation.list(snapshot.current_document.id).expect("list")[0].status,
            CandidateStatus::Committed
        );

        let restarted = DocumentApplicationService::new(temp.path().join("novel"));
        assert_eq!(
            restarted
                .read_document(snapshot.current_document.id)
                .await
                .expect("restart read"),
            "第一章正文：雨夜。"
        );
    }
}
