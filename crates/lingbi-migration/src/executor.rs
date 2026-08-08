//! MigrationExecutor — 构建目标 V3 托管书仓 + 迁移回执落盘。
//!
//! 纪律：source 全程只读；正文/结构文件字节原样（原子 tmp+rename 复制）；
//! book.yaml 补 id(uuid)；`ensure_tx_layer` 托管化；回执六字段落
//! `.lingbi/migration-receipt.json`。追踪 `_tracking-state.json` 原样搬入
//! （state_revision 等引擎语义不因迁移改变）。

use crate::inspector::{Classification, FileKind};
use crate::plan::MigrationPlan;
use crate::{app_error, copy_atomic};
use lingbi_book::tx;
use lingbi_contracts::{AppError, ErrorCode};
use std::path::Path;
use uuid::Uuid;

pub struct MigrationExecutor;

impl MigrationExecutor {
    pub fn execute(plan: &MigrationPlan) -> Result<(), AppError> {
        match plan.classification {
            Classification::StoryrepoLegacy => Self::execute_legacy(plan),
            Classification::FlutterV1 => Self::execute_flutter_v1(plan),
        }
    }

    fn execute_legacy(plan: &MigrationPlan) -> Result<(), AppError> {
        for step in &plan.steps {
            match step.kind {
                FileKind::Skip => {}
                FileKind::BookYaml => {
                    let dest = plan.destination.join(&step.destination);
                    copy_atomic(&plan.source.join(&step.source), &dest)?;
                    inject_book_id(&dest)?;
                }
                _ => {
                    copy_atomic(&plan.source.join(&step.source), &plan.destination.join(&step.destination))?;
                }
            }
        }
        tx::ensure_tx_layer(&plan.destination)?;
        write_receipt(plan)
    }

    fn execute_flutter_v1(plan: &MigrationPlan) -> Result<(), AppError> {
        // 目标树骨架（book.yaml + 追踪 由引擎语义生成）
        let book_yaml = plan.destination.join("book.yaml");
        if let Some(parent) = book_yaml.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| app_error(ErrorCode::DiskWriteFailed, format!("创建目录失败: {e}")))?;
        }
        std::fs::write(
            &book_yaml,
            format!("---\nid: {}\nname: {}\nstatus: writing\ntotal_chapters: 0\n---\n", Uuid::new_v4(), plan.book_name),
        )
        .map_err(|e| app_error(ErrorCode::DiskWriteFailed, format!("写 book.yaml 失败: {e}")))?;
        lingbi_book::tracking::init_state(&plan.destination)?;

        for step in &plan.steps {
            let source_path = plan.source.join(&step.source);
            let dest_path = plan.destination.join(&step.destination);
            match step.kind {
                FileKind::Chapter => {
                    copy_chapter_with_frontmatter(&source_path, &dest_path, &step.destination)?;
                }
                FileKind::Resource => {
                    copy_atomic(&source_path, &dest_path)?;
                }
                FileKind::Skip => {}
                _ => {
                    return Err(app_error(
                        ErrorCode::ProjectCorrupted,
                        format!("flutter-v1 计划含意外步骤: {:?}", step.kind),
                    ))
                }
            }
        }
        tx::ensure_tx_layer(&plan.destination)?;
        write_receipt(plan)
    }
}

/// 章复制：正文段 byte-for-byte。源文件已有 frontmatter（--- 段）→ 原样
/// 复制（含 frontmatter，字节不变）；无 frontmatter → 按 V3 六字段补头
/// （正文段 = 源全文，byte 不变），规则记入回执（migration_rule）。
fn copy_chapter_with_frontmatter(
    source: &Path,
    destination: &Path,
    dest_rel: &str,
) -> Result<(), AppError> {
    let bytes = std::fs::read(source)
        .map_err(|e| app_error(ErrorCode::ProjectCorrupted, format!("读章失败: {e}")))?;
    let text = String::from_utf8_lossy(&bytes);
    let fname = dest_rel
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".md"))
        .unwrap_or("未命名");
    // 目标文件名 `NNN-标题.md`：章号取 NNN，标题取 NNN- 之后的正文标题
    let (chapter_no, title) = match fname.split_once('-') {
        Some((no, rest)) if !rest.is_empty() => (no.to_owned(), rest.to_owned()),
        _ => (fname.to_owned(), fname.to_owned()),
    };

    let output = if text.trim_start().starts_with("---") && text.split("---").nth(1).is_some() {
        // 源自带 frontmatter：字节原样
        bytes
    } else {
        let header = format!(
            "---\ntitle: {title}\nchapter: {chapter_no}\nvolume: 1\nwords: 0\nstatus: 草稿\nsummary: \n---\n\n"
        );
        let mut out = header.into_bytes();
        out.extend_from_slice(&bytes);
        out
    };

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| app_error(ErrorCode::DiskWriteFailed, format!("创建目录失败: {e}")))?;
    }
    let tmp = destination.with_extension("mig.tmp");
    std::fs::write(&tmp, &output)
        .map_err(|e| app_error(ErrorCode::DiskWriteFailed, format!("写章失败: {e}")))?;
    std::fs::rename(&tmp, destination)
        .map_err(|e| app_error(ErrorCode::DiskWriteFailed, format!("提交章失败: {e}")))?;
    Ok(())
}

/// book.yaml 补 id(uuid)：在首个 `---` 行后插入 `id: <uuid>`。
/// CRLF 兼容：行尾 `\r` 先剥除再判断（Windows 用户可能产生 CRLF 书仓）。
fn inject_book_id(path: &Path) -> Result<(), AppError> {
    let bytes = std::fs::read(path)
        .map_err(|e| app_error(ErrorCode::ProjectCorrupted, format!("读 book.yaml 失败: {e}")))?;
    let text = String::from_utf8_lossy(&bytes);
    if text.lines().any(|line| line.trim_end_matches('\r').starts_with("id:")) {
        return Ok(()); // 已有 id（重复迁移到空目录的场景）
    }
    let crlf = text.contains("\r\n");
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    if lines.first().map(|l| l.trim_end_matches('\r').as_ref()) == Some("---") {
        lines.insert(1, format!("id: {}", Uuid::new_v4()));
    } else {
        lines.insert(0, format!("---\nid: {}", Uuid::new_v4()));
    }
    let separator = if crlf { "\r\n" } else { "\n" };
    let out = lines.join(separator) + separator;
    std::fs::write(path, out)
        .map_err(|e| app_error(ErrorCode::DiskWriteFailed, format!("写 book.yaml 失败: {e}")))?;
    Ok(())
}

/// 迁移回执落 `.lingbi/migration-receipt.json`（六字段，append-only 审计链）。
fn write_receipt(plan: &MigrationPlan) -> Result<(), AppError> {
    let receipt_path = plan.destination.join(".lingbi").join("migration-receipt.json");
    let receipt = crate::receipt::MigrationReceipt::build(plan)?;
    let json = receipt.to_json()?;
    std::fs::write(&receipt_path, json)
        .map_err(|e| app_error(ErrorCode::DiskWriteFailed, format!("写回执失败: {e}")))?;
    Ok(())
}
