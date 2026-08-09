//! P7-1 (#40) 章事务引擎验收：8 节点全链 / 哈希确定性 / checkpoint 可续 /
//! 上游失效 / 模型切换失效 / 机检 halt / 评审阻塞润色 / 棘轮 / legacy。
//!
//! 接缝：lingbi-workflow crate API（辅接缝）；主接缝（命令层 + e2e）在 #41。

mod common;

use common::{init_book, standard_script, DRAFT, POLISH, REVIEW_BLOCKING, REVIEW_OK, ScriptedProvider};
use lingbi_workflow::{run_chain, ChainStatus, Status, CHAIN_DEFINITIONS};
use std::sync::Arc;

fn completed(report: &lingbi_workflow::ChainReport) {
    assert_eq!(report.status, ChainStatus::Completed, "status");
    assert_eq!(report.nodes.len(), 8, "8 节点");
    for n in &report.nodes {
        assert_eq!(format!("{:?}", n.status), "Completed", "{} 完成", n.definition_id);
    }
    // 节点按定义序
    let order: Vec<&str> = report.nodes.iter().map(|n| n.definition_id.as_str()).collect();
    assert_eq!(order, CHAIN_DEFINITIONS.to_vec());
}

// ---------------------------------------------------------------------------
// 1. 8 节点链可执行（managed + legacy）
// ---------------------------------------------------------------------------


#[tokio::test]
async fn dbg_node_count() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));
    let p = Arc::new(ScriptedProvider::new(standard_script(REVIEW_OK, POLISH, DRAFT)));
    let report = run_chain(root, 1, p, &Default::default()).await.expect("run");
    eprintln!("status={:?}", report.status);
    for n in &report.nodes {
        eprintln!("{} {:?} note={:?} err={:?}", n.definition_id, n.status, n.note, n.error);
    }
}

#[tokio::test]
async fn full_chain_completes_and_settles_managed() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let report = run_chain(root, 1, provider.clone(), &Default::default()).await.expect("run");
    completed(&report);

    // 定稿正文 = 最终草稿（无阻塞 → polish 透传 → DRAFT）
    let settled = std::fs::read_to_string(root.join("定稿/正文/001-第一章.md")).unwrap();
    assert!(settled.contains(DRAFT), "定稿含草稿正文");
    // checkpoint + handoff 落盘
    assert!(root.join("工作区/工作流/第001章.json").exists());
    let handoff: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("工作区/工作流/handoff-001.json")).unwrap())
            .unwrap();
    assert_eq!(handoff["title"], "第一章");
    assert_eq!(handoff["chapter_path"], "定稿/正文/001-第一章.md");
    assert_eq!(handoff["nodes"].as_array().unwrap().len(), 8);
    // 追踪结算
    let s = lingbi_book::load_state(root).unwrap();
    assert!(s.chapters.contains_key("1"), "章已结算入追踪");
    assert!(s.state_revision >= 2, "修订已 bump");
    // AI 调用 = draft + review（polish 透传无 AI）
    assert_eq!(provider.calls(), 2);
}

#[tokio::test]
async fn full_chain_completes_on_legacy_book() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, false, "第一章", Some("主角踏上修行之路。"));

    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let report = run_chain(root, 1, provider, &Default::default()).await.expect("run");
    completed(&report);
    // legacy 首次结算即托管化（与 P5-4 采纳纪律一致）
    assert!(root.join(".lingbi").is_dir());
    assert!(std::fs::read_to_string(root.join("定稿/正文/001-第一章.md"))
        .unwrap()
        .contains(DRAFT));
}

