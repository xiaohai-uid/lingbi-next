//! P3 (#20) Book V3 事务门测试 —— 权威写入仅经事务 + external modification 检测。
//!
//! 断言基准：引擎字节路径（legacy 书仓）不受影响；托管书仓（.lingbi/ 存在）
//! 每次权威写入 = checkpoint → WAL 日志(Created) → 期望哈希 CAS 原子写 →
//! ContentWritten → 回执 → Completed → 清理日志。

use lingbi_book::cmd::cmd_commit;
use lingbi_book::tracking::{bump_revision, dump_state, init_state, load_state, new_state, save_state};
use lingbi_book::tx;
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_recovery::book_v3::recover_book_v3;
use lingbi_storage::book_v3::{BookV3Checkpoint, BookV3Receipt};
use lingbi_storage::{
    sha256_hex, AtomicFileStore, DiskAtomicFileStore, DocumentTransactionRepository,
};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn legacy_root() -> (TempDir, PathBuf) {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("书仓");
    std::fs::create_dir_all(&root).expect("create root");
    (temp, root)
}

fn managed_root() -> (TempDir, PathBuf) {
    let (temp, root) = legacy_root();
    tx::ensure_tx_layer(&root).expect("ensure tx layer");
    (temp, root)
}

fn receipts(root: &Path) -> Vec<BookV3Receipt> {
    read_json_dir(&root.join(".lingbi/receipts"))
}

fn checkpoints(root: &Path) -> Vec<BookV3Checkpoint> {
    read_json_dir(&root.join(".lingbi/checkpoints"))
}

fn read_json_dir<T: serde::de::DeserializeOwned>(dir: &Path) -> Vec<T> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("entry");
        if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
            let bytes = std::fs::read(entry.path()).expect("read json");
            out.push(serde_json::from_slice(&bytes).expect("parse json"));
        }
    }
    out
}

fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create dst");
    for entry in std::fs::read_dir(src).expect("read src") {
        let entry = entry.expect("entry");
        let target = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

/// corpus 014-status 的书树（ch1 已结算，ch2 正文已写、未结算）——托管提交集成夹具。
fn corpus_014_book() -> (TempDir, PathBuf) {
    let temp = TempDir::new().expect("temp dir");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/storyrepo-oracle/153eb63/steps/014-status/book");
    let root = temp.path().join("sr-oracle-live");
    copy_tree(&src.canonicalize().expect("canonical corpus"), &root);
    (temp, root)
}

// ---------- Cluster A：事务门 ----------

#[test]
fn legacy_save_state_writes_engine_bytes_without_lingbi() {
    let (_t, root) = legacy_root();
    let mut s = new_state(&root);
    bump_revision(&mut s);
    save_state(&root, &s).expect("save");

    let path = root.join("追踪/_tracking-state.json");
    let on_disk = std::fs::read_to_string(&path).expect("read state");
    assert_eq!(on_disk, dump_state(&s), "legacy 书仓字节路径必须与引擎一致");
    assert!(
        !root.join(".lingbi").exists(),
        "legacy 书仓不得产生 .lingbi/（回放门禁字节锁定）"
    );
}

#[test]
fn ensure_tx_layer_lands_six_dirs_idempotent() {
    let (_t, root) = managed_root();
    for dir in ["transactions", "candidates", "approvals", "receipts", "checkpoints", "versions"] {
        assert!(root.join(".lingbi").join(dir).is_dir(), "缺 .lingbi/{dir}");
    }
    tx::ensure_tx_layer(&root).expect("second ensure is idempotent");
}

#[test]
fn managed_save_state_journals_receipt_checkpoint_and_cleans() {
    let (_t, root) = managed_root();
    let mut s = new_state(&root);
    bump_revision(&mut s);
    save_state(&root, &s).expect("save");

    let path = root.join("追踪/_tracking-state.json");
    let bytes = std::fs::read(&path).expect("read state");
    assert_eq!(bytes, dump_state(&s).as_bytes());

    let rs = receipts(&root);
    assert_eq!(rs.len(), 1, "每次权威写入必须落回执");
    assert_eq!(rs[0].rel_path, "追踪/_tracking-state.json");
    assert_eq!(rs[0].after_revision, Some(2));
    assert_eq!(rs[0].after_content_hash, sha256_hex(&bytes));

    assert_eq!(checkpoints(&root).len(), 1, "每次权威写入必须落 checkpoint");

    let txs = DocumentTransactionRepository::new(&root);
    assert!(txs.list().expect("list").is_empty(), "成功事务日志必须清理");
}

#[test]
fn managed_init_first_write_succeeds() {
    let (_t, root) = managed_root();
    init_state(&root).expect("init");
    let rs = receipts(&root);
    assert_eq!(rs.len(), 1);
    assert_eq!(rs[0].after_revision, Some(1));
}

#[test]
fn external_revision_edit_conflicts_and_preserves_user_bytes() {
    let (_t, root) = managed_root();
    let mut s = new_state(&root);
    bump_revision(&mut s);
    save_state(&root, &s).expect("save rev 2");

    // 外部把修订号改成 9（模拟并发/外部修改）
    let path = root.join("追踪/_tracking-state.json");
    let text = std::fs::read_to_string(&path).expect("read");
    std::fs::write(&path, text.replace("\"state_revision\": 2,", "\"state_revision\": 9,"))
        .expect("external edit");

    let mut s2 = new_state(&root);
    bump_revision(&mut s2);
    let err = save_state(&root, &s2).expect_err("save must conflict");
    assert!(matches!(err, AppError { code: ErrorCode::DocumentConflict, .. }));
    assert!(
        std::fs::read_to_string(&path).expect("read").contains("\"state_revision\": 9"),
        "外部字节必须保留"
    );
}

#[test]
fn external_byte_edit_same_revision_conflicts() {
    let (_t, root) = managed_root();
    let mut s = new_state(&root);
    bump_revision(&mut s);
    save_state(&root, &s).expect("save rev 2");

    // 外部改 book 名（修订号不变）——CAS 必须拦下
    let path = root.join("追踪/_tracking-state.json");
    let text = std::fs::read_to_string(&path).expect("read");
    std::fs::write(&path, text.replace("书仓", "外部改名")).expect("external edit");

    let mut s2 = new_state(&root);
    bump_revision(&mut s2);
    let err = save_state(&root, &s2).expect_err("save must conflict");
    assert!(matches!(err, AppError { code: ErrorCode::DocumentConflict, .. }));
    assert!(std::fs::read_to_string(&path).expect("read").contains("外部改名"));
}

#[test]
fn idempotent_identical_resave_allowed() {
    // 批次 merge 的预刷新保存：load 后原样重存（无 bump）——幂等重写必须放行
    let (_t, root) = managed_root();
    let mut s = new_state(&root);
    bump_revision(&mut s);
    save_state(&root, &s).expect("save rev 2");

    let loaded = load_state(&root).expect("load");
    save_state(&root, &loaded).expect("identical resave allowed");
}

#[test]
fn corrupt_state_file_conflicts() {
    let (_t, root) = managed_root();
    init_state(&root).expect("init");
    std::fs::write(
        root.join("追踪/_tracking-state.json"),
        "{ 这不是 JSON {{{",
    )
    .expect("corrupt");
    let mut s = new_state(&root);
    bump_revision(&mut s);
    let err = save_state(&root, &s).expect_err("corrupt file must conflict");
    assert!(matches!(err, AppError { code: ErrorCode::DocumentConflict, .. }));
}

#[test]
fn append_authoritative_first_write_journals() {
    let (_t, root) = managed_root();
    let receipt = tx::write_authoritative(&root, "文风/金句库/001.md", "金句一\n".as_bytes(), None, None)
        .expect("append");

    let path = root.join("文风/金句库/001.md");
    assert_eq!(std::fs::read(&path).expect("read"), "金句一\n".as_bytes());
    assert_eq!(receipt.after_revision, None, "无修订语义文件不记修订");
    assert_eq!(receipt.after_content_hash, sha256_hex("金句一\n".as_bytes()));

    let rs = receipts(&root);
    assert_eq!(rs.len(), 1);
    assert_eq!(rs[0].rel_path, "文风/金句库/001.md");
    assert!(DocumentTransactionRepository::new(&root)
        .list()
        .expect("list")
        .is_empty());
}

#[test]
fn append_authoritative_cas_conflict_preserves_external_bytes() {
    let (_t, root) = managed_root();
    tx::write_authoritative(&root, "文风/金句库/001.md", b"v1\n", None, None).expect("v1");

    std::fs::write(root.join("文风/金句库/001.md"), "外部字节").expect("external edit");
    let expected = sha256_hex(b"v1\n");
    let err = tx::write_authoritative(
        &root,
        "文风/金句库/001.md",
        b"v1\nv2\n",
        Some(&expected),
        None,
    )
    .expect_err("CAS must conflict");
    assert!(matches!(err, AppError { code: ErrorCode::DocumentConflict, .. }));
    assert_eq!(
        std::fs::read_to_string(root.join("文风/金句库/001.md")).expect("read"),
        "外部字节",
        "外部字节必须保留"
    );
    // 失败留档：日志标记 Failed；回执数不变（v1 成功那次已有一张）
    let txs = DocumentTransactionRepository::new(&root).list().expect("list");
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].phase, lingbi_storage::TransactionPhase::Failed);
    assert_eq!(receipts(&root).len(), 1, "冲突事务不得新增回执");
}

