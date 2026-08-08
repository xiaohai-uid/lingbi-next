//! P2-03 tracking state 模块等价测试：与引擎 state.py 追踪状态行为一致。
//!
//! 关键：`dump_state` 输出必须与引擎 `json.dump(ensure_ascii=False, indent=1)` 逐字节一致
//! （corpus 快照本身即引擎产物 → roundtrip 字节级验证）。

use lingbi_book::tracking::{
    active_promises, bump_revision, dump_state, init_state, load_state, merge_chapter_facts,
    new_state, revision, save_state, unrevealed_gaps, TrackingState,
};
use lingbi_book::state::frontmatter;
use std::path::{Path, PathBuf};

fn corpus(root: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/storyrepo-oracle/153eb63/steps")
        .join(root)
        .join("book")
}

fn tracking_json(root: &Path) -> String {
    std::fs::read_to_string(root.join("追踪/_tracking-state.json")).unwrap()
}

#[test]
fn dump_byte_identical_to_engine_across_snapshots() {
    // 引擎 json.dump 产物 → Rust parse → Rust dump → 逐字节一致
    for step in ["000-initial", "012-commit-1", "015-commit-2", "019-revision-2"] {
        let root = corpus(step);
        let src = tracking_json(&root);
        let s = load_state(&root).unwrap_or_else(|e| panic!("{step}: {e}"));
        let dumped = dump_state(&s);
        assert_eq!(dumped, src, "dump 与引擎字节不一致: {step}");
    }
}

#[test]
fn new_state_like_engine() {
    // 引擎: book = os.path.basename(root) or "未命名"
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("样板书");
    std::fs::create_dir_all(&root).unwrap();
    let s = new_state(&root);
    assert_eq!(s.spec, 1);
    assert_eq!(s.book, "样板书"); // basename
    assert_eq!(s.imported_through_chapter, 0);
    assert_eq!(s.state_revision, 1);
    assert!(s.chapters.is_empty());
    assert_eq!(s.reader_knowledge.reveals.len(), 0);
}

#[test]
fn load_missing_state_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let err = load_state(dir.path()).unwrap_err();
    assert_eq!(err.code, lingbi_contracts::ErrorCode::BookNotFound);
}

#[test]
fn load_corrupted_state_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("追踪")).unwrap();
    std::fs::write(dir.path().join("追踪/_tracking-state.json"), "{ 坏 JSON").unwrap();
    let err = load_state(dir.path()).unwrap_err();
    assert_eq!(err.code, lingbi_contracts::ErrorCode::BookCorrupted);
}

#[test]
fn save_roundtrip_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let root = corpus("015-commit-2");
    let s = load_state(&root).unwrap();
    save_state(dir.path(), &s).unwrap();
    assert_eq!(tracking_json(dir.path()), tracking_json(&root));
    // 二次 load 结构相等
    let s2 = load_state(dir.path()).unwrap();
    assert_eq!(s, s2);
}

#[test]
fn init_state_idempotent_like_engine() {
    let dir = tempfile::tempdir().unwrap();
    assert!(init_state(dir.path()).unwrap());
    assert!(!init_state(dir.path()).unwrap()); // 已存在 → False
    assert!(dir.path().join("追踪/角色状态").is_dir());
    assert!(dir.path().join("追踪/逐章记录").is_dir());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("追踪/上下文.md")).unwrap(),
        "# 续写状态卡(7 栏,待首章后由 views 重建)\n"
    );
}

#[test]
fn revision_bump_monotonic() {
    let mut s = new_state(Path::new("书"));
    assert_eq!(revision(&s), 1);
    assert_eq!(bump_revision(&mut s), 2);
    assert_eq!(bump_revision(&mut s), 3);
    assert_eq!(s.state_revision, 3);
}

#[test]
fn merge_ch1_matches_corpus_commit1() {
    // 从 000-initial 空状态 → merge ch1 → bump → 与 012-commit-1 快照结构一致
    let initial = corpus("000-initial");
    let mut s = load_state(&initial).unwrap();
    let fm = frontmatter(&initial.join("定稿/正文/001-开局.md")).unwrap();
    let summary = fm.get("summary").cloned().unwrap_or_default();
    let warns = merge_chapter_facts(&mut s, 1, &fm, &summary).unwrap();
    assert!(warns.is_empty(), "warns: {:?}", warns);
    bump_revision(&mut s);
    let expected = load_state(&corpus("012-commit-1")).unwrap();
    assert_eq!(s, expected);
}

#[test]
fn merge_ch1_ch2_matches_corpus_commit2() {
    let initial = corpus("000-initial");
    let mut s = load_state(&initial).unwrap();
    for ch in [1, 2] {
        let dir = initial.join("定稿/正文");
        let fname = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .find(|f| f.starts_with(&format!("{:03}-", ch)))
            .unwrap();
        let fm = frontmatter(&dir.join(fname)).unwrap();
        let summary = fm.get("summary").cloned().unwrap_or_default();
        merge_chapter_facts(&mut s, ch, &fm, &summary).unwrap();
        bump_revision(&mut s);
    }
    let expected = load_state(&corpus("015-commit-2")).unwrap();
    assert_eq!(s, expected);
}

