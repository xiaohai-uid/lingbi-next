use lingbi_ai::{AiError, OpenAiCompatibleProvider};
use lingbi_application::{
    CreateProjectRequest, DocumentApplicationService, GeneratedCandidate, GenerationService,
    ProjectApplicationService, ProjectSessionSnapshot,
};
use lingbi_domain::{Document, Project};
use lingbi_security::{MemorySecretStore, SecretStore, SecretString};
use lingbi_writing::{GenerationManager, GenerationRequest, GenerationState};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::State;
use uuid::Uuid;

#[derive(Default)]
struct DesktopState {
    project_service: Arc<ProjectApplicationService>,
    document_services: Mutex<HashMap<String, Arc<DocumentApplicationService>>>,
    current: Mutex<Option<CurrentSession>>,
    secrets: MemorySecretStore,
    generation: GenerationManager,
    generation_services: Mutex<HashMap<String, Arc<GenerationService>>>,
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

#[tauri::command]
async fn project_create(
    state: State<'_, DesktopState>,
    name: String,
    root: String,
) -> Result<SessionDto, String> {
    let snapshot = state
        .project_service
        .create_project(CreateProjectRequest {
            name,
            root: root.clone().into(),
        })
        .await
        .map_err(|error| error.message)?;
    let documents = Arc::new(DocumentApplicationService::new(root.clone()));
    state
        .document_services
        .lock()
        .map_err(|_| "document service lock".to_owned())?
        .insert(root.clone(), documents);
    let current = CurrentSession { root, snapshot };
    let dto = SessionDto::from(current.clone());
    *state
        .current
        .lock()
        .map_err(|_| "session lock".to_owned())? = Some(current);
    Ok(dto)
}

#[tauri::command]
async fn project_open(state: State<'_, DesktopState>, root: String) -> Result<SessionDto, String> {
    let snapshot = state
        .project_service
        .open_project(root.clone().into())
        .await
        .map_err(|error| error.message)?;
    let documents = Arc::new(DocumentApplicationService::new(root.clone()));
    state
        .document_services
        .lock()
        .map_err(|_| "document service lock".to_owned())?
        .insert(root.clone(), documents);
    let current = CurrentSession { root, snapshot };
    let dto = SessionDto::from(current.clone());
    *state
        .current
        .lock()
        .map_err(|_| "session lock".to_owned())? = Some(current);
    Ok(dto)
}

#[tauri::command]
async fn project_get_session(state: State<'_, DesktopState>) -> Result<Option<SessionDto>, String> {
    Ok(state
        .current
        .lock()
        .map_err(|_| "session lock".to_owned())?
        .clone()
        .map(SessionDto::from))
}

#[tauri::command]
async fn document_create(
    state: State<'_, DesktopState>,
    project_id: String,
    title: String,
    content: String,
) -> Result<Document, String> {
    let root = current_root(&state)?;
    let documents = document_service(&state, &root)?;
    let project_id = Uuid::parse_str(&project_id).map_err(|error| error.to_string())?;
    documents
        .create_document(project_id, title, content)
        .await
        .map_err(|error| error.message)
}

#[tauri::command]
async fn document_open(
    state: State<'_, DesktopState>,
    document_id: String,
) -> Result<String, String> {
    let root = current_root(&state)?;
    let documents = document_service(&state, &root)?;
    let document_id = Uuid::parse_str(&document_id).map_err(|error| error.to_string())?;
    documents
        .read_document(document_id)
        .await
        .map_err(|error| error.message)
}

#[tauri::command]
async fn document_save(
    state: State<'_, DesktopState>,
    document_id: String,
    expected_revision: u64,
    content: String,
) -> Result<Document, String> {
    let root = current_root(&state)?;
    let documents = document_service(&state, &root)?;
    let document_id = Uuid::parse_str(&document_id).map_err(|error| error.to_string())?;
    documents
        .save_document(document_id, expected_revision, content)
        .await
        .map_err(|error| error.message)
}

#[derive(Serialize)]
struct ProviderTestDto {
    configured: bool,
}

#[tauri::command]
async fn provider_configure(
    state: State<'_, DesktopState>,
    key: String,
    base_url: Option<String>,
    model: Option<String>,
) -> Result<(), String> {
    state
        .secrets
        .put("provider_key", SecretString::new(key))
        .await
        .map_err(|error| error.message)?;
    state
        .secrets
        .put(
            "provider_base_url",
            SecretString::new(
                base_url.unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_owned()),
            ),
        )
        .await
        .map_err(|error| error.message)?;
    state
        .secrets
        .put(
            "provider_model",
            SecretString::new(model.unwrap_or_else(|| "gpt-4o-mini".to_owned())),
        )
        .await
        .map_err(|error| error.message)
}

#[tauri::command]
async fn provider_test(state: State<'_, DesktopState>) -> Result<ProviderTestDto, String> {
    let configured = state
        .secrets
        .get("provider_key")
        .await
        .map_err(|error| error.message)?
        .is_some();
    Ok(ProviderTestDto { configured })
}

