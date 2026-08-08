//! MigrationInspector — 只读扫描源，产出 classification + source_hash + 文件清单。

use crate::{app_error, sha256_hex, tree_hash};
use lingbi_book::state::chapter_files;
use lingbi_contracts::{AppError, ErrorCode};
use serde::Serialize;
use std::path::Path;

/// 源分类：storyrepo-legacy（目录树原样 + 托管化）/ flutter-v1（结构转换）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Classification {
    StoryrepoLegacy,
    FlutterV1,
}

impl Classification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Classification::StoryrepoLegacy => "storyrepo-legacy",
            Classification::FlutterV1 => "flutter-v1",
        }
    }
}

/// 文件迁移方式（plan/回执共用词汇）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FileKind {
    /// 字节原样搬入（storyrepo-legacy 全树；flutter-v1 无 frontmatter 文件）
    Copy,
    /// 章文件（flutter-v1：转 定稿/正文 + frontmatter 补全）
    Chapter,
    /// 资源文件（flutter-v1：归档 工作区/迁移存档/）
    Resource,
    /// book.yaml（补 id）
    BookYaml,
    /// 追踪状态（原样搬入，保持引擎语义）
    TrackingState,
    /// 跳过（V1 应用元数据）
    Skip,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceFile {
    pub rel_path: String,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationReport {
    pub source: std::path::PathBuf,
    pub classification: Classification,
    /// 排序后逐文件 sha256 拼接的整体哈希（迁移前后一致 = READ ONLY 证据）
    pub source_hash: String,
    pub files: Vec<SourceFile>,
    pub warnings: Vec<String>,
    /// flutter-v1：project.json 的 name（legacy 用 book.yaml name）
    pub project_name: Option<String>,
}

impl MigrationReport {
    pub fn source(&self) -> &Path {
        &self.source
    }
    pub fn project_name(&self) -> Option<&str> {
        self.project_name.as_deref()
    }
}

pub struct MigrationInspector;

impl MigrationInspector {
    /// 只读扫描。判定规则：
    /// - `source/.lingbi/project.json` 存在 → flutter-v1（复用 inspect_v1 扫描）
    /// - `source/book.yaml` + `追踪/_tracking-state.json` → storyrepo-legacy
    /// - 两者皆无 → 拒绝（UNSUPPORTED_SOURCE，fail-closed）
    pub fn inspect(source: &Path) -> Result<MigrationReport, AppError> {
        if !source.is_dir() {
            return Err(app_error(
                ErrorCode::ProjectCorrupted,
                format!("迁移源不存在或不是目录: {}", source.display()),
            ));
        }
        let source_hash = tree_hash(source, false)?;
        let v1_metadata = source.join(".lingbi").join("project.json");
        if v1_metadata.exists() {
            return Self::inspect_flutter_v1(source, source_hash);
        }
        let book_yaml = source.join("book.yaml");
        let tracking = source.join("追踪").join("_tracking-state.json");
        if book_yaml.exists() && tracking.exists() {
            return Self::inspect_storyrepo_legacy(source, source_hash);
        }
        Err(app_error(
            ErrorCode::ImportUnsupportedFormat,
            format!(
                "无法识别源格式: 缺 .lingbi/project.json 且缺 book.yaml+追踪 ({}); 支持 flutter-v1 与 storyrepo-legacy",
                source.display()
            ),
        ))
    }

    fn inspect_storyrepo_legacy(source: &Path, source_hash: String) -> Result<MigrationReport, AppError> {
        let mut warnings = Vec::new();
        let mut files = Vec::new();
        collect_tree(source, source, &mut files, &mut warnings)?;
        if chapter_files(source).is_empty() {
            warnings.push("源书仓无章节文件（定稿/正文 为空）".to_owned());
        }
        // book.yaml / 追踪状态 标记特殊规则（回执可查）
        for file in files.iter_mut() {
            if file.rel_path == "book.yaml" {
                file.kind = FileKind::BookYaml;
            }
            if file.rel_path == "追踪/_tracking-state.json" {
                file.kind = FileKind::TrackingState;
            }
        }
        Ok(MigrationReport {
            source: source.to_path_buf(),
            classification: Classification::StoryrepoLegacy,
            source_hash,
            files,
            warnings,
            project_name: None,
        })
    }

    fn inspect_flutter_v1(source: &Path, source_hash: String) -> Result<MigrationReport, AppError> {
        let v1_report = lingbi_import_export::flutter_v1::inspect_v1(source)?;
        let mut files = Vec::new();
        for old_path in v1_report.old_paths {
            let normalized = old_path.replace('\\', "/");
            let kind = if normalized.starts_with("章节内容/") || normalized.starts_with("chapters/") {
                FileKind::Chapter
            } else {
                FileKind::Resource
            };
            files.push(SourceFile {
                rel_path: normalized,
                kind,
            });
        }
        Ok(MigrationReport {
            source: source.to_path_buf(),
            classification: Classification::FlutterV1,
            source_hash,
            files,
            warnings: v1_report.warnings,
            project_name: v1_report.project_name,
        })
    }
}

/// 递归收集目录树（跳过 `.lingbi/`——V1 应用元数据不迁入）。
fn collect_tree(
    root: &Path,
    current: &Path,
    files: &mut Vec<SourceFile>,
    warnings: &mut Vec<String>,
) -> Result<(), AppError> {
    let mut entries: Vec<_> = std::fs::read_dir(current)
        .map_err(|e| app_error(ErrorCode::ProjectCorrupted, format!("读取目录失败: {e}")))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            // 应用元数据（.lingbi）与 VCS 元数据（.git）都不属于书仓内容
            if name == ".lingbi" || name == ".git" {
                warnings.push(format!("跳过元数据目录: {name}"));
                continue;
            }
            collect_tree(root, &path, files, warnings)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .map_err(|_| app_error(ErrorCode::ProjectCorrupted, "相对化路径失败".to_owned()))?
                .to_string_lossy()
                .into_owned();
            files.push(SourceFile {
                rel_path: rel,
                kind: FileKind::Copy,
            });
        }
    }
    Ok(())
}

/// 单文件哈希（validator/回执复用）。
pub fn file_hash(root: &Path, rel: &str) -> Result<String, AppError> {
    let bytes = std::fs::read(root.join(rel))
        .map_err(|e| app_error(ErrorCode::ProjectCorrupted, format!("读取失败: {e}")))?;
    Ok(sha256_hex(&bytes))
}