#[test]
fn merge_unknown_ids_warn_like_engine() {
    let mut s = new_state(Path::new("书"));
    let mut fm = std::collections::HashMap::new();
    fm.insert("promises_advanced".to_owned(), "P-999".to_owned());
    fm.insert("info_gaps_revealed".to_owned(), "S-999".to_owned());
    let warns = merge_chapter_facts(&mut s, 1, &fm, "").unwrap();
    assert_eq!(warns.len(), 2);
    assert!(warns[0].contains("P-999"));
    assert!(warns[1].contains("S-999"));
}

#[test]
fn merge_summary_truncates_by_codepoint_not_bytes() {
    // 引擎 summary[:80] 码点截断；Rust truncate 按字节会在多字节字符边界 panic——字符级截断
    let mut s = new_state(Path::new("书"));
    let fm = std::collections::HashMap::new();
    // 30 个汉字 = 90 字节 > 80 字节，字符数 30 < 80 码点
    let long_summary = "长".repeat(30);
    merge_chapter_facts(&mut s, 1, &fm, &long_summary).unwrap();
    assert_eq!(s.timeline[0].event, long_summary); // 码点截断:30 字不截
    let longer = "长".repeat(100);
    merge_chapter_facts(&mut s, 2, &fm, &longer).unwrap();
    assert_eq!(s.timeline[1].event.chars().count(), 80); // 截到 80 码点
}

#[test]
fn merge_non_numeric_chapter_key_fail_closed() {
    // 引擎 int(k) 抛 ValueError；本实现 BookCorrupted
    let mut s = new_state(Path::new("书"));
    s.chapters.insert("abc".to_owned(), lingbi_book::tracking::ChapterEntry {
        status: "committed".to_owned(),
        title: "x".to_owned(),
        words: "1".to_owned(),
    });
    let fm = std::collections::HashMap::new();
    let err = merge_chapter_facts(&mut s, 2, &fm, "").unwrap_err();
    assert_eq!(err.code, lingbi_contracts::ErrorCode::BookCorrupted);
}

#[test]
fn queries_like_engine() {
    let mut s = new_state(Path::new("书"));
    let mut fm = std::collections::HashMap::new();
    fm.insert("promises_new".to_owned(), "P-001,P-002".to_owned());
    fm.insert("info_gaps_new".to_owned(), "S-001".to_owned());
    merge_chapter_facts(&mut s, 1, &fm, "事件").unwrap();
    assert_eq!(active_promises(&s).len(), 2);
    assert_eq!(unrevealed_gaps(&s).len(), 1);
    fm.insert("promises_done".to_owned(), "P-001".to_owned());
    merge_chapter_facts(&mut s, 2, &fm, "").unwrap();
    assert_eq!(active_promises(&s).len(), 1);
    assert_eq!(active_promises(&s)[0].0, "P-002");
}

#[test]
fn revision_zero_passthrough_like_engine() {
    // 引擎 int(s.get("state_revision", 1)) 原样返回 0/负数；bump 继承
    let mut s = new_state(Path::new("书"));
    s.state_revision = 0;
    assert_eq!(revision(&s), 0);
    assert_eq!(bump_revision(&mut s), 1);
    s.state_revision = -5;
    assert_eq!(revision(&s), -5);
    assert_eq!(bump_revision(&mut s), -4);
}

#[test]
fn load_missing_scalar_keys_default_like_engine() {
    // 引擎: 缺失顶层键从 base 补全（spec=1/imported=0/state_revision=1）
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("书");
    std::fs::create_dir_all(root.join("追踪")).unwrap();
    std::fs::write(
        root.join("追踪/_tracking-state.json"),
        "{\"chapters\":{}}",
    )
    .unwrap();
    let s = load_state(&root).unwrap();
    assert_eq!(s.spec, 1);
    assert_eq!(s.book, "书"); // 缺失 book → basename 补全
    assert_eq!(s.imported_through_chapter, 0);
    assert_eq!(s.state_revision, 1);
}

#[test]
fn dump_reveals_float_raw_like_engine() {
    // 引擎 json.dump 浮点输出裸数字（不带引号）
    let mut s = new_state(Path::new("书"));
    s.reader_knowledge.reveals.push(serde_json::json!(3.5));
    let dumped = dump_state(&s);
    assert!(dumped.contains("\"reveals\": [\n   3.5\n  ]") || dumped.contains("3.5"));
    assert!(!dumped.contains("\"3.5\""));
}

#[test]
fn tracking_serde_roundtrip_via_serde_json() {
    let s = load_state(&corpus("015-commit-2")).unwrap();
    let json = serde_json::to_string(&s).unwrap();
    let back: TrackingState = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}
