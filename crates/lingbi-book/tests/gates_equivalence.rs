//! P2-06 gates 等价测试：三阶段门禁 exit code 与引擎一致。
//!
//! gate() 聚合本身（prewrite 跳过集/弱提示/全量）已在 #9 checks_equivalence
//! 的 doctor stdout 逐字节测试中钉死；本文件专测 exit code 接线
//! （引擎 `cli.cmd_doctor`：`return 0 if r["passed"] else 1`）。

use lingbi_book::gates::{exit_code, gate};
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

/// 三阶段门禁的 corpus doctor 步（prewrite ×2 / precommit ×2 / postcommit ×1）。
const DOCTOR_STEPS: [(&str, &str, i64); 5] = [
    ("001-doctor-prewrite-1", "prewrite", 1),
    ("002-doctor-prewrite-2", "prewrite", 2),
    ("009-doctor-precommit-1", "precommit", 1),
    ("010-doctor-precommit-2", "precommit", 2),
    ("016-doctor-postcommit-2", "postcommit", 2),
];

#[test]
fn gate_exit_codes_match_corpus_every_step() {
    // 验收标准：三阶段门禁 exit code 与 corpus 一致（全部 0）
    for (step, stage, chapter) in DOCTOR_STEPS {
        let src = corpus(step);
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("book");
        copy_dir_all(&src.join("book"), &root);
        let g = gate(&root, stage, Some(chapter)).unwrap_or_else(|e| panic!("{step}: {e}"));
        let expected: i32 = fs::read_to_string(src.join("exitcode"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(g.passed, "{step} corpus 步应 PASS");
        assert_eq!(exit_code(&g), expected, "{step} exit code 与 corpus 不一致");
    }
}

#[test]
fn gate_failure_returns_exit_one_like_engine() {
    // 失败路径：corpus 无 exit=1 步（全部 0），oracle 来源——
    // ①引擎 CLI 实跑本场景（占位符违规书）exit=1、stdout "gate precommit: FAIL"；
    // ②stdout 全文与 #9 ORACLE_PLACEHOLDER 逐字节一致。
    let src = corpus("000-initial");
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    copy_dir_all(&src.join("book"), &root);
    fs::write(
        root.join("定稿/正文/001-开局.md"),
        "---\ntitle: 开局\nsummary: 测试。\n---\n\n正文待补。\n",
    )
    .unwrap();
    fs::write(
        root.join("追踪/_tracking-state.json"),
        r#"{"spec": 1, "book": "样板书", "imported_through_chapter": 0, "state_revision": 1, "chapters": {}, "promises": {}, "info_gaps": {}, "timeline": [], "entity_states": {}, "reader_knowledge": {"reveals": []}}"#,
    )
    .unwrap();
    let g = gate(&root, "precommit", Some(1)).unwrap();
    assert!(!g.passed);
    assert_eq!(exit_code(&g), 1, "FAIL 门禁 exit code 应为 1");
    assert!(g.fails.iter().any(|f| f.starts_with("占位符: ")));
}
