//! P2-13 learn 等价测试：与引擎 learn.py 的 harvest_quotes / absorb_review /
//! style_stats / trend + cli.cmd_learn 输出逐字节一致（Python dict repr 格式）。
//!
//! 两层基准：
//! 1. corpus 快照 020-learn-2：在 019-revision-2 的 book 上跑 harvest(ch2) +
//!    style_stats，金句库文件与 stdout 必须与 020 快照一致。
//! 2. 7 个引擎 oracle 场景（生成脚本 /tmp/lingbi-learn-oracle/
//!    gen_learn_oracle.py——改测试场景必须先重跑引擎）。

use lingbi_book::learn::{
    absorb_review, absorb_stdout, harvest_quotes, learn_stdout, style_stats, trend, trend_stdout,
};
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

fn book019(dir: &tempfile::TempDir) -> PathBuf {
    let root = dir.path().join("book");
    copy_dir_all(&corpus("019-revision-2").join("book"), &root);
    root
}

// ── 1) corpus 020 ──

#[test]
fn corpus_learn_quotes_and_stdout_match() {
    let dir = tempfile::tempdir().unwrap();
    let root = book019(&dir);
    let stdout = learn_stdout(&root, 2).unwrap();
    let want = fs::read_to_string(corpus("020-learn-2").join("stdout.txt")).unwrap();
    assert_eq!(stdout, want, "learn stdout 与 corpus 020 不一致");
    let got_q = fs::read_to_string(root.join("文风").join("金句库").join("002.md")).unwrap();
    let want_q =
        fs::read_to_string(corpus("020-learn-2").join("book").join("文风").join("金句库").join("002.md"))
            .unwrap();
    assert_eq!(got_q, want_q, "金句库文件与 corpus 020 不一致");
}

// ── 2) 场景（引擎 oracle） ──

#[test]
fn harvest_dedup_second_time_no_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = book019(&dir);
    harvest_quotes(&root, 2).unwrap();
    let p = root.join("文风").join("金句库").join("002.md");
    let before = fs::read(&p).unwrap();
    let n2 = harvest_quotes(&root, 2).unwrap();
    let after = fs::read(&p).unwrap();
    assert_eq!(n2, 0);
    assert_eq!(after, before);
}

const BODY3: &str = concat!(
    "---\ntitle: 惊变\nchapter: 003\nvolume: 1\nwords: 60\nstatus: 定稿\n---\n\n",
    "子时，巷中无人。\n",
    "\n",
    "这是一段超过二十五个字的长段落用来测试金句收割的长度过滤应该被跳过。\n",
    "\n",
    "他说：来了。\n",
    "\n",
    "# 以井号开头的段落。\n",
    "\n",
    "没有句末标点的段落\n",
    "\n",
    "他道：走了。\n",
    "\n",
    "门内没有回应。\n",
);

#[test]
fn harvest_filters_but_keeps_ta_dao_quirk() {
    // 引擎怪癖锁定：>25 字/他说/# 开头/无句末标点跳过；「他道」不在过滤表内被收割
    let dir = tempfile::tempdir().unwrap();
    let root = book019(&dir);
    write(&root, "定稿/正文/003-惊变.md", BODY3);
    let n = harvest_quotes(&root, 3).unwrap();
    assert_eq!(n, 3);
    let q = fs::read_to_string(root.join("文风").join("金句库").join("003.md")).unwrap();
    assert_eq!(
        q,
        "# 第3章 金句\n\n子时，巷中无人。\n他道：走了。\n门内没有回应。\n"
    );
}

#[test]
fn absorb_skips_non_ai_flavor_review() {
    // corpus 评审（high+continuity / low+ai_flavor）→ 无可吸收 → 0 且不写文件
    let dir = tempfile::tempdir().unwrap();
    let root = book019(&dir);
    let n = absorb_review(&root, 2).unwrap();
    assert_eq!(n, 0);
    assert!(!root.join("文风").join("审查反例库.md").exists());
}

const REVIEW_ABSORB: &str = concat!(
    "{\n",
    " \"chapter\": 3,\n",
    " \"issues\": [\n",
    "  {\n",
    "   \"severity\": \"high\",\n",
    "   \"category\": \"ai_flavor\",\n",
    "   \"location\": \"x\",\n",
    "   \"description\": \"AI 味\",\n",
    "   \"evidence\": \"夜色如墨万籁俱寂\",\n",
    "   \"fix_hint\": \"删减\"\n",
    "  },\n",
    "  {\n",
    "   \"severity\": \"critical\",\n",
    "   \"category\": \"ai_flavor\",\n",
    "   \"location\": \"y\",\n",
    "   \"description\": \"AI 味2\",\n",
    "   \"evidence\": \"恰似画卷\",\n",
    "   \"fix_hint\": \"\"\n",
    "  },\n",
    "  {\n",
    "   \"severity\": \"high\",\n",
    "   \"category\": \"logic\",\n",
    "   \"location\": \"z\",\n",
    "   \"description\": \"非风格\",\n",
    "   \"evidence\": \"时间线矛盾\",\n",
    "   \"fix_hint\": \"\"\n",
    "  },\n",
    "  {\n",
    "   \"severity\": \"medium\",\n",
    "   \"category\": \"ai_flavor\",\n",
    "   \"location\": \"w\",\n",
    "   \"description\": \"低危\",\n",
    "   \"evidence\": \"如闪电般\",\n",
    "   \"fix_hint\": \"\"\n",
    "  }\n",
    " ]\n",
    "}\n",
);

