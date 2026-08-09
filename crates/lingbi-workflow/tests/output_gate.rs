//! P8-2 (#44) TokenLedger 四级记账 + OutputGate + RejectReason 验收。
//!
//! 接缝：lingbi-workflow crate API（辅接缝；命令层/e2e 在 #45）。

mod common;

use common::{init_book, standard_script, DRAFT, POLISH, REVIEW_BLOCKING, REVIEW_OK, ScriptedProvider};
use lingbi_workflow::{run_chain, ChainStatus, RejectReason};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// 状态化脚本：第 N 次评审返回 blocking、之后返回 clean（Validate→Repair→Validate）。
fn stateful_review_script(
    first_n_blocking: usize,
) -> impl Fn(&lingbi_ai::ChatRequest) -> String {
    let review_calls = Arc::new(AtomicUsize::new(0));
    move |req: &lingbi_ai::ChatRequest| {
        let user = req
            .messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        if user.contains("改写") {
            POLISH.to_owned()
        } else if user.contains("评审") {
            let n = review_calls.fetch_add(1, Ordering::SeqCst);
            if n < first_n_blocking {
                REVIEW_BLOCKING.to_owned()
            } else {
                REVIEW_OK.to_owned()
            }
        } else {
            DRAFT.to_owned()
        }
    }
}

// ---------------------------------------------------------------------------
// 1. OutputGate：Validate→Repair→Validate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gate_repair_loop_settles_after_clean_review() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let provider = Arc::new(ScriptedProvider::new(stateful_review_script(1)));
    let report = run_chain(root, 1, provider.clone(), &Default::default()).await.expect("run");
    assert_eq!(report.status, ChainStatus::Completed);
    assert_eq!(report.repair_attempts, 1);
    // 复评证据：评审执行了 2 次（draft 1 次 + review 2 次 + polish 改写 1 次）
    assert_eq!(provider.calls(), 4, "draft+评审x2+改写 = 4 次 AI 调用");
    // 定稿 = 润色改写文本
    let settled = std::fs::read_to_string(root.join("定稿/正文/001-第一章.md")).unwrap();
    assert!(settled.contains(POLISH));
    assert!(!settled.contains(DRAFT));
}

#[tokio::test]
async fn persistent_blocking_rejected_after_max_attempts() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    // 评审永远 blocking → 2 次修复后 Rejected
    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_BLOCKING, POLISH, DRAFT,
    )));
    let report = run_chain(root, 1, provider.clone(), &Default::default()).await.expect("run");
    assert_eq!(report.status, ChainStatus::Rejected);
    assert_eq!(report.reject_reason, Some(RejectReason::BlockingIssuesRemain));
    assert_eq!(report.repair_attempts, 2, "max_repair_attempts=2 生效");
    // 评审执行 3 次（首轮 + 2 次复评）、改写 2 次
    assert_eq!(provider.calls(), 5, "draft+评审x3+改写x2");
    // 不结算：定稿正文未被覆盖、追踪无该章
    let settled = std::fs::read_to_string(root.join("定稿/正文/001-第一章.md")).unwrap();
    assert!(!settled.contains(POLISH) && !settled.contains(DRAFT), "定稿未变");
    let s = lingbi_book::load_state(root).unwrap();
    assert!(!s.chapters.contains_key("1"), "未结算");
    // Run 台账 Rejected + 原因
    let rec = lingbi_workflow::run::load_run(root, &report.run_id).unwrap().unwrap();
    assert_eq!(rec.status, lingbi_workflow::RunStatus::Rejected);
    assert_eq!(rec.reject_reason, Some(RejectReason::BlockingIssuesRemain));
    assert_eq!(rec.repair_attempts, 2);
}

#[tokio::test]
async fn gate_clean_review_no_repair() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let report = run_chain(root, 1, provider.clone(), &Default::default()).await.expect("run");
    assert_eq!(report.status, ChainStatus::Completed);
    assert_eq!(report.repair_attempts, 0);
    assert_eq!(report.reject_reason, None);
    assert_eq!(provider.calls(), 2, "draft+评审（无修复）");
}

// ---------------------------------------------------------------------------
// 2. TokenLedger：四级可查
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ledger_records_ai_nodes_and_four_level_query() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let report = run_chain(root, 1, provider, &Default::default()).await.expect("run");
    assert_eq!(report.status, ChainStatus::Completed);

    // draft + review 两行（polish 透传无 AI 消耗不记账）
    let all = lingbi_workflow::ledger::query_ledger(root, None, None, None, None);
    assert_eq!(all.len(), 2, "draft+review 各一行");
    assert!(all.iter().all(|e| e.run_id == report.run_id));

    // 四级过滤
    let by_run = lingbi_workflow::ledger::query_ledger(root, Some(&report.run_id), None, None, None);
    assert_eq!(by_run.len(), 2);
    let by_node = lingbi_workflow::ledger::query_ledger(root, None, Some("draft"), None, None);
    assert_eq!(by_node.len(), 1);
    assert_eq!(by_node[0].definition_id, "draft");
    assert_eq!(by_node[0].completion_tokens, 22);
    let by_provider = lingbi_workflow::ledger::query_ledger(root, None, None, Some("scripted"), None);
    assert_eq!(by_provider.len(), 2);
    let by_model = lingbi_workflow::ledger::query_ledger(root, None, None, None, Some("scripted-model"));
    assert_eq!(by_model.len(), 2);
    // 组合过滤
    let combo = lingbi_workflow::ledger::query_ledger(
        root,
        Some(&report.run_id),
        Some("review"),
        Some("scripted"),
        Some("scripted-model"),
    );
    assert_eq!(combo.len(), 1);
    assert_eq!(combo[0].definition_id, "review");
}

#[tokio::test]
async fn ledger_appends_on_repair_and_rerun() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    // 修复循环：draft + review(1) + polish 改写 + review(2) = 4 行
    let p1 = Arc::new(ScriptedProvider::new(stateful_review_script(1)));
    let r1 = run_chain(root, 1, p1, &Default::default()).await.expect("run1");
    assert_eq!(r1.status, ChainStatus::Completed);
    let rows1 = lingbi_workflow::ledger::query_ledger(root, Some(&r1.run_id), None, None, None);
    assert_eq!(rows1.len(), 4, "draft+评审x2+改写");
    assert_eq!(rows1.iter().filter(|e| e.definition_id == "review").count(), 2, "复评追加行");

    // 复评/重跑追加（诚实记账）：重跑（settlement 棘轮失败前 draft/review 重跑）
    let p2 = Arc::new(ScriptedProvider::new(stateful_review_script(1)));
    let r2 = run_chain(root, 1, p2, &Default::default()).await.expect("run2");
    let rows2 = lingbi_workflow::ledger::query_ledger(root, Some(&r2.run_id), None, None, None);
    assert!(!rows2.is_empty(), "重跑独立记账");
    let total = lingbi_workflow::ledger::query_ledger(root, None, None, None, None);
    assert_eq!(total.len(), rows1.len() + rows2.len(), "台账只增不改");
}
