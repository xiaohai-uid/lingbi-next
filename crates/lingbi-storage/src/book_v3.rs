//! Book V3 事务层共享记录（P3 #20）—— `.lingbi/` 的 receipts/checkpoints 与目录契约。
//!
//! 分层：`lingbi-storage` 只持有记录类型与路径（gate/recovery 双方复用），
//! 事务语义（修订守卫/CAS/日志生命周期）在 `lingbi-book::tx`，收敛在 `lingbi-recovery::book_v3`。

use chrono::{DateTime, Utc};
use lingbi_contracts::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// `.lingbi/` 六目录（spec Implementation Decisions #2；旧书仓允许缺席）。
pub const TX_LAYER_DIRS: [&str; 6] = [
    "transactions",
    "candidates",
    "approvals",
    "receipts",
    "checkpoints",
    "versions",
];

/// 事务回执：权威写入完成的证据（`.lingbi/receipts/{tx_id}.json`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookV3Receipt {
    /// 对应事务日志 id。
    pub id: Uuid,
    /// 相对书仓根的权威文件路径（`/` 分隔）。
    pub rel_path: String,
    /// 写后修订号（仅追踪状态有修订语义；无修订文件为 None）。
    pub after_revision: Option<i64>,
    /// 写后内容 SHA-256。
    pub after_content_hash: String,
    pub committed_at: DateTime<Utc>,
}

/// 写前检查点（`.lingbi/checkpoints/{tx_id}.json`）——审计记录；
/// 恢复不依赖它（原子写保证文件全有或全无，哈希证据足以收敛）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookV3Checkpoint {
    pub id: Uuid,
    pub rel_path: String,
    pub after_content_hash: String,
    pub after_revision: Option<i64>,
    pub created_at: DateTime<Utc>,
}

pub fn tx_receipts_dir(root: &Path) -> PathBuf {
    root.join(".lingbi/receipts")
}

pub fn tx_receipt_path(root: &Path, id: Uuid) -> PathBuf {
    tx_receipts_dir(root).join(format!("{id}.json"))
}

pub fn tx_checkpoints_dir(root: &Path) -> PathBuf {
    root.join(".lingbi/checkpoints")
}

pub fn tx_checkpoint_path(root: &Path, id: Uuid) -> PathBuf {
    tx_checkpoints_dir(root).join(format!("{id}.json"))
}

/// serde 序列化 + 原子写（tmp+fsync+rename）——事务层记录统一写入口。
pub fn write_json_atomic(
    store: &impl crate::AtomicFileStore,
    path: &Path,
    value: &impl Serialize,
) -> Result<(), AppError> {
    let bytes = serde_json::to_vec(value).map_err(|e| {
        AppError::new(
            ErrorCode::ProjectCorrupted,
            format!("事务记录序列化失败: {e}"),
            false,
        )
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::new(
                ErrorCode::DiskWriteFailed,
                format!("创建事务层目录失败: {e}"),
                false,
            )
        })?;
    }
    store.write_atomic(path, &bytes, None)?;
    Ok(())
}
