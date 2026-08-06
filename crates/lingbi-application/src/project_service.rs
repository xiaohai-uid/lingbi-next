use chrono::Utc;
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::{Document, Project};
use lingbi_storage::{AtomicFileStore, DiskAtomicFileStore};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CreateProjectRequest {
    pub name: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ProjectSessionSnapshot {
    pub project: Project,
    pub current_document: Document,
    pub dirty: bool,
}

pub struct ProjectApplicationService {
    store: DiskAtomicFileStore,
}

impl ProjectApplicationService {
    pub fn new() -> Self {
        Self {
            store: DiskAtomicFileStore,
        }
    }

    pub async fn create_project(
        &self,
        request: CreateProjectRequest,
    ) -> Result<ProjectSessionSnapshot, AppError> {
        let root = request.root;
        if root.exists() {
            return Err(AppError::new(
                ErrorCode::ProjectPathExists,
                format!("project path already exists: {}", root.display()),
                false,
            ));
        }

        fs::create_dir_all(root.join(".lingbi")).map_err(project_io_error)?;
        fs::create_dir_all(root.join("chapters")).map_err(project_io_error)?;

        let now = Utc::now();
        let project = Project {
            id: Uuid::new_v4(),
            name: request.name,
            schema_version: 2,
            created_at: now,
            updated_at: now,
        };
        write_json(
            &self.store,
            &root.join(".lingbi").join("project.json"),
            &project,
        )?;

        let document = create_first_document(&self.store, &root, &project)?;
        write_json(
            &self.store,
            &root.join(".lingbi").join("documents.json"),
            std::slice::from_ref(&document),
        )?;

        Ok(ProjectSessionSnapshot {
            project,
            current_document: document,
            dirty: false,
        })
    }

    pub async fn open_project(&self, root: PathBuf) -> Result<ProjectSessionSnapshot, AppError> {
        if !root.exists() {
            return Err(AppError::new(
                ErrorCode::ProjectNotFound,
                format!("project path not found: {}", root.display()),
                false,
            ));
        }

        let project_bytes = self
            .store
            .read(&root.join(".lingbi").join("project.json"))?;
        let project: Project = serde_json::from_slice(&project_bytes).map_err(|error| {
            AppError::new(
                ErrorCode::ProjectCorrupted,
                format!("project metadata is invalid: {error}"),
                false,
            )
        })?;

        let documents = read_documents(&self.store, &root, project.id)?;
        let mut documents = documents;
        if documents.is_empty() {
            return Err(AppError::new(
                ErrorCode::ProjectCorrupted,
                "project has no documents".to_owned(),
                false,
            ));
        }
        documents.sort_by_key(|document| document.order);

        Ok(ProjectSessionSnapshot {
            project,
            current_document: documents.remove(0),
            dirty: false,
        })
    }
}

impl Default for ProjectApplicationService {
    fn default() -> Self {
        Self::new()
    }
}

fn create_first_document(
    store: &impl AtomicFileStore,
    root: &Path,
    project: &Project,
) -> Result<Document, AppError> {
    let now = Utc::now();
    let id = Uuid::new_v4();
    let content = "# 第一章\n\n";
    let path = root.join("chapters").join(format!("{id}.md"));
    let content_hash = store.write_atomic(&path, content.as_bytes(), None)?;

    Ok(Document {
        id,
        project_id: project.id,
        title: "第一章".to_owned(),
        order: 0,
        revision: 0,
        content_hash,
        created_at: now,
        updated_at: now,
    })
}

fn read_documents(
    store: &impl AtomicFileStore,
    root: &Path,
    project_id: Uuid,
) -> Result<Vec<Document>, AppError> {
    let index_path = root.join(".lingbi").join("documents.json");
    if index_path.exists() {
        let bytes = store.read(&index_path)?;
        return serde_json::from_slice(&bytes).map_err(|error| {
            AppError::new(
                ErrorCode::ProjectCorrupted,
                format!("document index is invalid: {error}"),
                false,
            )
        });
    }

    let chapters = root.join("chapters");
    if !chapters.exists() {
        return Ok(Vec::new());
    }

    let mut documents = Vec::new();
    let mut order = 0i64;
    for entry in fs::read_dir(&chapters).map_err(project_io_error)? {
        let entry = entry.map_err(project_io_error)?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Ok(id) = Uuid::parse_str(stem) else {
            continue;
        };
        let bytes = store.read(&path)?;
        let content_hash = hex_sha256(&bytes);
        let title = extract_title(&bytes);
        documents.push(Document {
            id,
            project_id,
            title,
            order,
            revision: 0,
            content_hash,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        });
        order += 1;
    }

    Ok(documents)
}

fn extract_title(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .find(|line| line.trim_start().starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim().to_owned())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "未命名".to_owned())
}

fn write_json<T: serde::Serialize + ?Sized>(
    store: &impl AtomicFileStore,
    path: &Path,
    value: &T,
) -> Result<(), AppError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        AppError::new(
            ErrorCode::ProjectCorrupted,
            format!("serialization failed: {error}"),
            false,
        )
    })?;
    store.write_atomic(path, &bytes, None)?;
    Ok(())
}

fn project_io_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("project I/O failed: {error}"),
        false,
    )
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn create_project_returns_project_and_first_document() {
        let temp = TempDir::new().expect("temp dir");
        let service = ProjectApplicationService::new();

        let snapshot = service
            .create_project(CreateProjectRequest {
                name: "测试小说".to_owned(),
                root: temp.path().join("novel"),
            })
            .await
            .expect("create project");

        assert_eq!(snapshot.project.schema_version, 2);
        assert_eq!(snapshot.current_document.title, "第一章");
        assert_eq!(snapshot.current_document.order, 0);
        assert!(temp.path().join("novel/.lingbi/project.json").exists());
        assert!(
            snapshot
                .current_document
                .physical_path()
                .starts_with("chapters")
        );
    }

    #[tokio::test]
    async fn open_project_restores_current_document() {
        let temp = TempDir::new().expect("temp dir");
        let service = ProjectApplicationService::new();
        let created = service
            .create_project(CreateProjectRequest {
                name: "测试小说".to_owned(),
                root: temp.path().join("novel"),
            })
            .await
            .expect("create project");

        let opened = service
            .open_project(temp.path().join("novel"))
            .await
            .expect("open project");

        assert_eq!(opened.project.id, created.project.id);
        assert_eq!(opened.current_document.id, created.current_document.id);
        assert_eq!(opened.current_document.title, "第一章");
        assert!(!opened.dirty);
    }
}
