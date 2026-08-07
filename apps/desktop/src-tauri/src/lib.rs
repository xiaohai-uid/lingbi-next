mod recent;

use lingbi_ai::{AiError, CancellationToken};
use lingbi_application::{
    default_root_for, Candidate, CreateProjectRequest, DocumentApplicationService,
    GenerationService, ProjectApplicationService, ProjectSessionSnapshot,
};
use lingbi_contracts::AppError;
use lingbi_domain::{Document, Project};
use lingbi_import_export::ImportExportService;
use lingbi_security::{KeyringSecretStore, SecretStore, SecretString};
use lingbi_writing::{GenerationManager, GenerationRequest, GenerationState};
use recent::{RecentProject, RecentProjects};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
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
    recent: Mutex<Option<RecentProjects>>,
}

#[derive(Clone)]
struct CurrentSession {
    root: String,
    snapshot: ProjectSessionSnapshot,
    recovered: bool,
    protected: bool,
}

#[derive(Serialize)]
struct SessionDto {
    project: Project,
    current_document: Document,
    dirty: bool,
    root: String,
    /// True when unfinished saves were safely recovered on open.
    recovered: bool,
    /// True when recovery detected external changes and protected the
    /// user's bytes (never auto-overwritten).
    protected: bool,
}

impl From<CurrentSession> for SessionDto {
    fn from(value: CurrentSession) -> Self {
        Self {
            project: value.snapshot.project,
            current_document: value.snapshot.current_document,
            dirty: value.snapshot.dirty,
            root: value.root,
            recovered: value.recovered,
            protected: value.protected,
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
    root: Option<String>,
) -> Result<SessionDto, CommandErrorDto> {
    // Novice flow: when no explicit root is provided, the app computes
    // {Documents}/LingBi/<name> so the user never needs to understand paths.
    let root = match root.as_deref().map(str::trim).filter(|root| !root.is_empty()) {
        Some(explicit) => explicit.to_owned(),
        None => default_root_for(&name)
            .map_err(CommandErrorDto::from)?
            .to_string_lossy()
            .into_owned(),
    };
    let request = CreateProjectRequest {
        name: name.clone(),
        root: root.clone().into(),
    };
    let snapshot = state
        .project_service
        .create_project(request)
        .await
        .map_err(CommandErrorDto::from)?;
    let documents = Arc::new(DocumentApplicationService::new(root.clone()));
    state
        .document_services
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "document service lock", false))?
        .insert(root.clone(), documents);
    record_recent(&state, &name, &root);
    let current = CurrentSession {
        root,
        snapshot,
        recovered: false,
        protected: false,
    };
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
    // Startup recovery: finish any interrupted save transactions BEFORE
    // the user sees the project. Recovery never auto-overwrites content;
    // when the user's bytes differ from the recorded state they are
    // preserved and surfaced to the user in plain language.
    let recovery = lingbi_recovery::RecoveryService::new(root.clone())
        .recover_all()
        .map_err(CommandErrorDto::from)?;
    let recovered = recovery
        .iter()
        .any(|outcome| outcome.action == lingbi_recovery::RecoveryAction::Recovered);
    let protected = recovery
        .iter()
        .any(|outcome| outcome.action == lingbi_recovery::RecoveryAction::PreserveUserBytes);

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
    record_recent(&state, &snapshot.project.name, &root);
    let current = CurrentSession {
        root,
        snapshot,
        recovered,
        protected,
    };
    let dto = SessionDto::from(current.clone());
    *state
        .current
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "session lock", false))? = Some(current);
    Ok(dto)
}

/// The default save location for a name-only project, so the UI can show
/// "将保存到 文档/LingBi/我的小说" without requiring the user to type a path.
#[tauri::command]
async fn project_default_root(name: String) -> Result<String, CommandErrorDto> {
    let root = default_root_for(&name).map_err(CommandErrorDto::from)?;
    Ok(root.to_string_lossy().into_owned())
}

#[tauri::command]
async fn recent_projects(
    state: State<'_, DesktopState>,
) -> Result<Vec<RecentProject>, CommandErrorDto> {
    let recent = state
        .recent
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "recent lock", false))?;
    match recent.as_ref() {
        Some(recent) => recent.load().map_err(CommandErrorDto::from),
        None => Ok(Vec::new()),
    }
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
    let document = documents
        .create_document(project_id, title, content)
        .await
        .map_err(CommandErrorDto::from)?;
    set_current_document(&state, document.clone())?;
    Ok(document)
}

