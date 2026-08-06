use lingbi_ai::{AiError, CancellationToken, OpenAiCompatibleProvider};
use lingbi_application::{
    Candidate, CreateProjectRequest, DocumentApplicationService, GenerationService,
    ProjectApplicationService, ProjectSessionSnapshot,
};
use lingbi_contracts::AppError;
use lingbi_domain::{Document, Project};
use lingbi_security::{KeyringSecretStore, SecretStore, SecretString};
use lingbi_writing::{GenerationManager, GenerationRequest, GenerationState};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};
use uuid::Uuid;

#[derive(Default)]
struct DesktopState {
    project_service: Arc<ProjectApplicationService>,
    document_services: Mutex<HashMap<String, Arc<DocumentApplicationService>>>,
    current: Mutex<Option<CurrentSession>>,
    secrets: KeyringSecretStore,
    generation: GenerationManager,
    generation_services: Mutex<HashMap<String, Arc<GenerationService>>>,
    generation_tokens: Mutex<HashMap<Uuid, CancellationToken>>,
}

#[derive(Clone)]
struct CurrentSession {
    root: String,
    snapshot: ProjectSessionSnapshot,
}

#[derive(Serialize)]
struct SessionDto {
    project: Project,
    current_document: Document,
    dirty: bool,
    root: String,
}

impl From<CurrentSession> for SessionDto {
    fn from(value: CurrentSession) -> Self {
        Self {
            project: value.snapshot.project,
            current_document: value.snapshot.current_document,
            dirty: value.snapshot.dirty,
            root: value.root,
        }
    }
}

#[derive(Clone, Serialize)]
struct CommandErrorDto {
    code: String,
    message: String,
    retryable: bool,
}

impl From<AppError> for CommandErrorDto {
    fn from(error: AppError) -> Self {
        Self {
            code: format!("{:?}", error.code),
            message: error.message,
            retryable: error.retryable,
        }
    }
}

fn command_error(code: &str, message: impl Into<String>, retryable: bool) -> CommandErrorDto {
    CommandErrorDto {
        code: code.to_owned(),
        message: message.into(),
        retryable,
    }
}

fn parse_uuid(value: &str) -> Result<Uuid, CommandErrorDto> {
    Uuid::parse_str(value).map_err(|error| {
        command_error(
            "INVALID_UUID",
            format!("invalid UUID: {value}: {error}"),
            false,
        )
    })
}

#[derive(Serialize)]
struct GenerationStartDto {
    task_id: Uuid,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum GenerationEventDto {
    Delta {
        task_id: Uuid,
        content: String,
    },
    Candidate {
        task_id: Uuid,
        candidate: Candidate,
    },
    Error {
        task_id: Uuid,
        error: CommandErrorDto,
    },
    Cancelled {
        task_id: Uuid,
    },
}

#[tauri::command]
async fn project_create(
    state: State<'_, DesktopState>,
    name: String,
    root: String,
) -> Result<SessionDto, CommandErrorDto> {
    let snapshot = state
        .project_service
        .create_project(CreateProjectRequest {
            name,
            root: root.clone().into(),
        })
        .await
        .map_err(CommandErrorDto::from)?;
    let documents = Arc::new(DocumentApplicationService::new(root.clone()));
    state
        .document_services
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "document service lock", false))?
        .insert(root.clone(), documents);
    let current = CurrentSession { root, snapshot };
    let dto = SessionDto::from(current.clone());
    *state
        .current
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "session lock", false))? = Some(current);
    Ok(dto)
}

#[tauri::command]
async fn project_open(
    state: State<'_, DesktopState>,
    root: String,
) -> Result<SessionDto, CommandErrorDto> {
    let snapshot = state
        .project_service
        .open_project(root.clone().into())
        .await
        .map_err(CommandErrorDto::from)?;
    let documents = Arc::new(DocumentApplicationService::new(root.clone()));
    state
        .document_services
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "document service lock", false))?
        .insert(root.clone(), documents);
    let current = CurrentSession { root, snapshot };
    let dto = SessionDto::from(current.clone());
    *state
        .current
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "session lock", false))? = Some(current);
    Ok(dto)
}

#[tauri::command]
async fn project_get_session(
    state: State<'_, DesktopState>,
) -> Result<Option<SessionDto>, CommandErrorDto> {
    Ok(state
        .current
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "session lock", false))?
        .clone()
        .map(SessionDto::from))
}

#[tauri::command]
async fn document_list(state: State<'_, DesktopState>) -> Result<Vec<Document>, CommandErrorDto> {
    let root = current_root(&state)?;
    let documents = document_service(&state, &root)?;
    documents.list_documents().map_err(CommandErrorDto::from)
}

