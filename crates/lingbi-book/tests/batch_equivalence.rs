//! P2-12 batch 等价测试：与引擎 batch.py 的 create_batch_plan / merge_batch +
//! cli.cmd_batch 输出逐字节一致（json.dumps indent=1；create 计划内嵌绝对路径与
//! created 时间戳，做结构比较 + 路径替换）。
//!
//! 两层基准：
//! 1. corpus 快照 021-batch-create-3-4：在 020-learn-2 的 book 上跑
//!    create_batch_plan([3,4])，计划结构与 corpus 计划文件一致（created 字段
//!    排除、根路径替换）；stdout 行按计划文件路径替换后字节一致。
//! 2. 6 个 merge 引擎 oracle 场景（生成脚本 /tmp/lingbi-batch-oracle/
//!    gen_batch_oracle.py——改测试场景必须先重跑引擎）。

use lingbi_book::batch::{create_batch_plan, create_plan_stdout, merge_batch};
use std::collections::BTreeSet;
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

/// JSON 内转义后的路径。
fn escaped(path: &Path) -> String {
    path.display().to_string().replace('\\', "\\\\")
}

/// 过 doctor precommit 的正文：500 个互异 CJK 字符（021 book.yaml 窗口 [0,500]）。
fn passing_body(n: i64, offset: u32) -> String {
    let title = if n == 3 { "惊变" } else { "余波" };
    let mut chars = String::new();
    for i in 0..500u32 {
        chars.push(char::from_u32(0x4e00 + offset + i).expect("cjk"));
    }
    let paras = (0..500)
        .step_by(100)
        .map(|i| chars.chars().skip(i).take(100).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "---\ntitle: {title}\nchapter: {n:03}\nvolume: 1\nwords: 500\nstatus: 定稿\n\
         promises_new: \npromises_advanced: \npromises_done: \n\
         info_gaps_new: \ninfo_gaps_revealed: \nentities_new: \n\
         must_cover: \nforbidden: \nsummary: 第{n}章发生之事。\n---\n\n{paras}\n"
    )
}

fn write_worker(root: &Path, n: i64, offset: u32) {
    let fname = format!("{n:03}-{}.md", if n == 3 { "惊变" } else { "余波" });
    write(root, &format!("定稿/正文/{fname}"), &passing_body(n, offset));
    write(
        root,
        &format!("定稿/记忆/章摘要/{n:03}.md"),
        &format!("# 章摘要 {n:03}\n第{n}章发生之事。\n"),
    );
}

// ── 1) create：corpus 021 计划结构与 stdout ──

#[test]
fn create_plan_matches_corpus_structure_and_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("sr-oracle-live");
    copy_dir_all(&corpus("020-learn-2").join("book"), &root);
    let plan = create_batch_plan(&root, &[3, 4]).unwrap();
    // created 格式：ISO seconds（引擎 datetime.now().isoformat(timespec="seconds")）
    let created_re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}$").unwrap();
    assert!(created_re.is_match(&plan.created), "created 格式: {}", plan.created);
    // 计划落盘（CLI 行为：工作区/批次-<%Y%m%d-%H%M%S>.json）
    let fname = format!(
        "批次-{}.json",
        plan.created.replace(['-', ':'], "").replace('T', "-")
    );
    let plan_path = root.join("工作区").join(fname);
    fs::create_dir_all(plan_path.parent().unwrap()).unwrap();
    fs::write(&plan_path, plan.to_json()).unwrap();
    // 结构对照：corpus 计划文件（根路径替换 + created 排除）
    let corpus_plan = fs::read_dir(corpus("021-batch-create-3-4").join("book").join("工作区"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "json"))
        .expect("corpus plan file");
    let corpus_root = Path::new(r"C:\Users\a1691\AppData\Local\Temp\sr-oracle-live");
    let mut want: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&corpus_plan)
            .unwrap()
            .replace(&escaped(corpus_root), &escaped(&root)),
    )
    .unwrap();
    let mut got: serde_json::Value =
        serde_json::from_str(&plan.to_json()).unwrap();
    want["created"] = serde_json::Value::String(String::new());
    got["created"] = serde_json::Value::String(String::new());
    assert_eq!(got, want, "计划结构与 corpus 不一致");
    // stdout 对照：路径 + 文件名替换后字节一致
    let want_stdout = fs::read_to_string(corpus("021-batch-create-3-4").join("stdout.txt")).unwrap();
    let expected = want_stdout.replace(
        &corpus_root.join("工作区").join(corpus_plan.file_name().unwrap()).display().to_string(),
        &plan_path.display().to_string(),
    );
    assert_eq!(create_plan_stdout(&plan, &plan_path), expected);
}

// ── 2) merge 场景（引擎 oracle） ──

fn merge_oracle_case(
    name: &str,
    chapters: &[i64],
    done: Option<&[i64]>,
    bodies: &[(i64, u32)],
    oracle: &str,
) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("sr-oracle-live");
    copy_dir_all(&corpus("021-batch-create-3-4").join("book"), &root);
    for &(ch, offset) in bodies {
        write_worker(&root, ch, offset);
    }
    let plan = create_batch_plan(&root, chapters).unwrap();
    let done_set = done.map(|d| d.iter().copied().collect::<BTreeSet<_>>());
    let r = merge_batch(&root, &plan, done_set.as_ref()).unwrap();
    let expected = oracle.replace("@ROOT@", &escaped(&root));
    assert_eq!(r.to_json() + "\n", expected, "{name}");
}

