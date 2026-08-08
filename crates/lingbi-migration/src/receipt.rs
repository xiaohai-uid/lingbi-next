//! MigrationReceipt — 六字段回执（source_hash / classification / destination /
//! destination_hash / migration_rule / warnings）。

use crate::app_error;
use crate::plan::{FileRule, MigrationPlan};
use crate::tree_hash;
use lingbi_contracts::{AppError, ErrorCode};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MigrationReceipt {
    /// 迁移完成后重算的源树哈希（== 迁移前 Inspector 值 = READ ONLY 证据）
    pub source_hash: String,
    pub classification: String,
    pub destination: String,
    /// 目标树哈希（排除 `.lingbi/` 与回执本身，确定性）
    pub destination_hash: String,
    pub migration_rule: Vec<FileRule>,
    pub warnings: Vec<String>,
}

impl MigrationReceipt {
    /// 从计划构建回执（源/目标哈希重算——validator 已断言源哈希不变）。
    pub fn build(plan: &MigrationPlan) -> Result<MigrationReceipt, AppError> {
        let source_hash = tree_hash(&plan.source, false)?;
        let destination_hash = tree_hash(&plan.destination, true)?;
        Ok(MigrationReceipt {
            source_hash,
            classification: plan.classification.as_str().to_owned(),
            destination: plan.destination.to_string_lossy().into_owned(),
            destination_hash,
            migration_rule: plan.rules(),
            warnings: plan.warnings.clone(),
        })
    }

    /// 序列化为 JSON 字符串（落盘/返回）。
    pub fn to_json(&self) -> Result<String, AppError> {
        serde_json::to_string_pretty(self).map_err(|e| {
            app_error(
                ErrorCode::ProjectCorrupted,
                format!("序列化迁移回执失败: {e}"),
            )
        })
    }
}