#[tauri::command]
async fn document_create(
    state: State<'_, DesktopState>,
    project_id: String,
    title: String,
    content: String,
) -> Result<Document, CommandErrorDto> {
    let root = current_root(&state)?;
    let documents = document_service(&state, &root)?;
    let project_id = parse_uuid(&project_id)?;
    documents
        .create_document(project_id, title, content)
        .await
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
async fn document_open(
    state: State<'_, DesktopState>,
    document_id: String,
) -> Result<String, CommandErrorDto> {
    let root = current_root(&state)?;
    let documents = document_service(&state, &root)?;
    let document_id = parse_uuid(&document_id)?;
    documents
        .read_document(document_id)
        .await
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
async fn document_save(
    state: State<'_, DesktopState>,
    document_id: String,
    expected_revision: u64,
    content: String,
) -> Result<Document, CommandErrorDto> {
    let root = current_root(&state)?;
    let documents = document_service(&state, &root)?;
    let document_id = parse_uuid(&document_id)?;
    documents
        .save_document(document_id, expected_revision, content)
        .await
        .map_err(CommandErrorDto::from)
}

#[derive(Serialize)]
struct ProviderTestDto {
    provider_id: String,
    model_id: String,
    ok: bool,
    latency_ms: u64,
    error: Option<String>,
}

#[tauri::command]
async fn provider_configure(
    state: State<'_, DesktopState>,
    key: String,
    base_url: Option<String>,
    model: Option<String>,
) -> Result<(), CommandErrorDto> {
    state
        .secrets
        .put("provider_key", SecretString::new(key))
        .await
        .map_err(CommandErrorDto::from)?;
    state
        .secrets
        .put(
            "provider_base_url",
            SecretString::new(
                base_url.unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_owned()),
            ),
        )
        .await
        .map_err(CommandErrorDto::from)?;
    state
        .secrets
        .put(
            "provider_model",
            SecretString::new(model.unwrap_or_else(|| "gpt-4o-mini".to_owned())),
        )
        .await
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
async fn provider_test(state: State<'_, DesktopState>) -> Result<ProviderTestDto, CommandErrorDto> {
    let provider = configured_provider(&state).await?;
    let health = provider.test_connection().await;
    Ok(ProviderTestDto {
        provider_id: health.provider_id,
        model_id: health.model_id,
        ok: health.ok,
        latency_ms: health.latency_ms,
        error: health.error,
    })
}

#[tauri::command]
async fn generation_start(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    chapter_id: String,
    instruction: String,
) -> Result<GenerationStartDto, CommandErrorDto> {
    let chapter_id = parse_uuid(&chapter_id)?;
    let root = current_root(&state)?;
    let documents = document_service(&state, &root)?;
    let provider = configured_provider(&state).await?;
    let service = Arc::new(GenerationService::new(root.clone(), provider, documents));
    state
        .generation_services
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "generation service lock", false))?
        .insert(root.clone(), service.clone());
    let task_id = state.generation.start_generation(GenerationRequest {
        chapter_id,
        instruction: instruction.clone(),
    });
    let cancel = CancellationToken::new();
    state
        .generation_tokens
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "generation token lock", false))?
        .insert(task_id, cancel.clone());
    let (deltas, mut deltas_rx) = tokio::sync::mpsc::unbounded_channel();
    let task_app = app.clone();
    let task_service = service.clone();
    let task_cancel = cancel.clone();
    tokio::spawn(async move {
        let result = task_service
            .generate_with_cancel_stream(chapter_id, instruction, task_cancel.clone(), deltas)
            .await;
        while let Some(delta) = deltas_rx.recv().await {
            let _ = task_app.emit(
                "generation-event",
                GenerationEventDto::Delta {
                    task_id,
                    content: delta,
                },
            );
        }
        let desktop_state = task_app.state::<DesktopState>();
        let _ = desktop_state
            .generation_tokens
            .lock()
            .map(|mut tokens| tokens.remove(&task_id));
        match result {
            Ok(candidate) => {
                let _ = desktop_state.generation.complete_success(task_id);
                let _ = task_app.emit(
                    "generation-event",
                    GenerationEventDto::Candidate { task_id, candidate },
                );
            }
            Err(error) => {
                if task_cancel.is_cancelled() {
                    let _ = task_app.emit(
                        "generation-event",
                        GenerationEventDto::Cancelled { task_id },
                    );
                } else {
                    let _ = desktop_state
                        .generation
                        .complete_failure(task_id, AiError::Cancelled);
                    let _ = task_app.emit(
                        "generation-event",
                        GenerationEventDto::Error {
                            task_id,
                            error: error.into(),
                        },
                    );
                }
            }
        }
    });
    Ok(GenerationStartDto { task_id })
}

#[tauri::command]
async fn candidate_list(
    state: State<'_, DesktopState>,
    chapter_id: String,
) -> Result<Vec<Candidate>, CommandErrorDto> {
    let root = current_root(&state)?;
    let service = generation_service(&state, &root)?;
    let chapter_id = parse_uuid(&chapter_id)?;
    service.list(chapter_id).map_err(CommandErrorDto::from)
}