#[tauri::command]
async fn document_open(
    state: State<'_, DesktopState>,
    document_id: String,
) -> Result<String, CommandErrorDto> {
    let root = current_root(&state)?;
    let documents = document_service(&state, &root)?;
    let document_id = parse_uuid(&document_id)?;
    let content = documents
        .read_document(document_id)
        .await
        .map_err(CommandErrorDto::from)?;
    let document = documents
        .get_document(document_id)
        .map_err(CommandErrorDto::from)?;
    set_current_document(&state, document)?;
    Ok(content)
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
    let document = documents
        .save_document(document_id, expected_revision, content)
        .await
        .map_err(CommandErrorDto::from)?;
    set_current_document(&state, document.clone())?;
    Ok(document)
}

#[derive(Serialize)]
struct ExportResultDto {
    format: String,
    path: String,
}

/// Export the current chapter to MD / TXT / DOCX inside the project's
/// `export/` folder. Novice users never pick a destination.
#[tauri::command]
async fn document_export(
    state: State<'_, DesktopState>,
    format: String,
) -> Result<ExportResultDto, CommandErrorDto> {
    let root = current_root(&state)?;
    let documents = document_service(&state, &root)?;
    let session = state
        .current
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "session lock", false))?
        .clone()
        .ok_or_else(|| command_error("NO_PROJECT_SESSION", "no project session", false))?;
    let document = session.snapshot.current_document;
    let content = documents
        .read_document(document.id)
        .await
        .map_err(CommandErrorDto::from)?;
    let export = ImportExportService::new(documents);
    let export_dir = PathBuf::from(&root).join("export");
    let base = export_dir.join(document.title.clone());
    let format = format.to_ascii_lowercase();
    let path = match format.as_str() {
        "md" => export
            .export_markdown(&content, &base.with_extension("md"))
            .map_err(CommandErrorDto::from)?,
        "txt" => export
            .export_txt(&content, &base.with_extension("txt"))
            .map_err(CommandErrorDto::from)?,
        "docx" => export
            .export_docx(&document.title, &content, &base.with_extension("docx"))
            .map_err(CommandErrorDto::from)?,
        other => {
            return Err(command_error(
                "UNSUPPORTED_EXPORT_FORMAT",
                format!("unsupported export format: {other}"),
                false,
            ))
        }
    };
    Ok(ExportResultDto {
        format,
        path: path.to_string_lossy().into_owned(),
    })
}

#[derive(Serialize)]
struct ProviderTestDto {
    provider_id: String,
    model_id: String,
    ok: bool,
    latency_ms: u64,
    error: Option<String>,
}

#[derive(Serialize)]
struct ProviderDefinitionDto {
    id: String,
    display_name: String,
    protocol: String,
    default_endpoint: String,
    recommended_model: String,
    models: Vec<String>,
}

/// The small set of AI services a novice user can pick from. The UI never
/// shows raw endpoints/models; definitions carry the defaults.
#[tauri::command]
async fn provider_list() -> Result<Vec<ProviderDefinitionDto>, CommandErrorDto> {
    Ok(lingbi_ai::provider_definitions()
        .iter()
        .map(|definition| ProviderDefinitionDto {
            id: definition.id.to_owned(),
            display_name: definition.display_name.to_owned(),
            protocol: definition.protocol.as_str().to_owned(),
            default_endpoint: definition.default_endpoint.to_owned(),
            recommended_model: definition.recommended_model.to_owned(),
            models: definition.models.iter().map(|model| (*model).to_owned()).collect(),
        })
        .collect())
}

#[tauri::command]
async fn provider_configure(
    state: State<'_, DesktopState>,
    provider_id: String,
    key: String,
    base_url: Option<String>,
    model: Option<String>,
) -> Result<(), CommandErrorDto> {
    let definition = lingbi_ai::find_provider(&provider_id).ok_or_else(|| {
        command_error(
            "UNKNOWN_PROVIDER",
            format!("unknown provider: {provider_id}"),
            false,
        )
    })?;
    state
        .secrets
        .put("provider_id", SecretString::new(provider_id))
        .await
        .map_err(CommandErrorDto::from)?;
    state
        .secrets
        .put("provider_key", SecretString::new(key))
        .await
        .map_err(CommandErrorDto::from)?;
    state
        .secrets
        .put(
            "provider_base_url",
            SecretString::new(base_url.unwrap_or_else(|| definition.default_endpoint.to_owned())),
        )
        .await
        .map_err(CommandErrorDto::from)?;
    state
        .secrets
        .put(
            "provider_model",
            SecretString::new(model.unwrap_or_else(|| definition.recommended_model.to_owned())),
        )
        .await
        .map_err(CommandErrorDto::from)
}

