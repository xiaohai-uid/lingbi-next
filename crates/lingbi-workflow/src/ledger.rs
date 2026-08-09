//! TokenLedger（P8 #25）—— 四级记账：run/node/provider/model 可查。
//!
//! `工作区/工作流/token-ledger.jsonl` 行式 JSON（append-only，只增不改），
//! 每次 AI 节点执行 append 一行（含复评/重跑——诚实记账）。查询 API 支持
//! 按 run_id / definition_id / provider / model 任意组合过滤。

use lingbi_contracts::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 台账文件相对路径（`工作区/工作流/token-ledger.jsonl`）。
pub fn ledger_path(root: &Path) -> std::path::PathBuf {
    root.join("工作区/工作流/token-ledger.jsonl")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenLedgerEntry {
    pub run_id: String,
    pub node_id: String,
    pub definition_id: String,
    pub provider: String,
    pub model: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub created_at: String,
}

/// append 一行（原子：写后追加；目录缺失自动创建）。
pub fn append_ledger(root: &Path, entry: &TokenLedgerEntry) -> Result<(), AppError> {
    let p = ledger_path(root);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::new(
                ErrorCode::DiskWriteFailed,
                format!("创建工作流目录失败: {e}"),
                false,
            )
        })?;
    }
    let mut line = serde_json::to_string(entry).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("台账序列化失败: {e}"),
            false,
        )
    })?;
    line.push('\n');
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .map_err(|e| {
            AppError::new(
                ErrorCode::DiskWriteFailed,
                format!("打开台账失败: {e}"),
                false,
            )
        })?;
    use std::io::Write;
    f.write_all(line.as_bytes()).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("写台账失败: {e}"),
            false,
        )
    })
}

/// 四级查询：None = 不过滤该维度；多维度组合过滤（全部满足）。
pub fn query_ledger(
    root: &Path,
    run_id: Option<&str>,
    definition_id: Option<&str>,
    provider: Option<&str>,
    model: Option<&str>,
) -> Vec<TokenLedgerEntry> {
    let p = ledger_path(root);
    let Ok(text) = std::fs::read_to_string(&p) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let Ok(entry) = serde_json::from_str::<TokenLedgerEntry>(line) else {
            continue; // 损坏行跳过（台账只读审计，不阻断）
        };
        let matched = run_id.map(|v| entry.run_id == v).unwrap_or(true)
            && definition_id.map(|v| entry.definition_id == v).unwrap_or(true)
            && provider.map(|v| entry.provider == v).unwrap_or(true)
            && model.map(|v| entry.model == v).unwrap_or(true);
        if matched {
            out.push(entry);
        }
    }
    out
}
