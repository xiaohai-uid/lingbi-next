use lingbi_application::{
    CreateProjectRequest, DocumentApplicationService, ProjectApplicationService,
};
use lingbi_domain::{Candidate, CandidateStatus};
use lingbi_mutation::{CommitIntent, IntentRepository};
use lingbi_recovery::RecoveryService;
use lingbi_storage::{CandidateRepository, DocumentRepository};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use uuid::Uuid;

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

async fn setup_failpoint(root: &Path, phase: &str) -> Uuid {
    let projects = ProjectApplicationService::new();
    let created = projects
        .create_project(CreateProjectRequest {
            name: "崩溃恢复".to_owned(),
            root: root.to_path_buf(),
        })
        .await
        .expect("create project");
    let document = created.current_document;
    let before_hash = document.content_hash.clone();
    let after_hash = hex_sha256(b"after content");
    let candidate = Candidate {
        id: Uuid::new_v4(),
        project_id: document.project_id,
        document_id: document.id,
        instruction: "write".to_owned(),
        base_revision: document.revision,
        base_content_hash: before_hash,
        content: "after content".to_owned(),
        content_hash: after_hash.clone(),
        provider_id: "fake".to_owned(),
        model_id: "fake-model".to_owned(),
        status: CandidateStatus::Approved,
        created_at: chrono::Utc::now(),
        approved_at: Some(chrono::Utc::now()),
        committed_at: None,
    };
    CandidateRepository::new(root)
        .write(&candidate)
        .expect("write candidate");
    let intent = CommitIntent {
        id: Uuid::new_v4(),
        candidate_id: candidate.id,
        approval_id: Uuid::new_v4(),
        target_path: document.physical_path().to_string_lossy().into_owned(),
        expected_revision: document.revision,
        idempotency_key: "key".to_owned(),
    };
    IntentRepository::new(root)
        .write(&intent)
        .expect("write intent");

    let body = root.join(document.physical_path());
    match phase {
        "AfterContentWrite" | "AfterMetadataWrite" | "BeforeReceipt" => {
            fs::write(&body, "after content").expect("body");
        }
        "External" => {
            fs::write(&body, "external").expect("body");
        }
        _ => {}
    }
    if matches!(phase, "AfterMetadataWrite" | "BeforeReceipt") {
        let mut updated = document.clone();
        updated.revision = 1;
        updated.content_hash = after_hash;
        DocumentRepository::new(root)
            .update(&updated)
            .expect("metadata");
    }
    document.id
}

async fn verify_recovered(root: &Path, document_id: Uuid, expect_external: bool) {
    let documents = DocumentApplicationService::new(root);
    let body = documents
        .read_document(document_id)
        .await
        .expect("read body");
    let loaded = DocumentRepository::new(root)
        .find(document_id)
        .expect("find")
        .expect("document");
    if expect_external {
        assert_eq!(body, "external");
        assert_eq!(loaded.revision, 0);
    } else {
        assert_eq!(body, "after content");
        assert_eq!(loaded.revision, 1);
        assert_eq!(loaded.content_hash, hex_sha256(b"after content"));
    }
    let receipts_dir = root.join(".lingbi/receipts");
    let receipt_exists =
        receipts_dir.exists() && receipts_dir.read_dir().expect("receipts").next().is_some();
    assert_eq!(receipt_exists, !expect_external);
}

#[tokio::test]
async fn every_crash_failpoint_recovers_and_project_still_opens() {
    for phase in [
        "AfterIntent",
        "AfterContentWrite",
        "AfterMetadataWrite",
        "BeforeReceipt",
        "External",
    ] {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("novel");
        let document_id = setup_failpoint(&root, phase).await;

        let recovery = RecoveryService::new(root.clone());
        recovery.recover_all().expect("recover");

        let projects = ProjectApplicationService::new();
        let opened = projects
            .open_project(root.clone())
            .await
            .expect("open project after recovery");
        assert_eq!(opened.current_document.id, document_id);
        verify_recovered(&root, document_id, phase == "External").await;
    }
}
