//! P2-02 book state 模块等价测试：章节文件/正文/front matter/hanzi 与引擎 state.py 一致。
//!
//! 基准：golden corpus `fixtures/storyrepo-oracle/153eb63/steps/000-initial/book`。

use lingbi_book::state::{body, chapter_files, chapter_num, fm_list, frontmatter, hanzi, latest_chapter};
use std::path::{Path, PathBuf};

fn corpus_book_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/storyrepo-oracle/153eb63/steps/000-initial/book")
}

fn corpus_ch1() -> PathBuf {
    corpus_book_root().join("定稿/正文/001-开局.md")
}

#[test]
fn chapter_files_sorted_like_engine() {
    // 引擎: sorted(f for f in os.listdir(定稿/正文) if re.match(r"\d{3,4}-", f) and f.endswith(".md"))
    let files = chapter_files(&corpus_book_root());
    assert_eq!(files, vec!["001-开局.md".to_owned(), "002-夜访.md".to_owned()]);
}

#[test]
fn chapter_files_empty_when_missing_dir() {
    let dir = tempfile::tempdir().unwrap();
    assert!(chapter_files(dir.path()).is_empty());
}

#[test]
fn chapter_num_like_engine() {
    assert_eq!(chapter_num("001-开局.md"), 1);
    assert_eq!(chapter_num("1234-长编号.md"), 1234); // 引擎 \d{3,4}
    assert_eq!(chapter_num("开局.md"), -1); // 引擎 -1
    assert_eq!(chapter_num("abc.md"), -1);
    // 边界：\d{3,4} 贪婪后回退失败 → -1（引擎 re.match 同）
    assert_eq!(chapter_num("12345-章.md"), -1);
    assert_eq!(chapter_num("12-章.md"), -1);
    assert_eq!(chapter_num("0000-章.md"), 0); // 4 位 0 合法
}

#[test]
fn latest_chapter_like_engine() {
    assert_eq!(latest_chapter(&corpus_book_root()), 2);
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(latest_chapter(dir.path()), 0); // 引擎 default=0
}

#[test]
fn frontmatter_parses_like_engine() {
    // 引擎: --- 开头 + seg 含 ':' 才解析；平铺 KV
    let fm = frontmatter(&corpus_ch1()).unwrap();
    assert_eq!(fm.get("title").map(String::as_str), Some("开局"));
    assert_eq!(fm.get("chapter").map(String::as_str), Some("001"));
    assert_eq!(fm.get("volume").map(String::as_str), Some("1"));
    assert_eq!(fm.get("words").map(String::as_str), Some("66"));
    assert_eq!(fm.get("status").map(String::as_str), Some("定稿"));
    assert_eq!(fm.get("promises_new").map(String::as_str), Some("P-001"));
    assert_eq!(fm.get("entities_new").map(String::as_str), Some("阿米,米粒"));
    assert!(fm.get("summary").unwrap().contains("阿米夜归"));
}

#[test]
fn frontmatter_guard_like_engine() {
    // 引擎防护: 非 --- 开头 → {};seg 无冒号 → {}
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("no-fm.md");
    std::fs::write(&p, "普通正文，无 front matter。\n").unwrap();
    assert!(frontmatter(&p).unwrap().is_empty());
    let p2 = dir.path().join("dash-no-colon.md");
    std::fs::write(&p2, "---\n这行没有冒号\n---\n\n正文\n").unwrap();
    assert!(frontmatter(&p2).unwrap().is_empty());
}

#[test]
fn body_strips_frontmatter_like_engine() {
    // 引擎: re.sub(r"^---.*?---\n", "", t, re.S) —— 非贪婪剥第一个 --- 块
    let b = body(&corpus_ch1()).unwrap();
    assert!(b.starts_with("\n山门之外，夜色如墨。"));
    assert!(b.contains("他笑了笑，说:今夜有客。"));
    assert!(!b.contains("title: 开局"));
}

#[test]
fn body_without_frontmatter_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("plain.md");
    std::fs::write(&p, "纯正文内容\n").unwrap();
    assert_eq!(body(&p).unwrap(), "纯正文内容\n");
}

#[test]
fn body_unclosed_frontmatter_unchanged_like_engine() {
    // 引擎 re.sub 无闭合 --- 不匹配 → 原样
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("unclosed.md");
    std::fs::write(&p, "---\ntitle: 未闭合\n正文里有 --- 符号。\n").unwrap();
    assert_eq!(body(&p).unwrap(), "---\ntitle: 未闭合\n正文里有 --- 符号。\n");
}

#[test]
fn missing_file_is_fail_closed() {
    // 引擎 raise FileNotFoundError；本实现 BookNotFound（不静默空值）
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("不存在.md");
    assert_eq!(
        body(&p).unwrap_err().code,
        lingbi_contracts::ErrorCode::BookNotFound
    );
    assert_eq!(
        frontmatter(&p).unwrap_err().code,
        lingbi_contracts::ErrorCode::BookNotFound
    );
}

#[test]
fn hanzi_counts_only_cjk_like_engine() {
    // 引擎: len(re.findall(r"[\u4e00-\u9fff]", text))
    assert_eq!(hanzi("山门之外，夜色如墨。"), 8); // 山门之外(4)+夜色如墨(4)
    assert_eq!(hanzi("abc123 汉字"), 2);
    assert_eq!(hanzi(""), 0);
    // corpus 精确值（引擎黑盒实测）：001=56 / 002=46
    let b1 = body(&corpus_ch1()).unwrap();
    assert_eq!(hanzi(&b1), 56);
    let b2 = body(&corpus_book_root().join("定稿/正文/002-夜访.md")).unwrap();
    assert_eq!(hanzi(&b2), 46);
}

#[test]
fn fm_list_splits_like_engine() {
    // 引擎: 逗号分隔 + 去空 + 剔除 ""/'' 占位
    let mut fm = std::collections::HashMap::new();
    fm.insert("a".to_owned(), "P-001,S-001, 米粒".to_owned());
    fm.insert("b".to_owned(), "".to_owned());
    fm.insert("c".to_owned(), "\"\"".to_owned());
    assert_eq!(fm_list(&fm, "a"), vec!["P-001", "S-001", "米粒"]);
    assert!(fm_list(&fm, "b").is_empty());
    assert!(fm_list(&fm, "c").is_empty());
    assert!(fm_list(&fm, "不存在").is_empty());
}
