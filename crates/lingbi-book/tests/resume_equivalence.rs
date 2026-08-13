//! P2-11 resume 等价测试：与引擎 resume.py 的 diagnose + cli.cmd_resume 输出
//! 逐字节一致（json.dumps indent=1 + print 尾换行）。
//!
//! 两层基准：
//! 1. corpus 快照 018-resume-3（diagnose(None) → not_started，target = latest+1）。
//! 2. 9 个引擎 oracle 场景（生成脚本 /tmp/lingbi-resume-oracle/
//!    gen_resume_oracle.py——改测试场景必须先重跑引擎），覆盖 diagnose 全分支：
//!    committed / reviewed / draft / blocking / corrupt / unknown / 缺状态 / 卡过期 / 空书。

use lingbi_book::resume::diagnose;
use std::fs;
use std::path::{Path, PathBuf};

fn corpus(step: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/storyrepo-oracle/153eb63/steps")
        .join(step)
}

fn copy_dir_all(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_all(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn write(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, content).unwrap();
}

fn diag_json(root: &Path, ch: Option<i64>) -> String {
    // 不含 print 尾换行；corpus 测试显式补 "\n"（场景 oracle 常量按 dumps 原文）
    diagnose(root, ch).to_json()
}

/// 场景基建：018 book + 修改 → diagnose 输出对照 oracle。
fn scenario(name: &str, ch: Option<i64>, modify: impl Fn(&Path), oracle: &str) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    copy_dir_all(&corpus("018-resume-3").join("book"), &root);
    modify(&root);
    assert_eq!(diag_json(&root, ch), oracle, "{name}");
}

// ── 1) corpus 018 ──

#[test]
fn corpus_resume_stdout_match() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    copy_dir_all(&corpus("018-resume-3").join("book"), &root);
    let want = fs::read_to_string(corpus("018-resume-3").join("stdout.txt")).unwrap();
    assert_eq!(diag_json(&root, None) + "\n", want);
}

// ── 2) 场景（引擎 oracle） ──

const BODY3: &str = "---\ntitle: 惊变\nchapter: 003\nvolume: 1\nwords: 60\nstatus: 定稿\n---\n\n城中惊变,米粒现身。\n";

const COMMITTED_CH2: &str = r#"{
 "status": "committed",
 "next_step": "第2章已结算:续写第3章或结转",
 "files": {
  "正文": true,
  "评审报告": true,
  "追踪已合并": true,
  "状态卡新鲜": true
 }
}"#;

#[test]
fn scenario_committed_ch2() {
    scenario("committed_ch2", Some(2), |_| {}, COMMITTED_CH2);
}

const REVIEWED_CH3: &str = r#"{
 "status": "reviewed",
 "next_step": "评审通过未结算:跑追踪合并 + 重建派生视图(Step 7)",
 "files": {
  "正文": true,
  "评审报告": true,
  "追踪已合并": false,
  "状态卡新鲜": false
 }
}"#;

#[test]
fn scenario_reviewed_ch3() {
    scenario("reviewed_ch3", Some(3), |root| {
        write(root, "定稿/正文/003-惊变.md", BODY3);
        // 复用 corpus 002 的有效评审（无 blocking）
        let review = fs::read_to_string(root.join("工作区").join("评审报告").join("第002章.json")).unwrap();
        write(root, "工作区/评审报告/第003章.json", &review);
    }, REVIEWED_CH3);
}

const DRAFT_CH3: &str = r#"{
 "status": "draft",
 "next_step": "正文已写未评审:跑机检 precommit,再评审(Step 5)",
 "files": {
  "正文": true,
  "评审报告": false,
  "追踪已合并": false,
  "状态卡新鲜": false
 }
}"#;

#[test]
fn scenario_draft_ch3() {
    scenario("draft_ch3", Some(3), |root| {
        write(root, "定稿/正文/003-惊变.md", BODY3);
    }, DRAFT_CH3);
}

