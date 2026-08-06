use lingbi_contracts::{AppError, ErrorCode};
use std::fs;
use std::path::{Path, PathBuf};

pub struct ProjectPathGuard {
    root: PathBuf,
}

impl ProjectPathGuard {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve(&self, relative: &Path) -> Result<PathBuf, AppError> {
        validate_relative(relative)?;

        let root = fs::canonicalize(&self.root).map_err(|error| {
            AppError::new(
                ErrorCode::ProjectCorrupted,
                format!("project root is not resolvable: {error}"),
                false,
            )
        })?;
        let candidate = self.root.join(relative);
        let canonical_candidate = canonicalize_existing_or_parent(&candidate).map_err(|error| {
            AppError::new(
                ErrorCode::ProjectCorrupted,
                format!("candidate path is not resolvable: {error}"),
                false,
            )
        })?;

        if !canonical_candidate.starts_with(&root) {
            return Err(AppError::new(
                ErrorCode::ProjectCorrupted,
                "path escapes project boundary".to_owned(),
                false,
            ));
        }

        Ok(candidate)
    }
}

fn validate_relative(relative: &Path) -> Result<(), AppError> {
    let raw = relative.to_string_lossy();
    if raw.contains('\0') {
        return Err(unsafe_path("path contains NUL"));
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return Err(unsafe_path("absolute path is not allowed"));
    }
    if raw.contains(":/") || raw.contains(":\\") || raw.starts_with("\\\\") {
        return Err(unsafe_path("Windows drive or UNC path is not allowed"));
    }

    let normalized = raw.replace('\\', "/");
    for segment in normalized.split('/') {
        if segment == ".." {
            return Err(unsafe_path("parent traversal is not allowed"));
        }
        if segment.is_empty() {
            return Err(unsafe_path("empty path segment is not allowed"));
        }
    }

    Ok(())
}

fn canonicalize_existing_or_parent(candidate: &Path) -> std::io::Result<PathBuf> {
    let mut current = candidate.to_path_buf();
    let mut missing: Vec<std::ffi::OsString> = Vec::new();

    loop {
        match fs::canonicalize(&current) {
            Ok(canonical) => {
                let mut result = canonical;
                for component in missing.iter().rev() {
                    result.push(component);
                }
                return Ok(result);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let Some(file_name) = current.file_name() else {
                    return Err(std::io::Error::other("path has no file name"));
                };
                missing.push(file_name.to_os_string());
                let Some(parent) = current.parent() else {
                    return Err(error);
                };
                if parent.as_os_str().is_empty() {
                    return Err(error);
                }
                current = parent.to_path_buf();
            }
            Err(error) => return Err(error),
        }
    }
}

fn unsafe_path(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::ProjectCorrupted, message.into(), false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn guard() -> (TempDir, ProjectPathGuard) {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("project");
        fs::create_dir_all(&root).expect("root");
        (temp, ProjectPathGuard::new(root))
    }

    #[test]
    fn accepts_normal_relative_path() {
        let (temp, guard) = guard();
        let resolved = guard
            .resolve(Path::new("chapters/chapter-1.md"))
            .expect("resolve");
        assert_eq!(resolved, temp.path().join("project/chapters/chapter-1.md"));
    }

    #[test]
    fn rejects_windows_parent_traversal() {
        let (_, guard) = guard();
        assert!(guard.resolve(Path::new(r"..\evil.txt")).is_err());
    }

    #[test]
    fn rejects_windows_drive_prefix() {
        let (_, guard) = guard();
        assert!(guard.resolve(Path::new(r"C:\evil.txt")).is_err());
    }

    #[test]
    fn rejects_unc_path() {
        let (_, guard) = guard();
        assert!(guard.resolve(Path::new(r"\\server\share")).is_err());
    }

    #[test]
    fn rejects_traversal_after_chapter_prefix() {
        let (_, guard) = guard();
        assert!(
            guard
                .resolve(Path::new(r"chapters\..\..\evil.txt"))
                .is_err()
        );
    }

    #[test]
    fn rejects_nul_byte() {
        let (_, guard) = guard();
        assert!(guard.resolve(Path::new("chapters/\0evil.txt")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let (temp, guard) = guard();
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, b"outside").expect("outside");
        let link = guard.root().join("escape.txt");
        symlink(&outside, &link).expect("symlink");

        assert!(guard.resolve(Path::new("escape.txt")).is_err());
    }
}