// ---------- 集成：托管提交 ----------

#[test]
fn managed_commit_writes_version_snapshot() {
    let (_t, root) = corpus_014_book();
    tx::ensure_tx_layer(&root).expect("ensure tx layer");
    // 复制树的书名是 sr-oracle-live，ch1 已结算、ch2 未结算
    let s = load_state(&root).expect("load");
    let rev_after = s.state_revision + 1;

    let out = cmd_commit(&root, 2, true).expect("commit ch2");
    assert!(
        out.stdout.contains(&format!("✅ commit ch2 (rev {rev_after})")),
        "{}",
        out.stdout
    );

    // Final Revision 快照：定稿/正文/002-夜访.md 原字节入 versions/（同样经事务门落回执）
    let body = std::fs::read(root.join("定稿/正文/002-夜访.md")).expect("body");
    let snap = root.join(format!(".lingbi/versions/第002章-r{rev_after}.md"));
    assert_eq!(std::fs::read(&snap).expect("snapshot"), body);
    assert!(
        receipts(&root)
            .iter()
            .any(|r| r.rel_path == format!(".lingbi/versions/第002章-r{rev_after}.md")),
        "快照写入必须落回执"
    );
    let txs = DocumentTransactionRepository::new(&root).list().expect("list");
    assert!(txs.is_empty(), "提交事务日志必须清理");
}

#[test]
fn managed_commit_over_edited_state_succeeds() {
    // 提交前的外部编辑（在 load 之前发生）不是竞争——写入基于最新磁盘状态，不得误报
    let (_t, root) = corpus_014_book();
    tx::ensure_tx_layer(&root).expect("ensure tx layer");
    let path = root.join("追踪/_tracking-state.json");
    let text = std::fs::read_to_string(&path).expect("read");
    std::fs::write(&path, text.replace("sr-oracle-live", "外部改名")).expect("external edit");

    let out = cmd_commit(&root, 2, true).expect("commit over edited state");
    assert!(out.stdout.contains("✅ commit ch2"), "{}", out.stdout);
    assert!(
        std::fs::read_to_string(&path)
            .expect("read")
            .contains("外部改名"),
        "编辑内容必须保留在权威状态里"
    );
}

// ---------- 集成：崩溃窗口恢复 ----------

#[test]
fn recovery_forward_completes_content_written_before_journal_phase() {
    // 模拟崩溃点：正文已原子落盘（write_atomic 返回后），但日志仍是 Created
    let (_t, root) = managed_root();
    let mut s = new_state(&root);
    bump_revision(&mut s);
    let json = dump_state(&s).as_bytes().to_vec();

    let store = DiskAtomicFileStore;
    store
        .write_atomic(&root.join("追踪/_tracking-state.json"), &json, None)
        .expect("content write");

    let repo = DocumentTransactionRepository::new(&root);
    let id = uuid::Uuid::new_v4();
    repo.begin(&lingbi_storage::DocumentTransaction {
        id,
        document_id: uuid::Uuid::new_v4(),
        before_revision: 1,
        before_hash: String::new(),
        after_revision: 2,
        after_hash: sha256_hex(&json),
        phase: lingbi_storage::TransactionPhase::Created,
        created_at: chrono::Utc::now(),
        body_relative_path: "追踪/_tracking-state.json".to_owned(),
    })
    .expect("begin");

    let report = recover_book_v3(&root).expect("recover");
    assert_eq!(report.completed, 1, "内容已落盘必须前向补完: {report:?}");
    assert_eq!(report.scanned, 1);

    assert_eq!(
        std::fs::read(root.join("追踪/_tracking-state.json")).expect("read"),
        json,
        "已落盘内容必须保留"
    );
    let rs = receipts(&root);
    assert_eq!(rs.len(), 1, "前向补完必须落回执");
    assert_eq!(rs[0].after_revision, Some(2));
    assert!(DocumentTransactionRepository::new(&root)
        .list()
        .expect("list")
        .is_empty());
}
