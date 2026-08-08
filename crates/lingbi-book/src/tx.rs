//! Book V3 事务门（P3 #20）—— 权威写入的唯一通道。
//!
//! 托管书仓（`root/.lingbi/` 存在）的每次权威写入：
//!   1. checkpoint（写前审计记录，`.lingbi/checkpoints/`）
//!   2. WAL 日志 begin（`DocumentTransaction` phase=Created，`.lingbi/transactions/`，先写日志后 apply）
//!   3. 期望哈希 CAS 原子写（tmp+fsync+rename；磁盘与期望不符 → `DocumentConflict`，外部字节保留）
//!   4. phase=ContentWritten
//!   5. 回执（`.lingbi/receipts/`，after_revision + after_content_hash）
//!   6. phase=Completed → 清理日志
//!
//! 崩溃窗口由 `lingbi-recovery::book_v3::recover_book_v3` 收敛（哈希证据，无回滚）：
//! 内容已落盘 → 前向补完；未生效 → 清理；外部字节 → 标记 Failed 保留。
//!
//! 边界：legacy 书仓（无 `.lingbi/`，spec「旧书仓允许缺席」）走引擎原字节路径
//! （tmp+rename / fs::write）——回放门禁字节锁定，本模块不介入。调用方分支：
//! `is_managed` 为真才进事务门，权威写入不存在第三条路径。

use crate::tracking::{dump_state, load_state, state_path, TrackingState};
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_storage::book_v3::{
    tx_checkpoint_path, tx_receipt_path, write_json_atomic, BookV3Checkpoint, BookV3Receipt,
    TX_LAYER_DIRS,
};
use lingbi_storage::{
    sha256_hex, AtomicFileStore, DiskAtomicFileStore, DocumentTransaction,
    DocumentTransactionRepository, TransactionPhase,
};
use std::path::Path;

/// 权威文件相对书仓根的路径（`/` 分隔，进回执/日志/checkpoint）。
const STATE_REL: &str = "追踪/_tracking-state.json";

/// 事务写回执（调用方可见的最小证据）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxReceipt {
    pub tx_id: uuid::Uuid,
    pub rel_path: String,
    pub after_revision: Option<i64>,
    pub after_content_hash: String,
}

/// 托管书仓判定：`.lingbi/` 事务层存在（spec「旧书仓允许缺席」——缺席即 legacy）。
pub fn is_managed(root: &Path) -> bool {
    root.join(".lingbi").is_dir()
}

/// 落地 `.lingbi/` 六目录（transactions/candidates/approvals/receipts/checkpoints/versions），
/// 幂等。仅托管书仓调用（应用建书/迁移时）；引擎命令路径不调用（legacy 字节锁定）。
pub fn ensure_tx_layer(root: &Path) -> Result<(), AppError> {
    for dir in TX_LAYER_DIRS {
        std::fs::create_dir_all(root.join(".lingbi").join(dir)).map_err(|e| {
            AppError::new(
                ErrorCode::DiskWriteFailed,
                format!("创建 .lingbi/{dir} 失败: {e}"),
                false,
            )
        })?;
    }
    Ok(())
}

/// 追踪状态权威写（托管书仓）——修订守卫 + 幂等重放 + 事务门。
///
/// 修订守卫：磁盘 `state_revision` 必须是 `s.state_revision - 1`（引擎语义：每次
/// 内容变更保存前 bump 恰好一次）；或磁盘与本轮保存逐字节相同（幂等重存，
/// 批次预刷新路径）——否则视为外部修改（并发丢更新），拒绝覆盖并保留用户字节。
pub fn write_authoritative_state(root: &Path, s: &TrackingState) -> Result<TxReceipt, AppError> {
    let json = dump_state(s);
    let expected = state_guard(root, s, &json)?;
    write_authoritative(
        root,
        STATE_REL,
        json.as_bytes(),
        expected.as_deref(),
        Some(s.state_revision),
    )
}

