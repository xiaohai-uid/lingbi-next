use crate::DocumentApplicationService;
use chrono::Utc;
use futures_util::StreamExt;
use lingbi_ai::{AiEvent, AiProvider, ChatMessage, ChatRequest};
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::Document;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedCandidate {
    pub id: Uuid,
    pub chapter_id: Uuid,
    pub instruction: String,
    pub content: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct GenerationService {
    root: PathBuf,
    provider: Arc<dyn AiProvider>,
    documents: Arc<DocumentApplicationService>,
}

impl GenerationService {
    pub fn new(
        root: impl Into<PathBuf>,
        provider: Arc<dyn AiProvider>,
        documents: Arc<DocumentApplicationService>,
    ) -> Self {
        Self {
            root: root.into(),
            provider,
            documents,
        }
    }

    pub async fn generate(
        &self,
        chapter_id: Uuid,
        instruction: impl Into<String>,
    ) -> Result<GeneratedCandidate, AppError> {
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

        let candidate = GeneratedCandidate {
            id: Uuid::new_v4(),
            chapter_id,
            instruction,
            content,
            status: "pending".to_owned(),
            created_at: Utc::now(),
        };
        self.write_candidate(&candidate)?;
        Ok(candidate)
    }

    pub fn list(&self, chapter_id: Uuid) -> Result<Vec<GeneratedCandidate>, AppError> {
        let dir = self.root.join(".lingbi/candidates");
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut candidates = Vec::new();
        for entry in fs::read_dir(&dir).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let candidate: GeneratedCandidate =
                serde_json::from_slice(&fs::read(&path).map_err(io_error)?).map_err(parse_error)?;
            if candidate.chapter_id == chapter_id {
                candidates.push(candidate);
            }
        }
        Ok(candidates)
    }

    pub async fn adopt(
        &self,
        candidate_id: Uuid,
        expected_revision: u64,
    ) -> Result<Document, AppError> {
        let candidate = self.read_candidate(candidate_id)?;
        if candidate.status != "pending" {
            return Err(AppError::new(
                ErrorCode::MutationNotApproved,
                "candidate is not pending".to_owned(),
                false,
            ));
        }

        let document = self
            .documents
            .save_document(
                candidate.chapter_id,
                expected_revision,
                candidate.content.clone(),
            )
            .await?;
        let mut adopted = candidate;
        adopted.status = "adopted".to_owned();
        self.write_candidate(&adopted)?;
        Ok(document)
    }

    pub fn reject(&self, candidate_id: Uuid) -> Result<(), AppError> {
        let mut candidate = self.read_candidate(candidate_id)?;
        candidate.status = "rejected".to_owned();
        self.write_candidate(&candidate)
    }

    fn read_candidate(&self, candidate_id: Uuid) -> Result<GeneratedCandidate, AppError> {
        let path = self
            .root
            .join(".lingbi/candidates")
            .join(format!("{candidate_id}.json"));
        let bytes = fs::read(&path).map_err(|_| {
            AppError::new(
                ErrorCode::DocumentNotFound,
                format!("candidate not found: {candidate_id}"),
                false,
            )
        })?;
        serde_json::from_slice(&bytes).map_err(parse_error)
    }

    fn write_candidate(&self, candidate: &GeneratedCandidate) -> Result<(), AppError> {
        let dir = self.root.join(".lingbi/candidates");
        fs::create_dir_all(&dir).map_err(io_error)?;
        let path = dir.join(format!("{}.json", candidate.id));
        let bytes = serde_json::to_vec(candidate).map_err(parse_error)?;
        fs::write(path, bytes).map_err(io_error)
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

fn io_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("generation I/O failed: {error}"),
        false,
    )
}

fn parse_error(error: serde_json::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("candidate metadata parse failed: {error}"),
        false,
    )
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

        assert_eq!(candidate.status, "pending");
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
            "adopted"
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
