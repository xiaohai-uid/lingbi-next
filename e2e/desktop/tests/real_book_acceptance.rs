//! P12-2 (#54) 真实《零界道种》产品级验收 harness（复制不改源）。
//!
//! 对源书仓副本跑完整产品命令链（P4 唯一边界 = lingbi-commands / lingbi-book
//! API，桌面 UI 同一实现）：
//!
//! ```text
//! 复制源 → migrate_book（storyrepo-legacy 托管化，回执六字段）
//! → 字节等价（定稿逐文件）→ source READ ONLY（前后 manifest hash 不变）
//! → open_book / tracking / outline / list_chapters（章节顺序）
//! → canon+memory（定稿/记忆/章摘要 全读）→ assemble_context（AI context
//!   build：风格宪法+设定+章摘要+追踪）→ build_contract / build_brief
//! → cmd_doctor exit 0 → run_chain（ScriptedProvider 注入，最后一章：
//!   Draft/Review/Settlement 全节点 + .lingbi/versions Final Revision 快照
//!   + 追踪结算）→ resume_workflow（断点诊断 JSON）
//! → scripts/validate-book-v3.py exit 0（P12-1 契约两档）
//! ```
//!
//! 源书仓：`LINGBI_REAL_BOOK` 环境变量（真实《零界道种》UNC/本地路径）优先，
//! 缺省用 `fixtures/storyrepo-oracle/153eb63/steps/000-initial/book`
//! （doctor 通过基准，CI 确定性）。章节数按追踪数据驱动（fixture 2 章 /
//! 真实书 4 章同码）。

use lingbi_book::{
    assemble_context, build_brief, build_contract, cmd_doctor, load_state,
};
use lingbi_commands::{create_chapter, get_outline, get_tracking_state, list_chapters, migrate_book, open_book, resume_workflow};
use lingbi_workflow::run::RunControl;
use lingbi_workflow::testing::{standard_script, DRAFT, REVIEW_OK, ScriptedProvider};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 源书仓：LINGBI_REAL_BOOK 优先（真实《零界道种》），缺省 corpus 基准。
fn source_book() -> PathBuf {
    if let Ok(real) = std::env::var("LINGBI_REAL_BOOK")
        && !real.is_empty()
    {
        return PathBuf::from(real);
    }
    let repo = std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir");
    // e2e/desktop → e2e → 仓库根（fixtures 在根下）。
    // 基线 = corpus 最终快照 022-report-write-2（最完整演化态：2 章 +
    // summary + rev4，doctor 通过基准）；000-initial 的追踪 chapters 为空。
    PathBuf::from(repo)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures/storyrepo-oracle/153eb63/steps/022-report-write-2/book")
}

/// 递归复制（排除 .git——验收只看书仓内容，仓库元数据不算书仓）。
fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("mkdir: {e}"))?;
    for entry in std::fs::read_dir(src).map_err(|e| format!("read {src:?}: {e}"))? {
        let entry = entry.map_err(|e| format!("entry: {e}"))?;
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| format!("copy {from:?}: {e}"))?;
        }
    }
    Ok(())
}