#[derive(Serialize)]
struct ProviderStatusDto {
    configured: bool,
    provider_id: String,
    model_id: String,
}

/// Whether an AI service is configured, and with which provider/model.
/// Never returns the API key.
#[tauri::command]
async fn provider_status(
    state: State<'_, DesktopState>,
) -> Result<ProviderStatusDto, CommandErrorDto> {
    let key = state
        .secrets
        .get("provider_key")
        .await
        .map_err(CommandErrorDto::from)?;
    let provider_id = state
        .secrets
        .get("provider_id")
        .await
        .map_err(CommandErrorDto::from)?
        .map(|value| value.expose().to_owned())
        .unwrap_or_else(|| "openai".to_owned());
    let model_id = state
        .secrets
        .get("provider_model")
        .await
        .map_err(CommandErrorDto::from)?
        .map(|value| value.expose().to_owned())
        .unwrap_or_default();
    Ok(ProviderStatusDto {
        configured: key.is_some(),
        provider_id,
        model_id,
    })
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
        // Producer: generation writes deltas into the channel as they
        // arrive. Consumer: this task forwards them to the UI as they
        // arrive (true streaming), while the generation is still running.
        let (result_tx, result_rx) = tokio::sync::oneshot::channel::<Result<Candidate, AppError>>();
        let generation_cancel = task_cancel.clone();
        let generation_task = tokio::spawn(async move {
            let result = task_service
                .generate_with_cancel_stream(chapter_id, instruction, generation_cancel, deltas)
                .await;
            let _ = result_tx.send(result);
        });
        while let Some(delta) = deltas_rx.recv().await {
            let _ = task_app.emit(
                "generation-event",
                GenerationEventDto::Delta {
                    task_id,
                    content: delta,
                },
            );
        }
        let result = result_rx
            .await
            .unwrap_or_else(|_| Err(AppError::new(
                lingbi_contracts::ErrorCode::AiInvalidResponse,
                "generation task ended without a result".to_owned(),
                false,
            )));
        let _ = generation_task.await;
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
    let document = service
        .adopt(candidate_id, expected_revision)
        .await
        .map_err(CommandErrorDto::from)?;
    set_current_document(&state, document.clone())?;
    Ok(document)
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

fn record_recent(state: &State<'_, DesktopState>, name: &str, root: &str) {
    let recent_guard = match state.recent.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if let Some(recent) = recent_guard.as_ref() {
        let _ = recent.record(name, root);
    }
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

fn set_current_document(
    state: &State<'_, DesktopState>,
    document: Document,
) -> Result<(), CommandErrorDto> {
    let mut current = state
        .current
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "session lock", false))?;
    if let Some(session) = current.as_mut() {
        session.snapshot.current_document = document;
    }
    Ok(())
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
    let provider_id = state
        .secrets
        .get("provider_id")
        .await
        .map_err(CommandErrorDto::from)?
        .map(|value| value.expose().to_owned())
        .unwrap_or_else(|| "openai".to_owned());
    let definition = lingbi_ai::find_provider(&provider_id).ok_or_else(|| {
        command_error("UNKNOWN_PROVIDER", format!("unknown provider: {provider_id}"), false)
    })?;
    let base_url = state
        .secrets
        .get("provider_base_url")
        .await
        .map_err(CommandErrorDto::from)?
        .map(|value| value.expose().to_owned());
    let model = state
        .secrets
        .get("provider_model")
        .await
        .map_err(CommandErrorDto::from)?
        .map(|value| value.expose().to_owned());
    Ok(lingbi_ai::build_provider(
        definition,
        key.expose(),
        base_url.as_deref(),
        model.as_deref(),
    ))
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
        .setup(|app| {
            let state = app.state::<DesktopState>();
            if let Ok(app_data) = app.path().app_data_dir() {
                let recent = RecentProjects::new(app_data.join("recent_projects.json"));
                if let Ok(mut guard) = state.recent.lock() {
                    *guard = Some(recent);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            project_create,
            project_open,
            project_get_session,
            project_default_root,
            recent_projects,
            document_list,
            document_create,
            document_open,
            document_save,
            document_export,
            provider_list,
            provider_configure,
            provider_status,
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
