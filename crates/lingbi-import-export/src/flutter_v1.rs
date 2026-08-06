use chrono::Utc;
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::{Document, Project};
use lingbi_storage::{AtomicFileStore, DiskAtomicFileStore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationReport {
    pub schema_version: Option<u32>,
    pub project_name: Option<String>,
    pub document_count: usize,
    pub old_paths: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationReceipt {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub documents_migrated: usize,
    pub content_hashes: Vec<(String, String)>,
}

struct V1Metadata {
    schema_version: Option<u32>,
    project_name: Option<String>,
    warnings: Vec<String>,
}

pub fn inspect_v1(root: &Path) -> Result<MigrationReport, AppError> {
    let metadata_path = root.join(".lingbi").join("project.json");
    let metadata = read_v1_metadata(&metadata_path, root)?;
    let mut warnings = metadata.warnings;
    let markdown_paths = scan_markdown(root)?;
    if markdown_paths.is_empty() {
        warnings.push("no markdown documents found".to_owned());
    }
    Ok(MigrationReport {
        schema_version: metadata.schema_version,
        project_name: metadata.project_name,
        document_count: markdown_paths.len(),
        old_paths: markdown_paths,
        warnings,
    })
}

pub fn migrate_v1_to_v2(source: &Path, destination: &Path) -> Result<MigrationReceipt, AppError> {
    if destination.exists() {
        return Err(AppError::new(
            ErrorCode::ProjectPathExists,
            format!(
                "migration destination already exists: {}",
                destination.display()
            ),
            false,
        ));
    }

    let report = inspect_v1(source)?;
    fs::create_dir_all(destination.join(".lingbi")).map_err(io_error)?;
    fs::create_dir_all(destination.join("chapters")).map_err(io_error)?;

    let now = Utc::now();
    let project = Project {
        id: Uuid::new_v4(),
        name: report.project_name.unwrap_or_else(|| {
            source
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "project".to_owned())
        }),
        schema_version: 2,
        created_at: now,
        updated_at: now,
    };
    let store = DiskAtomicFileStore;
    write_json(
        &store,
        &destination.join(".lingbi").join("project.json"),
        &project,
    )?;

    let mut documents = Vec::new();
    let mut content_hashes = Vec::new();
    for (order, old_path) in report.old_paths.iter().enumerate() {
        let bytes = fs::read(source.join(old_path)).map_err(io_error)?;
        let content_hash = hex_sha256(&bytes);
        let id = Uuid::new_v4();
        let title = Path::new(old_path)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "未命名".to_owned());
        let new_path = destination.join("chapters").join(format!("{id}.md"));
        store.write_atomic(&new_path, &bytes, None)?;
        documents.push(Document {
            id,
            project_id: project.id,
            title,
            order: order as i64,
            revision: 0,
            content_hash: content_hash.clone(),
            created_at: now,
            updated_at: now,
        });
        content_hashes.push((old_path.clone(), content_hash));
    }

    write_json(
        &store,
        &destination.join(".lingbi").join("documents.json"),
        &documents,
    )?;

    Ok(MigrationReceipt {
        source: source.to_path_buf(),
        destination: destination.to_path_buf(),
        documents_migrated: documents.len(),
        content_hashes,
    })
}

fn read_v1_metadata(path: &Path, root: &Path) -> Result<V1Metadata, AppError> {
    let mut warnings = Vec::new();
    if !path.exists() {
        warnings.push("project metadata is missing".to_owned());
        return Ok(V1Metadata {
            schema_version: None,
            project_name: root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            warnings,
        });
    }
    let bytes = fs::read(path).map_err(io_error)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::new(
            ErrorCode::ProjectCorrupted,
            format!("project metadata is invalid: {error}"),
            false,
        )
    })?;
    let schema_version = value
        .get("schemaVersion")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let project_name = value
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    if schema_version != Some(1) {
        warnings.push("schemaVersion is not 1".to_owned());
    }
    Ok(V1Metadata {
        schema_version,
        project_name,
        warnings,
    })
}

fn scan_markdown(root: &Path) -> Result<Vec<String>, AppError> {
    let mut results = Vec::new();
    scan_markdown_recursive(root, root, &mut results)?;
    results.sort();
    Ok(results)
}

