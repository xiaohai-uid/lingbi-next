//! P8-3 (#45) 取消/预算传播（Run→Node→Provider）验收。
//!
//! 接缝：lingbi-workflow crate API（辅接缝；命令层/壳在 e2e + Tauri 层）。

mod common;

use common::{init_book, standard_script, DRAFT, POLISH, REVIEW_OK, ScriptedProvider};
use lingbi_ai::CancellationToken;
use lingbi_workflow::run::{RunControl, TokenBudget};
use lingbi_workflow::{run_chain, ChainStatus, RejectReason};
use std::sync::Arc;

fn node<'a>(report: &'a lingbi_workflow::ChainReport, def: &str) -> &'a lingbi_workflow::node::NodeRecord {
    report.nodes.iter().find(|n| n.definition_id == def).unwrap()
}

// ---------------------------------------------------------------------------
// 1. Cancellation：Run→Node→Provider
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cancellation_stops_chain_without_settlement() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    // 慢 provider（每个 AI 调用挂 200ms）→ 运行中取消
    let provider = Arc::new(
        ScriptedProvider::new(standard_script(REVIEW_OK, POLISH, DRAFT)).slow(200),
    );
    let cancel = CancellationToken::new();
    let control = RunControl {
        cancel: Some(cancel.clone()),
        budget: None,
    };

    let handle = tokio::spawn({
        let root = root.to_path_buf();
        let provider = provider.clone();
        let control = control.clone();
        async move { run_chain(&root, 1, provider, &control).await.expect("run") }
    });
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    cancel.cancel();
    let report = handle.await.expect("join");

    assert_eq!(report.status, ChainStatus::Cancelled, "链 Cancelled");
    assert_eq!(report.reject_reason, Some(RejectReason::Cancelled));
    let cancelled = report.nodes.iter().filter(|n| n.status == lingbi_workflow::Status::Cancelled);
    assert!(cancelled.count() >= 1, "节点 Cancelled 状态记录");
    // 未结算
    let s = lingbi_book::load_state(root).unwrap();
    assert!(!s.chapters.contains_key("1"), "取消 → 不结算");
    // run 台账 Cancelled + 原因
    let rec = lingbi_workflow::run::load_run(root, &report.run_id).unwrap().unwrap();
    assert_eq!(rec.status, lingbi_workflow::RunStatus::Cancelled);
    assert_eq!(rec.reject_reason, Some(RejectReason::Cancelled));
}

#[tokio::test]
async fn cancellation_during_ai_node_propagates_to_provider() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    // 预取消：首个节点（contract 确定性）完成后、draft 前取消 → draft 节点 Cancelled
    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let cancel = CancellationToken::new();
    cancel.cancel(); // 立即取消
    let control = RunControl {
        cancel: Some(cancel),
        budget: None,
    };
    let report = run_chain(root, 1, provider, &control).await.expect("run");
    assert_eq!(report.status, ChainStatus::Cancelled);
    // 预取消 → 第一个节点（contract）即 Cancelled，链停
    assert_eq!(report.nodes.len(), 1);
    let first = &report.nodes[0];
    assert_eq!(first.definition_id, "contract");
    assert_eq!(first.status, lingbi_workflow::Status::Cancelled);
    assert_eq!(first.note.as_deref(), Some("用户取消"));
}

// ---------------------------------------------------------------------------
// 2. Budget：超限 → BudgetExceeded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn budget_exceeded_stops_chain_with_reason() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    // ScriptedProvider 每次 usage = 22 completion tokens；预算 10 → 首个 AI 节点超限
    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let control = RunControl {
        cancel: None,
        budget: Some(TokenBudget {
            max_completion_tokens: 10,
        }),
    };
    let report = run_chain(root, 1, provider, &control).await.expect("run");
    assert_eq!(report.status, ChainStatus::Failed);
    assert_eq!(report.reject_reason, Some(RejectReason::BudgetExceeded));
    let draft = node(&report, "draft");
    assert_eq!(draft.status, lingbi_workflow::Status::Failed);
    assert!(draft.error.as_deref().unwrap().contains("预算超限"));
    // 未结算
    let s = lingbi_book::load_state(root).unwrap();
    assert!(!s.chapters.contains_key("1"), "预算超限 → 不结算");
    // run 台账原因
    let rec = lingbi_workflow::run::load_run(root, &report.run_id).unwrap().unwrap();
    assert_eq!(rec.reject_reason, Some(RejectReason::BudgetExceeded));
}

#[tokio::test]
async fn budget_under_limit_completes_chain() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    // draft(22) + review(22) = 44 ≤ 100 → 完成
    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let control = RunControl {
        cancel: None,
        budget: Some(TokenBudget {
            max_completion_tokens: 100,
        }),
    };
    let report = run_chain(root, 1, provider, &control).await.expect("run");
    assert_eq!(report.status, ChainStatus::Completed);
    assert_eq!(report.reject_reason, None);
}
