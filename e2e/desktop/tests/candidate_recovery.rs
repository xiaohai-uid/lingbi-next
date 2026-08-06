use lingbi_ai::FakeProvider;
use lingbi_application::{
    CreateProjectRequest, DocumentApplicationService, GenerationService, ProjectApplicationService,
};
use lingbi_recovery::RecoveryService;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn generated_candidate_scans_without_json_parse_error() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("novel");
    let projects = ProjectApplicationService::new();
    let created = projects
        .create_project(CreateProjectRequest {
            name: "候选恢复测试".to_owned(),
            root: root.clone(),
        })
        .await
        .expect("create project");
    let documents = Arc::new(DocumentApplicationService::new(root.clone()));
    let generation = GenerationService::new(
        root.clone(),
        Arc::new(FakeProvider::new("第一章正文：雨夜。")),
        documents,
    );

    generation
        .generate(created.current_document.id, "写一个雨夜开场")
        .await
        .expect("generate candidate");

    let recovery = RecoveryService::new(root);
    let incidents = recovery
        .scan()
        .expect("scan must parse unified candidate schema");
    assert!(incidents.iter().any(|incident| incident.kind
        == lingbi_recovery::RecoveryIncidentKind::OrphanCandidate));
}
