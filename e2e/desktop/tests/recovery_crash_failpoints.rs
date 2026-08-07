use chrono::Utc;
use lingbi_application::{
    CreateProjectRequest, DocumentApplicationService, ProjectApplicationService,
};
use lingbi_domain::{Candidate, CandidateStatus};
use lingbi_mutation::{
    Approval, ApprovalRepository, CommitIntent, CommitReceipt, IntentRepository, ReceiptRepository,
};
use lingbi_recovery::RecoveryService;
use lingbi_storage::{
    CandidateRepository, DocumentRepository, DocumentTransaction, DocumentTransactionRepository,
    TransactionPhase,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use uuid::Uuid;

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Reproduce a crash at every phase of the unified mutation production
/// path (Task 13):
///
/// ```text
/// Approval 后 → Intent 后 → 正文写入后 → metadata 更新后 → Receipt 写入前
/// → Candidate Committed 前 (+ 外部编辑)
/// ```
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
        created_at: Utc::now(),
        approved_at: Some(Utc::now()),
        committed_at: None,
    };
    CandidateRepository::new(root)
        .write(&candidate)
        .expect("write candidate");

    // Approval is always persisted first in the production path.
    let approval = Approval {
        id: Uuid::new_v4(),
        candidate_id: candidate.id,
        actor: "user".to_owned(),
        approved_at: Utc::now(),
    };
    ApprovalRepository::new(root)
        .write(&approval)
        .expect("write approval");

    let intent = CommitIntent {
        id: Uuid::new_v4(),
        candidate_id: candidate.id,
        approval_id: approval.id,
        target_path: document.physical_path().to_string_lossy().into_owned(),
        expected_revision: document.revision,
        idempotency_key: "key".to_owned(),
    };
    if phase != "AfterApproval" {
        IntentRepository::new(root)
            .write(&intent)
            .expect("write intent");
    }

    // The unified engine now journals a DocumentTransaction through the
    // commit; simulate the exact on-disk shape at each crash point.
    let transaction = DocumentTransaction {
        id: Uuid::new_v4(),
        document_id: document.id,
        before_revision: document.revision,
        before_hash: document.content_hash.clone(),
        after_revision: document.revision + 1,
        after_hash: hex_sha256(b"after content"),
        phase: match phase {
            "AfterApproval" => TransactionPhase::Created,
            "AfterIntent" => TransactionPhase::Created,
            "AfterContentWrite" => TransactionPhase::ContentWritten,
            "AfterMetadataWrite" | "BeforeReceipt" | "BeforeCommitted" => {
                TransactionPhase::IndexUpdated
            }
            "External" => TransactionPhase::Created,
            _ => TransactionPhase::Created,
        },
        created_at: chrono::Utc::now(),
        body_relative_path: document.physical_path().to_string_lossy().into_owned(),
    };
    if phase != "AfterApproval" {
        DocumentTransactionRepository::new(root)
            .begin(&transaction)
            .expect("begin transaction");
    }

    let body = root.join(document.physical_path());
    match phase {
        "AfterContentWrite" | "AfterMetadataWrite" | "BeforeReceipt" | "BeforeCommitted" => {
            fs::write(&body, "after content").expect("body");
        }
        "External" => {
            fs::write(&body, "external").expect("body");
        }
        _ => {}
    }
    if matches!(
        phase,
        "AfterMetadataWrite" | "BeforeReceipt" | "BeforeCommitted"
    ) {
        let mut updated = document.clone();
        updated.revision = 1;
        updated.content_hash = after_hash.clone();
        DocumentRepository::new(root)
            .update(&updated)
            .expect("metadata");
    }
    if phase == "BeforeCommitted" {
        // Crash between Receipt 写入 and Candidate Committed.
        let receipt = CommitReceipt {
            id: Uuid::new_v4(),
            candidate_id: candidate.id,
            target_path: document.physical_path().to_string_lossy().into_owned(),
            after_revision: 1,
            after_content_hash: after_hash.clone(),
            committed_at: Utc::now(),
            idempotency_key: "key".to_owned(),
        };
        ReceiptRepository::new(root)
            .write(&receipt)
            .expect("write receipt");
    }
    document.id
}

