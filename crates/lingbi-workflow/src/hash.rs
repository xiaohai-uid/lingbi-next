//! 规范化哈希 + 书仓上下文指纹（P7 章事务引擎，#24）。
//!
//! - `canonical_hash`：键序固定的 JSON 哈希（serde_json Map 默认 BTreeMap，
//!   键排序 → 跨进程/跨平台确定性）。
//! - `book_input_hash`：书仓上下文指纹——大纲/文风/设定/记忆 + book.yaml +
//!   追踪状态（排序 manifest）。覆盖链的全部确定性输入文件；排除
//!   `.lingbi/**`（应用托管层）、`.git/**`、`工作区/**`（可丢弃产物）。

use lingbi_contracts::{AppError, ErrorCode};
use std::path::Path;

/// 确定性 JSON 哈希（`(key, value)` 键序固定；Map 键排序后序列化）。
pub fn canonical_hash(entries: &[(&str, serde_json::Value)]) -> String {
    let mut map = serde_json::Map::new();
    for (k, v) in entries {
        map.insert((*k).to_owned(), v.clone());
    }
    let bytes = serde_json::to_vec(&serde_json::Value::Object(map))
        .expect("canonical json 序列化恒成功");
    lingbi_storage::sha256_hex(&bytes)
}

/// 章文件当前哈希（链输入的一部分；无章文件 = "absent"）。
pub fn chapter_state_hash(root: &Path, ch: i64) -> Result<String, AppError> {
    for f in lingbi_book::chapter_files(root) {
        if lingbi_book::chapter_num(&f) == ch {
            let bytes = std::fs::read(root.join(lingbi_book::state::BODY_DIR).join(&f))
                .map_err(|e| {
                    AppError::new(
                        ErrorCode::DiskWriteFailed,
                        format!("读章文件失败: {e}"),
                        false,
                    )
                })?;
            return Ok(lingbi_storage::sha256_hex(&bytes));
        }
    }
    Ok("absent".to_owned())
}

/// 书仓上下文指纹（排序 manifest：相对路径 + 文件哈希，整体再哈希）。
pub fn book_input_hash(root: &Path) -> Result<String, AppError> {
    let mut entries: Vec<(String, String)> = Vec::new();
    for rel in ["大纲", "文风", "定稿/设定", "定稿/记忆"] {
        collect_dir(root, &root.join(rel), &mut entries)?;
    }
    for rel in ["book.yaml", "追踪/_tracking-state.json"] {
        let p = root.join(rel);
        if let Ok(bytes) = std::fs::read(&p) {
            entries.push((rel.to_owned(), lingbi_storage::sha256_hex(&bytes)));
        }
    }
    entries.sort();
    let manifest = serde_json::to_string(&entries).expect("manifest 序列化恒成功");
    Ok(lingbi_storage::sha256_hex(manifest.as_bytes()))
}

/// 递归收集目录内文件（排序 rel 路径 + 哈希）；跳过 `.lingbi`/`.git` 目录。
fn collect_dir(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, String)>,
) -> Result<(), AppError> {
    if !dir.is_dir() {
        return Ok(());
    }
    let mut files = Vec::new();
    collect_files(dir, &mut files)?;
    files.sort();
    for f in files {
        let rel = f
            .strip_prefix(root)
            .map_err(|_| {
                AppError::new(ErrorCode::BookCorrupted, "指纹路径越界".to_owned(), false)
            })?
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = std::fs::read(&f).map_err(|e| {
            AppError::new(
                ErrorCode::DiskWriteFailed,
                format!("读指纹文件失败: {e}"),
                false,
            )
        })?;
        out.push((rel, lingbi_storage::sha256_hex(&bytes)));
    }
    Ok(())
}

fn collect_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), AppError> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("扫描指纹目录失败: {e}"),
            false,
        )
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == ".lingbi" || name == ".git" {
                continue;
            }
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}