const BLOCKING_CH3: &str = r#"{
 "status": "draft",
 "next_step": "评审有 blocking:定点修复后直接进润色,不重跑评审",
 "files": {
  "正文": true,
  "评审报告": true,
  "追踪已合并": false,
  "状态卡新鲜": false
 }
}"#;

#[test]
fn scenario_blocking_review() {
    scenario("blocking_ch3", Some(3), |root| {
        write(root, "定稿/正文/003-惊变.md", BODY3);
        write(root, "工作区/评审报告/第003章.json",
            "{\n \"chapter\": 3,\n \"issues\": [\n  {\n   \"severity\": \"critical\",\n   \"category\": \"logic\",\n   \"location\": \"003.md:1\",\n   \"description\": \"致命\",\n   \"evidence\": \"\",\n   \"fix_hint\": \"\"\n  }\n ]\n}");
    }, BLOCKING_CH3);
}

const CORRUPT_CH3: &str = r#"{
 "status": "draft",
 "next_step": "评审报告损坏:重跑评审(Step 5)或用户裁决",
 "files": {
  "正文": true,
  "评审报告": true,
  "追踪已合并": false,
  "状态卡新鲜": false
 }
}"#;

#[test]
fn scenario_corrupt_review() {
    scenario("corrupt_ch3", Some(3), |root| {
        write(root, "定稿/正文/003-惊变.md", BODY3);
        write(root, "工作区/评审报告/第003章.json", "not json{{");
    }, CORRUPT_CH3);
}

const UNKNOWN_CH3: &str = r#"{
 "status": "unknown",
 "next_step": "异常:第3章有评审报告但无正文,请人工核查,勿直接续写覆盖",
 "files": {
  "正文": false,
  "评审报告": true,
  "追踪已合并": false,
  "状态卡新鲜": true
 }
}"#;

#[test]
fn scenario_review_without_body() {
    scenario("unknown_ch3", Some(3), |root| {
        let review = fs::read_to_string(root.join("工作区").join("评审报告").join("第002章.json")).unwrap();
        write(root, "工作区/评审报告/第003章.json", &review);
    }, UNKNOWN_CH3);
}

const NO_STATE_DRAFT: &str = r#"{
 "status": "draft",
 "next_step": "正文已写未评审:跑机检 precommit,再评审(Step 5)(缺追踪状态:先 init_state)",
 "files": {
  "正文": true,
  "评审报告": false,
  "追踪已合并": false,
  "状态卡新鲜": false
 }
}"#;

#[test]
fn scenario_draft_without_state_file() {
    scenario("no_state_draft", Some(3), |root| {
        write(root, "定稿/正文/003-惊变.md", BODY3);
        fs::remove_file(root.join("追踪").join("_tracking-state.json")).unwrap();
    }, NO_STATE_DRAFT);
}

const CARD_STALE: &str = r#"{
 "status": "committed",
 "next_step": "第2章已结算:续写第3章或结转;状态卡过期,先重建视图再续写",
 "files": {
  "正文": true,
  "评审报告": true,
  "追踪已合并": true,
  "状态卡新鲜": false
 }
}"#;

#[test]
fn scenario_stale_card_appends_hint() {
    scenario("card_stale", Some(2), |root| {
        // 改写正文（mtime 晚于状态卡）→ 状态卡过期
        let p = root.join("定稿").join("正文").join("002-夜访.md");
        let t = fs::read_to_string(&p).unwrap();
        fs::write(p, t + "\n后补一段。\n").unwrap();
    }, CARD_STALE);
}

const NO_CHAPTERS: &str = r#"{
 "status": "not_started",
 "next_step": "第3章尚未动笔:先跑机检 prewrite,再按 合同→任务书→起草 推进",
 "files": {
  "正文": false,
  "评审报告": false,
  "追踪已合并": false,
  "状态卡新鲜": true
 }
}"#;

#[test]
fn scenario_no_chapters_default_target() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    copy_dir_all(&corpus("000-initial").join("book"), &root);
    assert_eq!(diag_json(&root, None), NO_CHAPTERS);
}
