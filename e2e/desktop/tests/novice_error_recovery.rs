//! Novice error-recovery golden path (Task 22).
//!
//! Every failure scenario a normal user can hit must behave the same way:
//! 应用不崩 · 正文不丢 · 错误可识别（typed code，UI 映射中文提示）· 有下一步按钮。
//!
//! Scenarios: AI 401 / 429 / 500 / network down / generation interrupted /
//! user cancel / external file modification / disk write failure /
//! abnormal app close (crash failpoints, covered in
//! recovery_crash_failpoints.rs — referenced here).

use lingbi_ai::{AiError, CancellationToken, FakeProvider};
use lingbi_application::{
    CreateProjectRequest, DocumentApplicationService, GenerationService, ProjectApplicationService,
};
use lingbi_contracts::{AppError, ErrorCode};
use std::sync::Arc;
use tempfile::TempDir;

async fn setup() -> (
    TempDir,
    lingbi_application::ProjectSessionSnapshot,
    Arc<DocumentApplicationService>,
) {
    let temp = TempDir::new().expect("temp dir");
    let service = ProjectApplicationService::new();
    let snapshot = service
        .create_project(CreateProjectRequest {
            name: "错误恢复测试".to_owned(),
            root: temp.path().join("novel"),
        })
        .await
        .expect("create");
    let documents = Arc::new(DocumentApplicationService::new(temp.path().join("novel")));
    (temp, snapshot, documents)
}

/// Typed codes the desktop UI renders as 中文 + 下一步按钮. Every scenario
/// error must land on one of these (Task 9 / Task 22 contract).
fn is_humanized_code(code: ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::AiAuthFailed
            | ErrorCode::AiNoApiKey
            | ErrorCode::AiRateLimited
            | ErrorCode::AiServerError
            | ErrorCode::AiTimeout
            | ErrorCode::AiNetworkError
            | ErrorCode::AiInvalidResponse
            | ErrorCode::AiCancelled
            | ErrorCode::DocumentConflict
            | ErrorCode::CandidateStale
    )
}

fn assert_humanized(error: &AppError, what: &str) {
    assert!(
        is_humanized_code(error.code),
        "{what}: error code {} must be typed for the humanized UI, message: {}",
        error.code as u32,
        error.message
    );
}

#[tokio::test]
async fn ai_401_fails_typed_and_keeps_document_intact() {
    let (temp, snapshot, documents) = setup().await;
    let provider = Arc::new(FakeProvider::with_error(AiError::AuthFailed));
    let generation = GenerationService::new(temp.path().join("novel"), provider, documents.clone());

    let error = generation
        .generate(snapshot.current_document.id, "写")
        .await
        .expect_err("401 must fail");
    assert_humanized(&error, "AI 401");

    assert!(
        generation
            .list(snapshot.current_document.id)
            .expect("list")
            .is_empty()
    );
    assert_eq!(
        documents
            .read_document(snapshot.current_document.id)
            .await
            .expect("read"),
        "# 第一章\n\n",
        "body must be untouched"
    );
}

#[tokio::test]
async fn ai_429_fails_typed_and_keeps_document_intact() {
    let (temp, snapshot, documents) = setup().await;
    let provider = Arc::new(FakeProvider::with_error(AiError::RateLimited));
    let generation = GenerationService::new(temp.path().join("novel"), provider, documents.clone());

    let error = generation
        .generate(snapshot.current_document.id, "写")
        .await
        .expect_err("429 must fail");
    assert_humanized(&error, "AI 429");
    assert_eq!(
        documents
            .read_document(snapshot.current_document.id)
            .await
            .expect("read"),
        "# 第一章\n\n"
    );
}

#[tokio::test]
async fn ai_500_fails_typed_and_keeps_document_intact() {
    let (temp, snapshot, documents) = setup().await;
    let provider = Arc::new(FakeProvider::with_error(AiError::Server(500)));
    let generation = GenerationService::new(temp.path().join("novel"), provider, documents.clone());

    let error = generation
        .generate(snapshot.current_document.id, "写")
        .await
        .expect_err("500 must fail");
    assert_humanized(&error, "AI 500");
    assert_eq!(
        documents
            .read_document(snapshot.current_document.id)
            .await
            .expect("read"),
        "# 第一章\n\n"
    );
}

#[tokio::test]
async fn network_down_fails_typed_and_keeps_document_intact() {
    let (temp, snapshot, documents) = setup().await;
    let provider = Arc::new(FakeProvider::with_error(AiError::Network));
    let generation = GenerationService::new(temp.path().join("novel"), provider, documents.clone());

    let error = generation
        .generate(snapshot.current_document.id, "写")
        .await
        .expect_err("network must fail");
    assert_humanized(&error, "network down");
    assert_eq!(
        documents
            .read_document(snapshot.current_document.id)
            .await
            .expect("read"),
        "# 第一章\n\n"
    );
}