/// 目录 manifest sha256（排序 rel 路径 + 字节）——source READ ONLY 证据。
fn manifest_hash(root: &Path) -> String {
    let mut files = BTreeMap::new();
    for (base, names) in walkdir(root) {
        for name in names {
            let path = base.join(&name);
            if path.file_name().and_then(|n| n.to_str()) == Some(".git") {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = std::fs::read(&path).unwrap_or_default();
            files.insert(rel, bytes);
        }
    }
    let mut hasher = Sha256::new();
    for (rel, bytes) in files {
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        hasher.update(&bytes);
        hasher.update([0u8]);
    }
    format!("{:x}", hasher.finalize())
}

/// 手工 walk（std 无递归目录遍历）。
fn walkdir(root: &Path) -> Vec<(PathBuf, Vec<String>)> {
    fn walk(root: &Path, out: &mut Vec<(PathBuf, Vec<String>)>) {
        if let Ok(rd) = std::fs::read_dir(root) {
            let mut names = Vec::new();
            for e in rd.flatten() {
                if e.file_name() == ".git" {
                    continue;
                }
                if e.path().is_dir() {
                    walk(&e.path(), out);
                } else {
                    names.push(e.file_name().to_string_lossy().into_owned());
                }
            }
            names.sort();
            out.push((root.to_path_buf(), names));
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

/// 最后一章正文文件（定稿/正文/{n:03}-*.md，从磁盘取，不依赖 title）。
fn last_chapter_path(root: &Path, n: i64) -> PathBuf {
    let dir = root.join("定稿/正文");
    let prefix = format!("{n:03}-");
    let hit = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("正文目录 {dir:?}: {e}"))
        .flatten()
        .find(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(prefix.as_str())
        })
        .unwrap_or_else(|| panic!("缺第 {n} 章正文文件"));
    hit.path()
}

#[tokio::test]
async fn real_book_product_acceptance() {
    // ---- 0. 源 READ ONLY 基线（复制前 manifest hash）----
    let src = source_book();
    assert!(src.is_dir(), "源书仓缺失: {}", src.display());
    let source_before = manifest_hash(&src);

    // ---- 1. 复制源（不改源）----
    let tmp = std::env::temp_dir().join("E2E验收书P12");
    let _ = std::fs::remove_dir_all(&tmp);
    let copy = tmp.join("源副本");
    copy_tree(&src, &copy).expect("copy source");

    // ---- 2. 迁移（storyrepo-legacy → 托管化，回执六字段）----
    let dest = tmp.join("迁移书");
    let receipt = migrate_book(
        copy.to_str().expect("copy utf8"),
        dest.to_str().expect("dest utf8"),
    )
    .expect("migrate_book");
    assert_eq!(receipt.classification, "storyrepo-legacy", "分类");
    assert!(!receipt.source_hash.is_empty(), "source_hash");
    assert!(!receipt.destination_hash.is_empty(), "destination_hash");
    assert!(!receipt.migration_rule.is_empty(), "migration_rule 逐文件");
    // 源 READ ONLY 的独立证据 = 前后 manifest hash 比对（步骤 12）；
    // 回执 source_hash 为 Inspector 自身指纹，算法独立不在此对齐。

    // ---- 3. 字节等价（定稿 全部文件逐字节）----
    let src_final = src.join("定稿");
    let dst_final = dest.join("定稿");
    for (base, names) in walkdir(&src_final) {
        for name in names {
            let rel = base
                .strip_prefix(&src_final)
                .unwrap()
                .join(&name)
                .to_string_lossy()
                .replace('\\', "/");
            let a = std::fs::read(base.join(&name)).expect("src bytes");
            let b = std::fs::read(dst_final.join(&rel)).expect("dst bytes");
            assert_eq!(a, b, "定稿字节不一致: {rel}");
        }
    }

    // ---- 4. open_book / tracking / outline / list_chapters（章节顺序）----
    let dest_str = dest.to_str().expect("dest utf8");
    let session = open_book(dest_str).expect("open_book");
    assert!(session.managed, "迁移后托管化");
    let tracking = get_tracking_state(dest_str).expect("tracking");
    let n = tracking.chapters.len() as i64;
    assert!(n >= 1, "追踪至少一章");
    let chapters = list_chapters(dest_str).expect("list_chapters");
    assert!(
        chapters.len() as i64 >= n,
        "磁盘章节数 ≥ 追踪数（在途章允许：真实书 ch4 即正文在、追踪未登记）"
    );
    for i in 0..n {
        let c = &chapters[i as usize];
        assert_eq!(c.ch, i + 1, "章节顺序 {i}");
        let entry = &tracking.chapters[&(i + 1).to_string()];
        assert_eq!(c.title, entry.title, "章节标题与追踪一致 ch{}", i + 1);
    }
    let outline = get_outline(dest_str).expect("outline");
    // 大纲 = 全卷计划（含未来章）——必须覆盖全部已提交章且编号递增有序
    assert!(
        outline.len() as i64 >= n,
        "大纲行数 ≥ 已提交章节数（含未来计划）"
    );
    let outline_chs: Vec<i64> = outline.iter().map(|o| o.ch).collect();
    assert_eq!(outline_chs, {
        let mut v = outline_chs.clone();
        v.sort_unstable();
        v.dedup();
        v
    }, "大纲编号有序唯一");
    for ch in 1..=n {
        assert!(outline_chs.contains(&ch), "大纲缺已提交章 {ch}");
    }

    // ---- 5. canon + memory（定稿/设定 非空；章摘要全读）----
    let settings = dest.join("定稿/设定");
    assert!(settings.is_dir(), "设定目录");
    assert!(
        std::fs::read_dir(&settings).expect("settings").count() >= 1,
        "设定内容非空"
    );
    for ch in 1..=n {
        let summary = dest.join(format!("定稿/记忆/章摘要/{ch:03}.md"));
        assert!(summary.is_file(), "章摘要缺失 ch{ch}");
        assert!(
            !std::fs::read_to_string(&summary).expect("summary").trim().is_empty(),
            "章摘要空 ch{ch}"
        );
    }

    // ---- 6. AI context build（风格宪法+设定+章摘要+追踪 装配）----
    let ctx = assemble_context(&dest, &tracking, 1);
    let any_content = !ctx.recent_endings.is_empty()
        || !ctx.summary_chain.is_empty()
        || !ctx.entity_cards.is_empty()
        || !ctx.next_chapter_injection.is_empty();
    assert!(any_content, "AI 上下文四路至少一路非空");
    assert!(ctx.budget_kb > 0.0, "budget 计算");
    // 章摘要链实证：第 2 章上下文含前章摘要（摘要链按设计只含前章）
    if n >= 2 {
        let ctx2 = assemble_context(&dest, &tracking, 2);
        assert!(!ctx2.summary_chain.is_empty(), "第 2 章上下文含章摘要链");
    }

    // ---- 7. Contract / Brief ----
    let contract = build_contract(&dest, &tracking, 1);
    assert!(!contract.goal.is_empty(), "合同 goal 非空");
    assert!(
        !contract.hard_constraints.is_empty() || !contract.forbidden.is_empty(),
        "合同约束/禁项"
    );
    let brief = build_brief(&dest, &tracking, 1, Some(&contract), None).expect("brief");
    assert!(brief.contains("写作任务书"), "任务书标题: {brief}");

    // ---- 8. doctor（prewrite 门禁）----
    let doctor = cmd_doctor(&dest, "prewrite", None).expect("doctor");
    assert_eq!(doctor.exit, 0, "doctor 未过: {}", doctor.stdout);

    // ---- 9. 链（新章；Draft/Review/Settlement + Final Revision）----
    // 已提交章走定稿棘轮（回炉），链的结算语义落在新章上——与真实产品
    // 流程一致：作者建章 → 补计划 → 运行工作流。
    let new_title = "新章";
    let new_ch = create_chapter(dest_str, new_title).expect("create_chapter");
    let chain_ch = new_ch.ch;
    assert!(chain_ch > n, "新章编号在已提交章之后（磁盘 latest+1）");
    // 补章计划（prewrite 门禁摘要项必需）
    let new_path = dest.join(format!("定稿/正文/{chain_ch:03}-{new_title}.md"));
    let plan = format!(
        "---\ntitle: {new_title}\nchapter: {chain_ch:03}\nvolume: 1\nwords: 0\nstatus: 草稿\nsummary: 验收链新章摘要。\n---\n\n"
    );
    std::fs::write(&new_path, plan).expect("write plan");

    let provider = Arc::new(ScriptedProvider::new(standard_script(
        REVIEW_OK, "P12润色", DRAFT,
    )));
    let before_rev = tracking.state_revision;
    let run = lingbi_workflow::run_chain(&dest, chain_ch, provider, &RunControl::default())
        .await
        .expect("run_chain");
    assert_eq!(
        run.status,
        lingbi_workflow::ChainStatus::Completed,
        "8 节点链完成; 未完成节点: {:?}",
        run.nodes
            .iter()
            .filter(|n| n.status != lingbi_workflow::node::NodeStatus::Completed)
            .map(|n| (n.definition_id.clone(), n.error.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(run.nodes.len(), 8, "节点数");
    assert_eq!(run.repair_attempts, 0, "评审 clean 无返工");
    // 定稿落盘 = 草稿文本（REVIEW_OK → polish 透传 → DRAFT）
    let settled = std::fs::read_to_string(last_chapter_path(&dest, chain_ch)).expect("settled");
    assert!(settled.contains(DRAFT), "定稿正文 = 草稿文本: {settled}");
    // Final Revision 快照（.lingbi/versions）
    let versions = dest.join(".lingbi/versions");
    assert!(versions.is_dir(), "versions 目录");
    assert!(
        std::fs::read_dir(&versions).expect("versions").count() >= 1,
        "Final Revision 快照 ≥1"
    );
    // 追踪结算（revision bump）
    let after = load_state(&dest).expect("load_state");
    assert!(after.state_revision > before_rev, "追踪结算 rev bump");
    // checkpoint + handoff 落盘
    assert!(
        dest.join(format!("工作区/工作流/第{chain_ch:03}章.json")).is_file(),
        "checkpoint 落盘"
    );
    assert!(
        dest.join(format!("工作区/工作流/handoff-{chain_ch:03}.json")).is_file(),
        "handoff 落盘"
    );

    // ---- 10. resume（断点诊断 JSON 可解析）----
    let diag = resume_workflow(dest_str).expect("resume_workflow");
    let v: serde_json::Value = serde_json::from_str(&diag).expect("diagnose json");
    assert!(v["status"].is_string(), "diagnose status: {diag}");
    assert!(v["next_step"].is_string(), "diagnose next_step: {diag}");

    // ---- 11. 契约校验器（P12-1 两档：真实书仓形态 exit 0）----
    let repo = std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir");
    let validator = PathBuf::from(&repo)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("scripts/validate-book-v3.py");
    let out = std::process::Command::new("python3")
        .arg(&validator)
        .arg(&dest)
        .output()
        .expect("validator 运行");
    assert!(
        out.status.success(),
        "validate-book-v3.py 未过: {}",
        format!("{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    );

    // ---- 12. source READ ONLY（源 manifest hash 全程不变）----
    let source_after = manifest_hash(&src);
    assert_eq!(source_before, source_after, "源书仓必须只读（复制不改源）");

    let _ = std::fs::remove_dir_all(&tmp);
}
