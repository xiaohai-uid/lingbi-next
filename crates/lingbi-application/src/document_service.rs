use chrono::Utc;
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::Document;
use lingbi_security::ProjectPathGuard;
use lingbi_storage::{AtomicFileStore, DiskAtomicFileStore};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct DocumentApplicationService {
    root: PathBuf,
    guard: ProjectPathGuard,
    store: DiskAtomicFileStore,
}

impl DocumentApplicationService {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let _ = std::fs::create_dir_all(&root);
        let guard = ProjectPathGuard::new(root.clone());
        Self {
            root,
            guard,
            store: DiskAtomicFileStore,
        }
    }

    pub async fn create_document(
        &self,
        project_id: Uuid,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<Document, AppError> {
        let content = content.into();
        let id = Uuid::new_v4();
        let relative = Path::new("chapters").join(format!("{id}.md"));
        let path = self.guard.resolve(&relative)?;
        let content_hash = self.store.write_atomic(&path, content.as_bytes(), None)?;
        let now = Utc::now();
        let mut documents = self.read_index()?;
        let order = documents
            .iter()
            .map(|document| document.order)
            .max()
            .unwrap_or(-1)
            + 1;
        let document = Document {
            id,
            project_id,
            title: title.into(),
            order,
            revision: 0,
            content_hash,
            created_at: now,
            updated_at: now,
        };
        documents.push(document.clone());
        self.write_index(&documents)?;
        Ok(document)
    }

    pub async fn read_document(&self, document_id: Uuid) -> Result<String, AppError> {
        let document = self.find_document(document_id)?;
        let relative = Path::new("chapters").join(format!("{}.md", document.id));
        let path = self.guard.resolve(&relative)?;
        let bytes = self.store.read(&path)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    pub fn list_documents(&self) -> Result<Vec<Document>, AppError> {
        let mut documents = self.read_index()?;
        documents.sort_by_key(|document| document.order);
        Ok(documents)
    }

    pub async fn save_document(
        &self,
        document_id: Uuid,
        expected_revision: u64,
        content: impl Into<String>,
    ) -> Result<Document, AppError> {
        let content = content.into();
        let mut documents = self.read_index()?;
        let index = documents
            .iter()
            .position(|document| document.id == document_id)
            .ok_or_else(|| document_not_found(document_id))?;
        let document = &documents[index];
        if document.revision != expected_revision {
            return Err(AppError::new(
                ErrorCode::DocumentConflict,
                "document revision conflict".to_owned(),
                false,
            ));
        }
        let relative = Path::new("chapters").join(format!("{}.md", document.id));
        let path = self.guard.resolve(&relative)?;
        let content_hash =
            self.store
                .write_atomic(&path, content.as_bytes(), Some(&document.content_hash))?;
        let mut updated = document.clone();
        updated.revision += 1;
        updated.content_hash = content_hash;
        updated.updated_at = Utc::now();
        documents[index] = updated.clone();
        self.write_index(&documents)?;
        Ok(updated)
    }

    fn find_document(&self, document_id: Uuid) -> Result<Document, AppError> {
        self.read_index()?
            .into_iter()
            .find(|document| document.id == document_id)
            .ok_or_else(|| document_not_found(document_id))
    }

    fn read_index(&self) -> Result<Vec<Document>, AppError> {
        let path = self.root.join(".lingbi/documents.json");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = self.store.read(&path)?;
        serde_json::from_slice(&bytes).map_err(|error| {
            AppError::new(
                ErrorCode::ProjectCorrupted,
                format!("document index is invalid: {error}"),
                false,
            )
        })
    }

    fn write_index(&self, documents: &[Document]) -> Result<(), AppError> {
        let path = self.root.join(".lingbi/documents.json");
        let bytes = serde_json::to_vec(documents).map_err(|error| {
            AppError::new(
                ErrorCode::ProjectCorrupted,
                format!("document index serialization failed: {error}"),
                false,
            )
        })?;
        self.store.write_atomic(&path, &bytes, None)?;
        Ok(())
    }
}

fn document_not_found(document_id: Uuid) -> AppError {
    AppError::new(
        ErrorCode::DocumentNotFound,
        format!("document not found: {document_id}"),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn create_read_save_round_trip() {
        let temp = TempDir::new().expect("temp dir");
        let service = DocumentApplicationService::new(temp.path().join("project"));
        let project_id = Uuid::new_v4();

        let created = service
            .create_document(project_id, "第一章", "# 第一章\n\n原始")
            .await
            .expect("create");
        let content = service.read_document(created.id).await.expect("read");
        assert_eq!(content, "# 第一章\n\n原始");

        let saved = service
            .save_document(created.id, 0, "# 第一章\n\n修订")
            .await
            .expect("save");
        assert_eq!(saved.revision, 1);
        assert_eq!(
            service.read_document(created.id).await.expect("read"),
            "# 第一章\n\n修订"
        );
    }

    #[tokio::test]
    async fn stale_revision_is_rejected() {
        let temp = TempDir::new().expect("temp dir");
        let service = DocumentApplicationService::new(temp.path().join("project"));
        let created = service
            .create_document(Uuid::new_v4(), "第一章", "original")
            .await
            .expect("create");

        let result = service.save_document(created.id, 1, "stale").await;

        assert!(matches!(
            result,
            Err(AppError {
                code: ErrorCode::DocumentConflict,
                ..
            })
        ));
        assert_eq!(
            service.read_document(created.id).await.expect("read"),
            "original"
        );
    }

    #[tokio::test]
    async fn external_content_change_is_conflict_even_when_revision_matches() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("project");
        let service = DocumentApplicationService::new(&root);
        let created = service
            .create_document(Uuid::new_v4(), "第一章", "original")
            .await
            .expect("create");
        let path = root.join(created.physical_path());
        std::fs::write(&path, "external").expect("external edit");

        let result = service
            .save_document(created.id, created.revision, "replacement")
            .await;

        assert!(matches!(
            result,
            Err(AppError {
                code: ErrorCode::DocumentConflict,
                ..
            })
        ));
        assert_eq!(
            std::fs::read_to_string(&path).expect("read external"),
            "external"
        );
    }

    #[tokio::test]
    async fn list_documents_returns_documents_in_order() {
        let temp = TempDir::new().expect("temp dir");
        let service = DocumentApplicationService::new(temp.path().join("project"));
        let project_id = Uuid::new_v4();

        service
            .create_document(project_id, "第一章", "first")
            .await
            .expect("create first");
        service
            .create_document(project_id, "第二章", "second")
            .await
            .expect("create second");

        let documents = service.list_documents().expect("list");

        assert_eq!(documents.len(), 2);
        assert_eq!(documents[0].order, 0);
        assert_eq!(documents[1].order, 1);
    }
}