#[tauri::command]
async fn candidate_adopt(
    state: State<'_, DesktopState>,
    candidate_id: String,
    expected_revision: u64,
) -> Result<Document, CommandErrorDto> {
    let root = current_root(&state)?;
    let service = generation_service(&state, &root)?;
    let candidate_id = parse_uuid(&candidate_id)?;
    service
        .adopt(candidate_id, expected_revision)
        .await
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
async fn candidate_reject(
    state: State<'_, DesktopState>,
    candidate_id: String,
) -> Result<(), CommandErrorDto> {
    let root = current_root(&state)?;
    let service = generation_service(&state, &root)?;
    let candidate_id = parse_uuid(&candidate_id)?;
    service.reject(candidate_id).map_err(CommandErrorDto::from)
}

#[tauri::command]
async fn generation_cancel(
    state: State<'_, DesktopState>,
    task_id: String,
) -> Result<(), CommandErrorDto> {
    let task_id = parse_uuid(&task_id)?;
    if let Some(cancel) = state
        .generation_tokens
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "generation token lock", false))?
        .remove(&task_id)
    {
        cancel.cancel();
    }
    state
        .generation
        .cancel_generation(task_id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
async fn generation_status(
    state: State<'_, DesktopState>,
    task_id: String,
) -> Result<Option<GenerationState>, CommandErrorDto> {
    let task_id = parse_uuid(&task_id)?;
    Ok(state.generation.generation_status(task_id))
}

fn current_root(state: &State<'_, DesktopState>) -> Result<String, CommandErrorDto> {
    state
        .current
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "session lock", false))?
        .as_ref()
        .map(|session| session.root.clone())
        .ok_or_else(|| command_error("NO_PROJECT_SESSION", "no project session", false))
}

fn document_service(
    state: &State<'_, DesktopState>,
    root: &str,
) -> Result<Arc<DocumentApplicationService>, CommandErrorDto> {
    state
        .document_services
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "document service lock", false))?
        .get(root)
        .cloned()
        .ok_or_else(|| {
            command_error(
                "DOCUMENT_SERVICE_NOT_INITIALIZED",
                "document service not initialized",
                false,
            )
        })
}

fn generation_service(
    state: &State<'_, DesktopState>,
    root: &str,
) -> Result<Arc<GenerationService>, CommandErrorDto> {
    state
        .generation_services
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "generation service lock", false))?
        .get(root)
        .cloned()
        .ok_or_else(|| {
            command_error(
                "GENERATION_SERVICE_NOT_INITIALIZED",
                "generation service not initialized",
                false,
            )
        })
}

async fn configured_provider(
    state: &State<'_, DesktopState>,
) -> Result<Arc<dyn lingbi_ai::AiProvider>, CommandErrorDto> {
    let key = state
        .secrets
        .get("provider_key")
        .await
        .map_err(CommandErrorDto::from)?
        .ok_or_else(|| {
            command_error(
                "PROVIDER_NOT_CONFIGURED",
                "provider key is not configured",
                false,
            )
        })?;
    let base_url = state
        .secrets
        .get("provider_base_url")
        .await
        .map_err(CommandErrorDto::from)?
        .map(|value| value.expose().to_owned())
        .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_owned());
    let model = state
        .secrets
        .get("provider_model")
        .await
        .map_err(CommandErrorDto::from)?
        .map(|value| value.expose().to_owned())
        .unwrap_or_else(|| "gpt-4o-mini".to_owned());
    Ok(Arc::new(OpenAiCompatibleProvider::new(
        key.expose(),
        base_url,
        model,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingbi_contracts::ErrorCode;

    #[test]
    fn app_error_maps_to_structured_command_error() {
        let error = AppError::new(ErrorCode::DocumentConflict, "conflict".to_owned(), false);
        let dto = CommandErrorDto::from(error);
        assert_eq!(dto.code, "DocumentConflict");
        assert_eq!(dto.message, "conflict");
        assert!(!dto.retryable);
    }

    #[test]
    fn command_error_serializes_structured_fields() {
        let dto = command_error("INVALID_UUID", "bad id", false);
        let json = serde_json::to_value(dto).expect("json");
        assert_eq!(json["code"], "INVALID_UUID");
        assert_eq!(json["message"], "bad id");
        assert_eq!(json["retryable"], false);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(DesktopState::default())
        .invoke_handler(tauri::generate_handler![
            project_create,
            project_open,
            project_get_session,
            document_list,
            document_create,
            document_open,
            document_save,
            provider_configure,
            provider_test,
            generation_start,
            generation_cancel,
            generation_status,
            candidate_list,
            candidate_adopt,
            candidate_reject,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LingBi Next");
}
