//! 章链 checkpoint 持久化（P7 章事务引擎，#24）。
//!
//! 存 `工作区/工作流/第NNN章.json`——工作区 = mutable 契约（可丢弃可重建，
//! 与 P4 候选稿/评审报告同类别），managed/legacy 书仓通用，不经事务门。
//! 恢复语义：每次 run_chain 开头加载；输入未变的 Completed 节点复用，
//! 输入变化的节点标 Invalidated 后重跑（见 `chain`/`lib`）。

use crate::node::NodeRecord;
use lingbi_contracts::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 工作流产物目录（工作区 = mutable，可丢弃）。
pub const WORKFLOW_DIR: &str = "工作区/工作流";

/// 单章 checkpoint。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterCheckpoint {
    pub ch: i64,
    /// 链定义版本（8 节点序/定义变更时递增）。
    pub chain_version: u32,
    /// 本次运行的书仓上下文指纹（审计用；失效判定以节点 input_hash 为准）。
    pub book_input_hash: String,
    /// 节点记录（按定义序，每定义至多一条）。
    pub nodes: Vec<NodeRecord>,
}

impl ChapterCheckpoint {
    pub fn new(ch: i64, chain_version: u32, book_input_hash: String) -> Self {
        Self {
            ch,
            chain_version,
            book_input_hash,
            nodes: Vec::new(),
        }
    }
}

/// 章 checkpoint 文件相对路径（`工作区/工作流/第NNN章.json`）。
pub fn checkpoint_path(root: &Path, ch: i64) -> std::path::PathBuf {
    root.join(WORKFLOW_DIR).join(format!("第{ch:03}章.json"))
}

/// handoff 报告相对路径（`工作区/工作流/handoff-{ch:03}.json`）。
pub fn handoff_path(root: &Path, ch: i64) -> std::path::PathBuf {
    root.join(WORKFLOW_DIR).join(format!("handoff-{ch:03}.json"))
}

/// 加载章 checkpoint（不存在/损坏 → None，全新运行；损坏留档不阻断）。
pub fn load(root: &Path, ch: i64) -> Result<Option<ChapterCheckpoint>, AppError> {
    let p = checkpoint_path(root, ch);
    let Ok(bytes) = std::fs::read(&p) else {
        return Ok(None);
    };
    match serde_json::from_slice::<ChapterCheckpoint>(&bytes) {
        Ok(cp) => Ok(Some(cp)),
        Err(_) => Ok(None),
    }
}

/// 落盘章 checkpoint（原子写 tmp+rename，工作区 mutable）。
pub fn save(root: &Path, cp: &ChapterCheckpoint) -> Result<(), AppError> {
    let p = checkpoint_path(root, cp.ch);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::new(
                ErrorCode::DiskWriteFailed,
                format!("创建工作流目录失败: {e}"),
                false,
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(cp).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("checkpoint 序列化失败: {e}"),
            false,
        )
    })?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("写 checkpoint 失败: {e}"),
            false,
        )
    })?;
    std::fs::rename(&tmp, &p).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("checkpoint 落盘失败: {e}"),
            false,
        )
    })?;
    Ok(())
}
