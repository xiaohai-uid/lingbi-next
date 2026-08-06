use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::Candidate;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use crate::atomic_file::{AtomicFileStore, DiskAtomicFileStore};

pub struct CandidateRepository {
    root: PathBuf,
    store: DiskAtomicFileStore,
}

impl CandidateRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            store: DiskAtomicFileStore,
        }
    }

    pub fn write(&self, candidate: &Candidate) -> Result<(), AppError> {
        let path = self.candidate_path(candidate.id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let bytes = serde_json::to_vec(candidate).map_err(parse_error)?;
        self.store.write_atomic(&path, &bytes, None)?;
        Ok(())
    }

    pub fn read(&self, id: Uuid) -> Result<Option<Candidate>, AppError> {
        let path = self.candidate_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(io_error)?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(parse_error)
    }

    pub fn list(&self) -> Result<Vec<Candidate>, AppError> {
        let dir = self.candidate_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut candidates = Vec::new();
        for entry in fs::read_dir(&dir).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path).map_err(io_error)?;
            candidates.push(serde_json::from_slice(&bytes).map_err(parse_error)?);
        }
        Ok(candidates)
    }

    pub fn delete(&self, id: Uuid) -> Result<(), AppError> {
        let path = self.candidate_path(id);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error(error)),
        }
    }

    fn candidate_dir(&self) -> PathBuf {
        self.root.join(".lingbi/candidates")
    }

    fn candidate_path(&self, id: Uuid) -> PathBuf {
        self.candidate_dir().join(format!("{id}.json"))
    }
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("candidate I/O failed: {error}"),
        false,
    )
}

fn parse_error(error: serde_json::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("candidate metadata parse failed: {error}"),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use lingbi_domain::CandidateStatus;
    use tempfile::TempDir;

    fn candidate() -> Candidate {
        Candidate {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            document_id: Uuid::new_v4(),
            instruction: "write".to_owned(),
            base_revision: 0,
            base_content_hash: "before".to_owned(),
            content: "content".to_owned(),
            content_hash: "after".to_owned(),
            provider_id: "fake".to_owned(),
            model_id: "fake-model".to_owned(),
            status: CandidateStatus::Pending,
            created_at: Utc::now(),
            approved_at: None,
            committed_at: None,
        }
    }

    #[test]
    fn candidate_repository_round_trip() {
        let temp = TempDir::new().expect("temp dir");
        let repository = CandidateRepository::new(temp.path().join("project"));
        let candidate = candidate();

        repository.write(&candidate).expect("write");
        let loaded = repository
            .read(candidate.id)
            .expect("read")
            .expect("candidate");

        assert_eq!(loaded.content, candidate.content);
        assert_eq!(loaded.status, CandidateStatus::Pending);
        assert_eq!(repository.list().expect("list").len(), 1);
        repository.delete(candidate.id).expect("delete");
        assert!(repository.read(candidate.id).expect("read").is_none());
    }
}
