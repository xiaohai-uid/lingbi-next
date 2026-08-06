use lingbi_contracts::{AppError, ErrorCode};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub trait AtomicFileStore {
    fn read(&self, path: &Path) -> Result<Vec<u8>, AppError>;

    fn write_atomic(
        &self,
        path: &Path,
        bytes: &[u8],
        expected_hash: Option<&str>,
    ) -> Result<String, AppError>;
}

#[derive(Debug, Default, Clone)]
pub struct DiskAtomicFileStore;

impl AtomicFileStore for DiskAtomicFileStore {
    fn read(&self, path: &Path) -> Result<Vec<u8>, AppError> {
        fs::read(path).map_err(|error| {
            let code = if error.kind() == std::io::ErrorKind::NotFound {
                ErrorCode::DocumentNotFound
            } else {
                ErrorCode::ProjectCorrupted
            };
            AppError::new(code, format!("read failed: {error}"), false)
        })
    }

    fn write_atomic(
        &self,
        path: &Path,
        bytes: &[u8],
        expected_hash: Option<&str>,
    ) -> Result<String, AppError> {
        let content_hash = hex_sha256(bytes);
        if let Some(expected) = expected_hash
            && !expected.eq_ignore_ascii_case(&content_hash)
        {
            return Err(AppError::new(
                ErrorCode::DocumentConflict,
                "expected content hash does not match".to_owned(),
                false,
            ));
        }

        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| {
            AppError::new(
                ErrorCode::ProjectCorrupted,
                format!("create parent failed: {error}"),
                false,
            )
        })?;

        let temp_path = temp_path_for(path);
        let write_result = (|| -> std::io::Result<()> {
            let mut file = File::create(&temp_path)?;
            file.write_all(bytes)?;
            file.flush()?;
            file.sync_all()?;
            Ok(())
        })();

        if let Err(error) = write_result {
            let _ = fs::remove_file(&temp_path);
            return Err(AppError::new(
                ErrorCode::ProjectCorrupted,
                format!("temporary write failed: {error}"),
                false,
            ));
        }

        if let Err(error) = fs::rename(&temp_path, path) {
            let _ = fs::remove_file(&temp_path);
            return Err(AppError::new(
                ErrorCode::ProjectCorrupted,
                format!("atomic replacement failed: {error}"),
                false,
            ));
        }

        Ok(content_hash)
    }
}

fn temp_path_for(path: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_owned());
    path.with_file_name(format!("{file_name}.tmp-{}-{nanos}", std::process::id()))
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn normal_write_is_readable() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join("chapter.md");
        let store = DiskAtomicFileStore;

        let hash = store
            .write_atomic(&path, b"canonical", None)
            .expect("write");

        assert_eq!(hash.len(), 64);
        assert_eq!(store.read(&path).expect("read"), b"canonical");
    }

    #[test]
    fn replacement_replaces_canonical_bytes() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join("chapter.md");
        let store = DiskAtomicFileStore;

        store.write_atomic(&path, b"first", None).expect("first");
        store.write_atomic(&path, b"second", None).expect("second");

        assert_eq!(store.read(&path).expect("read"), b"second");
    }

    #[test]
    fn hash_conflict_preserves_canonical_bytes() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join("chapter.md");
        let store = DiskAtomicFileStore;
        store
            .write_atomic(&path, b"canonical", None)
            .expect("write");

        let result = store.write_atomic(&path, b"replacement", Some("0000"));

        assert!(result.is_err());
        assert_eq!(store.read(&path).expect("read"), b"canonical");
    }

    #[test]
    fn stale_temp_file_does_not_become_canonical() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join("chapter.md");
        let store = DiskAtomicFileStore;
        store
            .write_atomic(&path, b"canonical", None)
            .expect("write");

        let stale = temp.path().join("chapter.md.tmp-stale");
        fs::write(&stale, b"stale").expect("stale temp");

        assert_eq!(store.read(&path).expect("read"), b"canonical");

        store
            .write_atomic(&path, b"replacement", None)
            .expect("write");

        assert_eq!(store.read(&path).expect("read"), b"replacement");
        assert_eq!(fs::read(&stale).expect("stale still exists"), b"stale");
    }
}