#[tauri::command]
async fn generation_start(
    state: State<'_, DesktopState>,
    chapter_id: String,
    instruction: String,
) -> Result<GeneratedCandidate, String> {
    let chapter_id = Uuid::parse_str(&chapter_id).map_err(|error| error.to_string())?;
    let root = current_root(&state)?;
    let documents = document_service(&state, &root)?;
    let provider = configured_provider(&state).await?;
    let service = Arc::new(GenerationService::new(root.clone(), provider, documents));
    state
        .generation_services
        .lock()
        .map_err(|_| "generation service lock".to_owned())?
        .insert(root.clone(), service.clone());
    let task_id = state.generation.start_generation(GenerationRequest {
        chapter_id,
        instruction: instruction.clone(),
    });
    match service.generate(chapter_id, instruction).await {
        Ok(candidate) => {
            state
                .generation
                .complete_success(task_id)
                .map_err(|error| error.message)?;
            Ok(candidate)
        }
        Err(error) => {
            let _ = state
                .generation
                .complete_failure(task_id, AiError::InvalidResponse);
            Err(error.message)
        }
    }
}

#[tauri::command]
async fn candidate_list(
    state: State<'_, DesktopState>,
    chapter_id: String,
) -> Result<Vec<GeneratedCandidate>, String> {
    let root = current_root(&state)?;
    let service = generation_service(&state, &root)?;
    let chapter_id = Uuid::parse_str(&chapter_id).map_err(|error| error.to_string())?;
    service.list(chapter_id).map_err(|error| error.message)
}

#[tauri::command]
async fn candidate_adopt(
    state: State<'_, DesktopState>,
    candidate_id: String,
    expected_revision: u64,
) -> Result<Document, String> {
    let root = current_root(&state)?;
    let service = generation_service(&state, &root)?;
    let candidate_id = Uuid::parse_str(&candidate_id).map_err(|error| error.to_string())?;
    service
        .adopt(candidate_id, expected_revision)
        .await
        .map_err(|error| error.message)
}

#[tauri::command]
async fn candidate_reject(
    state: State<'_, DesktopState>,
    candidate_id: String,
) -> Result<(), String> {
    let root = current_root(&state)?;
    let service = generation_service(&state, &root)?;
    let candidate_id = Uuid::parse_str(&candidate_id).map_err(|error| error.to_string())?;
    service.reject(candidate_id).map_err(|error| error.message)
}

#[tauri::command]
async fn generation_cancel(state: State<'_, DesktopState>, task_id: String) -> Result<(), String> {
    let task_id = Uuid::parse_str(&task_id).map_err(|error| error.to_string())?;
    state
        .generation
        .cancel_generation(task_id)
        .map_err(|error| error.message)
}

#[tauri::command]
async fn generation_status(
    state: State<'_, DesktopState>,
    task_id: String,
) -> Result<Option<GenerationState>, String> {
    let task_id = Uuid::parse_str(&task_id).map_err(|error| error.to_string())?;
    Ok(state.generation.generation_status(task_id))
}

fn current_root(state: &State<'_, DesktopState>) -> Result<String, String> {
    state
        .current
        .lock()
        .map_err(|_| "session lock".to_owned())?
        .as_ref()
        .map(|session| session.root.clone())
        .ok_or_else(|| "no project session".to_owned())
}

fn document_service(
    state: &State<'_, DesktopState>,
    root: &str,
) -> Result<Arc<DocumentApplicationService>, String> {
    state
        .document_services
        .lock()
        .map_err(|_| "document service lock".to_owned())?
        .get(root)
        .cloned()
        .ok_or_else(|| "document service not initialized".to_owned())
}

fn generation_service(
    state: &State<'_, DesktopState>,
    root: &str,
) -> Result<Arc<GenerationService>, String> {
    state
        .generation_services
        .lock()
        .map_err(|_| "generation service lock".to_owned())?
        .get(root)
        .cloned()
        .ok_or_else(|| "generation service not initialized".to_owned())
}

async fn configured_provider(
    state: &State<'_, DesktopState>,
) -> Result<Arc<dyn lingbi_ai::AiProvider>, String> {
    let key = state
        .secrets
        .get("provider_key")
        .await
        .map_err(|error| error.message)?
        .ok_or_else(|| "provider key is not configured".to_owned())?;
    let base_url = state
        .secrets
        .get("provider_base_url")
        .await
        .map_err(|error| error.message)?
        .map(|value| value.expose().to_owned())
        .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_owned());
    let model = state
        .secrets
        .get("provider_model")
        .await
        .map_err(|error| error.message)?
        .map(|value| value.expose().to_owned())
        .unwrap_or_else(|| "gpt-4o-mini".to_owned());
    Ok(Arc::new(OpenAiCompatibleProvider::new(
        key.expose(),
        base_url,
        model,
    )))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(DesktopState::default())
        .invoke_handler(tauri::generate_handler![
            project_create,
            project_open,
            project_get_session,
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