const MERGE_OK: &str = concat!(
    "{\n",
    " \"merged\": [\n",
    "  3,\n",
    "  4\n",
    " ],\n",
    " \"rejected\": [],\n",
    " \"skipped\": [],\n",
    " \"warns\": {\n",
    "  \"3\": [],\n",
    "  \"4\": []\n",
    " },\n",
    " \"revision\": 6,\n",
    " \"ok\": true\n",
    "}\n",
);

#[test]
fn merge_ok_sequential_commit() {
    merge_oracle_case("merge_ok", &[3, 4], None, &[(3, 0), (4, 3000)], MERGE_OK);
}

const MERGE_SKIPPED: &str = concat!(
    "{\n",
    " \"merged\": [\n",
    "  3\n",
    " ],\n",
    " \"rejected\": [],\n",
    " \"skipped\": [\n",
    "  4\n",
    " ],\n",
    " \"warns\": {\n",
    "  \"3\": []\n",
    " },\n",
    " \"revision\": 5,\n",
    " \"ok\": false\n",
    "}\n",
);

#[test]
fn merge_skips_unreturned_chapters() {
    merge_oracle_case("merge_skipped", &[3, 4], Some(&[3]), &[(3, 0)], MERGE_SKIPPED);
}

const MERGE_REJECTED_GATE: &str = concat!(
    "{\n",
    " \"merged\": [],\n",
    " \"rejected\": [\n",
    "  {\n",
    "   \"chapter\": 3,\n",
    "   \"fails\": [\n",
    "    \"字数窗口: 第3章 512 字,超出上限 500\",\n",
    "    \"占位符: 第3章 含占位符「待补」\"\n",
    "   ]\n",
    "  },\n",
    "  {\n",
    "   \"chapter\": 4,\n",
    "   \"reason\": \"缺正文(worker 未产出文件): @ROOT@\\\\定稿\\\\正文\\\\004-*.md\"\n",
    "  }\n",
    " ],\n",
    " \"skipped\": [],\n",
    " \"warns\": {},\n",
    " \"revision\": 4,\n",
    " \"ok\": false\n",
    "}\n",
);

#[test]
fn merge_rejects_gate_failure_without_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("sr-oracle-live");
    copy_dir_all(&corpus("021-batch-create-3-4").join("book"), &root);
    // ch3 正文含占位符（首段前插 待补）→ doctor precommit 失败；ch4 未产出
    write(
        &root,
        "定稿/正文/003-惊变.md",
        &passing_body(3, 0).replacen("\n\n", "待补\n\n", 1),
    );
    write(&root, "定稿/记忆/章摘要/003.md", "# 章摘要 003\n第3章发生之事。\n");
    let plan = create_batch_plan(&root, &[3, 4]).unwrap();
    let r = merge_batch(&root, &plan, None).unwrap();
    let expected = MERGE_REJECTED_GATE.replace("@ROOT@", &escaped(&root));
    assert_eq!(r.to_json() + "\n", expected);
}

const MERGE_REJECTED_MISSING: &str = concat!(
    "{\n",
    " \"merged\": [],\n",
    " \"rejected\": [\n",
    "  {\n",
    "   \"chapter\": 3,\n",
    "   \"reason\": \"缺正文(worker 未产出文件): @ROOT@\\\\定稿\\\\正文\\\\003-*.md\"\n",
    "  },\n",
    "  {\n",
    "   \"chapter\": 4,\n",
    "   \"reason\": \"缺正文(worker 未产出文件): @ROOT@\\\\定稿\\\\正文\\\\004-*.md\"\n",
    "  }\n",
    " ],\n",
    " \"skipped\": [],\n",
    " \"warns\": {},\n",
    " \"revision\": 4,\n",
    " \"ok\": false\n",
    "}\n",
);

#[test]
fn merge_rejects_missing_bodies() {
    merge_oracle_case("merge_rejected_missing", &[3, 4], None, &[], MERGE_REJECTED_MISSING);
}

#[test]
fn merge_rejects_committed_chapters_idempotently() {
    // 幂等守卫：计划含已提交章（rev 未变）→ 整批拒绝，零写入
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("sr-oracle-live");
    copy_dir_all(&corpus("021-batch-create-3-4").join("book"), &root);
    let plan = create_batch_plan(&root, &[1]).unwrap(); // 第1章已提交
    let err = merge_batch(&root, &plan, None).unwrap_err();
    assert_eq!(
        err.message,
        "批次含已提交章 1(rev 4)。重写走 storyrepo revision --chapter N(重放式级联);或为剩余章另建计划。"
    );
    // 零写入：状态与视图保持原样
    let state = fs::read_to_string(root.join("追踪").join("_tracking-state.json")).unwrap();
    assert!(state.contains("\"state_revision\": 4"));
}

#[test]
fn merge_rejects_stale_expected_revision() {
    // 并发守卫：计划 expected_revision 与当前不符 → 拒绝
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("sr-oracle-live");
    copy_dir_all(&corpus("021-batch-create-3-4").join("book"), &root);
    let mut plan = create_batch_plan(&root, &[3, 4]).unwrap();
    plan.expected_revision = 99;
    let err = merge_batch(&root, &plan, None).unwrap_err();
    assert_eq!(
        err.message,
        "批次计划期望 rev 99,当前 4——状态已变化,请重建计划"
    );
}
