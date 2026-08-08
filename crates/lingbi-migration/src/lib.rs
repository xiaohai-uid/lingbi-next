//! lingbi-migration — P6 V1 → Book V3 一次迁移五件套。
//!
//! 管线：MigrationInspector（只读扫描 + source_hash）→ MigrationPlan
//! （逐文件目标映射 + 可行性）→ MigrationExecutor（构建目标树 + 托管化）→
//! MigrationValidator（逐文件字节对比 + 契约/doctor + READ ONLY 断言）→
//! MigrationReceipt（六字段回执，落 `.lingbi/migration-receipt.json`）。
//!
//! 字节纪律（P5-4 教训）：目标文件名派生自真实文件名/白名单，不信任源
//! frontmatter 拼路径；正文 byte-for-byte；source 全程只读。

pub mod executor;
pub mod inspector;
pub mod plan;
pub mod receipt;
pub mod validator;

pub use executor::MigrationExecutor;
pub use inspector::{Classification, FileKind, MigrationInspector, MigrationReport, SourceFile};
pub use plan::{FileRule, MigrationPlan, PlanStep};
pub use receipt::MigrationReceipt;
pub use validator::MigrationValidator;

use lingbi_contracts::{AppError, ErrorCode};
use sha2::Digest;
use std::path::Path;

/// 五件套一次性入口：inspect → plan → execute → validate，返回六字段回执。
/// 校验失败 fail-closed（目标保留供人工排查，可删除后重试——目标为空
/// 目录时允许重复迁移）。
pub fn migrate_book(source: &Path, destination: &Path) -> Result<MigrationReceipt, AppError> {
    let report = MigrationInspector::inspect(source)?;
    let plan = MigrationPlan::plan(&report, destination)?;
    MigrationExecutor::execute(&plan)?;
    MigrationValidator::validate(source, destination, report.classification)?;
    MigrationReceipt::build(&plan)
}

fn app_error(code: ErrorCode, message: impl Into<String>) -> AppError {
    AppError::new(code, message.into(), false)
}

/// 排序后的逐文件 sha256 拼接 → 整体 sha256（确定性树哈希；
/// 排除 `.lingbi/` 与迁移回执——事务层由应用生成，不参与源/目标等价）。
pub(crate) fn tree_hash(root: &Path, exclude_lingbi: bool) -> Result<String, AppError> {
    let mut entries: Vec<(String, String)> = Vec::new();
    collect_hashes(root, root, exclude_lingbi, &mut entries)?;
    entries.sort();
    let mut hasher = sha2::Sha256::new();
    for (rel, hash) in entries {
        hasher.update(rel.as_bytes());
        hasher.update(b"\0");
        hasher.update(hash.as_bytes());
        hasher.update(b"\0");
    }
    Ok(hex(&hasher.finalize()))
}

fn collect_hashes(
    root: &Path,
    current: &Path,
    exclude_lingbi: bool,
    out: &mut Vec<(String, String)>,
) -> Result<(), AppError> {
    let mut entries: Vec<_> = std::fs::read_dir(current)
        .map_err(|e| app_error(ErrorCode::ProjectCorrupted, format!("读取目录失败: {e}")))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if exclude_lingbi && path.file_name().and_then(|n| n.to_str()) == Some(".lingbi") {
                continue;
            }
            collect_hashes(root, &path, exclude_lingbi, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .map_err(|_| app_error(ErrorCode::ProjectCorrupted, "相对化路径失败".to_owned()))?
                .to_string_lossy()
                .into_owned();
            if exclude_lingbi && rel == "migration-receipt.json" {
                continue;
            }
            let bytes = std::fs::read(&path)
                .map_err(|e| app_error(ErrorCode::ProjectCorrupted, format!("读取文件失败: {e}")))?;
            out.push((rel, hex(&sha2::Sha256::digest(&bytes))));
        }
    }
    Ok(())
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex(&sha2::Sha256::digest(bytes))
}

/// 原子复制（tmp + rename，与引擎 legacy 写路径同纪律）：目标为全新
/// 文件，无 CAS 冲突面；迁移审计由回执承担。
pub(crate) fn copy_atomic(source: &Path, destination: &Path) -> Result<(), AppError> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            app_error(
                ErrorCode::DiskWriteFailed,
                format!("创建目标目录失败: {e}"),
            )
        })?;
    }
    let tmp = destination.with_extension("mig.tmp");
    std::fs::copy(source, &tmp)
        .map_err(|e| app_error(ErrorCode::DiskWriteFailed, format!("复制失败: {e}")))?;
    std::fs::rename(&tmp, destination)
        .map_err(|e| app_error(ErrorCode::DiskWriteFailed, format!("提交失败: {e}")))?;
    Ok(())
}
