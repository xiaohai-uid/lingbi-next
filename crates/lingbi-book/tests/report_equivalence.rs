//! P2-14 report 等价测试：与引擎 report.py 的 author_report + cli.cmd_report 输出
//! 逐字节一致（作者报告函数自带尾换行，CLI print 再补一个）。
//!
//! 两层基准：
//! 1. corpus 快照 022-report-write-2（021 book 上 author_report(write, None)）。
//! 2. 7 个引擎 oracle 场景（生成脚本 /tmp/lingbi-report-oracle/
//!    gen_report_oracle.py——改测试场景必须先重跑引擎）。

use lingbi_book::report::author_report;
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

/// 引擎 cmd_report：print(author_report(...)) → 函数自带 \n + print \n。
fn report_stdout(root: &Path, stage: &str, ch: Option<i64>) -> String {
    author_report(root, stage, ch).unwrap() + "\n"
}

fn book021(dir: &tempfile::TempDir) -> PathBuf {
    let root = dir.path().join("book");
    copy_dir_all(&corpus("021-batch-create-3-4").join("book"), &root);
    root
}

fn scenario(name: &str, stage: &str, ch: Option<i64>, modify: impl Fn(&Path), oracle: &str) {
    let dir = tempfile::tempdir().unwrap();
    let root = book021(&dir);
    modify(&root);
    assert_eq!(report_stdout(&root, stage, ch), oracle, "{name}");
}

// ── 1) corpus 022 ──

#[test]
fn corpus_report_stdout_match() {
    let dir = tempfile::tempdir().unwrap();
    let root = book021(&dir);
    let want = fs::read_to_string(corpus("022-report-write-2").join("stdout.txt")).unwrap();
    assert_eq!(report_stdout(&root, "write", None), want);
}

// ── 2) 场景（引擎 oracle） ──

const STAGE_PRECOMMIT: &str = concat!(
    "# 作者报告(precommit)\n",
    "\n",
    "## 一、状态\n",
    "- 最新章: 2 / 计划 4 章\n",
    "- 正文总字数: 102 字\n",
    "- 活跃承诺: 1  未曝光信息差: 1\n",
    "\n",
    "## 二、本次做了什么(precommit)\n",
    "- 机检(precommit): 通过 11 项,问题 0 项 → 放行\n",
    "- 审查: 第2章 问题 2 条,blocking 0 条\n",
    "- 已结算章: 2 章(最新 第2章)\n",
    "\n",
    "## 三、下一步建议\n",
    "- 续写第3章:合同→任务书→起草→机检→评审→结算\n",
    "\n",
);

#[test]
fn scenario_gate_stage_reports_check_counts() {
    scenario("stage_precommit", "precommit", Some(2), |_| {}, STAGE_PRECOMMIT);
}

const NO_STATE: &str = concat!(
    "# 作者报告(write)\n",
    "\n",
    "## 一、状态\n",
    "- 最新章: 2 / 计划 4 章\n",
    "- 正文总字数: 102 字\n",
    "- 活跃承诺: 0  未曝光信息差: 0\n",
    "\n",
    "## 二、本次做了什么(write)\n",
    "- 机检: 本阶段不涉及\n",
    "- 审查: 第2章 问题 2 条,blocking 0 条\n",
    "- 已结算章: 0 章(最新 无)\n",
    "\n",
    "## 三、下一步建议\n",
    "- 缺追踪状态:先跑 init_state\n",
    "\n",
);

#[test]
fn scenario_missing_state_suggests_init() {
    scenario("no_state", "write", None, |root| {
        fs::remove_file(root.join("追踪").join("_tracking-state.json")).unwrap();
    }, NO_STATE);
}

const NO_REVIEW: &str = concat!(
    "# 作者报告(write)\n",
    "\n",
    "## 一、状态\n",
    "- 最新章: 2 / 计划 4 章\n",
    "- 正文总字数: 102 字\n",
    "- 活跃承诺: 1  未曝光信息差: 1\n",
    "\n",
    "## 二、本次做了什么(write)\n",
    "- 机检: 本阶段不涉及\n",
    "- 审查: 第3章尚无评审报告\n",
    "- 已结算章: 2 章(最新 第2章)\n",
    "\n",
    "## 三、下一步建议\n",
    "- 续写第4章:合同→任务书→起草→机检→评审→结算\n",
    "\n",
);

