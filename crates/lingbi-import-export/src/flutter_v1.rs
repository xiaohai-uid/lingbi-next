use lingbi_contracts::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationReport {
    pub schema_version: Option<u32>,
    pub project_name: Option<String>,
    pub document_count: usize,
    pub resource_count: usize,
    pub old_paths: Vec<String>,
    pub warnings: Vec<String>,
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
    let chapter_paths: Vec<_> = markdown_paths
        .iter()
        .filter(|path| is_chapter_path(path))
        .cloned()
        .collect();
    let resource_paths: Vec<_> = markdown_paths
        .iter()
        .filter(|path| !is_chapter_path(path))
        .cloned()
        .collect();
    if markdown_paths.is_empty() {
        warnings.push("no markdown documents found".to_owned());
    }
    for path in &resource_paths {
        warnings.push(format!("migrating {path} as a resource, not a chapter"));
    }
    Ok(MigrationReport {
        schema_version: metadata.schema_version,
        project_name: metadata.project_name,
        document_count: chapter_paths.len(),
        resource_count: resource_paths.len(),
        old_paths: markdown_paths,
        warnings,
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

fn is_chapter_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.starts_with("章节内容/") || normalized.starts_with("chapters/")
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

fn io_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("migration I/O failed: {error}"),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn inspect_reports_chapter_and_resource_counts() {
        let temp = source_with_metadata("novel", false);
        let source = temp.path().join("novel");
        fs::create_dir_all(source.join("章节内容")).expect("章节内容");
        fs::write(source.join("章节内容/第一章.md"), "# 第一章\n\n正文 A").expect("第一章");
        fs::write(source.join("章节内容/第二章.md"), "# 第二章\n\n正文 B").expect("第二章");
        fs::create_dir_all(source.join("小说资料")).expect("小说资料");
        fs::write(source.join("小说资料/世界观.md"), "世界观资料").expect("世界观");
        fs::write(source.join("小说资料/人物库.md"), "人物资料").expect("人物库");
        fs::create_dir_all(source.join("project_meta")).expect("project_meta");
        fs::write(source.join("project_meta/notes.md"), "项目笔记").expect("notes");

        let report = inspect_v1(&source).expect("inspect");
        assert_eq!(report.document_count, 2);
        assert_eq!(report.resource_count, 3);
        assert_eq!(report.project_name.as_deref(), Some("旧版玄幻项目"));
        assert!(
            report
                .old_paths
                .iter()
                .any(|p| p.replace('\\', "/") == "章节内容/第一章.md")
        );
    }

    #[test]
    fn inspect_detects_duplicate_title_as_chapter_and_resource() {
        // 同名文件跨目录：章节目录内为 chapter，其余为 resource
        let temp = source_with_metadata("novel", false);
        let source = temp.path().join("novel");
        fs::write(source.join("chapters/第一章.md"), "A").expect("a");
        fs::create_dir_all(source.join("project_meta")).expect("meta");
        fs::write(source.join("project_meta/第一章.md"), "B").expect("b");

        let report = inspect_v1(&source).expect("inspect");
        assert_eq!(report.document_count, 1);
        assert_eq!(report.resource_count, 1);
    }

    #[test]
    fn inspect_missing_metadata_reports_warning() {
        let temp = TempDir::new().expect("temp dir");
        let source = temp.path().join("missing");
        fs::create_dir_all(source.join("chapters")).expect("chapters");
        fs::write(source.join("chapters/第一章.md"), "# 第一章").expect("chapter");

        let report = inspect_v1(&source).expect("inspect");
        assert!(report.warnings.iter().any(|w| w.contains("missing")));
    }

    #[test]
    fn corrupt_metadata_is_rejected() {
        let temp = source_with_metadata("corrupt", true);
        let source = temp.path().join("corrupt");
        assert!(inspect_v1(&source).is_err());
    }
}
