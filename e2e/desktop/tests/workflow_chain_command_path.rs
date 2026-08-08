//! P7-2 桌面 E2E（#41 验收）：命令层 `run_chain` 命令路径全链路。
//!
//! 在真实书仓（managed 经 create_book / legacy 经测试 fixture）上走
//! `run_chain`：8 节点 DTO → 定稿/追踪/checkpoint/handoff → 复用与失效
//! 重跑 → 错误映射。命令层（lingbi-commands）即生产 Tauri 命令背后的
//! 同一实现——Tauri 壳只做会话/provider 构建与错误映射（book_cmds.rs），
//! 因此本条路径 = 桌面 UI 将实际调用的命令路径。
//!
//! AI 节点用 `lingbi_workflow::testing::ScriptedProvider`（单一事实源）。

use lingbi_commands::*;
use lingbi_workflow::testing::{init_book, standard_script, DRAFT, REVIEW_OK, ScriptedProvider};
use std::path::Path;
use std::sync::Arc;

fn book_plan(dir: &Path, title: &str, summary: &str) {
    // 作者写章计划：create_chapter 模板 + summary（prewrite 门禁摘要项必需）
    let fm = format!(
        "---\ntitle: {title}\nchapter: 001\nvolume: 1\nwords: 0\nstatus: 草稿\nsummary: {summary}\n---\n\n"
    );
    std::fs::write(dir.join("定稿/正文/001-第一章.md"), fm).expect("plan");
}

#[tokio::test]
async fn managed_full_chain_via_commands() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let root = dir.path().join("新书").to_string_lossy().into_owned();

    let session = create_book(&root, "新书").expect("create_book");
    assert!(session.managed);
    create_chapter(&root, "第一章").expect("create_chapter");
    book_plan(Path::new(&root), "第一章", "主角踏上修行之路。");

    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, "润色稿", DRAFT,
    )));
    let run = run_chain(&root, 1, provider.clone()).await.expect("run_chain");
    assert_eq!(run.status, "Completed", "8 节点链完成");
    assert_eq!(run.nodes.len(), 8);
    assert!(run.nodes.iter().all(|n| !n.input_hash.is_empty() && !n.output_hash.is_empty()));
    // 节点按定义序 + provider 快照
    let order: Vec<&str> = run.nodes.iter().map(|n| n.definition_id.as_str()).collect();
    assert_eq!(order, ["contract", "brief", "draft", "machine-check", "review", "polish", "settlement", "handoff"]);
    let draft = run.nodes.iter().find(|n| n.definition_id == "draft").unwrap();
    assert_eq!(draft.provider.as_deref(), Some("scripted"));
    assert_eq!(draft.model.as_deref(), Some("scripted-model"));
    assert!(draft.usage.is_some());
    // 机检清单 + 交付报告
    assert_eq!(run.checks.len(), 11);
    let handoff = run.handoff.expect("handoff");
    assert_eq!(handoff.chapter_path, "定稿/正文/001-第一章.md");
    assert_eq!(handoff.title, "第一章");

    // 定稿正文 = 最终草稿（无阻塞 → polish 透传）
    let settled = std::fs::read_to_string(Path::new(&root).join("定稿/正文/001-第一章.md")).unwrap();
    assert!(settled.contains(DRAFT), "定稿含草稿正文");
    // 追踪结算
    let tracking = get_tracking_state(&root).expect("tracking");
    assert!(tracking.chapters.contains_key("1"));
    assert!(tracking.state_revision >= 2);
    // checkpoint + handoff 落盘
    assert!(Path::new(&root).join("工作区/工作流/第001章.json").exists());
    assert!(Path::new(&root).join("工作区/工作流/handoff-001.json").exists());
    // AI 调用 = draft + review（polish 透传无 AI）
    assert_eq!(provider.calls(), 2);
}

