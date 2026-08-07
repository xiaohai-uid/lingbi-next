//! Windows filesystem realism tests (Task 14).
//!
//! Coverage: Chinese project names, Chinese chapter titles, paths with
//! spaces, Chinese user directories, Documents paths, read-only targets,
//! locked targets, existing same-name projects, existing same-name
//! chapters, rename collisions, external edits.
//!
//! Core principle: FAILING is acceptable — OVERWRITING user content is
//! never acceptable. Every test asserts that a failure leaves the user's
//! bytes intact.

use lingbi_application::{
    CreateProjectRequest, DocumentApplicationService, ProjectApplicationService,
};
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_storage::DiskAtomicFileStore;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use uuid::Uuid;

async fn create_project_at(root: &Path, name: &str) -> (ProjectApplicationService, Uuid) {
    let projects = ProjectApplicationService::new();
    let created = projects
        .create_project(CreateProjectRequest {
            name: name.to_owned(),
            root: root.to_path_buf(),
        })
        .await
        .expect("create project");
    (projects, created.current_document.id)
}

#[tokio::test]
async fn chinese_project_and_chapter_names_round_trip() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("雨夜里的车站");
    let (projects, document_id) = create_project_at(&root, "雨夜里的车站").await;

    let documents = DocumentApplicationService::new(&root);
    documents
        .save_document(document_id, 0, "第一章正文：雨，一直在下。")
        .await
        .expect("save chinese body");

    let chapter = documents
        .create_document(Uuid::new_v4(), "第三章：迷雾", "第三章正文")
        .await
        .expect("create chinese chapter");

    let opened = projects.open_project(root.clone()).await.expect("reopen");
    assert_eq!(opened.project.name, "雨夜里的车站");
    assert_eq!(
        documents.read_document(chapter.id).await.expect("read"),
        "第三章正文"
    );
    let loaded = lingbi_storage::DocumentRepository::new(&root)
        .find(document_id)
        .expect("find")
        .expect("doc");
    assert_eq!(loaded.revision, 1);
}

#[tokio::test]
async fn paths_with_spaces_and_chinese_user_dirs_work() {
    let temp = TempDir::new().expect("temp dir");
    // Simulates "C:\Users\小明\Documents\我的小说" — spaces + Chinese.
    let root = temp
        .path()
        .join("Users")
        .join("小明")
        .join("Documents")
        .join("我的 小说");
    let (_, document_id) = create_project_at(&root, "我的 小说").await;

    let documents = DocumentApplicationService::new(&root);
    documents
        .save_document(document_id, 0, "带空格路径正文")
        .await
        .expect("save with spaces");
    assert_eq!(
        documents.read_document(document_id).await.expect("read"),
        "带空格路径正文"
    );
}

#[tokio::test]
async fn same_name_project_gets_unique_default_root() {
    let temp = TempDir::new().expect("temp dir");
    let projects = ProjectApplicationService::new();
    let first = lingbi_application::unique_root(temp.path(), "同名小说").expect("first");
    projects
        .create_project(CreateProjectRequest {
            name: "同名小说".to_owned(),
            root: first.clone(),
        })
        .await
        .expect("create first");
    let second = lingbi_application::unique_root(temp.path(), "同名小说").expect("second");
    assert_eq!(second, temp.path().join("同名小说-2"));
    projects
        .create_project(CreateProjectRequest {
            name: "同名小说".to_owned(),
            root: second.clone(),
        })
        .await
        .expect("create second");
    // Both projects must be independently openable.
    let first_opened = projects.open_project(first).await.expect("open first");
    let second_opened = projects.open_project(second).await.expect("open second");
    assert_ne!(first_opened.project.id, second_opened.project.id);
    assert_eq!(first_opened.current_document.title, "第一章");
}

#[tokio::test]
async fn same_name_chapter_never_overwrites_existing_chapter() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("novel");
    let (_, document_id) = create_project_at(&root, "novel").await;
    let documents = DocumentApplicationService::new(&root);
    documents
        .save_document(document_id, 0, "第一章：最初的正文")
        .await
        .expect("save first");

    let duplicate = documents
        .create_document(Uuid::new_v4(), "第一章", "另一章")
        .await
        .expect("create duplicate title");

    assert_ne!(
        duplicate.id, document_id,
        "duplicate title gets a new chapter"
    );
    assert_eq!(
        documents
            .read_document(document_id)
            .await
            .expect("read original"),
        "第一章：最初的正文",
        "original chapter content must be untouched"
    );
    assert_eq!(
        documents
            .read_document(duplicate.id)
            .await
            .expect("read dup"),
        "另一章"
    );
}