// ---------------------------------------------------------------------------
// 2. input_hash/output_hash 记录 + 确定性
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hashes_recorded_and_deterministic_across_identical_books() {
    let b1 = tempfile::TempDir::new().unwrap();
    let b2 = tempfile::TempDir::new().unwrap();
    let d1 = b1.path().join("书");
    let d2 = b2.path().join("书");
    init_book(&d1, true, "第一章", Some("主角踏上修行之路。"));
    init_book(&d2, true, "第一章", Some("主角踏上修行之路。"));

    let r1 = {
        let p = Arc::new(ScriptedProvider::new(standard_script(
            REVIEW_OK, POLISH, DRAFT,
        )));
        run_chain(&d1, 1, p, &Default::default()).await.expect("run1")
    };
    let r2 = {
        let p = Arc::new(ScriptedProvider::new(standard_script(
            REVIEW_OK, POLISH, DRAFT,
        )));
        run_chain(&d2, 1, p, &Default::default()).await.expect("run2")
    };
    // input 确定性：contract 是唯一不继承路径相关上游的节点（input 由书仓
    // 上下文指纹 + 章文件构成，与位置无关）；下游 input 经 upstream output_hash
    // 传递路径相关——引擎契约内嵌「追踪状态」绝对路径（字节等价冻结，见
    // contract.rs sources）。同位置重跑的 input 稳定性由复用测试（node_id
    // 不变）覆盖。
    let c1 = r1.nodes.iter().find(|n| n.definition_id == "contract").unwrap();
    let c2 = r2.nodes.iter().find(|n| n.definition_id == "contract").unwrap();
    assert_eq!(c1.input_hash, c2.input_hash, "contract input 确定性");
    // output 确定性：不内嵌路径的节点（AI/结算/交付）跨书一致
    for (a, b) in r1.nodes.iter().zip(r2.nodes.iter()) {
        assert!(!a.input_hash.is_empty() && !a.output_hash.is_empty());
        if !matches!(
            a.definition_id.as_str(),
            "contract" | "brief" | "machine-check" | "handoff"
        ) {
            assert_eq!(a.output_hash, b.output_hash, "{} output 确定性", a.definition_id);
        }
    }
    // provider/model 快照只出现在 AI 节点
    for n in &r1.nodes {
        let is_ai = matches!(n.definition_id.as_str(), "draft" | "review" | "polish");
        assert_eq!(n.provider.is_some(), is_ai, "{} provider", n.definition_id);
        if is_ai {
            assert_eq!(n.provider.as_deref(), Some("scripted"));
            if n.definition_id == "polish" {
                assert_eq!(n.usage, None, "透传无 AI 消耗");
            } else {
                assert_eq!(n.usage.as_ref().unwrap().completion_tokens, 22);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 3. checkpoint 可续：失败后修复重跑，已完成且输入未变节点复用
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resume_after_draft_failure_reuses_contract_and_brief() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    // 第一次：draft 失败（系统提示词标记触发）
    let failing = Arc::new(
        ScriptedProvider::new(standard_script(REVIEW_OK, POLISH, DRAFT)).fail_on("资深网络小说作家"),
    );
    let report = run_chain(root, 1, failing, &Default::default()).await.expect("run1");
    assert_eq!(report.status, ChainStatus::Failed);
    let draft = report.nodes.iter().find(|n| n.definition_id == "draft").unwrap();
    assert_eq!(format!("{:?}", draft.status), "Failed");
    assert!(draft.error.is_some(), "draft 有错误记录");
    assert!(report.nodes.iter().all(|n| n.definition_id == "draft" || n.status == Status::Completed));

    // 第二次：好 provider → contract/brief 复用（node_id 未变），draft 起重跑
    let good = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let report2 = run_chain(root, 1, good.clone(), &Default::default()).await.expect("run2");
    completed(&report2);
    for def in ["contract", "brief"] {
        let a = report.nodes.iter().find(|n| n.definition_id == def).unwrap();
        let b = report2.nodes.iter().find(|n| n.definition_id == def).unwrap();
        assert_eq!(a.node_id, b.node_id, "{def} 复用(同一记录)");
        assert_eq!(a.input_hash, b.input_hash);
    }
    let d2 = report2.nodes.iter().find(|n| n.definition_id == "draft").unwrap();
    assert_ne!(d2.node_id, draft.node_id, "draft 重新执行");
    // 复用后 AI 调用 = draft + review（polish 透传）
    assert_eq!(good.calls(), 2);
}

// ---------------------------------------------------------------------------
// 4. 上游变化 invalidate 依赖节点
// ---------------------------------------------------------------------------

#[tokio::test]
async fn outline_change_invalidates_whole_chain() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let p = Arc::new(ScriptedProvider::new(standard_script(REVIEW_OK, POLISH, DRAFT)));
    let _ = run_chain(root, 1, p.clone(), &Default::default()).await.expect("run1"); // 首跑建立 checkpoint
    // 机检 halt 场景：把章计划破坏（摘要清空）→ 链 halt
    std::fs::write(
        root.join("定稿/正文/001-第一章.md"),
        "---\ntitle: 第一章\nchapter: 001\nvolume: 1\nwords: 0\nstatus: 草稿\n---\n\n",
    )
    .unwrap();
    let r2 = run_chain(root, 1, p.clone(), &Default::default()).await.expect("run2");
    assert_eq!(r2.status, ChainStatus::Halted, "摘要空 → 机检 halt");
    assert!(r2.nodes.iter().any(|n| n.definition_id == "machine-check" && n.status == Status::Completed));
    // 下游旧记录保留但已失效（output_hash 清空，绝不复用）
    for def in ["review", "settlement", "handoff"] {
        let n = r2.nodes.iter().find(|n| n.definition_id == def).unwrap();
        assert_eq!(format!("{:?}", n.status), "Invalidated", "{def} 失效");
        assert!(n.output_hash.is_empty(), "{def} output_hash 清空");
    }

    // 改大纲（书仓上下文指纹变化）→ 全链失效重跑
    std::fs::write(root.join("大纲/章纲.md"), "# 第一章\n\n主角入宗门修行。\n").unwrap();
    let p3 = Arc::new(ScriptedProvider::new(standard_script(REVIEW_OK, POLISH, DRAFT)));
    let r3 = run_chain(root, 1, p3, &Default::default()).await.expect("run3");
    for (a, b) in r2.nodes.iter().zip(r3.nodes.iter()) {
        assert_eq!(a.definition_id, b.definition_id);
        if a.status == Status::Completed && b.status == Status::Completed {
            assert_ne!(a.node_id, b.node_id, "{} 失效重跑", a.definition_id);
        }
    }
    assert_eq!(r3.status, ChainStatus::Halted, "摘要仍空 → 仍 halt(全部节点已重跑)");
}

#[tokio::test]
async fn model_switch_invalidates_ai_nodes_only() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    // run1：模型 A，review 失败 → draft Completed, review Failed（无结算）
    let p1 = Arc::new(
        ScriptedProvider::new(standard_script(REVIEW_OK, POLISH, DRAFT)).fail_on("请评审以下草稿"),
    );
    let r1 = run_chain(root, 1, p1, &Default::default()).await.expect("run1");
    assert_eq!(r1.status, ChainStatus::Failed);
    let draft1 = r1.nodes.iter().find(|n| n.definition_id == "draft").unwrap();
    let contract1 = r1.nodes.iter().find(|n| n.definition_id == "contract").unwrap();
    assert_eq!(format!("{:?}", draft1.status), "Completed");

    // run2：切换模型 B → draft 起失效重跑（模型切换立即生效，能力 8）；
    // contract/brief 输入未变 → 复用
    let p2 = Arc::new(
        ScriptedProvider::new(standard_script(REVIEW_OK, POLISH, DRAFT))
            .with_ids("scripted", "model-2")
            .fail_on("请评审以下草稿"),
    );
    let r2 = run_chain(root, 1, p2.clone(), &Default::default()).await.expect("run2");
    assert_eq!(r2.status, ChainStatus::Failed);
    let draft2 = r2.nodes.iter().find(|n| n.definition_id == "draft").unwrap();
    assert_ne!(draft1.node_id, draft2.node_id, "模型切换 → draft 失效重跑");
    let contract2 = r2.nodes.iter().find(|n| n.definition_id == "contract").unwrap();
    assert_eq!(contract1.node_id, contract2.node_id, "contract 复用(非 AI 不受模型影响)");
    assert_eq!(p2.calls(), 2, "draft+review 各一次");

    // run3：切回模型 A → 与 run1 同快照（input_hash 相等），但记录已被 run2
    // 替换 → draft 重新执行；contract/brief 全程复用（checkpoint 可续）
    let p3 = Arc::new(ScriptedProvider::new(standard_script(REVIEW_OK, POLISH, DRAFT)));
    let r3 = run_chain(root, 1, p3.clone(), &Default::default()).await.expect("run3");
    completed(&r3);
    let draft3 = r3.nodes.iter().find(|n| n.definition_id == "draft").unwrap();
    assert_ne!(draft2.node_id, draft3.node_id, "模型与 run2 不同 → 重新执行");
    assert_eq!(draft1.input_hash, draft3.input_hash, "与 run1 同快照 → input_hash 相等");
    let contract3 = r3.nodes.iter().find(|n| n.definition_id == "contract").unwrap();
    assert_eq!(contract1.node_id, contract3.node_id, "contract 全程复用");
    let review3 = r3.nodes.iter().find(|n| n.definition_id == "review").unwrap();
    assert_eq!(format!("{:?}", review3.status), "Completed");
}

// ---------------------------------------------------------------------------
// 5. 机检状态门禁 halt / 评审阻塞润色 / 透传
// ---------------------------------------------------------------------------

#[tokio::test]
async fn machine_check_halt_on_empty_summary_then_resume_after_plan_fix() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    // 无 summary → prewrite 摘要项 fail → 链 halt
    init_book(root, true, "第一章", None);

    let p = Arc::new(ScriptedProvider::new(standard_script(REVIEW_OK, POLISH, DRAFT)));
    let r1 = run_chain(root, 1, p.clone(), &Default::default()).await.expect("run1");
    assert_eq!(r1.status, ChainStatus::Halted);
    assert!(r1.checks.iter().any(|c| !c.ok), "机检有失败项");
    assert!(
        r1.nodes
            .iter()
            .find(|n| n.definition_id == "machine-check")
            .unwrap()
            .note
            .as_deref()
            .unwrap()
            .contains("状态门禁未过")
    );

    // 作者补章计划（摘要）→ 重跑 → 全链重跑 → 完成
    std::fs::write(
        root.join("定稿/正文/001-第一章.md"),
        "---\ntitle: 第一章\nchapter: 001\nvolume: 1\nwords: 0\nstatus: 草稿\nsummary: 主角踏上修行之路。\n---\n\n",
    )
    .unwrap();
    let p2 = Arc::new(ScriptedProvider::new(standard_script(REVIEW_OK, POLISH, DRAFT)));
    let r2 = run_chain(root, 1, p2, &Default::default()).await.expect("run2");
    completed(&r2);
    assert!(
        std::fs::read_to_string(root.join("定稿/正文/001-第一章.md"))
            .unwrap()
            .contains(DRAFT)
    );
}

