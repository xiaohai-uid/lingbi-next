//! MigrationValidator — 逐文件字节对比 + 最小树契约校验 + doctor + READ ONLY 断言。

use crate::inspector::Classification;
use crate::{app_error, tree_hash};
use lingbi_book::cmd::cmd_doctor;
use lingbi_contracts::{AppError, ErrorCode};
use std::path::Path;

pub struct MigrationValidator;

impl MigrationValidator {
    /// 校验（fail-closed）：任一断言失败返回 Err，目标保留供排查。
    /// 1. 逐文件字节对比（storyrepo-legacy：rel 相同字节相等；
    ///    flutter-v1：源全文是目标文件后缀——frontmatter 补全只加头不改正文）
    /// 2. 最小 V3 树契约（book.yaml / required_dirs / 章命名 / .lingbi/ 六目录）
    /// 3. doctor（prewrite 阶段）exit 0
    /// 4. source READ ONLY：重算源哈希可读（一致性由调用方对 Inspector 值断言）
    pub fn validate(
        source: &Path,
        destination: &Path,
        classification: Classification,
    ) -> Result<(), AppError> {
        match classification {
            Classification::StoryrepoLegacy => Self::compare_legacy(source, destination)?,
            Classification::FlutterV1 => Self::compare_flutter_v1(source, destination)?,
        }
        Self::check_tree_contract(destination)?;
        Self::run_doctor(destination)?;
        let _ = tree_hash(source, false)?;
        Ok(())
    }

    /// storyrepo-legacy：源/目标 rel 相同，逐文件字节相等（book.yaml 允许
    /// 补 id——generate-book-id 规则）。
    fn compare_legacy(source: &Path, destination: &Path) -> Result<(), AppError> {
        let mut src: Vec<String> = Vec::new();
        collect_rel(source, source, &mut src)?;
        src.sort();
        for rel in &src {
            if rel == "book.yaml" {
                continue;
            }
            let src_bytes = std::fs::read(source.join(rel))
                .map_err(|e| app_error(ErrorCode::ProjectCorrupted, format!("读源失败: {e}")))?;
            let dst_path = destination.join(rel);
            if !dst_path.exists() {
                return Err(app_error(
                    ErrorCode::ProjectCorrupted,
                    format!("目标缺失文件: {rel}"),
                ));
            }
            let dst_bytes = std::fs::read(&dst_path)
                .map_err(|e| app_error(ErrorCode::ProjectCorrupted, format!("读目标失败: {e}")))?;
            if src_bytes != dst_bytes {
                return Err(app_error(
                    ErrorCode::ProjectCorrupted,
                    format!("字节不一致（迁移有损）: {rel}"),
                ));
            }
        }
        Ok(())
    }

    /// flutter-v1：源全文是目标文件的后缀（目标 = frontmatter 头 + 源全文；
    /// 源自带 frontmatter 时目标 == 源）。资源文件 rel 不同（工作区/迁移存档/）
    /// ——按文件名后缀在目标中查找。
    fn compare_flutter_v1(source: &Path, destination: &Path) -> Result<(), AppError> {
        let mut src: Vec<String> = Vec::new();
        collect_rel(source, source, &mut src)?;
        for rel in &src {
            if rel.starts_with(".lingbi/") {
                continue;
            }
            let src_bytes = std::fs::read(source.join(rel))
                .map_err(|e| app_error(ErrorCode::ProjectCorrupted, format!("读源失败: {e}")))?;
            let mut matched = false;
            let dst_root = if rel.starts_with("章节内容/") || rel.starts_with("chapters/") {
                destination.join("定稿/正文")
            } else {
                destination.join("工作区/迁移存档")
            };
            let dst_name = rel.rsplit('/').next().unwrap_or(rel);
            let dst_candidates = find_by_name(&dst_root, dst_name)?;
            for dst in &dst_candidates {
                let dst_bytes = std::fs::read(dst)
                    .map_err(|e| app_error(ErrorCode::ProjectCorrupted, format!("读目标失败: {e}")))?;
                if dst_bytes == src_bytes || ends_with(&dst_bytes, &src_bytes) {
                    matched = true;
                    break;
                }
            }
            if !matched {
                return Err(app_error(
                    ErrorCode::ProjectCorrupted,
                    format!("flutter-v1 迁移有损（正文不一致）: {rel}"),
                ));
            }
        }
        Ok(())
    }

    /// 最小 V3 树契约（tree.json 核心项，Rust 侧校验；完整契约校验走
    /// scripts/validate-book-v3.py 本地验收）。
    fn check_tree_contract(destination: &Path) -> Result<(), AppError> {
        for dir in ["大纲", "定稿", "文风", "工作区", "追踪"] {
            if !destination.join(dir).is_dir() {
                return Err(app_error(
                    ErrorCode::BookCorrupted,
                    format!("目标缺契约目录: {dir}"),
                ));
            }
        }
        if !destination.join("book.yaml").is_file() {
            return Err(app_error(ErrorCode::BookCorrupted, "目标缺 book.yaml".to_owned()));
        }
        if !destination.join("追踪/_tracking-state.json").is_file() {
            return Err(app_error(
                ErrorCode::BookCorrupted,
                "目标缺 追踪/_tracking-state.json".to_owned(),
            ));
        }
        for dir in ["transactions", "candidates", "approvals", "receipts", "checkpoints", "versions"] {
            if !destination.join(".lingbi").join(dir).is_dir() {
                return Err(app_error(
                    ErrorCode::BookCorrupted,
                    format!("目标缺 .lingbi/{dir}"),
                ));
            }
        }
        Ok(())
    }

    /// doctor（prewrite 阶段）：迁移后书仓通过引擎门禁。
    fn run_doctor(destination: &Path) -> Result<(), AppError> {
        let out = cmd_doctor(destination, "prewrite", None)?;
        if out.exit != 0 {
            return Err(app_error(
                ErrorCode::BookCorrupted,
                format!("迁移后 doctor 未通过 (exit {}): {}", out.exit, out.stdout),
            ));
        }
        Ok(())
    }
}

fn collect_rel(root: &Path, current: &Path, out: &mut Vec<String>) -> Result<(), AppError> {
    let mut entries: Vec<_> = std::fs::read_dir(current)
        .map_err(|e| app_error(ErrorCode::ProjectCorrupted, format!("读取目录失败: {e}")))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some(".lingbi") {
                continue;
            }
            collect_rel(root, &path, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .map_err(|_| app_error(ErrorCode::ProjectCorrupted, "相对化路径失败".to_owned()))?
                .to_string_lossy()
                .into_owned();
            out.push(rel);
        }
    }
    Ok(())
}

fn find_by_name(root: &Path, name: &str) -> Result<Vec<std::path::PathBuf>, AppError> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .map_err(|e| app_error(ErrorCode::ProjectCorrupted, format!("读取目录失败: {e}")))?
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if entry.file_name().to_string_lossy() == name {
                out.push(path);
            }
        }
    }
    Ok(out)
}

fn ends_with(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && &haystack[haystack.len() - needle.len()..] == needle
}