fn scan_markdown_recursive(
    root: &Path,
    current: &Path,
    results: &mut Vec<String>,
) -> Result<(), AppError> {
    for entry in fs::read_dir(current).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some(".lingbi") {
                continue;
            }
            scan_markdown_recursive(root, &path, results)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            let relative = path.strip_prefix(root).map_err(|_| {
                AppError::new(
                    ErrorCode::ProjectCorrupted,
                    "failed to relativize legacy path".to_owned(),
                    false,
                )
            })?;
            results.push(relative.to_string_lossy().into_owned());
        }
    }
    Ok(())
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

fn io_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("migration I/O failed: {error}"),
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
    use lingbi_application::ProjectApplicationService;
    use std::fs;
    use tempfile::TempDir;

    fn source_with_metadata(name: &str, corrupt: bool) -> TempDir {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join(name);
        fs::create_dir_all(root.join(".lingbi")).expect("metadata dir");
        fs::create_dir_all(root.join("chapters")).expect("chapters dir");
        let metadata = if corrupt {
            "{broken".to_owned()
        } else {
            serde_json::json!({
                "schemaVersion": 1,
                "id": "legacy-project",
                "name": "旧版玄幻项目",
                "genre": "玄幻",
                "platform": "起点",
                "createdAt": "2026-07-01T00:00:00.000Z",
                "updatedAt": "2026-07-01T00:00:00.000Z"
            })
            .to_string()
        };
        fs::write(root.join(".lingbi/project.json"), metadata).expect("metadata");
        temp
    }

    #[test]
    fn normal_project_migrates_without_touching_source() {
        let temp = source_with_metadata("novel", false);
        let source = temp.path().join("novel");
        let chapter = source.join("chapters/第一章.md");
        fs::write(&chapter, "# 第一章\n\n原始正文").expect("chapter");
        let destination = temp.path().join("v2");

        let receipt = migrate_v1_to_v2(&source, &destination).expect("migrate");

        assert_eq!(receipt.documents_migrated, 1);
        assert_eq!(
            fs::read_to_string(&chapter).expect("source"),
            "# 第一章\n\n原始正文"
        );
        assert!(destination.join(".lingbi/project.json").exists());
        assert!(
            destination
                .join("chapters")
                .read_dir()
                .expect("chapters")
                .next()
                .is_some()
        );
    }

    #[tokio::test]
    async fn migrated_project_opens_with_production_service() {
        let temp = source_with_metadata("novel", false);
        let source = temp.path().join("novel");
        fs::write(source.join("chapters/第一章.md"), "# 第一章\n\n原始正文").expect("chapter");
        let destination = temp.path().join("v2");
        migrate_v1_to_v2(&source, &destination).expect("migrate");

        let service = ProjectApplicationService::new();
        let opened = service
            .open_project(destination.clone())
            .await
            .expect("open");

        assert_eq!(opened.project.schema_version, 2);
        assert_eq!(opened.project.name, "旧版玄幻项目");
        assert_eq!(opened.current_document.title, "第一章");
        assert!(
            destination
                .join(opened.current_document.physical_path())
                .exists()
        );
    }

    #[test]
    fn duplicate_titles_get_distinct_v2_paths() {
        let temp = source_with_metadata("novel", false);
        let source = temp.path().join("novel");
        fs::write(source.join("chapters/第一章.md"), "A").expect("a");
        fs::create_dir_all(source.join("project_meta")).expect("meta");
        fs::write(source.join("project_meta/第一章.md"), "B").expect("b");
        let destination = temp.path().join("v2");

        migrate_v1_to_v2(&source, &destination).expect("migrate");

        let files: Vec<_> = fs::read_dir(destination.join("chapters"))
            .expect("chapters")
            .map(|entry| entry.expect("entry").path())
            .collect();
        assert_eq!(files.len(), 2);
        assert_ne!(files[0], files[1]);
    }

    #[test]
    fn missing_metadata_migrates_with_warning() {
        let temp = TempDir::new().expect("temp dir");
        let source = temp.path().join("missing");
        fs::create_dir_all(source.join("chapters")).expect("chapters");
        fs::write(source.join("chapters/第一章.md"), "# 第一章").expect("chapter");

        let report = inspect_v1(&source).expect("inspect");
        assert!(report.warnings.iter().any(|w| w.contains("missing")));

        let destination = temp.path().join("v2");
        let receipt = migrate_v1_to_v2(&source, &destination).expect("migrate");
        assert_eq!(receipt.documents_migrated, 1);
    }

    #[test]
    fn corrupt_metadata_is_rejected() {
        let temp = source_with_metadata("corrupt", true);
        let source = temp.path().join("corrupt");
        assert!(inspect_v1(&source).is_err());
    }
}