#[tokio::test]
async fn blocking_review_repair_loop_then_settles() {
    // P8 OutputGate 语义：blocking → polish 改写 → 复评（首轮阻塞、次轮干净）
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let review_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let rc = review_calls.clone();
    let script = move |req: &lingbi_ai::ChatRequest| {
        let user = req.messages.last().map(|m| m.content.clone()).unwrap_or_default();
        if user.contains("改写") {
            POLISH.to_owned()
        } else if user.contains("评审") {
            if rc.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                REVIEW_BLOCKING.to_owned()
            } else {
                REVIEW_OK.to_owned()
            }
        } else {
            DRAFT.to_owned()
        }
    };
    let provider = Arc::new(ScriptedProvider::new(script));
    let report = run_chain(root, 1, provider.clone(), &Default::default()).await.expect("run");
    completed(&report);
    assert_eq!(report.repair_attempts, 1, "一次修复后复评通过");
    // 修复证据：repair_attempts=1 + 定稿 = 润色改写文本（最终 polish 记录
    // 是复评通过后的透传——如实记录最后一次动作）
    // 定稿 = 润色后文本
    let settled = std::fs::read_to_string(root.join("定稿/正文/001-第一章.md")).unwrap();
    assert!(settled.contains(POLISH));
    assert!(!settled.contains(DRAFT), "定稿是改写稿而非原草稿");
    // 评审报告落盘
    assert!(root.join("工作区/评审报告/第001章.json").exists());
    assert!(root.join("工作区/评审报告/第001章.md").exists());
}

