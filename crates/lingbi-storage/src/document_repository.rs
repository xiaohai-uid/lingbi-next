use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::Document;
use std::path::PathBuf;
use uuid::Uuid;

use crate::atomic_file::{AtomicFileStore, DiskAtomicFileStore};

pub struct DocumentRepository {
    root: PathBuf,
    store: DiskAtomicFileStore,
}

impl DocumentRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            store: DiskAtomicFileStore,
        }
    }

    pub fn list(&self) -> Result<Vec<Document>, AppError> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = self.store.read(&path)?;
        serde_json::from_slice(&bytes).map_err(parse_error)
    }

    pub fn find(&self, id: Uuid) -> Result<Option<Document>, AppError> {
        Ok(self.list()?.into_iter().find(|document| document.id == id))
    }

    pub fn update(&self, document: &Document) -> Result<(), AppError> {
        let mut documents = self.list()?;
        let Some(index) = documents.iter().position(|item| item.id == document.id) else {
            return Err(AppError::new(
                ErrorCode::DocumentNotFound,
                format!("document not found: {}", document.id),
                false,
            ));
        };
        documents[index] = document.clone();
        self.write(&documents)
    }

    pub fn write(&self, documents: &[Document]) -> Result<(), AppError> {
        let path = self.index_path();
        let bytes = serde_json::to_vec(documents).map_err(parse_error)?;
        self.store.write_atomic(&path, &bytes, None)?;
        Ok(())
    }

    fn index_path(&self) -> PathBuf {
        self.root.join(".lingbi/documents.json")
    }
}

fn parse_error(error: serde_json::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("document repository parse failed: {error}"),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn document_repository_reads_index() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("project");
        let now = Utc::now();
        let document = Document {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: "第一章".to_owned(),
            order: 0,
            revision: 3,
            content_hash: "hash".to_owned(),
            created_at: now,
            updated_at: now,
        };
        fs::create_dir_all(root.join(".lingbi")).expect("lingbi");
        fs::write(
            root.join(".lingbi/documents.json"),
            serde_json::to_vec(&vec![document.clone()]).expect("json"),
        )
        .expect("write");

        let repository = DocumentRepository::new(root);
        let loaded = repository.find(document.id).expect("find").expect("doc");
        assert_eq!(loaded.revision, 3);
    }
}