#[test]
fn scenario_chapter_without_review() {
    scenario("no_review", "write", Some(3), |_| {}, NO_REVIEW);
}

const REVIEW_BLOCKING: &str = concat!(
    "# 作者报告(write)\n",
    "\n",
    "## 一、状态\n",
    "- 最新章: 2 / 计划 4 章\n",
    "- 正文总字数: 102 字\n",
    "- 活跃承诺: 1  未曝光信息差: 1\n",
    "\n",
    "## 二、本次做了什么(write)\n",
    "- 机检: 本阶段不涉及\n",
    "- 审查: 第2章 问题 1 条,blocking 1 条\n",
    "- 已结算章: 2 章(最新 第2章)\n",
    "\n",
    "## 三、下一步建议\n",
    "- 第2章存在 blocking:定点修复后进润色,勿重跑评审\n",
    "\n",
);

#[test]
fn scenario_blocking_review_suggests_fix() {
    scenario("review_blocking", "write", Some(2), |root| {
        write(
            root,
            "工作区/评审报告/第002章.json",
            "{\n \"chapter\": 2,\n \"issues\": [\n  {\n   \"severity\": \"critical\",\n   \"category\": \"logic\",\n   \"location\": \"x\",\n   \"description\": \"致命\",\n   \"evidence\": \"\",\n   \"fix_hint\": \"\"\n  }\n ]\n}\n",
        );
    }, REVIEW_BLOCKING);
}

/// 审查损坏分支：括号内为解析器错误文本（Python vs serde 文案不同，review.rs 已声明
/// 差异）——该行仅断言前缀，其余行逐字节对照。
#[test]
fn scenario_corrupt_review_prefix_asserted() {
    let dir = tempfile::tempdir().unwrap();
    let root = book021(&dir);
    write(&root, "工作区/评审报告/第002章.json", "not json{{");
    let got = report_stdout(&root, "write", Some(2));
    let lines: Vec<&str> = got.lines().collect();
    assert!(lines[9].starts_with("- 审查: 第2章评审报告损坏("), "损坏行: {}", lines[9]);
    assert_eq!(lines[10], "- 已结算章: 2 章(最新 第2章)");
    assert!(got.contains("## 三、下一步建议\n- 第2章存在 blocking:定点修复后进润色,勿重跑评审\n"));
}

const POSTCOMMIT_OK: &str = concat!(
    "# 作者报告(postcommit)\n",
    "\n",
    "## 一、状态\n",
    "- 最新章: 2 / 计划 4 章\n",
    "- 正文总字数: 102 字\n",
    "- 活跃承诺: 1  未曝光信息差: 1\n",
    "\n",
    "## 二、本次做了什么(postcommit)\n",
    "- 机检(postcommit): 通过 11 项,问题 0 项 → 放行\n",
    "- 审查: 第2章 问题 2 条,blocking 0 条\n",
    "- 已结算章: 2 章(最新 第2章)\n",
    "\n",
    "## 三、下一步建议\n",
    "- 本批次可结转:git add -A && git commit(或先重建派生视图)\n",
    "\n",
);

#[test]
fn scenario_postcommit_ok_suggests_handoff() {
    scenario("postcommit_ok", "postcommit", Some(2), |_| {}, POSTCOMMIT_OK);
}

const PLAN_MISSING: &str = concat!(
    "# 作者报告(write)\n",
    "\n",
    "## 一、状态\n",
    "- 最新章: 2 / 计划 0 章\n",
    "- 正文总字数: 102 字\n",
    "- 活跃承诺: 1  未曝光信息差: 1\n",
    "\n",
    "## 二、本次做了什么(write)\n",
    "- 机检: 本阶段不涉及\n",
    "- 审查: 第2章 问题 2 条,blocking 0 条\n",
    "- 已结算章: 2 章(最新 第2章)\n",
    "\n",
    "## 三、下一步建议\n",
    "- 续写第3章:合同→任务书→起草→机检→评审→结算\n",
    "\n",
);

#[test]
fn scenario_missing_book_yaml_plan_zero() {
    scenario("plan_missing", "write", None, |root| {
        fs::remove_file(root.join("book.yaml")).unwrap();
    }, PLAN_MISSING);
}
