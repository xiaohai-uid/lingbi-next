use lingbi_application::{DocumentApplicationService, ProjectApplicationService};
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = env::args_os()
        .nth(1)
        .ok_or("usage: cross_platform_edit <project-root>")?;
    let root = PathBuf::from(root);
    let expected = "# 第一章\n\nRust edited fixture.\n";

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let projects = ProjectApplicationService::new();
        let opened = projects.open_project(root.clone()).await?;
        let documents = DocumentApplicationService::new(root.clone());
        let updated = documents
            .save_document(
                opened.current_document.id,
                opened.current_document.revision,
                expected,
            )
            .await?;
        let proof = serde_json::json!({
            "project_id": opened.project.id.to_string(),
            "project_name": opened.project.name,
            "document_id": updated.id.to_string(),
            "revision": updated.revision,
            "content_hash": updated.content_hash,
            "expected_content": expected,
        });
        let proof_dir = root.join(".lingbi");
        fs::create_dir_all(&proof_dir)?;
        fs::write(
            proof_dir.join("cross-platform-proof.json"),
            serde_json::to_vec_pretty(&proof)?,
        )?;
        Ok(())
    })
}
