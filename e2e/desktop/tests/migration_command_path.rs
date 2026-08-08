//! P6 桌面 E2E：迁移命令路径（#37 验收②）。
//!
//! corpus 000-initial 快照（storyrepo-legacy 形态，真实《零界道种》同构）
//! → `migrate_book` 命令 → 断言：六字段回执、source 只读（哈希不变）、
//! 正文 byte-for-byte、`.lingbi/` 六目录就位、迁移后 open_book 可打开。
//!
//! 命令层（lingbi-commands）即生产 Tauri 命令背后的同一实现（见 P4 注释）。

use lingbi_commands::*;
use std::path::{Path, PathBuf};

fn corpus_initial() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/storyrepo-oracle/153eb63/steps/000-initial/book")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create");
    for entry in std::fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let dst = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &dst);
        } else {
            std::fs::copy(entry.path(), dst).expect("copy");
        }
    }
}

#[test]
fn migrate_book_command_path_legacy() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let source = temp.path().join("source");
    copy_tree(&corpus_initial(), &source);
    let destination = temp.path().join("v3");

    let receipt = migrate_book(&source.to_string_lossy(), &destination.to_string_lossy())
        .expect("migrate_book");

    // 回执六字段
    assert!(!receipt.source_hash.is_empty());
    assert_eq!(receipt.classification, "storyrepo-legacy");
    assert_eq!(receipt.destination, destination.to_string_lossy());
    assert!(!receipt.destination_hash.is_empty());
    assert!(receipt.migration_rule.iter().any(|r| r.rule == "generate-book-id"));
    // warnings 字段存在

    // 正文 byte-for-byte + .lingbi 六目录
    for rel in ["定稿/正文/001-开局.md", "定稿/正文/002-夜访.md"] {
        assert_eq!(
            std::fs::read(source.join(rel)).expect("source"),
            std::fs::read(destination.join(rel)).expect("dest"),
            "byte-for-byte: {rel}"
        );
    }
    for dir in ["transactions", "candidates", "approvals", "receipts", "checkpoints", "versions"] {
        assert!(destination.join(".lingbi").join(dir).is_dir(), ".lingbi/{dir}");
    }

    // source READ ONLY：迁移后源哈希 == 回执 source_hash
    let report = lingbi_migration::MigrationInspector::inspect(&source).expect("inspect");
    assert_eq!(report.source_hash, receipt.source_hash, "source READ ONLY");

    // 迁移后 open_book 可打开（托管书仓）；章节枚举走目录扫描
    // （000-initial 追踪为初始状态 chapters={}，open_book 的 chapter_count
    // 读追踪——0 是引擎语义；实际章节由 list_chapters 目录枚举给出）
    let opened = open_book(&destination.to_string_lossy()).expect("open migrated book");
    assert_eq!(opened.managed, true, "migrated book is managed");
    let chapters = list_chapters(&destination.to_string_lossy()).expect("list chapters");
    assert_eq!(chapters.len(), 2, "two chapters migrated");
    assert_eq!(chapters[0].ch, 1);
    assert_eq!(chapters[0].title, "开局");
}

#[test]
fn migrate_book_destination_conflict_rejected() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    let source = temp.path().join("source");
    copy_tree(&corpus_initial(), &source);
    let destination = temp.path().join("occupied");
    std::fs::create_dir_all(destination.join("大纲")).expect("create");

    let err = migrate_book(&source.to_string_lossy(), &destination.to_string_lossy())
        .expect_err("must fail");
    assert_eq!(err.code, "ProjectPathExists");
}
