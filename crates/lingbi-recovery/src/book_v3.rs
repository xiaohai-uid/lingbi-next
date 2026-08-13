//! Book V3 事务日志恢复（P3 #20）—— 崩溃后收敛 `.lingbi/transactions/`。
//!
//! 不依赖回滚：权威写入本身是原子写（tmp+fsync+rename），文件全有或全无；
//! 因此收敛只按哈希证据判定：
//! - 目标内容 == `after_hash` → 内容已落盘 → 前向补完（补回执 → Completed → 清理日志）
//! - 目标内容 == `before_hash`（或 before 为空且文件缺失）→ 事务未生效 → 清理日志
//! - 目标内容为其他字节 → 崩溃窗口内外部修改 → 保留用户字节，日志标记 Failed 留档
//! - IndexUpdated/Completed 残留 → 已完成 → 清理日志；Failed → 留档不动
//! - 日志损坏 → 记入 errors，保留文件（不销毁证据）
//!
//! 幂等：恢复后无残留日志，二次调用为无操作。回执/checkpoint 为 append-only 历史，从不清理。

use chrono::Utc;
use lingbi_contracts::AppError;
use lingbi_storage::book_v3::{tx_receipt_path, write_json_atomic, BookV3Receipt};
use lingbi_storage::{
    sha256_hex, DiskAtomicFileStore, DocumentTransaction, DocumentTransactionRepository,
    TransactionPhase,
};
use std::path::Path;

/// 恢复报告（测试与审计消费）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BookV3RecoveryReport {
    /// 扫描到的日志条数。
    pub scanned: usize,
    /// 前向补完（内容已落盘，补回执并完成）。
    pub completed: usize,
    /// 清理（未生效/已完成残留）。
    pub cleaned: usize,
    /// 外部修改冲突（保留用户字节，日志标记 Failed）。
    pub failed: usize,
    /// 损坏日志（保留文件，记录错误）。
    pub errors: Vec<String>,
}

/// 收敛书仓 `.lingbi/transactions/` 全部未决事务（幂等，可在每次打开时调用）。
pub fn recover_book_v3(root: &Path) -> Result<BookV3RecoveryReport, AppError> {
    let dir = root.join(".lingbi/transactions");
    if !dir.exists() {
        return Ok(BookV3RecoveryReport::default());
    }
    let repo = DocumentTransactionRepository::new(root);
    let store = DiskAtomicFileStore;
    let mut report = BookV3RecoveryReport::default();

    for entry in std::fs::read_dir(&dir).map_err(|e| {
        AppError::new(
            lingbi_contracts::ErrorCode::ProjectCorrupted,
            format!("读取事务日志目录失败: {e}"),
            false,
        )
    })? {
        let entry = entry.map_err(|e| {
            AppError::new(
                lingbi_contracts::ErrorCode::ProjectCorrupted,
                format!("读取事务日志条目失败: {e}"),
                false,
            )
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        report.scanned += 1;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                report
                    .errors
                    .push(format!("日志读取失败({}): {e}", path.display()));
                continue;
            }
        };
        let tx: DocumentTransaction = match serde_json::from_slice(&bytes) {
            Ok(tx) => tx,
            Err(e) => {
                report
                    .errors
                    .push(format!("日志损坏({}): {e}", path.display()));
                continue; // 保留文件供审计
            }
        };
        let target = root.join(&tx.body_relative_path);
        // 读失败区分：NotFound = 目标缺失（未生效语义）；其它 IO 错误 = 无法判定 → 记入
        // errors 并跳过（日志保留，下次恢复再试），不臆断为外部修改。
        let current = match std::fs::read(&target) {
            Ok(bytes) => Some(sha256_hex(&bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                report.errors.push(format!(
                    "目标读取失败({}): {e}",
                    target.display()
                ));
                continue;
            }
        };
        match tx.phase {
            TransactionPhase::Created | TransactionPhase::ContentWritten => {
                if current.as_deref() == Some(tx.after_hash.as_str()) {
                    // 内容已原子落盘 → 前向补完
                    write_json_atomic(
                        &store,
                        &tx_receipt_path(root, tx.id),
                        &BookV3Receipt {
                            id: tx.id,
                            rel_path: tx.body_relative_path.clone(),
                            after_revision: (tx.after_revision != 0)
                                .then_some(tx.after_revision as i64),
                            after_content_hash: tx.after_hash.clone(),
                            committed_at: Utc::now(),
                        },
                    )?;
                    repo.set_phase(tx.id, TransactionPhase::Completed)?;
                    repo.delete(tx.id)?;
                    report.completed += 1;
                } else if current.as_deref() == empty_if_absent(&tx.before_hash) {
                    // 未生效（仍是写前字节 / 首次写入文件缺失）→ 清理
                    repo.delete(tx.id)?;
                    report.cleaned += 1;
                } else {
                    // 崩溃窗口内外部修改 → 保留用户字节，Failed 留档
                    repo.set_phase(tx.id, TransactionPhase::Failed)?;
                    report.failed += 1;
                }
            }
            TransactionPhase::IndexUpdated | TransactionPhase::Completed => {
                // 已完成残留 → 清理
                repo.delete(tx.id)?;
                report.cleaned += 1;
            }
            TransactionPhase::Failed => {}
        }
    }
    Ok(report)
}

/// before_hash 为空（首次写入，无前置镜像）时与「文件缺失」等价。
fn empty_if_absent(hash: &str) -> Option<&str> {
    if hash.is_empty() {
        None
    } else {
        Some(hash)
    }
}