#[tokio::test]
async fn clean_review_polish_passthrough_without_ai() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let report = run_chain(root, 1, provider, &Default::default()).await.expect("run");
    completed(&report);
    let polish = report.nodes.iter().find(|n| n.definition_id == "polish").unwrap();
    assert_eq!(polish.note.as_deref(), Some("无阻塞问题，跳过改写"));
    let draft = report.nodes.iter().find(|n| n.definition_id == "draft").unwrap();
    assert_eq!(polish.output_hash, draft.output_hash, "透传 output_hash = 草稿哈希");
}

#[tokio::test]
async fn invalid_review_json_fails_review_node() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let script = |req: &lingbi_ai::ChatRequest| {
        let user = req.messages.last().map(|m| m.content.clone()).unwrap_or_default();
        if user.contains("评审") {
            "not-json".to_owned()
        } else {
            DRAFT.to_owned()
        }
    };
    let provider = Arc::new(ScriptedProvider::new(script));
    let report = run_chain(root, 1, provider, &Default::default()).await.expect("run");
    assert_eq!(report.status, ChainStatus::Failed);
    let review = report.nodes.iter().find(|n| n.definition_id == "review").unwrap();
    assert_eq!(format!("{:?}", review.status), "Failed");
    assert!(review.error.as_deref().unwrap().contains("评审解析失败"));
}

