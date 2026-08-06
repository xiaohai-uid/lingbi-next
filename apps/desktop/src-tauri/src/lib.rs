use lingbi_application::{
    CreateProjectRequest, DocumentApplicationService, ProjectApplicationService,
    ProjectSessionSnapshot,
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
async fn provider_configure(state: State<'_, DesktopState>, key: String) -> Result<(), String> {
    state
        .secrets
        .put("provider_key", SecretString::new(key))
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
) -> Result<String, String> {
    let chapter_id = Uuid::parse_str(&chapter_id).map_err(|error| error.to_string())?;
    Ok(state
        .generation
        .start_generation(GenerationRequest {
            chapter_id,
            instruction,
        })
        .to_string())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running LingBi Next");
}
