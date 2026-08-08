//! flutter-v1 迁移测试（票 2：#38）——章节 frontmatter 补全 + 正文段
//! byte-for-byte + 资源归档 + 回执 rule + 反例。

use lingbi_migration::{migrate_book, MigrationInspector};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn v1_source(temp: &TempDir) -> PathBuf {
    let source = temp.path().join("v1src");
    fs::create_dir_all(source.join(".lingbi")).expect("meta dir");
    fs::create_dir_all(source.join("章节内容")).expect("chapters dir");
    fs::create_dir_all(source.join("小说资料")).expect("resources dir");
    fs::write(
        source.join(".lingbi/project.json"),
        r#"{"schemaVersion":1,"id":"legacy","name":"旧版玄幻项目","genre":"玄幻"}"#,
    )
    .expect("metadata");
    // 无 frontmatter 的章节（V1 典型形态）→ 迁移需补 V3 六字段头
    fs::write(source.join("章节内容/第一章.md"), "# 第一章\n\n雨夜，林渊推开旧车站的门。")
        .expect("ch1");
    fs::write(source.join("章节内容/第二章.md"), "# 第二章\n\n正文 B").expect("ch2");
    fs::write(source.join("小说资料/世界观.md"), "世界观资料").expect("world");
    source
}

#[test]
fn flutter_v1_migrates_chapters_with_frontmatter_and_archives_resources() {
    let temp = TempDir::new().expect("temp");
    let source = v1_source(&temp);
    let destination = temp.path().join("v3");

    let receipt = migrate_book(&source, &destination).expect("migrate");

    // 回执：classification + migration_rule 如实记录
    assert_eq!(receipt.classification, "flutter-v1");
    assert!(
        receipt
            .migration_rule
            .iter()
            .any(|r| r.rule == "frontmatter-补全" && r.destination.starts_with("定稿/正文/")),
        "chapter rule: {:?}",
        receipt.migration_rule
    );
    assert!(
        receipt
            .migration_rule
            .iter()
            .any(|r| r.rule == "resource-archived"),
        "resource rule: {:?}",
        receipt.migration_rule
    );

    // 章节落 定稿/正文/{ch:03}-{title}.md，正文段 byte-for-byte
    let ch1 = destination.join("定稿/正文/001-第一章.md");
    assert!(ch1.exists(), "001-第一章.md");
    let text = fs::read_to_string(&ch1).expect("ch1");
    assert!(text.starts_with("---\n"), "frontmatter header");
    for field in ["title: 第一章", "chapter: 001", "volume: 1", "words: 0", "status: 草稿", "summary:"] {
        assert!(text.contains(field), "frontmatter field {field}: {text}");
    }
    // 正文段 = 源全文（byte 不变）
    let body_start = text.find("\n---\n\n").expect("body start") + "\n---\n\n".len();
    assert_eq!(
        &text[body_start..],
        "# 第一章\n\n雨夜，林渊推开旧车站的门。",
        "body byte-for-byte"
    );
    let ch2 = destination.join("定稿/正文/002-第二章.md");
    assert!(ch2.exists(), "002-第二章.md");

    // 资源归档到 工作区/迁移存档/ 且字节不变
    let archived = destination.join("工作区/迁移存档/小说资料/世界观.md");
    assert_eq!(
        fs::read(&archived).expect("archived"),
        fs::read(source.join("小说资料/世界观.md")).expect("source world"),
        "resource bytes"
    );

    // .lingbi 六目录 + 追踪 + book.yaml（name 取自 project.json）
    assert!(destination.join(".lingbi/transactions").is_dir());
    let book_yaml = fs::read_to_string(destination.join("book.yaml")).expect("book.yaml");
    assert!(book_yaml.contains("name: 旧版玄幻项目"), "name from project.json");

    // source READ ONLY
    let report = MigrationInspector::inspect(&source).expect("re-inspect");
    assert_eq!(report.source_hash, receipt.source_hash, "source READ ONLY");
}

#[test]
fn flutter_v1_source_with_frontmatter_preserved_byte_identical() {
    // 源自带 frontmatter：目标文件字节 == 源文件字节（连头都不改）
    let temp = TempDir::new().expect("temp");
    let source = temp.path().join("v1src");
    fs::create_dir_all(source.join(".lingbi")).expect("meta");
    fs::create_dir_all(source.join("chapters")).expect("chapters");
    fs::write(
        source.join(".lingbi/project.json"),
        r#"{"schemaVersion":1,"name":"带头项目"}"#,
    )
    .expect("meta");
    let chapter = "---\ntitle: 第一章\nchapter: 001\nvolume: 1\nwords: 66\nstatus: 定稿\nsummary: x\n---\n\n正文内容";
    fs::write(source.join("chapters/第一章.md"), chapter).expect("chapter");

    let destination = temp.path().join("v3");
    let _ = migrate_book(&source, &destination).expect("migrate");

    assert_eq!(
        fs::read(destination.join("定稿/正文/001-第一章.md")).expect("dest"),
        chapter.as_bytes(),
        "byte identical when source already has frontmatter"
    );
}

#[test]
fn flutter_v1_duplicate_chapter_title_rejected() {
    let temp = TempDir::new().expect("temp");
    let source = temp.path().join("v1src");
    fs::create_dir_all(source.join(".lingbi")).expect("meta");
    fs::create_dir_all(source.join("chapters")).expect("chapters");
    fs::create_dir_all(source.join("章节内容")).expect("章节内容");
    fs::write(
        source.join(".lingbi/project.json"),
        r#"{"schemaVersion":1,"name":"重复项目"}"#,
    )
    .expect("meta");
    // 跨目录同标题（V1 允许，V3 目标命名 {ch:03}-{title} 冲突 → fail-closed）
    fs::write(source.join("chapters/第一章.md"), "A").expect("a");
    fs::write(source.join("章节内容/第一章.md"), "B").expect("b");

    let err = migrate_book(&source, &temp.path().join("v3")).expect_err("must fail");
    assert_eq!(err.code, lingbi_contracts::ErrorCode::ProjectCorrupted);
    assert!(err.message.contains("重复"), "message: {}", err.message);
}
