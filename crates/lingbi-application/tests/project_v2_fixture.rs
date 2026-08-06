use lingbi_application::{DocumentApplicationService, ProjectApplicationService};
use std::path::PathBuf;

#[tokio::test]
async fn rust_opens_shared_project_v2_fixture() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/projects/project-v2");
    let projects = ProjectApplicationService::new();
    let opened = projects
        .open_project(fixture.clone())
        .await
        .expect("open project v2 fixture");

    assert_eq!(opened.project.schema_version, 2);
    assert_eq!(opened.project.name, "V2兼容测试");
    assert_eq!(opened.current_document.title, "第一章");
    assert_eq!(opened.current_document.revision, 0);

    let documents = DocumentApplicationService::new(fixture);
    let content = documents
        .read_document(opened.current_document.id)
        .await
        .expect("read fixture manuscript");
    assert_eq!(content, "# 第一章\n\nV2 fixture content.\n");
}