#[tokio::test]
async fn generation_interrupted_mid_stream_writes_no_partial_content() {
    let (temp, snapshot, documents) = setup().await;
    // Provider that yields one delta then a network error: the partial
    // text must NOT become a candidate or touch the document.
    let provider = Arc::new(InterruptedProvider);
    let generation = GenerationService::new(temp.path().join("novel"), provider, documents.clone());

    let error = generation
        .generate(snapshot.current_document.id, "写")
        .await
        .expect_err("interrupted generation must fail");
    assert_humanized(&error, "interrupted");
    assert!(
        generation
            .list(snapshot.current_document.id)
            .expect("list")
            .is_empty(),
        "no candidate may exist after interruption"
    );
    assert_eq!(
        documents
            .read_document(snapshot.current_document.id)
            .await
            .expect("read"),
        "# 第一章\n\n",
        "no partial AI content may reach the document"
    );
}

#[tokio::test]
async fn user_cancel_is_typed_and_keeps_document_intact() {
    let (temp, snapshot, documents) = setup().await;
    let provider = Arc::new(FakeProvider::new("不该落盘的正文"));
    let generation = GenerationService::new(temp.path().join("novel"), provider, documents.clone());
    let cancel = CancellationToken::new();
    cancel.cancel();

    let error = generation
        .generate_with_cancel_stream(
            snapshot.current_document.id,
            "写",
            cancel,
            tokio::sync::mpsc::unbounded_channel().0,
        )
        .await
        .expect_err("cancelled generation must fail");
    assert_humanized(&error, "cancel");
    assert!(
        generation
            .list(snapshot.current_document.id)
            .expect("list")
            .is_empty()
    );
    assert_eq!(
        documents
            .read_document(snapshot.current_document.id)
            .await
            .expect("read"),
        "# 第一章\n\n"
    );
}

#[tokio::test]
async fn external_file_modification_is_detected_and_preserved() {
    let (temp, snapshot, documents) = setup().await;
    documents
        .save_document(
            snapshot.current_document.id,
            snapshot.current_document.revision,
            "应用保存的正文",
        )
        .await
        .expect("save");

    // External edit: user content must win.
    std::fs::write(
        temp.path()
            .join("novel/chapters")
            .join(format!("{}.md", snapshot.current_document.id)),
        "用户外部编辑",
    )
    .expect("external edit");

    let error = documents
        .save_document(snapshot.current_document.id, 1, "应用试图覆盖")
        .await
        .expect_err("stale save must fail");
    assert_humanized(&error, "external modification");
    assert_eq!(
        documents
            .read_document(snapshot.current_document.id)
            .await
            .expect("read"),
        "用户外部编辑",
        "user bytes must be preserved"
    );
}

#[tokio::test]
async fn disk_write_failure_is_typed_and_preserves_content() {
    let (temp, snapshot, documents) = setup().await;
    documents
        .save_document(
            snapshot.current_document.id,
            snapshot.current_document.revision,
            "原始正文",
        )
        .await
        .expect("save");

    let body = temp
        .path()
        .join("novel/chapters")
        .join(format!("{}.md", snapshot.current_document.id));
    make_read_only(&body);
    let result = documents
        .save_document(snapshot.current_document.id, 1, "写不进去的正文")
        .await;
    make_writable(&body);

    // POSIX may replace the file atomically; Windows fails. Either way the
    // bytes on disk must be exactly one of the two versions, never garbage.
    let on_disk = std::fs::read_to_string(&body).expect("read body");
    assert!(
        on_disk == "原始正文" || on_disk == "写不进去的正文",
        "disk content corrupted: {on_disk}"
    );
    if let Err(error) = result {
        assert_humanized(&error, "disk write failure");
    }
}

#[cfg(windows)]
fn make_read_only(path: &std::path::Path) {
    let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(path, permissions).expect("set readonly");
}

#[cfg(not(windows))]
fn make_read_only(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o444);
    std::fs::set_permissions(path, permissions).expect("set readonly");
}

#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
// Windows has no PermissionsExt (unstable); the readonly bit is the only
// permission, so clearing it is the correct way to make the file writable.
fn make_writable(path: &std::path::Path) {
    let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
    permissions.set_readonly(false);
    std::fs::set_permissions(path, permissions).expect("set writable");
}

#[cfg(not(windows))]
fn make_writable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o644);
    std::fs::set_permissions(path, permissions).expect("set writable");
}

/// Emits one delta, then fails with a network error mid-stream.
struct InterruptedProvider;

impl lingbi_ai::AiProvider for InterruptedProvider {
    fn provider_id(&self) -> &str {
        "interrupted"
    }
    fn model_id(&self) -> &str {
        "interrupted-model"
    }
    fn stream_chat(&self, _request: lingbi_ai::ChatRequest) -> lingbi_ai::AiStream {
        Box::pin(async_stream::stream! {
            yield Ok(lingbi_ai::AiEvent::ContentDelta("半截正文".to_owned()));
            yield Err(AiError::Network);
        })
    }
}