#[tokio::test]
async fn rename_collision_target_keeps_both_files() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("novel");
    let (_, document_id) = create_project_at(&root, "novel").await;
    let documents = DocumentApplicationService::new(&root);
    documents
        .save_document(document_id, 0, "原本内容")
        .await
        .expect("save");

    // A rename collision would point at the same chapter title; the
    // storage layer must never reuse the same physical file for a new
    // chapter. Simulate by writing another document with the same title
    // and verifying the original file (by id) is still the original.
    let collision = documents
        .create_document(Uuid::new_v4(), "原本内容", "覆盖者")
        .await
        .expect("create");
    let store = DiskAtomicFileStore;
    let original_bytes = lingbi_storage::AtomicFileStore::read(
        &store,
        &root.join(format!("chapters/{document_id}.md")),
    )
    .expect("read original file");
    assert_eq!(String::from_utf8_lossy(&original_bytes), "原本内容");
    let collision_bytes = lingbi_storage::AtomicFileStore::read(
        &store,
        &root.join(format!("chapters/{}.md", collision.id)),
    )
    .expect("read collision file");
    assert_eq!(String::from_utf8_lossy(&collision_bytes), "覆盖者");
}

#[tokio::test]
async fn external_edit_is_detected_and_preserved() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("novel");
    let (_, document_id) = create_project_at(&root, "novel").await;
    let documents = DocumentApplicationService::new(&root);
    documents
        .save_document(document_id, 0, "应用保存的内容")
        .await
        .expect("save");

    // User edits the file outside the app.
    fs::write(
        root.join(format!("chapters/{document_id}.md")),
        "用户外部编辑的内容",
    )
    .expect("external edit");

    let result = documents
        .save_document(document_id, 1, "应用试图覆盖")
        .await;
    assert!(result.is_err(), "save with stale expected hash must fail");
    assert_eq!(
        fs::read_to_string(root.join(format!("chapters/{document_id}.md"))).expect("read body"),
        "用户外部编辑的内容",
        "external user content must never be overwritten"
    );
}

#[tokio::test]
async fn read_only_target_fails_cleanly_and_preserves_content() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("novel");
    let (_, document_id) = create_project_at(&root, "novel").await;
    let documents = DocumentApplicationService::new(&root);
    documents
        .save_document(document_id, 0, "原始正文")
        .await
        .expect("save");
    let body = root.join(format!("chapters/{document_id}.md"));

    make_read_only(&body);
    let result = documents
        .save_document(document_id, 1, "无法写入的内容")
        .await;
    make_writable(&body);

    // Never corrupt: either the platform replaces the file atomically
    // (POSIX rename ignores the read-only bit) with the exact new
    // content, or the save fails cleanly and the original stays.
    match result {
        Ok(document) => {
            assert_eq!(document.revision, 2);
            assert_eq!(
                fs::read_to_string(&body).expect("read body"),
                "无法写入的内容",
                "successful save must contain exactly the new content"
            );
        }
        Err(_) => {
            assert_eq!(
                fs::read_to_string(&body).expect("read body"),
                "原始正文",
                "read-only failure must never corrupt or overwrite content"
            );
        }
    }
}

#[cfg(windows)]
#[tokio::test]
async fn locked_target_fails_cleanly_and_preserves_content() {
    use std::os::windows::fs::OpenOptionsExt;

    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("novel");
    let (_, document_id) = create_project_at(&root, "novel").await;
    let documents = DocumentApplicationService::new(&root);
    documents
        .save_document(document_id, 0, "原始正文")
        .await
        .expect("save");
    let body = root.join(format!("chapters/{document_id}.md"));

    // Hold the file with an exclusive lock (like another app would).
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&body)
        .expect("lock file");
    let result = documents
        .save_document(document_id, 1, "无法写入的内容")
        .await;
    drop(lock);

    assert!(
        result.is_err(),
        "locked target must fail cleanly: {result:?}"
    );
    assert_eq!(
        fs::read_to_string(&body).expect("read body"),
        "原始正文",
        "locked failure must never corrupt or overwrite content"
    );
}

#[tokio::test]
async fn truncated_or_corrupted_target_fails_and_never_overwrites() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("novel");
    let (_, document_id) = create_project_at(&root, "novel").await;
    let documents = DocumentApplicationService::new(&root);
    documents
        .save_document(document_id, 0, "原始正文")
        .await
        .expect("save");
    let body = root.join(format!("chapters/{document_id}.md"));

    // External tool replaced the body with unrelated bytes.
    fs::write(&body, "其他程序写入的数据").expect("external write");
    let result = documents.save_document(document_id, 1, "应用写入").await;

    assert!(matches!(
        result,
        Err(AppError {
            code: ErrorCode::DocumentConflict,
            ..
        })
    ));
    assert_eq!(
        fs::read_to_string(&body).expect("read body"),
        "其他程序写入的数据",
        "unknown external bytes must be preserved"
    );
}

#[cfg(windows)]
fn make_read_only(path: &Path) {
    use std::os::windows::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).expect("set readonly");
}

#[cfg(not(windows))]
fn make_read_only(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o444);
    fs::set_permissions(path, permissions).expect("set readonly");
}

#[cfg(windows)]
fn make_writable(path: &Path) {
    use std::os::windows::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).expect("set writable");
}

#[cfg(not(windows))]
fn make_writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o644);
    fs::set_permissions(path, permissions).expect("set writable");
}

/// Ensure the helper set stays used even when cfg'd platforms differ.
#[allow(dead_code)]
fn _path_helper() -> PathBuf {
    PathBuf::new()
}
