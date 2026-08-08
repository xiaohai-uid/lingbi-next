//! P3 (#20) Book V3 崩溃注入恢复矩阵 —— 按崩溃相位手工构造磁盘现场，恢复必须收敛。
//!
//! 收敛语义（基于哈希证据，不依赖任何回滚）：
//! - 内容 == after_hash → 前向补完（补回执 → Completed → 清理日志）
//! - 内容 == before_hash 或文件缺失 → 未生效（清理日志）
//! - 内容为其他字节 → 外部修改（保留用户字节，日志标记 Failed）
//! - IndexUpdated/Completed 残留 → 清理；Failed → 留档不动
//! - 损坏日志 → 记录错误，保留文件

use lingbi_recovery::book_v3::{recover_book_v3, BookV3RecoveryReport};
use lingbi_storage::book_v3::BookV3Receipt;
use lingbi_storage::{
    sha256_hex, AtomicFileStore, DiskAtomicFileStore, DocumentTransaction,
    DocumentTransactionRepository, TransactionPhase,
};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const STATE_REL: &str = "追踪/_tracking-state.json";

fn managed_root() -> (TempDir, PathBuf) {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("书仓");
    std::fs::create_dir_all(&root).expect("create root");
    for dir in ["transactions", "candidates", "approvals", "receipts", "checkpoints", "versions"] {
        std::fs::create_dir_all(root.join(".lingbi").join(dir)).expect("create dir");
    }
    (temp, root)
}

/// 手工写入一条日志（before/after 为内容字符串，空串 = 无前置镜像/无后续哈希）。
fn journal(
    root: &Path,
    phase: TransactionPhase,
    before: &str,
    after: &str,
) -> uuid::Uuid {
    let repo = DocumentTransactionRepository::new(root);
    let id = uuid::Uuid::new_v4();
    repo.begin(&DocumentTransaction {
        id,
        document_id: uuid::Uuid::new_v4(),
        before_revision: 1,
        before_hash: if before.is_empty() {
            String::new()
        } else {
            sha256_hex(before.as_bytes())
        },
        after_revision: 2,
        after_hash: sha256_hex(after.as_bytes()),
        phase: TransactionPhase::Created,
        created_at: chrono::Utc::now(),
        body_relative_path: STATE_REL.to_owned(),
    })
    .expect("begin");
    if phase != TransactionPhase::Created {
        repo.set_phase(id, phase).expect("set phase");
    }
    id
}

fn write_state(root: &Path, content: &str) {
    DiskAtomicFileStore
        .write_atomic(&root.join(STATE_REL), content.as_bytes(), None)
        .expect("write state");
}

fn read_state(root: &Path) -> String {
    std::fs::read_to_string(root.join(STATE_REL)).expect("read state")
}

fn receipts(root: &Path) -> Vec<BookV3Receipt> {
    let dir = root.join(".lingbi/receipts");
    if !dir.exists() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("read dir") {
        let entry = entry.expect("entry");
        if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
            let bytes = std::fs::read(entry.path()).expect("read");
            out.push(serde_json::from_slice(&bytes).expect("parse"));
        }
    }
    out
}