async fn verify_recovered(root: &Path, document_id: Uuid, phase: &str) {
    let documents = DocumentApplicationService::new(root);
    let body = documents
        .read_document(document_id)
        .await
        .expect("read body");
    let loaded = DocumentRepository::new(root)
        .find(document_id)
        .expect("find")
        .expect("document");
    let receipts_dir = root.join(".lingbi/receipts");
    let receipt_exists =
        receipts_dir.exists() && receipts_dir.read_dir().expect("receipts").next().is_some();

    // After recovery no transaction may linger in a non-terminal state.
    let transactions_dir = root.join(".lingbi/transactions");
    let lingering: Vec<String> = if transactions_dir.exists() {
        transactions_dir
            .read_dir()
            .expect("transactions dir")
            .flatten()
            .filter_map(|entry| {
                let bytes = std::fs::read(entry.path()).ok()?;
                let tx: DocumentTransaction = serde_json::from_slice(&bytes).ok()?;
                (tx.phase != TransactionPhase::Failed && tx.phase != TransactionPhase::Completed)
                    .then(|| format!("{:?}", tx.phase))
            })
            .collect()
    } else {
        Vec::new()
    };
    assert!(
        lingering.is_empty(),
        "transactions must converge after recovery: {lingering:?}"
    );

    match phase {
        // Crash before any intent: nothing to recover, nothing overwritten.
        "AfterApproval" => {
            assert_eq!(body, "# 第一章\n\n", "no write may happen before intent");
            assert_eq!(loaded.revision, 0);
            assert!(!receipt_exists);
        }
        // Crash before the candidate was marked committed: the receipt
        // exists, so recovery marks the candidate committed and cleans up.
        "BeforeCommitted" => {
            assert_eq!(body, "after content");
            assert_eq!(loaded.revision, 1);
            assert_eq!(loaded.content_hash, hex_sha256(b"after content"));
            assert!(receipt_exists);
            let candidate = CandidateRepository::new(root)
                .read(load_candidate_id(root, document_id).await)
                .expect("read candidate")
                .expect("candidate");
            assert_eq!(candidate.status, CandidateStatus::Committed);
            assert!(
                !root
                    .join(".lingbi/intents")
                    .read_dir()
                    .expect("intents")
                    .next()
                    .is_some(),
                "intent must be cleaned up after recovery"
            );
        }
        // External edits must never be overwritten.
        "External" => {
            assert_eq!(body, "external");
            assert_eq!(loaded.revision, 0);
            assert!(!receipt_exists);
        }
        // Every other crash phase finishes the commit.
        _ => {
            assert_eq!(body, "after content");
            assert_eq!(loaded.revision, 1);
            assert_eq!(loaded.content_hash, hex_sha256(b"after content"));
            assert!(receipt_exists);
        }
    }
}

async fn load_candidate_id(root: &Path, document_id: Uuid) -> Uuid {
    let documents = DocumentRepository::new(root);
    let document = documents.find(document_id).expect("find").expect("doc");
    let _ = document;
    // Candidate lookup by document: read the single candidate file.
    let dir = root.join(".lingbi/candidates");
    let entry = dir
        .read_dir()
        .expect("candidates dir")
        .next()
        .expect("candidate");
    let bytes = fs::read(entry.expect("entry").path()).expect("read candidate");
    let candidate: Candidate = serde_json::from_slice(&bytes).expect("parse candidate");
    candidate.id
}

#[tokio::test]
async fn every_crash_failpoint_recovers_and_project_still_opens() {
    for phase in [
        "AfterApproval",
        "AfterIntent",
        "AfterContentWrite",
        "AfterMetadataWrite",
        "BeforeReceipt",
        "BeforeCommitted",
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
        verify_recovered(&root, document_id, phase).await;
    }
}
