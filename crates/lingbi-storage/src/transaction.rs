use chrono::{DateTime, Utc};
use lingbi_contracts::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use crate::atomic_file::{AtomicFileStore, DiskAtomicFileStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionPhase {
    Created,
    ContentWritten,
    IndexUpdated,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentTransaction {
    pub id: Uuid,
    pub document_id: Uuid,
    pub before_revision: u64,
    pub before_hash: String,
    pub after_revision: u64,
    pub after_hash: String,
    pub phase: TransactionPhase,
    pub created_at: DateTime<Utc>,
    pub body_relative_path: String,
}

pub struct DocumentTransactionRepository {
    root: PathBuf,
    store: DiskAtomicFileStore,
}

impl DocumentTransactionRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            store: DiskAtomicFileStore,
        }
    }

    pub fn begin(&self, transaction: &DocumentTransaction) -> Result<(), AppError> {
        let path = self.transaction_path(transaction.id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let bytes = serde_json::to_vec(transaction).map_err(parse_error)?;
        self.store.write_atomic(&path, &bytes, None)?;
        Ok(())
    }

    pub fn set_phase(
        &self,
        id: Uuid,
        phase: TransactionPhase,
    ) -> Result<DocumentTransaction, AppError> {
        let mut transaction = self.get(id)?.ok_or_else(|| {
            AppError::new(
                ErrorCode::ProjectCorrupted,
                format!("transaction not found: {id}"),
                false,
            )
        })?;
        transaction.phase = phase;
        let bytes = serde_json::to_vec(&transaction).map_err(parse_error)?;
        let path = self.transaction_path(id);
        self.store.write_atomic(&path, &bytes, None)?;
        Ok(transaction)
    }

    pub fn get(&self, id: Uuid) -> Result<Option<DocumentTransaction>, AppError> {
        let path = self.transaction_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(io_error)?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(parse_error)
    }

    pub fn list(&self) -> Result<Vec<DocumentTransaction>, AppError> {
        let dir = self.transaction_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut transactions: Vec<DocumentTransaction> = Vec::new();
        for entry in fs::read_dir(&dir).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path).map_err(io_error)?;
            transactions.push(serde_json::from_slice(&bytes).map_err(parse_error)?);
        }
        transactions.sort_by_key(|transaction| transaction.created_at);
        Ok(transactions)
    }

    pub fn delete(&self, id: Uuid) -> Result<(), AppError> {
        let path = self.transaction_path(id);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error(error)),
        }
    }

    fn transaction_dir(&self) -> PathBuf {
        self.root.join(".lingbi/transactions")
    }

    fn transaction_path(&self, id: Uuid) -> PathBuf {
        self.transaction_dir().join(format!("{id}.json"))
    }
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("transaction I/O failed: {error}"),
        false,
    )
}

fn parse_error(error: serde_json::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("transaction metadata parse failed: {error}"),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn transaction() -> DocumentTransaction {
        DocumentTransaction {
            id: Uuid::new_v4(),
            document_id: Uuid::new_v4(),
            before_revision: 0,
            before_hash: "before".to_owned(),
            after_revision: 1,
            after_hash: "after".to_owned(),
            phase: TransactionPhase::Created,
            created_at: Utc::now(),
            body_relative_path: "chapters/chapter.md".to_owned(),
        }
    }

    #[test]
    fn transaction_phase_persists_and_deletes() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("project");
        let repository = DocumentTransactionRepository::new(&root);
        let tx = transaction();

        repository.begin(&tx).expect("begin");
        let updated = repository
            .set_phase(tx.id, TransactionPhase::ContentWritten)
            .expect("phase");

        assert_eq!(updated.phase, TransactionPhase::ContentWritten);
        assert_eq!(
            repository
                .get(tx.id)
                .expect("get")
                .expect("transaction")
                .phase,
            TransactionPhase::ContentWritten
        );
        repository.delete(tx.id).expect("delete");
        assert!(repository.get(tx.id).expect("get").is_none());
    }
}
