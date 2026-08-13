//! P8-1 (#43) Run 持久化 + 副作用幂等 + Node Recovery 验收。
//!
//! 接缝：lingbi-workflow crate API（辅接缝；命令层/e2e 在 #45）。

mod common;

use common::{init_book, standard_script, DRAFT, POLISH, REVIEW_OK, ScriptedProvider};
use lingbi_workflow::{run_chain, ChainStatus, RejectReason, RunRecord, RunStatus};
use std::sync::Arc;

fn node<'a>(report: &'a lingbi_workflow::ChainReport, def: &str) -> &'a lingbi_workflow::node::NodeRecord {
    report.nodes.iter().find(|n| n.definition_id == def).unwrap()
}

/// failpoint 经进程级 env 注入——本二进制内所有测试串行（避免并发泄漏）。
static CHAIN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// ---------------------------------------------------------------------------
// 1. Run 台账：唯一 run_id + Running → 终态原子改写 + current_node 进度
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_ledger_persisted_with_terminal_status() {
    let _guard = CHAIN_LOCK.lock().unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let report = run_chain(root, 1, provider, &Default::default()).await.expect("run");
    assert_eq!(report.status, ChainStatus::Completed);
    assert!(!report.run_id.is_empty());

    let rec: RunRecord = serde_json::from_str(
        &std::fs::read_to_string(root.join("工作区/工作流/runs").join(format!("{}.json", report.run_id)))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(rec.run_id, report.run_id);
    assert_eq!(rec.workflow_version, lingbi_workflow::CHAIN_VERSION);
    assert_eq!(rec.status, RunStatus::Completed);
    assert_eq!(rec.current_node, "handoff", "终态 current_node = 最后执行节点");
    assert!(!rec.created_at.is_empty() && !rec.updated_at.is_empty());

    // list_runs 可见
    let runs = lingbi_workflow::run::list_runs(root);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].run_id, report.run_id);
}

#[tokio::test]
async fn run_id_unique_per_run() {
    let _guard = CHAIN_LOCK.lock().unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    async fn run(root: &std::path::Path) -> lingbi_workflow::ChainReport {
        let p = Arc::new(ScriptedProvider::new(standard_script(
            REVIEW_OK, POLISH, DRAFT,
        )));
        run_chain(root, 1, p, &Default::default()).await.expect("run")
    }
    let r1 = run(root).await;
    let r2 = run(root).await;
    assert_ne!(r1.run_id, r2.run_id);
    let runs = lingbi_workflow::run::list_runs(root);
    assert_eq!(runs.len(), 2, "每次运行一条台账");
}

#[tokio::test]
async fn halted_chain_records_run_status_and_reject_reason() {
    let _guard = CHAIN_LOCK.lock().unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", None); // 摘要空 → 机检 halt

    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let report = run_chain(root, 1, provider, &Default::default()).await.expect("run");
    assert_eq!(report.status, ChainStatus::Halted);
    assert_eq!(report.reject_reason, Some(RejectReason::MachineCheckFailed));

    let rec = lingbi_workflow::run::load_run(root, &report.run_id)
        .unwrap()
        .expect("台账存在");
    assert_eq!(rec.status, RunStatus::Halted);
    assert_eq!(rec.reject_reason, Some(RejectReason::MachineCheckFailed));
}

// ---------------------------------------------------------------------------
// 2. 副作用幂等：崩溃恢复不重复执行（注入 failpoint）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn crash_recovery_does_not_double_commit() {
    let _guard = CHAIN_LOCK.lock().unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路."));

    // 第一次：结算窗口注入崩溃（正文已事务落盘、cmd_commit 未执行）
    std::env::set_var("LINGBI_FAILPOINT", "settlement-after-body");
    let p1 = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let r1 = run_chain(root, 1, p1.clone(), &Default::default()).await.expect("run1");
    std::env::remove_var("LINGBI_FAILPOINT");
    assert_eq!(r1.status, ChainStatus::Failed);
    let settlement1 = node(&r1, "settlement");
    assert!(settlement1.error.as_deref().unwrap().contains("failpoint"));
    // 正文已落盘但未结算（追踪无该章、修订未 bump）
    let s1 = lingbi_book::load_state(root).unwrap();
    assert!(!s1.chapters.contains_key("1"), "结算未完成");
    assert_eq!(s1.state_revision, 1, "修订未 bump");
    let body1 = std::fs::read(root.join("定稿/正文/001-第一章.md")).unwrap();
    assert!(String::from_utf8_lossy(&body1).contains(DRAFT), "正文已落盘");

    // 第二次：修复重跑 → CAS 幂等重写 + cmd_commit 单次提交
    let p2 = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let r2 = run_chain(root, 1, p2, &Default::default()).await.expect("run2");
    assert_eq!(r2.status, ChainStatus::Completed);
    let s2 = lingbi_book::load_state(root).unwrap();
    assert!(s2.chapters.contains_key("1"));
    assert_eq!(s2.state_revision, 2, "修订只 bump 一次(2 = init 1 + 单次提交)");
    // 时间线无重复（fact 事件只合并一次）
    let body2 = std::fs::read(root.join("定稿/正文/001-第一章.md")).unwrap();
    assert_eq!(body1, body2, "正文字节一致(幂等重写)");
}

#[tokio::test]
async fn side_effect_nodes_carry_idempotency_key() {
    let _guard = CHAIN_LOCK.lock().unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let report = run_chain(root, 1, provider, &Default::default()).await.expect("run");
    for def in ["draft", "settlement"] {
        let n = node(&report, def);
        assert_eq!(n.idempotency_key.as_deref(), Some(n.node_id.as_str()), "{def} 幂等键 = node_id");
    }
    for def in ["contract", "review"] {
        assert_eq!(node(&report, def).idempotency_key, None, "{def} 非副作用无幂等键");
    }
}

// ---------------------------------------------------------------------------
// 3. Node Recovery：input 指纹未变 Completed 可 skip（失败重跑复用）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn node_recovery_reuses_unchanged_completed_nodes() {
    let _guard = CHAIN_LOCK.lock().unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let failing = Arc::new(
        ScriptedProvider::new(standard_script(REVIEW_OK, POLISH, DRAFT)).fail_on("资深网络小说作家"),
    );
    let r1 = run_chain(root, 1, failing, &Default::default()).await.expect("run1");
    assert_eq!(r1.status, ChainStatus::Failed);

    let good = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let r2 = run_chain(root, 1, good.clone(), &Default::default()).await.expect("run2");
    assert_eq!(r2.status, ChainStatus::Completed);
    for def in ["contract", "brief"] {
        let a = node(&r1, def);
        let b = node(&r2, def);
        assert_eq!(a.node_id, b.node_id, "{def} 复用（input 未变 Completed 可 skip）");
    }
    assert_eq!(good.calls(), 2, "复用后仅 draft+review 各一次");
}

// ---------------------------------------------------------------------------
// 4. 棘轮拒绝带类型化原因
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ratchet_rejection_records_reason() {
    let _guard = CHAIN_LOCK.lock().unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let p1 = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let r1 = run_chain(root, 1, p1, &Default::default()).await.expect("run1");
    assert_eq!(r1.status, ChainStatus::Completed);

    let p2 = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let r2 = run_chain(root, 1, p2, &Default::default()).await.expect("run2");
    assert_eq!(r2.status, ChainStatus::Failed);
    assert_eq!(r2.reject_reason, Some(RejectReason::RatchetRejected));
    let rec = lingbi_workflow::run::load_run(root, &r2.run_id).unwrap().unwrap();
    assert_eq!(rec.reject_reason, Some(RejectReason::RatchetRejected));
}
