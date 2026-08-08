//! MigrationPlan — 逐文件目标映射 + 可行性校验（fail-closed）。

use crate::inspector::{Classification, FileKind, MigrationReport};
use crate::app_error;
use lingbi_contracts::{AppError, ErrorCode};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// 回执/计划共用的逐文件规则记录。
#[derive(Debug, Clone, Serialize)]
pub struct FileRule {
    pub source: String,
    pub destination: String,
    pub rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanStep {
    pub source: String,
    pub destination: String,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationPlan {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub classification: Classification,
    /// 新书仓名（flutter-v1 取 project.json name；legacy 取 book.yaml name）
    pub book_name: String,
    pub steps: Vec<PlanStep>,
    pub warnings: Vec<String>,
}

/// 章文件命名非法字符（与 create_chapter 同款白名单纪律）。
fn title_has_illegal_chars(title: &str) -> bool {
    title.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || title.chars().any(char::is_control)
}

impl MigrationPlan {
    pub fn plan(report: &MigrationReport, destination: &Path) -> Result<MigrationPlan, AppError> {
        if destination.exists() {
            let non_empty = std::fs::read_dir(destination)
                .map(|rd| rd.flatten().next().is_some())
                .unwrap_or(true);
            if non_empty {
                return Err(app_error(
                    ErrorCode::ProjectPathExists,
                    format!("迁移目标已存在且非空: {}", destination.display()),
                ));
            }
        }
        match report.classification {
            Classification::StoryrepoLegacy => Self::plan_legacy(report, destination),
            Classification::FlutterV1 => Self::plan_flutter_v1(report, destination),
        }
    }

    fn plan_legacy(
        report: &MigrationReport,
        destination: &Path,
    ) -> Result<MigrationPlan, AppError> {
        let book_name = book_name_from_yaml(&report)?;
        let mut steps = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for file in &report.files {
            if !seen.insert(file.rel_path.clone()) {
                return Err(app_error(
                    ErrorCode::ProjectCorrupted,
                    format!("源文件清单重复: {}", file.rel_path),
                ));
            }
            steps.push(PlanStep {
                source: file.rel_path.clone(),
                destination: file.rel_path.clone(),
                kind: file.kind,
            });
        }
        Ok(MigrationPlan {
            source: report.source().to_path_buf(),
            destination: destination.to_path_buf(),
            classification: Classification::StoryrepoLegacy,
            book_name,
            steps,
            warnings: report.warnings.clone(),
        })
    }

    fn plan_flutter_v1(
        report: &MigrationReport,
        destination: &Path,
    ) -> Result<MigrationPlan, AppError> {
        let book_name = report
            .project_name()
            .unwrap_or("迁移书仓")
            .to_owned();
        let mut steps = Vec::new();
        let mut titles = std::collections::HashMap::<String, i64>::new();
        for file in &report.files {
            match file.kind {
                FileKind::Chapter => {
                    let stem = Path::new(&file.rel_path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "未命名".to_owned());
                    if title_has_illegal_chars(&stem) {
                        return Err(app_error(
                            ErrorCode::ProjectCorrupted,
                            format!("章标题含非法字符（拒绝迁移，请改名后重试）: {stem}"),
                        ));
                    }
                    let ch = titles.entry(stem.clone()).or_insert_with(|| 1);
                    if *ch > 1 {
                        return Err(app_error(
                            ErrorCode::ProjectCorrupted,
                            format!("章标题重复（拒绝迁移，请改名后重试）: {stem}"),
                        ));
                    }
                    let dest = format!("定稿/正文/{ch:03}-{stem}.md");
                    *ch += 1;
                    steps.push(PlanStep {
                        source: file.rel_path.clone(),
                        destination: dest,
                        kind: FileKind::Chapter,
                    });
                }
                FileKind::Resource => steps.push(PlanStep {
                    source: file.rel_path.clone(),
                    destination: format!("工作区/迁移存档/{}", file.rel_path),
                    kind: FileKind::Resource,
                }),
                FileKind::Skip => {}
                _ => {
                    return Err(app_error(
                        ErrorCode::ProjectCorrupted,
                        format!("意外的文件类别: {:?}", file.kind),
                    ))
                }
            }
        }
        Ok(MigrationPlan {
            source: report.source().to_path_buf(),
            destination: destination.to_path_buf(),
            classification: Classification::FlutterV1,
            book_name,
            steps,
            warnings: report.warnings.clone(),
        })
    }

    /// 计划级规则表（回执六字段 migration_rule 的来源）。
    pub fn rules(&self) -> Vec<FileRule> {
        self.steps
            .iter()
            .map(|step| {
                let rule = match step.kind {
                    FileKind::Copy => "copy-as-is".to_owned(),
                    FileKind::Chapter => "frontmatter-补全".to_owned(),
                    FileKind::Resource => "resource-archived".to_owned(),
                    FileKind::BookYaml => "generate-book-id".to_owned(),
                    FileKind::TrackingState => "copy-as-is".to_owned(),
                    FileKind::Skip => "skip".to_owned(),
                };
                FileRule {
                    source: step.source.clone(),
                    destination: step.destination.clone(),
                    rule,
                }
            })
            .collect()
    }
}

/// 从源 book.yaml 读取 name（frontmatter 风格：`---\nname: xxx\n---`）。
fn book_name_from_yaml(report: &MigrationReport) -> Result<String, AppError> {
    let rel = report
        .files
        .iter()
        .find(|f| f.rel_path == "book.yaml")
        .ok_or_else(|| app_error(ErrorCode::ProjectCorrupted, "源缺 book.yaml".to_owned()))?;
    let bytes = std::fs::read(report.source().join(&rel.rel_path))
        .map_err(|e| app_error(ErrorCode::ProjectCorrupted, format!("读 book.yaml 失败: {e}")))?;
    let text = String::from_utf8_lossy(&bytes);
    for line in text.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim() == "name" {
                let name = v.trim();
                if !name.is_empty() {
                    return Ok(name.to_owned());
                }
            }
        }
    }
    Ok("迁移书仓".to_owned())
}
