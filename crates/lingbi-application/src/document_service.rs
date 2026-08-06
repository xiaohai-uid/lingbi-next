use chrono::Utc;
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::Document;
use lingbi_security::ProjectPathGuard;
use lingbi_storage::{
    AtomicFileStore, DiskAtomicFileStore, DocumentTransaction, DocumentTransactionRepository,
    TransactionPhase,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct DocumentApplicationService {
    root: PathBuf,
    guard: ProjectPathGuard,
    store: DiskAtomicFileStore,
    transactions: DocumentTransactionRepository,
}

impl DocumentApplicationService {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let _ = std::fs::create_dir_all(&root);
        let guard = ProjectPathGuard::new(root.clone());
        Self {
            root: root.clone(),
            guard,
            store: DiskAtomicFileStore,
            transactions: DocumentTransactionRepository::new(root),
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
        let after_hash = hex_sha256(content.as_bytes());
        let transaction = DocumentTransaction {
            id: Uuid::new_v4(),
            document_id,
            before_revision: document.revision,
            before_hash: document.content_hash.clone(),
            after_revision: document.revision + 1,
            after_hash: after_hash.clone(),
            phase: TransactionPhase::Created,
            created_at: Utc::now(),
            body_relative_path: relative.to_string_lossy().into_owned(),
        };
        self.transactions.begin(&transaction)?;
        let content_hash =
            self.store
                .write_atomic(&path, content.as_bytes(), Some(&document.content_hash))?;
        self.transactions
            .set_phase(transaction.id, TransactionPhase::ContentWritten)?;
        let mut updated = document.clone();
        updated.revision += 1;
        updated.content_hash = content_hash.clone();
        updated.updated_at = Utc::now();
        documents[index] = updated.clone();
        let index_path = self.root.join(".lingbi/documents.json");
        let index_before_bytes = self.store.read(&index_path)?;
        let index_before_hash = hex_sha256(&index_before_bytes);
        let index_after_bytes = serde_json::to_vec(&documents).map_err(|error| {
            AppError::new(
                ErrorCode::ProjectCorrupted,
                format!("document index serialization failed: {error}"),
                false,
            )
        })?;
        self.store
            .write_atomic(&index_path, &index_after_bytes, Some(&index_before_hash))?;
        self.transactions
            .set_phase(transaction.id, TransactionPhase::IndexUpdated)?;
        let verified_index = self.store.read(&index_path)?;
        if hex_sha256(&verified_index) != hex_sha256(&index_after_bytes) {
            return Err(AppError::new(
                ErrorCode::ProjectCorrupted,
                "document index post-write verification failed".to_owned(),
                false,
            ));
        }
        self.transactions
            .set_phase(transaction.id, TransactionPhase::Completed)?;
        self.transactions.delete(transaction.id)?;
        Ok(updated)
    }

    fn find_document(&self, document_id: Uuid) -> Result<Document, AppError> {
        self.read_index()?
            .into_iter()
            .find(|document| document.id == document_id)
            .ok_or_else(|| document_not_found(document_id))
    }

    fn read_index(&self) -> Result<Vec<Document>, AppError> {
        self.recover_pending()?;
        self.read_index_raw()
    }

    fn read_index_raw(&self) -> Result<Vec<Document>, AppError> {
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

    pub fn recover_pending(&self) -> Result<(), AppError> {
        let transactions = self.transactions.list()?;
        for transaction in transactions {
            self.recover_transaction(&transaction)?;
        }
        Ok(())
    }

    fn recover_transaction(&self, transaction: &DocumentTransaction) -> Result<(), AppError> {
        if transaction.phase == TransactionPhase::Failed {
            return Ok(());
        }
        let body_path = self
            .guard
            .resolve(Path::new(&transaction.body_relative_path))?;
        let body_hash = if body_path.exists() {
            Some(hex_sha256(&self.store.read(&body_path)?))
        } else {
            None
        };
        let documents = self.read_index_raw()?;
        let Some(current) = documents
            .iter()
            .find(|document| document.id == transaction.document_id)
        else {
            return self.mark_failed(transaction);
        };

        let current_matches_before = current.revision == transaction.before_revision
            && current
                .content_hash
                .eq_ignore_ascii_case(&transaction.before_hash);
        let current_matches_after = current.revision == transaction.after_revision
            && current
                .content_hash
                .eq_ignore_ascii_case(&transaction.after_hash);
        let body_matches_before = body_hash
            .as_deref()
            .is_some_and(|hash| hash.eq_ignore_ascii_case(&transaction.before_hash));
        let body_matches_after = body_hash
            .as_deref()
            .is_some_and(|hash| hash.eq_ignore_ascii_case(&transaction.after_hash));

        if body_matches_before && current_matches_before {
            return self.transactions.delete(transaction.id);
        }
        if body_matches_after && current_matches_before {
            return self.complete_transaction(transaction);
        }
        if body_matches_after && current_matches_after {
            self.transactions
                .set_phase(transaction.id, TransactionPhase::Completed)?;
            return self.transactions.delete(transaction.id);
        }
        self.mark_failed(transaction)
    }

    fn complete_transaction(&self, transaction: &DocumentTransaction) -> Result<(), AppError> {
        let mut documents = self.read_index_raw()?;
        let Some(index) = documents
            .iter()
            .position(|document| document.id == transaction.document_id)
        else {
            return self.mark_failed(transaction);
        };
        if documents[index].revision == transaction.after_revision
            && documents[index]
                .content_hash
                .eq_ignore_ascii_case(&transaction.after_hash)
        {
            self.transactions
                .set_phase(transaction.id, TransactionPhase::Completed)?;
            return self.transactions.delete(transaction.id);
        }

        documents[index].revision = transaction.after_revision;
        documents[index].content_hash = transaction.after_hash.clone();
        documents[index].updated_at = Utc::now();
        let index_path = self.root.join(".lingbi/documents.json");
        let current_index = self.store.read(&index_path)?;
        let current_index_hash = hex_sha256(&current_index);
        let bytes = serde_json::to_vec(&documents).map_err(|error| {
            AppError::new(
                ErrorCode::ProjectCorrupted,
                format!("document index serialization failed: {error}"),
                false,
            )
        })?;
        self.store
            .write_atomic(&index_path, &bytes, Some(&current_index_hash))?;
        self.transactions
            .set_phase(transaction.id, TransactionPhase::Completed)?;
        self.transactions.delete(transaction.id)
    }

    fn mark_failed(&self, transaction: &DocumentTransaction) -> Result<(), AppError> {
        self.transactions
            .set_phase(transaction.id, TransactionPhase::Failed)
            .map(|_| ())
    }
}

fn document_not_found(document_id: Uuid) -> AppError {
    AppError::new(
        ErrorCode::DocumentNotFound,
        format!("document not found: {document_id}"),
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
    async fn recover_cleans_intent_only_transaction() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("project");
        let service = DocumentApplicationService::new(&root);
        let created = service
            .create_document(Uuid::new_v4(), "第一章", "original")
            .await
            .expect("create");
        let tx_id = Uuid::new_v4();
        write_transaction(
            &root,
            TxSpec {
                id: tx_id,
                document_id: created.id,
                phase: "created",
                before_revision: created.revision,
                before_hash: created.content_hash.clone(),
                after_revision: created.revision + 1,
                after_hash: hex_sha256(b"after"),
                body_relative_path: created.physical_path(),
            },
        );

        service.recover_pending().expect("recover");

        assert!(!transaction_path(&root, tx_id).exists());
        assert_eq!(
            service.read_document(created.id).await.expect("read"),
            "original"
        );
    }

    #[tokio::test]
    async fn recover_completes_transaction_after_content_write() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("project");
        let service = DocumentApplicationService::new(&root);
        let created = service
            .create_document(Uuid::new_v4(), "第一章", "original")
            .await
            .expect("create");
        let tx_id = Uuid::new_v4();
        let after_hash = hex_sha256(b"after");
        std::fs::write(root.join(created.physical_path()), "after").expect("write body");
        write_transaction(
            &root,
            TxSpec {
                id: tx_id,
                document_id: created.id,
                phase: "content_written",
                before_revision: created.revision,
                before_hash: created.content_hash.clone(),
                after_revision: created.revision + 1,
                after_hash: after_hash.clone(),
                body_relative_path: created.physical_path(),
            },
        );

        service.recover_pending().expect("recover");

        let documents = service.list_documents().expect("list");
        assert_eq!(documents[0].revision, 1);
        assert_eq!(documents[0].content_hash, after_hash);
        assert_eq!(
            service.read_document(created.id).await.expect("read"),
            "after"
        );
        assert!(!transaction_path(&root, tx_id).exists());
    }

    #[tokio::test]
    async fn recover_cleans_transaction_after_metadata_write() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("project");
        let service = DocumentApplicationService::new(&root);
        let created = service
            .create_document(Uuid::new_v4(), "第一章", "original")
            .await
            .expect("create");
        let tx_id = Uuid::new_v4();
        let after_hash = hex_sha256(b"after");
        std::fs::write(root.join(created.physical_path()), "after").expect("write body");
        let mut documents = service.list_documents().expect("list");
        documents[0].revision = 1;
        documents[0].content_hash = after_hash.clone();
        std::fs::write(
            root.join(".lingbi/documents.json"),
            serde_json::to_vec(&documents).expect("serialize"),
        )
        .expect("write index");
        write_transaction(
            &root,
            TxSpec {
                id: tx_id,
                document_id: created.id,
                phase: "index_updated",
                before_revision: 0,
                before_hash: created.content_hash.clone(),
                after_revision: 1,
                after_hash,
                body_relative_path: created.physical_path(),
            },
        );

        service.recover_pending().expect("recover");

        assert!(!transaction_path(&root, tx_id).exists());
        assert_eq!(
            service.read_document(created.id).await.expect("read"),
            "after"
        );
    }

    #[tokio::test]
    async fn recover_preserves_external_body_after_content_write() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("project");
        let service = DocumentApplicationService::new(&root);
        let created = service
            .create_document(Uuid::new_v4(), "第一章", "original")
            .await
            .expect("create");
        let tx_id = Uuid::new_v4();
        std::fs::write(root.join(created.physical_path()), "external").expect("external");
        write_transaction(
            &root,
            TxSpec {
                id: tx_id,
                document_id: created.id,
                phase: "content_written",
                before_revision: created.revision,
                before_hash: created.content_hash.clone(),
                after_revision: created.revision + 1,
                after_hash: hex_sha256(b"after"),
                body_relative_path: created.physical_path(),
            },
        );

        service.recover_pending().expect("recover");

        assert_eq!(
            service.read_document(created.id).await.expect("read"),
            "external"
        );
        assert_eq!(
            service.list_documents().expect("list")[0].revision,
            created.revision
        );
        assert!(transaction_path(&root, tx_id).exists());
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

    struct TxSpec {
        id: Uuid,
        document_id: Uuid,
        phase: &'static str,
        before_revision: u64,
        before_hash: String,
        after_revision: u64,
        after_hash: String,
        body_relative_path: std::path::PathBuf,
    }

    fn write_transaction(root: &std::path::Path, spec: TxSpec) {
        let path = transaction_path(root, spec.id);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("transactions dir");
        let value = serde_json::json!({
            "id": spec.id,
            "document_id": spec.document_id,
            "before_revision": spec.before_revision,
            "before_hash": spec.before_hash,
            "after_revision": spec.after_revision,
            "after_hash": spec.after_hash,
            "phase": spec.phase,
            "created_at": chrono::Utc::now(),
            "body_relative_path": spec.body_relative_path.to_string_lossy(),
        });
        std::fs::write(path, serde_json::to_vec(&value).expect("serialize")).expect("write tx");
    }

    fn transaction_path(root: &std::path::Path, tx_id: Uuid) -> std::path::PathBuf {
        root.join(".lingbi/transactions")
            .join(format!("{tx_id}.json"))
    }
}
