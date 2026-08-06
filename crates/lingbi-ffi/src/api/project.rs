use lingbi_application::ProjectApplicationService;
use lingbi_contracts::AppError;
use lingbi_domain::{Document, Project};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RustProject {
    pub id: String,
    pub name: String,
    pub schema_version: u32,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Project> for RustProject {
    fn from(project: Project) -> Self {
        Self {
            id: project.id.to_string(),
            name: project.name,
            schema_version: project.schema_version,
            created_at: project.created_at.to_rfc3339(),
            updated_at: project.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RustDocument {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub order: i64,
    pub revision: u64,
    pub content_hash: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Document> for RustDocument {
    fn from(document: Document) -> Self {
        Self {
            id: document.id.to_string(),
            project_id: document.project_id.to_string(),
            title: document.title,
            order: document.order,
            revision: document.revision,
            content_hash: document.content_hash,
            created_at: document.created_at.to_rfc3339(),
            updated_at: document.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RustProjectSession {
    pub project: RustProject,
    pub current_document: RustDocument,
    pub dirty: bool,
}

#[derive(Debug, Clone)]
pub struct RustAppError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl From<AppError> for RustAppError {
    fn from(error: AppError) -> Self {
        Self {
            code: format!("{:?}", error.code),
            message: error.message,
            retryable: error.retryable,
        }
    }
}

pub async fn open_project(root: String) -> Result<RustProjectSession, RustAppError> {
    let service = ProjectApplicationService::new();
    let snapshot = service
        .open_project(PathBuf::from(root))
        .await
        .map_err(RustAppError::from)?;
    Ok(RustProjectSession {
        project: snapshot.project.into(),
        current_document: snapshot.current_document.into(),
        dirty: snapshot.dirty,
    })
}

pub fn project_v2_schema_version() -> u32 {
    2
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingbi_application::{CreateProjectRequest, ProjectApplicationService};
    use tempfile::TempDir;

    #[test]
    fn open_project_returns_typed_rust_session() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("novel");
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime
            .block_on(
                ProjectApplicationService::new().create_project(CreateProjectRequest {
                    name: "FFI测试".to_owned(),
                    root: root.clone(),
                }),
            )
            .expect("create project");

        let session = runtime
            .block_on(open_project(root.to_string_lossy().into_owned()))
            .expect("open project");

        assert_eq!(session.project.name, "FFI测试");
        assert_eq!(session.project.schema_version, 2);
        assert_eq!(session.current_document.title, "第一章");
        assert_eq!(session.current_document.revision, 0);
    }
}
