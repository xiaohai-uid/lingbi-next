//! lingbi-migration 五件套测试（票 1：#37 storyrepo-legacy 分类 + 反例）。

use lingbi_migration::{migrate_book, Classification, MigrationInspector, MigrationReceipt};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// corpus 000-initial 快照（字节锁定，真实零界道种同构）作 storyrepo-legacy 源。
fn corpus_initial() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/storyrepo-oracle/153eb63/steps/000-initial/book")
}

fn corpus_copy() -> (TempDir, PathBuf) {
    let temp = TempDir::new().expect("temp");
    let source = temp.path().join("source");
    copy_tree(&corpus_initial(), &source);
    (temp, source)
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create");
    for entry in fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let dst = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &dst);
        } else {
            fs::copy(entry.path(), dst).expect("copy");
        }
    }
}

fn assert_receipt_fields(receipt: &MigrationReceipt) {
    assert!(!receipt.source_hash.is_empty(), "source_hash");
    assert_eq!(receipt.classification, "storyrepo-legacy", "classification");
    assert!(!receipt.destination.is_empty(), "destination");
    assert!(!receipt.destination_hash.is_empty(), "destination_hash");
    assert!(!receipt.migration_rule.is_empty(), "migration_rule");
    assert!(
        receipt
            .migration_rule
            .iter()
            .any(|r| r.rule == "generate-book-id" && r.source == "book.yaml"),
        "book.yaml rule: {:?}",
        receipt.migration_rule
    );
    // warnings 字段存在（可为空）
    let _ = &receipt.warnings;
}

#[test]
fn legacy_book_migrates_losslessly_with_six_field_receipt() {
    let (temp, source) = corpus_copy();
    let destination = temp.path().join("v3");

    let receipt = migrate_book(&source, &destination).expect("migrate");

    assert_receipt_fields(&receipt);

    // 正文 byte-for-byte：源与目标逐文件相等
    for rel in ["定稿/正文/001-开局.md", "定稿/正文/002-夜访.md", "追踪/_tracking-state.json"] {
        assert_eq!(
            fs::read(source.join(rel)).expect("source"),
            fs::read(destination.join(rel)).expect("dest"),
            "byte-for-byte: {rel}"
        );
    }

    // .lingbi/ 六目录就位 + 回执落盘
    for dir in ["transactions", "candidates", "approvals", "receipts", "checkpoints", "versions"] {
        assert!(destination.join(".lingbi").join(dir).is_dir(), ".lingbi/{dir}");
    }
    let on_disk: serde_json::Value = serde_json::from_slice(
        &fs::read(destination.join(".lingbi/migration-receipt.json")).expect("receipt file"),
    )
    .expect("receipt json");
    assert_eq!(on_disk["classification"], "storyrepo-legacy");

    // book.yaml 补 id(uuid)
    let book_yaml = fs::read_to_string(destination.join("book.yaml")).expect("book.yaml");
    assert!(book_yaml.lines().any(|l| l.starts_with("id: ")), "id injected");
    // 其余键原样保留
    assert!(book_yaml.contains("name: 样板书"), "name kept");

    // source READ ONLY：迁移后源树哈希 == Inspector 迁移前值
    let report = MigrationInspector::inspect(&source).expect("re-inspect");
    assert_eq!(report.source_hash, receipt.source_hash, "source READ ONLY");
}

#[test]
fn legacy_tracking_state_revision_untouched() {
    // 迁移不改引擎语义：state_revision 原样（不是事务 bump）
    let (temp, source) = corpus_copy();
    let destination = temp.path().join("v3");
    let _ = migrate_book(&source, &destination).expect("migrate");
    let state: serde_json::Value = serde_json::from_slice(
        &fs::read(destination.join("追踪/_tracking-state.json")).expect("state"),
    )
    .expect("state json");
    assert_eq!(state["state_revision"], 1, "engine revision untouched");
}

#[test]
fn destination_exists_non_empty_rejected() {
    let (temp, source) = corpus_copy();
    let destination = temp.path().join("occupied");
    fs::create_dir_all(destination.join("大纲")).expect("create");
    fs::write(destination.join("book.yaml"), "---\n").expect("write");

    let err = migrate_book(&source, &destination).expect_err("must fail");
    assert_eq!(err.code, lingbi_contracts::ErrorCode::ProjectPathExists);
}

#[test]
fn unsupported_source_rejected() {
    let temp = TempDir::new().expect("temp");
    let source = temp.path().join("unknown");
    fs::create_dir_all(&source).expect("create");
    fs::write(source.join("random.txt"), "x").expect("write");

    let err = migrate_book(&source, &temp.path().join("v3")).expect_err("must fail");
    assert_eq!(err.code, lingbi_contracts::ErrorCode::ImportUnsupportedFormat);
}

#[test]
fn missing_book_yaml_rejected() {
    let temp = TempDir::new().expect("temp");
    let source = temp.path().join("broken");
    // 有追踪但无 book.yaml → 无法识别（fail-closed）
    fs::create_dir_all(source.join("追踪")).expect("create");
    fs::write(source.join("追踪/_tracking-state.json"), "{}").expect("write");

    let err = migrate_book(&source, &temp.path().join("v3")).expect_err("must fail");
    assert_eq!(err.code, lingbi_contracts::ErrorCode::ImportUnsupportedFormat);
}

#[test]
fn legacy_book_yaml_crlf_gets_id_injected() {
    // CRLF 书仓（Windows 用户可能产生）：book.yaml 补 id 不破坏结构
    let (temp, source) = corpus_copy();
    let book_yaml = source.join("book.yaml");
    let lf = fs::read_to_string(&book_yaml).expect("read");
    fs::write(&book_yaml, lf.replace('\n', "\r\n")).expect("crlf");
    let destination = temp.path().join("v3");

    let _ = migrate_book(&source, &destination).expect("migrate");

    let out = fs::read_to_string(destination.join("book.yaml")).expect("book.yaml");
    assert!(out.lines().any(|l| l.starts_with("id: ")), "id injected: {out}");
    // 结构仍合法：--- 开头 + name 保留
    assert!(out.starts_with("---"), "frontmatter start: {out}");
    assert!(out.contains("name: 样板书"), "name kept");
}

#[test]
fn classification_detects_legacy_vs_v1() {
    let (temp, source) = corpus_copy();
    let report = MigrationInspector::inspect(&source).expect("inspect");
    assert_eq!(report.classification, Classification::StoryrepoLegacy);

    // flutter-v1 形态（chapters/ + .lingbi/project.json）→ FlutterV1
    let v1 = temp.path().join("v1src");
    fs::create_dir_all(v1.join(".lingbi")).expect("create");
    fs::create_dir_all(v1.join("chapters")).expect("create");
    fs::write(
        v1.join(".lingbi/project.json"),
        r#"{"schemaVersion":1,"name":"旧项目"}"#,
    )
    .expect("write");
    fs::write(v1.join("chapters/第一章.md"), "正文").expect("write");
    let report = MigrationInspector::inspect(&v1).expect("inspect");
    assert_eq!(report.classification, Classification::FlutterV1);
}
