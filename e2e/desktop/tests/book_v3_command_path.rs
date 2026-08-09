//! P4 桌面 E2E：Book V3 命令路径全链路（#21 验收③）。
//!
//! 在真实书仓（临时目录 + 追踪状态 + book.yaml + 正文）上走完整命令路径：
//!
//! ```text
//! open_book → create_chapter → open_chapter → save_chapter
//!   → build_contract → build_brief → run_check
//!   → create_candidate → adopt_candidate（正文落盘 + 结算）
//!   → get_run_ledger → resume_workflow
//! ```
//!
//! 命令层（lingbi-commands）即生产 Tauri 命令背后的同一实现——Tauri 壳只
//! 做会话/错误映射（见 apps/desktop/src-tauri/src/book_cmds.rs），因此本条
//! 路径 = 桌面 UI 将实际调用的命令路径。

use lingbi_commands::*;
use std::path::Path;

fn init_book(dir: &Path) {
    lingbi_book::tracking::init_state(dir).expect("init state");
    std::fs::write(
        dir.join("book.yaml"),
        "---\nname: E2E 书\ntotal_chapters: 0\nmin_words_per_chapter: 100\nmax_words_per_chapter: 300\ntarget_words: 100000\n---\n",
    )
    .expect("book.yaml");
}

#[test]
fn book_v3_create_book_command_path() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let root = dir.path().join("新书").to_string_lossy().into_owned();
    // 建书 → 建章 → 存章 → 重启（重开）→ 正文完整
    let session = create_book(&root, "新书").expect("create_book");
    assert!(session.managed);
    assert_eq!(session.name, "新书");
    assert_eq!(session.state_revision, 1);

    let created = create_chapter(&root, "第一章").expect("create_chapter");
    assert_eq!(created.ch, 1);
    let opened = open_chapter(&root, 1).expect("open_chapter");
    save_chapter(&root, 1, "新书第一章正文", Some(opened.content_hash)).expect("save");

    // 重开（模拟重启）：内容完整、追踪修订不变
    let reopened = open_book(&root).expect("reopen");
    assert!(reopened.managed);
    assert_eq!(reopened.state_revision, 1);
    let after = open_chapter(&root, 1).expect("after reopen");
    assert!(after.body.contains("新书第一章正文"));
}

#[test]
fn book_v3_command_path_full_chain() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let root = dir.path().to_string_lossy().into_owned();
    init_book(dir.path());

    // 1. open_book（legacy 书仓，无 .lingbi）
    let session = open_book(&root).expect("open_book");
    assert!(!session.managed);
    assert_eq!(session.state_revision, 1);
    assert_eq!(session.chapter_count, 0);

    // 2. create_chapter → open_chapter
    let created = create_chapter(&root, "第一章").expect("create_chapter");
    assert_eq!(created.ch, 1);
    let opened = open_chapter(&root, 1).expect("open_chapter");
    assert_eq!(opened.title, "第一章");

    // 3. save_chapter（CAS 期望 = open 的哈希）
    let expected = opened.content_hash.clone();
    let body = "第一章的正文。\n\n阿米穿过长廊,推开了尽头那扇门。";
    let receipt = save_chapter(&root, 1, body, Some(expected)).expect("save_chapter");
    assert!(receipt.after_content_hash.len() == 64);
    let saved = open_chapter(&root, 1).expect("reopen");
    assert!(saved.body.contains("阿米穿过长廊"));

    // 4. build_contract / build_brief / run_check
    let contract = build_contract(&root, 1).expect("build_contract");
    assert!(contract.contains("\"goal\""));
    let brief = build_brief(&root, 1).expect("build_brief");
    assert!(brief.contains("## "));
    let checks = run_check(&root, None).expect("run_check");
    assert_eq!(checks.len(), 11);

    // 5. create_candidate → adopt_candidate（正文落盘 + 追踪结算）
    let draft = "第二章候选:深夜,油灯下,阿米摊开了地图。";
    let candidate = create_candidate(&root, 1, draft).expect("create_candidate");
    assert_eq!(candidate.ch, 1);
    let adopted = adopt_candidate(&root, &candidate.id).expect("adopt_candidate");
    assert_eq!(adopted.exit, 0, "stdout: {}", adopted.stdout);
    let after = open_chapter(&root, 1).expect("after adopt");
    assert!(after.body.contains("油灯下"));
    let tracking = get_tracking_state(&root).expect("tracking_state");
    assert_eq!(tracking.state_revision, 2); // init rev1 + commit bump
    assert!(tracking.chapters.contains_key("1"));

    // 6. get_run_ledger / resume_workflow
    // P5-4：legacy 书仓首次写入即托管化——台账含正文事务回执
    let ledger = get_run_ledger(&root).expect("run_ledger");
    assert!(!ledger.receipts.is_empty(), "first save adopts tx layer: {ledger:?}");
    assert!(ledger
        .receipts
        .iter()
        .any(|r| r["rel_path"] == "定稿/正文/001-第一章.md"));
    let resume = resume_workflow(&root).expect("resume_workflow");
    assert!(resume.contains("\"status\""));

    // 7. get_outline（无细纲蓝图 → 空表）
    let outline = get_outline(&root).expect("get_outline");
    assert!(outline.is_empty());
}

#[test]
fn book_v3_command_path_managed_transaction_layer() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let root = dir.path().to_string_lossy().into_owned();
    init_book(dir.path());
    // 托管书仓（.lingbi/ 事务层）
    lingbi_book::tx::ensure_tx_layer(dir.path()).expect("ensure_tx_layer");

    let session = open_book(&root).expect("open_book");
    assert!(session.managed);
    let created = create_chapter(&root, "第一章").expect("create_chapter");
    let opened = open_chapter(&root, 1).expect("open_chapter");
    let expected = opened.content_hash.clone();
    save_chapter(&root, 1, "托管书仓正文", Some(expected)).expect("save_chapter");
    assert_eq!(created.ch, 1);

    // 事务层留下写前 checkpoint + 提交回执（append-only 历史）
    let ledger = get_run_ledger(&root).expect("run_ledger");
    assert_eq!(ledger.checkpoints.len(), 2); // 建章 + 存章
    assert_eq!(ledger.receipts.len(), 2);
    assert!(ledger
        .receipts
        .iter()
        .any(|r| r["rel_path"] == "定稿/正文/001-第一章.md"));

    // 外部修改检测：open 的哈希已过期 → 存盘被拒、用户字节保留
    std::fs::write(
        dir.path().join("定稿/正文/001-第一章.md"),
        "外部写入\n",
    )
    .expect("external write");
    let err = save_chapter(&root, 1, "新正文", Some(opened.content_hash)).expect_err("conflict");
    assert_eq!(err.code, "DocumentConflict");
    let disk =
        std::fs::read_to_string(dir.path().join("定稿/正文/001-第一章.md")).expect("read");
    assert!(disk.contains("外部写入"));
}