fn tx_files(root: &Path) -> Vec<PathBuf> {
    let dir = root.join(".lingbi/transactions");
    if !dir.exists() {
        return Vec::new();
    }
    std::fs::read_dir(&dir)
        .expect("read dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect()
}

// ---------- 崩溃矩阵 ----------

#[test]
fn crash_after_journal_before_content_cleaned() {
    // 崩溃点：日志已写、正文未写（内容仍为写前字节）
    let (_t, root) = managed_root();
    write_state(&root, "旧内容");
    journal(&root, TransactionPhase::Created, "旧内容", "新内容");

    let report = recover_book_v3(&root).expect("recover");
    assert_eq!(report.completed, 0);
    assert_eq!(report.cleaned, 1, "{report:?}");
    assert_eq!(read_state(&root), "旧内容", "未生效不得改动正文");
    assert!(tx_files(&root).is_empty());
    assert!(receipts(&root).is_empty(), "未生效事务不得落回执");
}

#[test]
fn crash_after_content_write_before_phase_forward_completed() {
    // 崩溃点：正文已落盘（write_atomic 返回），日志仍 Created → 前向补完
    let (_t, root) = managed_root();
    write_state(&root, "旧内容");
    write_state(&root, "新内容");
    journal(&root, TransactionPhase::Created, "旧内容", "新内容");

    let report = recover_book_v3(&root).expect("recover");
    assert_eq!(report.completed, 1, "{report:?}");
    assert_eq!(read_state(&root), "新内容", "已落盘内容必须保留");
    assert_eq!(receipts(&root).len(), 1, "前向补完必须落回执");
    assert!(tx_files(&root).is_empty());
}

#[test]
fn crash_after_phase_before_receipt_forward_completed() {
    // 崩溃点：日志 ContentWritten、回执未写 → 前向补完
    let (_t, root) = managed_root();
    write_state(&root, "旧内容");
    write_state(&root, "新内容");
    journal(&root, TransactionPhase::ContentWritten, "旧内容", "新内容");

    let report = recover_book_v3(&root).expect("recover");
    assert_eq!(report.completed, 1, "{report:?}");
    assert_eq!(receipts(&root).len(), 1);
    assert!(tx_files(&root).is_empty());
}

#[test]
fn crash_window_external_bytes_marked_failed_and_preserved() {
    // 崩溃窗口内被外部写入（既非 before 也非 after）→ 保留用户字节，日志标记 Failed
    let (_t, root) = managed_root();
    write_state(&root, "旧内容");
    write_state(&root, "外部注入字节");
    journal(&root, TransactionPhase::ContentWritten, "旧内容", "新内容");

    let report = recover_book_v3(&root).expect("recover");
    assert_eq!(report.failed, 1, "{report:?}");
    assert_eq!(read_state(&root), "外部注入字节", "外部字节必须保留");
    assert_eq!(tx_files(&root).len(), 1, "Failed 日志留档");
    let repo = DocumentTransactionRepository::new(&root);
    assert_eq!(
        repo.list().expect("list")[0].phase,
        TransactionPhase::Failed
    );
    assert!(receipts(&root).is_empty(), "冲突事务不得落回执");
}

#[test]
fn completed_leftover_cleaned() {
    // 崩溃点：事务已完成但日志文件未清理
    let (_t, root) = managed_root();
    write_state(&root, "旧内容");
    write_state(&root, "新内容");
    journal(&root, TransactionPhase::Completed, "旧内容", "新内容");

    let report = recover_book_v3(&root).expect("recover");
    assert_eq!(report.cleaned, 1, "{report:?}");
    assert!(tx_files(&root).is_empty());
    assert_eq!(read_state(&root), "新内容");
}

#[test]
fn index_updated_leftover_cleaned() {
    let (_t, root) = managed_root();
    write_state(&root, "旧内容");
    write_state(&root, "新内容");
    journal(&root, TransactionPhase::IndexUpdated, "旧内容", "新内容");

    let report = recover_book_v3(&root).expect("recover");
    assert_eq!(report.cleaned, 1, "{report:?}");
    assert!(tx_files(&root).is_empty());
}

#[test]
fn failed_phase_left_untouched() {
    let (_t, root) = managed_root();
    write_state(&root, "旧内容");
    journal(&root, TransactionPhase::Failed, "旧内容", "新内容");

    let report = recover_book_v3(&root).expect("recover");
    assert_eq!(report.scanned, 1, "{report:?}");
    assert_eq!(report.cleaned, 0);
    assert_eq!(report.completed, 0);
    assert_eq!(tx_files(&root).len(), 1, "Failed 留档不动");
}

#[test]
fn first_write_missing_target_cleaned() {
    // 首次写入（before_hash 空）崩溃：目标文件不存在 → 未生效
    let (_t, root) = managed_root();
    journal(&root, TransactionPhase::Created, "", "新内容");

    let report = recover_book_v3(&root).expect("recover");
    assert_eq!(report.cleaned, 1, "{report:?}");
    assert!(!root.join(STATE_REL).exists(), "未生效不得创建文件");
    assert!(tx_files(&root).is_empty());
}

#[test]
fn unparseable_journal_recorded_and_kept() {
    let (_t, root) = managed_root();
    let path = root.join(".lingbi/transactions/坏文件.json");
    std::fs::write(&path, "{ 不是 JSON").expect("write garbage");

    let report = recover_book_v3(&root).expect("recover");
    assert_eq!(report.errors.len(), 1, "{report:?}");
    assert!(path.exists(), "损坏日志保留供审计");
}

#[test]
fn target_io_error_recorded_and_journal_kept() {
    // 目标不是文件（目录占位）→ 真实 IO 错误：记 errors、跳过、日志保留，不臆断外部修改
    let (_t, root) = managed_root();
    let dir = root.join(STATE_REL);
    std::fs::create_dir_all(&dir).expect("create dir at target path");
    journal(&root, TransactionPhase::ContentWritten, "旧内容", "新内容");

    let report = recover_book_v3(&root).expect("recover");
    assert_eq!(report.errors.len(), 1, "{report:?}");
    assert_eq!(tx_files(&root).len(), 1, "无法判定的事务保留日志,下轮再试");
}

#[test]
fn recovery_is_idempotent() {
    let (_t, root) = managed_root();
    write_state(&root, "旧内容");
    write_state(&root, "新内容");
    journal(&root, TransactionPhase::Created, "旧内容", "新内容");

    let first = recover_book_v3(&root).expect("first");
    assert_eq!(first.completed, 1);
    let second = recover_book_v3(&root).expect("second");
    assert_eq!(second.scanned, 0, "二次恢复必须无操作: {second:?}");
    assert_eq!(second.completed, 0);
}

#[test]
fn no_lingbi_is_noop() {
    let (_t, root) = legacy_root_like();
    let report = recover_book_v3(&root).expect("recover");
    assert_eq!(report, BookV3RecoveryReport::default(), "{report:?}");
}

fn legacy_root_like() -> (TempDir, PathBuf) {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("旧书仓");
    std::fs::create_dir_all(&root).expect("create root");
    (temp, root)
}