#[tokio::test]
async fn legacy_chain_resume_reuses_completed_nodes() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let root = dir.path().join("旧书").to_string_lossy().into_owned();
    // legacy 书仓（无 .lingbi）：直接初始化（引擎字节路径）
    init_book(Path::new(&root), false, "第一章", Some("主角踏上修行之路。"));

    // 第一次：draft 失败 → 链 Failed，contract/brief 已完成
    let failing = Arc::new(
        ScriptedProvider::new(standard_script(REVIEW_OK, "润色稿", DRAFT))
            .fail_on("资深网络小说作家"),
    );
    let r1 = run_chain(&root, 1, failing).await.expect("run1");
    assert_eq!(r1.status, "Failed");
    let draft1 = r1.nodes.iter().find(|n| n.definition_id == "draft").unwrap();
    assert_eq!(draft1.status, "Failed");
    assert!(draft1.error.is_some());

    // 第二次：好 provider → contract/brief 复用（node_id 未变），链完成
    let good = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, "润色稿", DRAFT,
    )));
    let r2 = run_chain(&root, 1, good.clone()).await.expect("run2");
    assert_eq!(r2.status, "Completed");
    for def in ["contract", "brief"] {
        let a = r1.nodes.iter().find(|n| n.definition_id == def).unwrap();
        let b = r2.nodes.iter().find(|n| n.definition_id == def).unwrap();
        assert_eq!(a.node_id, b.node_id, "{def} 复用");
    }
    let draft2 = r2.nodes.iter().find(|n| n.definition_id == "draft").unwrap();
    assert_ne!(draft1.node_id, draft2.node_id, "draft 重新执行");
    // legacy 首次结算即托管化（P5-4 纪律）
    assert!(Path::new(&root).join(".lingbi").is_dir());
    assert_eq!(good.calls(), 2, "复用后仅 draft+review 各一次");
}

#[tokio::test]
async fn outline_change_invalidates_and_ratchet_blocks_resettlement() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let root = dir.path().join("新书").to_string_lossy().into_owned();
    create_book(&root, "新书").expect("create_book");
    create_chapter(&root, "第一章").expect("create_chapter");
    book_plan(Path::new(&root), "第一章", "主角踏上修行之路。");

    let p = Arc::new(ScriptedProvider::new(standard_script(REVIEW_OK, "润色稿", DRAFT)));
    let r1 = run_chain(&root, 1, p.clone()).await.expect("run1");
    assert_eq!(r1.status, "Completed");

    // 改大纲（书仓上下文指纹变化）→ 重跑：全部节点失效重跑，
    // settlement 遇定稿棘轮拒绝（已提交章不可直接改写）——诚实语义
    let outline = Path::new(&root).join("大纲");
    std::fs::create_dir_all(&outline).expect("大纲 dir");
    std::fs::write(outline.join("章纲.md"), "# 第一章\n\n主角入宗门修行。\n").expect("大纲写入");
    let r2 = run_chain(&root, 1, p.clone()).await.expect("run2");
    assert_eq!(r2.status, "Failed");
    let contract2 = r2.nodes.iter().find(|n| n.definition_id == "contract").unwrap();
    let contract1 = r1.nodes.iter().find(|n| n.definition_id == "contract").unwrap();
    assert_ne!(contract1.node_id, contract2.node_id, "大纲变化 → contract 失效重跑");
    let draft2 = r2.nodes.iter().find(|n| n.definition_id == "draft").unwrap();
    let draft1 = r1.nodes.iter().find(|n| n.definition_id == "draft").unwrap();
    assert_ne!(draft1.node_id, draft2.node_id, "draft 失效重跑");
    let settlement = r2.nodes.iter().find(|n| n.definition_id == "settlement").unwrap();
    assert_eq!(settlement.status, "Failed");
    assert!(settlement.error.as_deref().unwrap().contains("棘轮"));
    // 已结算正文不被覆盖
    let bytes = std::fs::read(Path::new(&root).join("定稿/正文/001-第一章.md")).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains(DRAFT));
}

#[tokio::test]
async fn missing_book_fails_contract_node_with_reason() {
    // 书仓不存在 → 链不崩溃：contract 节点 Failed + 原因落 error（引擎
    // 容错语义——节点级失败报告，而非整链硬错误）
    let dir = tempfile::TempDir::new().expect("tempdir");
    let root = dir.path().join("不存在").to_string_lossy().into_owned();
    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, "润色稿", DRAFT,
    )));
    let run = run_chain(&root, 1, provider).await.expect("run");
    assert_eq!(run.status, "Failed");
    let contract = run.nodes.iter().find(|n| n.definition_id == "contract").unwrap();
    assert_eq!(contract.status, "Failed");
    assert!(contract.error.as_deref().unwrap().contains("缺追踪状态"));
    assert_eq!(run.nodes.len(), 1, "链止于第一个失败节点");
}
