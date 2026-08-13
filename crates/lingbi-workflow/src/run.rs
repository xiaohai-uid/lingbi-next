//! Run 台账（P8 #25）：一次链运行的可追踪记录。
//!
//! 每次 `run_chain` 生成 `run_id`，RunRecord 落
//! `工作区/工作流/runs/{run_id}.json`：运行开始写 Running，结束原子改写
//! 终态（Completed/Failed/Halted/Rejected/Cancelled）。崩溃后可从台账回答
//! 「上次跑到哪、什么状态」；`get_chain_status`（壳层）读台账（重启后可查）。

use lingbi_contracts::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 运行台账目录（工作区 mutable，可丢弃）。
pub const RUNS_DIR: &str = "工作区/工作流/runs";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
    Halted,
    Rejected,
    Cancelled,
}

/// 类型化拒绝原因（OutputGate/机检/棘轮/预算/取消）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    /// OutputGate 修复次数用尽仍有阻塞问题。
    BlockingIssuesRemain,
    /// 机检状态门禁不过（链 Halted）。
    MachineCheckFailed,
    /// 定稿棘轮：已提交章不可直接改写。
    RatchetRejected,
    /// token 预算超限。
    BudgetExceeded,
    /// 用户取消。
    Cancelled,
}

/// 运行控制（P8 #25）：取消与 token 预算的 Run→Node→Provider 传播。
#[derive(Clone, Default)]
pub struct RunControl {
    /// 取消令牌：节点间检查 + AI 节点经 stream_chat_with_cancel 传播。
    pub cancel: Option<lingbi_ai::CancellationToken>,
    /// 运行级预算：累计 completion tokens 超限 → BudgetExceeded。
    pub budget: Option<TokenBudget>,
}

/// 运行级 token 预算（completion tokens）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenBudget {
    pub max_completion_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    pub run_id: String,
    /// 链定义版本（= CHAIN_VERSION）。
    pub workflow_version: u32,
    pub status: RunStatus,
    /// 执行中/最后执行的节点 definition_id。
    pub current_node: String,
    /// OutputGate 已执行修复次数。
    pub repair_attempts: u32,
    pub reject_reason: Option<RejectReason>,
    pub created_at: String,
    pub updated_at: String,
}

/// run 台账文件相对路径（`工作区/工作流/runs/{run_id}.json`）。
pub fn run_path(root: &Path, run_id: &str) -> std::path::PathBuf {
    root.join(RUNS_DIR).join(format!("{run_id}.json"))
}

/// 运行开始：写 Running 台账（原子）。
pub fn begin_run(root: &Path, run_id: &str, workflow_version: u32) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let record = RunRecord {
        run_id: run_id.to_owned(),
        workflow_version,
        status: RunStatus::Running,
        current_node: String::new(),
        repair_attempts: 0,
        reject_reason: None,
        created_at: now.clone(),
        updated_at: now,
    };
    write_record(root, &record)
}

/// 运行终态：更新 status/current_node/repair_attempts/reject_reason 并原子改写。
pub fn finish_run(
    root: &Path,
    run_id: &str,
    status: RunStatus,
    current_node: &str,
    repair_attempts: u32,
    reject_reason: Option<RejectReason>,
) -> Result<(), AppError> {
    let p = run_path(root, run_id);
    let bytes = std::fs::read(&p).map_err(|_| {
        AppError::new(
            ErrorCode::BookNotFound,
            format!("缺 run 台账: {run_id}"),
            false,
        )
    })?;
    let mut record: RunRecord = serde_json::from_slice(&bytes).map_err(|e| {
        AppError::new(
            ErrorCode::BookCorrupted,
            format!("run 台账损坏: {e}"),
            false,
        )
    })?;
    record.status = status;
    record.current_node = current_node.to_owned();
    record.repair_attempts = repair_attempts;
    record.reject_reason = reject_reason;
    record.updated_at = chrono::Utc::now().to_rfc3339();
    write_record(root, &record)
}

/// 读 run 台账（不存在 → None；损坏 → 错误留档，返回 None 不阻断）。
pub fn load_run(root: &Path, run_id: &str) -> Result<Option<RunRecord>, AppError> {
    let p = run_path(root, run_id);
    let Ok(bytes) = std::fs::read(&p) else {
        return Ok(None);
    };
    match serde_json::from_slice::<RunRecord>(&bytes) {
        Ok(r) => Ok(Some(r)),
        Err(_) => Ok(None),
    }
}

/// 列全部 run 台账（按 run_id 排序，读取失败跳过）。
pub fn list_runs(root: &Path) -> Vec<RunRecord> {
    let dir = root.join(RUNS_DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(r) = serde_json::from_slice::<RunRecord>(&bytes) {
                out.push(r);
            }
        }
    }
    out.sort_by(|a, b| a.run_id.cmp(&b.run_id));
    out
}

fn write_record(root: &Path, record: &RunRecord) -> Result<(), AppError> {
    let p = run_path(root, &record.run_id);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::new(
                ErrorCode::DiskWriteFailed,
                format!("创建运行台账目录失败: {e}"),
                false,
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(record).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("run 台账序列化失败: {e}"),
            false,
        )
    })?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("写 run 台账失败: {e}"),
            false,
        )
    })?;
    std::fs::rename(&tmp, &p).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("run 台账落盘失败: {e}"),
            false,
        )
    })?;
    Ok(())
}
