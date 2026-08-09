//! 节点记录类型（P7 章事务引擎，#24）。
//!
//! 每节点持久化记录：node_id / definition_id / definition_version /
//! input_hash / output_hash / status / provider / model / usage / error。
//! checkpoint 语义见 `checkpoint` 模块；哈希语义见 `hash` 模块。

use serde::{Deserialize, Serialize};

/// 节点状态（checkpoint 落盘字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// 已入链但未执行（本运行未到达；通常只出现在失败的运行后的补齐场景，落盘不保留）
    Pending,
    /// 执行中（崩溃窗口内；恢复/重跑按非 Completed 处理，重新执行）
    Running,
    /// 执行完成，产物有效（input_hash 一致时可复用）
    Completed,
    /// 执行失败（error 记录原因；重跑时重新执行）
    Failed,
    /// 上游输入变化作废（output_hash 已清空，绝不复用）
    Invalidated,
}

/// 单节点持久化记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRecord {
    /// `{definition_id}-{uuid}`；草稿文件名派生自 uuid 段（`{ch:03}-{uuid}.md`）。
    pub node_id: String,
    pub definition_id: String,
    /// 节点实现版本（定义变更时递增，进入 input_hash）。
    pub definition_version: u32,
    /// 输入指纹（definition + 书仓上下文指纹 + 上游 output_hash + params）。
    pub input_hash: String,
    /// 产物哈希；Invalidated/Failed 时清空（防旧节点复用）。
    pub output_hash: String,
    pub status: NodeStatus,
    /// AI 节点：本次运行解析的 provider/model 快照（能力 8）。
    pub provider: Option<String>,
    pub model: Option<String>,
    /// AI 节点 token 用量（节点级记账；账本持久化随 P8 #25）。
    pub usage: Option<lingbi_ai::TokenUsage>,
    /// 副作用节点幂等键（draft/settlement = node_id；崩溃恢复对照用，
    /// 配合棘轮 + CAS 保证副作用不重复执行，P8 #25）。
    pub idempotency_key: Option<String>,
    /// 失败原因（status == Failed 时非空）。
    pub error: Option<String>,
    /// 语义说明（如「无阻塞问题，跳过改写」/「状态门禁未过」）。
    pub note: Option<String>,
}