// ---------------------------------------------------------------------------
// 6. 定稿棘轮 / 红线记录
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ratchet_rejects_second_settlement_keeping_chapter_bytes() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let p1 = Arc::new(ScriptedProvider::new(standard_script(REVIEW_OK, POLISH, DRAFT)));
    let r1 = run_chain(root, 1, p1, &Default::default()).await.expect("run1");
    completed(&r1);
    let settled1 = std::fs::read(root.join("定稿/正文/001-第一章.md")).unwrap();

    let p2 = Arc::new(ScriptedProvider::new(standard_script(REVIEW_OK, POLISH, DRAFT)));
    let r2 = run_chain(root, 1, p2, &Default::default()).await.expect("run2");
    assert_eq!(r2.status, ChainStatus::Failed);
    let settlement = r2.nodes.iter().find(|n| n.definition_id == "settlement").unwrap();
    assert_eq!(format!("{:?}", settlement.status), "Failed");
    assert!(settlement.error.as_deref().unwrap().contains("棘轮"));
    let settled2 = std::fs::read(root.join("定稿/正文/001-第一章.md")).unwrap();
    assert_eq!(settled1, settled2, "已结算正文不被覆盖");
}

#[tokio::test]
async fn draft_redline_hits_recorded_without_halting() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let redline_draft = "他终于明白了一切。TODO 补细节。";
    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, redline_draft,
    )));
    let report = run_chain(root, 1, provider, &Default::default()).await.expect("run");
    completed(&report);
    let mc = report.nodes.iter().find(|n| n.definition_id == "machine-check").unwrap();
    let note = mc.note.as_deref().unwrap();
    assert!(note.contains("红线"), "红线命中记录: {note}");
    assert!(note.contains("占位符"), "占位符命中记录: {note}");
    // 链未 halt（红线仅记录，供 Review 输入）
    assert_eq!(report.status, ChainStatus::Completed);
}

// ---------------------------------------------------------------------------
// 7. handoff 报告内容
// ---------------------------------------------------------------------------

#[tokio::test]
async fn handoff_report_carries_title_words_and_chain_summary() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    init_book(root, true, "第一章", Some("主角踏上修行之路。"));

    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, POLISH, DRAFT,
    )));
    let report = run_chain(root, 1, provider, &Default::default()).await.expect("run");
    let handoff = report.handoff.expect("handoff");
    assert_eq!(handoff.ch, 1);
    assert_eq!(handoff.title, "第一章");
    assert!(handoff.words > 0);
    assert_eq!(handoff.nodes.len(), 8);
    assert!(handoff.nodes.iter().all(|n| !n.input_hash.is_empty() && !n.output_hash.is_empty()));
    // handoff 文件与报告一致
    let file: lingbi_workflow::HandoffReport = serde_json::from_str(
        &std::fs::read_to_string(root.join("工作区/工作流/handoff-001.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(file.chapter_path, handoff.chapter_path);
}
