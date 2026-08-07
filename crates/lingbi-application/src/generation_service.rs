use crate::DocumentApplicationService;
use chrono::Utc;
use futures_util::StreamExt;
use lingbi_ai::{AiEvent, AiProvider, CancellationToken, ChatMessage, ChatRequest};
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::{Candidate, CandidateStatus, Document};
use lingbi_storage::CandidateRepository;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::MutationCoordinator;

pub struct GenerationService {
    provider: Arc<dyn AiProvider>,
    documents: Arc<DocumentApplicationService>,
    candidates: CandidateRepository,
    coordinator: MutationCoordinator,
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
            documents: documents.clone(),
            candidates: CandidateRepository::new(root.clone()),
            coordinator: MutationCoordinator::new(root, documents.clone()),
        }
    }

    pub async fn generate(
        &self,
        chapter_id: Uuid,
        instruction: impl Into<String>,
    ) -> Result<Candidate, AppError> {
        let (_deltas, _) = tokio::sync::mpsc::unbounded_channel();
        self.generate_with_cancel_stream(chapter_id, instruction, CancellationToken::new(), _deltas)
            .await
    }

    pub async fn generate_with_cancel_stream(
        &self,
        chapter_id: Uuid,
        instruction: impl Into<String>,
        cancel: CancellationToken,
        deltas: UnboundedSender<String>,
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

        let mut stream = self
            .provider
            .stream_chat_with_cancel(request, cancel.clone());
        let mut content = String::new();
        while let Some(event) = stream.next().await {
            // Belt and braces: even if the provider does not honor the
            // token, the service stops the moment cancellation arrives.
            if cancel.is_cancelled() {
                return Err(AppError::new(
                    ErrorCode::AiCancelled,
                    "AI generation cancelled".to_owned(),
                    false,
                ));
            }
            match event.map_err(ai_error)? {
                AiEvent::ContentDelta(delta) => {
                    let _ = deltas.send(delta.clone());
                    content.push_str(&delta);
                }
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
        _expected_revision: u64,
    ) -> Result<Document, AppError> {
        let (document, _receipt) = self.coordinator.approve_and_commit(candidate_id).await?;
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
        lingbi_ai::AiError::Cancelled => (ErrorCode::AiCancelled, false),
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
        assert!(
            temp.path()
                .join("novel/.lingbi/receipts")
                .join(format!("{}.json", candidate.id))
                .exists()
        );
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

    #[tokio::test]
    async fn stale_candidate_is_rejected_and_user_edits_survive() {
        let (temp, snapshot, documents) = setup().await;
        let provider = Arc::new(FakeProvider::new("AI 候选正文"));
        let generation =
            GenerationService::new(temp.path().join("novel"), provider, documents.clone());
        let candidate = generation
            .generate(snapshot.current_document.id, "写")
            .await
            .expect("generate");

        documents
            .save_document(
                snapshot.current_document.id,
                snapshot.current_document.revision,
                "用户手动保存的新正文",
            )
            .await
            .expect("user save");

        let result = generation
            .adopt(candidate.id, snapshot.current_document.revision)
            .await;

        assert!(matches!(
            result,
            Err(AppError {
                code: ErrorCode::CandidateStale,
                ..
            })
        ));
        assert_eq!(
            documents
                .read_document(snapshot.current_document.id)
                .await
                .expect("read"),
            "用户手动保存的新正文"
        );
        assert_eq!(
            generation.list(snapshot.current_document.id).expect("list")[0].status,
            CandidateStatus::Stale
        );
    }

    #[tokio::test]
    async fn adopt_goes_through_the_single_unified_mutation_path() {
        let (temp, snapshot, documents) = setup().await;
        let provider = Arc::new(FakeProvider::new("统一路径正文"));
        let generation = GenerationService::new(temp.path().join("novel"), provider, documents);
        let candidate = generation
            .generate(snapshot.current_document.id, "写")
            .await
            .expect("generate");

        let adopted = generation.adopt(candidate.id, 0).await.expect("adopt");

        // Approval persisted.
        let approvals = temp.path().join("novel/.lingbi/approvals");
        assert!(
            approvals.read_dir().expect("approvals").next().is_some(),
            "approval must be persisted"
        );
        // CommitIntent persisted.
        let intents = temp.path().join("novel/.lingbi/intents");
        assert!(
            intents.read_dir().expect("intents").next().is_some(),
            "commit intent must be persisted"
        );
        // Receipt persisted and candidate committed.
        assert!(
            temp.path()
                .join("novel/.lingbi/receipts")
                .join(format!("{}.json", candidate.id))
                .exists()
        );
        assert_eq!(
            generation.list(snapshot.current_document.id).expect("list")[0].status,
            CandidateStatus::Committed
        );
        // Metadata updated: revision and content hash on disk.
        let loaded = lingbi_storage::DocumentRepository::new(temp.path().join("novel"))
            .find(snapshot.current_document.id)
            .expect("find")
            .expect("document");
        assert_eq!(loaded.revision, adopted.revision);
        assert_eq!(loaded.revision, 1);
        assert_eq!(
            loaded.content_hash, candidate.content_hash,
            "metadata content hash must match committed content"
        );
    }

    #[tokio::test]
    async fn consecutive_candidate_adoptions_advance_revisions() {
        let (temp, snapshot, documents) = setup().await;
        let provider = Arc::new(FakeProvider::new("正文"));
        let generation = GenerationService::new(temp.path().join("novel"), provider, documents);
        let document_id = snapshot.current_document.id;
        let mut revision = snapshot.current_document.revision;

        for index in 0..10 {
            let candidate = generation
                .generate(document_id, format!("写第 {index} 次"))
                .await
                .expect("generate");
            let adopted = generation
                .adopt(candidate.id, revision)
                .await
                .unwrap_or_else(|error| panic!("adoption {index} failed: {error}"));
            assert_eq!(adopted.revision, revision + 1);
            revision += 1;
        }

        assert_eq!(revision, 10);
        assert_eq!(generation.list(document_id).expect("list").len(), 10);
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::*;
    use crate::{CreateProjectRequest, DocumentApplicationService, ProjectApplicationService};
    use lingbi_ai::AiStream;
    use std::time::Instant;
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

    /// Provider that emits one ContentDelta every `delay`, then completes.
    /// Mirrors the acceptance contract: "测试 Provider 每 500ms 返回一个 chunk".
    struct ChunkedProvider {
        chunks: Vec<String>,
        delay: std::time::Duration,
    }

    impl ChunkedProvider {
        fn new(chunks: Vec<&str>) -> Self {
            Self {
                chunks: chunks.into_iter().map(str::to_owned).collect(),
                delay: std::time::Duration::from_millis(500),
            }
        }
    }

    impl AiProvider for ChunkedProvider {
        fn provider_id(&self) -> &str {
            "chunked"
        }
        fn model_id(&self) -> &str {
            "chunked-model"
        }
        fn stream_chat(&self, _request: ChatRequest) -> AiStream {
            let chunks = self.chunks.clone();
            let delay = self.delay;
            Box::pin(async_stream::stream! {
                for chunk in chunks {
                    tokio::time::sleep(delay).await;
                    yield Ok(AiEvent::ContentDelta(chunk));
                }
                yield Ok(AiEvent::Completed);
            })
        }
    }

    #[tokio::test]
    async fn first_delta_arrives_before_provider_completes() {
        let (temp, snapshot, documents) = setup().await;
        let provider = Arc::new(ChunkedProvider::new(vec![
            "第一段",
            "第二段",
            "第三段",
            "第四段",
            "第五段",
        ]));
        let generation = GenerationService::new(temp.path().join("novel"), provider, documents);

        let (deltas, mut deltas_rx) = tokio::sync::mpsc::unbounded_channel();
        let started = Instant::now();
        let generation_task = tokio::spawn({
            let chapter_id = snapshot.current_document.id;
            let service = generation;
            async move {
                service
                    .generate_with_cancel_stream(
                        chapter_id,
                        "写一个雨夜开场",
                        CancellationToken::new(),
                        deltas,
                    )
                    .await
            }
        });

        let first_delta = tokio::time::timeout(std::time::Duration::from_secs(3), deltas_rx.recv())
            .await
            .expect("first delta must arrive while generation is running")
            .expect("delta channel open");
        let first_delta_time = started.elapsed();

        let candidate = generation_task
            .await
            .expect("generation task")
            .expect("generate");
        let completion_time = started.elapsed();

        assert_eq!(first_delta, "第一段");
        assert_eq!(candidate.content, "第一段第二段第三段第四段第五段");
        assert!(
            first_delta_time < completion_time,
            "first delta at {first_delta_time:?} must arrive before completion at {completion_time:?}"
        );
        // The first 500ms chunk must be visible well before the stream ends
        // (~2.5s); a buffered implementation would only deliver it at the end.
        assert!(
            first_delta_time < std::time::Duration::from_secs(1),
            "first delta took {first_delta_time:?}, streaming is not real-time"
        );
        // With 5 chunks at 500ms, completion must take >= ~2.5s, proving
        // the deltas were not buffered until the end.
        assert!(
            completion_time >= std::time::Duration::from_millis(2000),
            "provider completion too fast: {completion_time:?}"
        );
    }
}