/// 修订守卫：返回 CAS 期望哈希（None = 首次写入，无前置镜像）。
fn state_guard(
    root: &Path,
    s: &TrackingState,
    json: &str,
) -> Result<Option<String>, AppError> {
    let path = state_path(root);
    if !path.exists() {
        return Ok(None);
    }
    let disk = std::fs::read(&path).map_err(|e| {
        AppError::new(
            ErrorCode::DocumentConflict,
            format!("追踪状态读取失败(外部修改?): {e}"),
            false,
        )
    })?;
    let parsed: TrackingState = serde_json::from_slice(&disk).map_err(|_| {
        AppError::new(
            ErrorCode::DocumentConflict,
            "追踪状态已被外部修改/损坏,拒绝覆盖(保留用户字节)".to_owned(),
            false,
        )
    })?;
    if parsed.state_revision == s.state_revision - 1 {
        return Ok(Some(sha256_hex(&disk)));
    }
    // 幂等重存：load 规范化后的磁盘状态与本轮保存逐字节一致（批次预刷新）
    if dump_state(&load_state(root)?) == json {
        return Ok(Some(sha256_hex(&disk)));
    }
    Err(AppError::new(
        ErrorCode::DocumentConflict,
        format!(
            "追踪状态外部修改(revision {} → {}),拒绝覆盖(保留用户字节)",
            parsed.state_revision, s.state_revision
        ),
        false,
    ))
}

/// 权威文件通用事务写（托管书仓）——`rel` 为相对书仓根的 `/` 分隔路径。
/// `expected_hash` = CAS 期望哈希（None = 首次写入）；`after_revision` = 写后修订号
/// （仅追踪状态有修订语义，其余权威文件 None）。
pub fn write_authoritative(
    root: &Path,
    rel: &str,
    bytes: &[u8],
    expected_hash: Option<&str>,
    after_revision: Option<i64>,
) -> Result<TxReceipt, AppError> {
    let store = DiskAtomicFileStore;
    let txs = DocumentTransactionRepository::new(root);
    let tx_id = uuid::Uuid::new_v4();
    let after_hash = sha256_hex(bytes);
    let now = chrono::Utc::now();

    // 1. checkpoint（写前审计；恢复不依赖）
    write_json_atomic(
        &store,
        &tx_checkpoint_path(root, tx_id),
        &BookV3Checkpoint {
            id: tx_id,
            rel_path: rel.to_owned(),
            after_content_hash: after_hash.clone(),
            after_revision,
            created_at: now,
        },
    )?;

    // 2. WAL 日志 begin（先写日志后 apply）
    txs.begin(&DocumentTransaction {
        id: tx_id,
        // Book V3 无 document 索引（章=三位章号、状态=追踪状态），占位 id 仅满足 V2 结构
        document_id: uuid::Uuid::new_v4(),
        // before_revision = after - 1（引擎状态修订恒 ≥1，None→0）；saturating 防 rev-0 回绕
        before_revision: after_revision.map(|r| r.saturating_sub(1) as u64).unwrap_or(0),
        before_hash: expected_hash.unwrap_or("").to_owned(),
        after_revision: after_revision.unwrap_or(0) as u64,
        after_hash: after_hash.clone(),
        phase: TransactionPhase::Created,
        created_at: now,
        body_relative_path: rel.to_owned(),
    })?;

    // 3. CAS 原子写（外部修改在此被拦下，文件保留用户字节）
    if let Err(e) = store.write_atomic(&root.join(rel), bytes, expected_hash) {
        let _ = txs.set_phase(tx_id, TransactionPhase::Failed); // 冲突留档
        return Err(e);
    }

    // 4. 内容已落盘
    txs.set_phase(tx_id, TransactionPhase::ContentWritten)?;

    // 5. 回执（append-only 历史）
    write_json_atomic(
        &store,
        &tx_receipt_path(root, tx_id),
        &BookV3Receipt {
            id: tx_id,
            rel_path: rel.to_owned(),
            after_revision,
            after_content_hash: after_hash.clone(),
            committed_at: chrono::Utc::now(),
        },
    )?;

    // 6. 完成并清理日志
    txs.set_phase(tx_id, TransactionPhase::Completed)?;
    txs.delete(tx_id)?;

    Ok(TxReceipt {
        tx_id,
        rel_path: rel.to_owned(),
        after_revision,
        after_content_hash: after_hash,
    })
}

/// Final Revision 快照（`.lingbi/versions/第NNN章-r{rev}.md`，章节结算时写入）。
/// `.lingbi/**` 也是权威类别（tree 契约），因此同样经事务门：首次写（无前置镜像，
/// CAS 期望为 None），journal + 回执照常，崩溃收敛语义与其它权威写入一致。
pub fn write_version_snapshot(
    root: &Path,
    ch: i64,
    rev: i64,
    body: &[u8],
) -> Result<(), AppError> {
    let rel = format!(".lingbi/versions/第{ch:03}章-r{rev}.md");
    write_authoritative(root, &rel, body, None, Some(rev))?;
    Ok(())
}