#[test]
fn absorb_collects_high_ai_flavor_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let root = book019(&dir);
    write(&root, "工作区/评审报告/第003章.json", REVIEW_ABSORB);
    let n = absorb_review(&root, 3).unwrap();
    assert_eq!(n, 2); // high/critical + ai_flavor 各 1；logic/medium 跳过
    let f = fs::read_to_string(root.join("文风").join("审查反例库.md")).unwrap();
    assert_eq!(
        f,
        "# 审查反例库(AI 味反例,机检动态合并)\n\n夜色如墨万籁俱寂\n恰似画卷\n"
    );
}

#[test]
fn absorb_dedup_second_time_zero() {
    let dir = tempfile::tempdir().unwrap();
    let root = book019(&dir);
    write(
        &root,
        "工作区/评审报告/第003章.json",
        "{\n \"chapter\": 3,\n \"issues\": [\n  {\n   \"severity\": \"high\",\n   \"category\": \"ai_flavor\",\n   \"location\": \"x\",\n   \"description\": \"AI 味\",\n   \"evidence\": \"夜色如墨万籁俱寂\",\n   \"fix_hint\": \"\"\n  }\n ]\n}\n",
    );
    absorb_review(&root, 3).unwrap();
    let n2 = absorb_review(&root, 3).unwrap();
    assert_eq!(n2, 0);
}

#[test]
fn style_stats_dialogue_and_redline() {
    let dir = tempfile::tempdir().unwrap();
    let root = book019(&dir);
    write(
        &root,
        "定稿/正文/003-惊变.md",
        "---\ntitle: 惊变\nchapter: 003\nvolume: 1\nwords: 60\nstatus: 定稿\n---\n\n\
         他终于明白，一切皆是虚妄。\n\n“沉默。”\n\n无对话段。\n",
    );
    let sts = style_stats(&root).unwrap();
    let s3 = sts.iter().find(|(n, _)| *n == 3).map(|(_, s)| s).unwrap();
    assert_eq!(s3.hanzi, 17);
    assert_eq!(s3.paragraphs, 3);
    assert_eq!(s3.dialogue_ratio, 1.0 / 3.0);
    assert_eq!(s3.redline_hits, 1);
}

#[test]
fn trend_output_matches_engine_repr() {
    let dir = tempfile::tempdir().unwrap();
    let root = book019(&dir);
    let entries = trend(&root, 10).unwrap();
    let stdout = trend_stdout(&entries);
    assert_eq!(
        stdout,
        concat!(
            "  ch1: {'dialogue_ratio': 0.0, 'redline_hits': 0, 'avg_sentence_len': 9.3, 'words': 56}\n",
            "  ch2: {'dialogue_ratio': 0.0, 'redline_hits': 0, 'avg_sentence_len': 7.7, 'words': 46}\n",
        )
    );
}

#[test]
fn absorb_trims_counterexamples_to_last_200() {
    // 引擎 entries[-200:]：超限裁最旧（保留最新 200 条）
    let dir = tempfile::tempdir().unwrap();
    let root = book019(&dir);
    write(
        &root,
        "工作区/评审报告/第003章.json",
        "{\n \"chapter\": 3,\n \"issues\": [\n  {\n   \"severity\": \"high\",\n   \"category\": \"ai_flavor\",\n   \"location\": \"x\",\n   \"description\": \"AI 味\",\n   \"evidence\": \"夜色如墨万籁俱寂\",\n   \"fix_hint\": \"\"\n  }\n ]\n}\n",
    );
    // 既有 200 条（头部 + 条目）
    let mut existing = String::from("# 审查反例库(AI 味反例,机检动态合并)\n\n");
    for i in 0..200 {
        existing.push_str(&format!("旧反例{i}\n"));
    }
    write(&root, "文风/审查反例库.md", &existing);
    let n = absorb_review(&root, 3).unwrap();
    assert_eq!(n, 1);
    let f = fs::read_to_string(root.join("文风").join("审查反例库.md")).unwrap();
    let body = f.split("\n\n").nth(1).unwrap();
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(lines.len(), 200, "超限后应保留 200 条");
    assert_eq!(lines[0], "旧反例1", "最旧的 旧反例0 应被裁掉");
    assert_eq!(lines[199], "夜色如墨万籁俱寂", "最新吸收的应保留");
}

#[test]
fn absorb_stdout_line_matches() {
    assert_eq!(
        absorb_stdout(2, 3),
        "✅ 吸收反例 2 条(ch3)→ 文风/审查反例库.md(机检红线已动态生效)\n"
    );
}
