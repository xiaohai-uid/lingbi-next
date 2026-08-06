use lingbi_ai::FakeProvider;
use lingbi_application::{
    CreateProjectRequest, DocumentApplicationService, GenerationService, ProjectApplicationService,
};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn real_desktop_golden_path() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("novel");
    let projects = ProjectApplicationService::new();

    let created = projects
        .create_project(CreateProjectRequest {
            name: "测试小说".to_owned(),
            root: root.clone(),
        })
        .await
        .expect("create project");
    assert_eq!(created.current_document.title, "第一章");
    assert!(root.join(created.current_document.physical_path()).exists());

    let documents = Arc::new(DocumentApplicationService::new(root.clone()));
    let provider = Arc::new(FakeProvider::new("第一章正文：雨夜，林渊推开旧车站的门。"));
    let generation = GenerationService::new(root.clone(), provider, documents.clone());

    let candidate = generation
        .generate(created.current_document.id, "生成一个雨夜开场")
        .await
        .expect("generate candidate");
    assert_eq!(candidate.status, "pending");
    assert!(candidate.content.contains("第一章正文"));

    let adopted = generation
        .adopt(candidate.id, 0)
        .await
        .expect("adopt candidate");
    assert_eq!(adopted.revision, 1);
    assert_eq!(
        documents
            .read_document(created.current_document.id)
            .await
            .expect("read adopted manuscript"),
        candidate.content
    );

    let reopened_projects = ProjectApplicationService::new();
    let reopened = reopened_projects
        .open_project(root.clone())
        .await
        .expect("reopen project");
    let reopened_documents = DocumentApplicationService::new(root);
    assert_eq!(
        reopened_documents
            .read_document(reopened.current_document.id)
            .await
            .expect("read reopened manuscript"),
        candidate.content
    );
}
