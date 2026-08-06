use lingbi_ai::{AiError, FakeProvider};
use lingbi_application::{
    CreateProjectRequest, DocumentApplicationService, GenerationService, ProjectApplicationService,
};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn offline_manuscript_stays_usable_when_ai_fails() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("novel");
    let projects = ProjectApplicationService::new();
    let created = projects
        .create_project(CreateProjectRequest {
            name: "离线小说".to_owned(),
            root: root.clone(),
        })
        .await
        .expect("create project");
    let documents = Arc::new(DocumentApplicationService::new(root.clone()));

    documents
        .save_document(created.current_document.id, 0, "# 第一章\n\n离线本地正文")
        .await
        .expect("local save must work offline");
    assert_eq!(
        documents
            .read_document(created.current_document.id)
            .await
            .expect("local read must work offline"),
        "# 第一章\n\n离线本地正文"
    );

    let generation = GenerationService::new(
        root.clone(),
        Arc::new(FakeProvider::with_error(AiError::Network)),
        documents.clone(),
    );
    let result = generation
        .generate(created.current_document.id, "在线 AI 生成")
        .await;

    assert!(
        result.is_err(),
        "online-only AI must fail gracefully offline"
    );
    assert!(
        generation
            .list(created.current_document.id)
            .expect("candidate list")
            .is_empty(),
        "failed generation must not create a candidate"
    );
    assert_eq!(
        documents
            .read_document(created.current_document.id)
            .await
            .expect("manuscript must remain readable"),
        "# 第一章\n\n离线本地正文"
    );
}
