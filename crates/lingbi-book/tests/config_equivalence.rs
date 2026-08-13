//! P2-01 config 模块等价测试：book.yaml 解析与 storyrepo 引擎 config.py 行为一致。
//!
//! 基准：golden corpus `fixtures/storyrepo-oracle/153eb63/steps/000-initial/book/book.yaml`。
//! 语义来源：`engine/storyrepo/config.py`（平铺 KV、--- 分节、数值转 int、默认合并、max<min 修正）。

use lingbi_book::{load_config, BookConfig};
use std::path::{Path, PathBuf};

fn corpus_book_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/storyrepo-oracle/153eb63/steps/000-initial/book")
}

fn write_book_yaml(dir: &Path, content: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("book.yaml"), content).unwrap();
}

#[test]
fn corpus_book_yaml_equivalence() {
    // 引擎期望（黑盒记录）：name=样板书 / status=writing / total=4 / min=100 / max=300 / target=200000
    let cfg = load_config(&corpus_book_root()).expect("corpus book.yaml 应可解析");
    assert_eq!(cfg.name, "样板书");
    assert_eq!(cfg.status.as_deref(), Some("writing"));
    assert_eq!(cfg.total_chapters, 4);
    assert_eq!(cfg.min_words_per_chapter, 100);
    assert_eq!(cfg.max_words_per_chapter, 300);
    assert_eq!(cfg.target_words, 200000);
}

#[test]
fn defaults_merge_like_engine() {
    // 引擎 test_foundation 的 max=2800 来自 make_book 显式写入(min+800)；
    // 未写 max 时保留默认 3800，且 3800<2000 不触发修正。
    let dir = tempfile::tempdir().unwrap();
    write_book_yaml(
        dir.path(),
        "---\nname: 测试书\ntotal_chapters: 120\nmin_words_per_chapter: 2000\n---\n",
    );
    let cfg = load_config(dir.path()).unwrap();
    assert_eq!(cfg.name, "测试书");
    assert_eq!(cfg.total_chapters, 120);
    assert_eq!(cfg.min_words_per_chapter, 2000);
    assert_eq!(cfg.max_words_per_chapter, 3800); // 默认保留
    assert_eq!(cfg.target_words, 990000); // 默认
}

#[test]
fn max_less_than_min_fixed_like_engine() {
    // 引擎: cfg["max"] < cfg["min"] → max = min + 800
    let dir = tempfile::tempdir().unwrap();
    write_book_yaml(
        dir.path(),
        "name: 书\nmin_words_per_chapter: 5000\nmax_words_per_chapter: 2000\n",
    );
    let cfg = load_config(dir.path()).unwrap();
    assert_eq!(cfg.max_words_per_chapter, 5800); // 5000+800
}

#[test]
fn frontmatter_dashes_stripped_like_engine() {
    // 引擎: t.split("---", 2)[1] —— 带 --- 分节与不带分节等价
    let dir = tempfile::tempdir().unwrap();
    write_book_yaml(dir.path(), "name: 无分节书\nmin_words_per_chapter: 500\n");
    let cfg = load_config(dir.path()).unwrap();
    assert_eq!(cfg.name, "无分节书");
    assert_eq!(cfg.min_words_per_chapter, 500);
}

#[test]
fn quoted_and_int_coercion_like_engine() {
    // 引擎: v.strip("\"'"); 整数正则 fullmatch(r"-?\d+") 才转 int
    let dir = tempfile::tempdir().unwrap();
    write_book_yaml(
        dir.path(),
        "name: \"带引号\"\ntarget_words: 200000\nmax_words_per_chapter: -1\n",
    );
    let cfg = load_config(dir.path()).unwrap();
    assert_eq!(cfg.name, "带引号");
    assert_eq!(cfg.target_words, 200000);
    // max=-1 < min(默认3000) → 修正 min+800 = 3800
    assert_eq!(cfg.max_words_per_chapter, 3800);
}

#[test]
fn unknown_keys_preserved_in_extras() {
    // 引擎: 未知键保留（dict 全量返回）
    let dir = tempfile::tempdir().unwrap();
    write_book_yaml(dir.path(), "name: 书\ncustom_flag: hello\ncustom_num: 42\n");
    let cfg = load_config(dir.path()).unwrap();
    assert_eq!(cfg.extras.get("custom_flag").unwrap(), &serde_json::json!("hello"));
    assert_eq!(cfg.extras.get("custom_num").unwrap(), &serde_json::json!(42));
}

#[test]
fn missing_book_yaml_errors() {
    // 引擎: FileNotFoundError
    let dir = tempfile::tempdir().unwrap();
    let err = load_config(dir.path()).expect_err("缺 book.yaml 应报错");
    assert_eq!(err.code, lingbi_contracts::ErrorCode::BookNotFound);
}

#[test]
fn numeric_key_with_non_int_value_is_fail_closed() {
    // 引擎宽容(存字符串)；本实现 fail-closed（仓库纪律）→ 显式拒绝
    let dir = tempfile::tempdir().unwrap();
    write_book_yaml(dir.path(), "name: 书\ntarget_words: 99万\n");
    let err = load_config(dir.path()).expect_err("数值键收到非整数应报错");
    assert_eq!(err.code, lingbi_contracts::ErrorCode::BookCorrupted);
}

#[test]
fn numeric_overflow_is_fail_closed_not_panic() {
    // 引擎 Python int 任意精度可解析；Rust i64 溢出 → BookCorrupted（绝不 panic）
    let dir = tempfile::tempdir().unwrap();
    write_book_yaml(
        dir.path(),
        "name: 书\ntotal_chapters: 99999999999999999999999\n",
    );
    let err = load_config(dir.path()).expect_err("i64 溢出应报错而非崩溃");
    assert_eq!(err.code, lingbi_contracts::ErrorCode::BookCorrupted);
}

#[test]
fn config_serde_roundtrip() {
    let cfg = BookConfig {
        name: "样板书".into(),
        status: Some("writing".into()),
        id: None,
        total_chapters: 4,
        min_words_per_chapter: 100,
        max_words_per_chapter: 300,
        target_words: 200000,
        extras: Default::default(),
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let back: BookConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg, back);
}
