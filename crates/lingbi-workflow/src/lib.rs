//! lingbi-workflow — 章事务引擎（P7 #24）。
//!
//! 8 节点链（Contract → Brief → Draft → Machine Check → Review → Polish →
//! Settlement → Handoff）端到端执行。每次运行把每节点固化为持久化记录
//! （node_id/definition_version/input_hash/output_hash/status/provider/model/
//! usage/error，checkpoint 落 `工作区/工作流/第NNN章.json`）。
//!
//! 重跑语义：
//! - 输入未变的 Completed 节点复用（checkpoint 可续，不重复 AI 消耗）；
//! - 输入变化（书仓上下文指纹/章文件/上游 output_hash/provider+model 快照）
//!   使本节点与后续 Completed 记录标 Invalidated（output_hash 清空）后重跑，
//!   绝不复用旧节点；
//! - 节点执行失败 → Failed + error，链停；修复后重跑从该节点续；
//! - machine-check 状态门禁不过 → 链 halt（后续节点不执行）。
//!
//! 接缝：本 crate API 为辅接缝（单元测试 + FakeProvider/ScriptedProvider）；
//! 主接缝 = `lingbi-commands::run_chain`（P4 命令边界）。

pub mod chain;
pub mod checkpoint;
pub mod hash;
pub mod node;
pub mod run;
#[cfg(feature = "test-support")]
pub mod testing;

use crate::chain::{exec_brief, exec_contract, NodeArtifact};
use crate::checkpoint::ChapterCheckpoint;
use crate::node::{NodeRecord, NodeStatus};
use lingbi_ai::AiProvider;
use lingbi_contracts::{AppError, ErrorCode};
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;

pub use crate::chain::{CHAIN_DEFINITIONS, CHAIN_VERSION, DRAFT_DIR};
pub use crate::hash::{book_input_hash, chapter_state_hash};
pub use crate::node::NodeStatus as Status;
pub use crate::run::{RejectReason, RunRecord, RunStatus};

/// 链运行结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChainReport {
    pub ch: i64,
    /// Completed = 全链完成（含结算/交付）；Halted = 机检状态门禁不过；
    /// Failed = 节点执行失败（error 见失败节点记录）；Rejected = OutputGate
    /// 拒绝（P8 #25）。
    pub status: ChainStatus,
    /// 本次运行 run_id（台账 `工作区/工作流/runs/{run_id}.json`）。
    pub run_id: String,
    /// OutputGate 已执行修复次数。
    pub repair_attempts: u32,
    /// 类型化拒绝原因（停机/拒绝时）。
    pub reject_reason: Option<RejectReason>,
    pub book_input_hash: String,
    /// 节点记录（按定义序；失败/停机时止于失败节点）。
    pub nodes: Vec<NodeRecord>,
    /// machine-check 机检清单（报告用；仅执行过时填充，复用时不重算）。
    pub checks: Vec<lingbi_book::CheckResult>,
    /// 当前草稿相对路径（结算后 None）。
    pub draft_path: Option<String>,
    /// 评审报告相对路径（`工作区/评审报告/第NNN章.json`）。
    pub review_path: Option<String>,
    pub handoff: Option<HandoffReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ChainStatus {
    Completed,
    Halted,
    Failed,
    /// OutputGate 拒绝（修复次数用尽仍有阻塞问题；reject_reason 给原因）。
    Rejected,
}

/// 交付报告（handoff 节点产物，落 `工作区/工作流/handoff-{ch:03}.json`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct HandoffReport {
    pub ch: i64,
    pub chapter_path: String,
    pub title: String,
    pub words: usize,
    pub nodes: Vec<HandoffNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct HandoffNode {
    pub definition_id: String,
    pub status: String,
    pub input_hash: String,
    pub output_hash: String,
}

/// 运行一次本章 8 节点链。provider/model 快照每次运行解析（能力 8），
/// 进入 AI 节点 input_hash——切换模型即失效重跑。
pub async fn run_chain(
    root: &Path,
    ch: i64,
    provider: Arc<dyn AiProvider>,
) -> Result<ChainReport, AppError> {
    let book_hash = crate::hash::book_input_hash(root)?;
    let provider_id = provider.provider_id().to_owned();
    let model_id = provider.model_id().to_owned();
    let mut cp = checkpoint::load(root, ch)?
        .unwrap_or_else(|| ChapterCheckpoint::new(ch, CHAIN_VERSION, book_hash.clone()));
    cp.book_input_hash = book_hash.clone(); // 审计：反映本次运行的指纹
    normalize_nodes(&mut cp.nodes);

    // Run 台账（P8 #25）：Running 落盘 → 终态原子改写（崩溃后可从台账
    // 回答「上次跑到哪、什么状态」）
    let run_id = uuid::Uuid::new_v4().to_string();
    crate::run::begin_run(root, &run_id, CHAIN_VERSION)?;
    let mut repair_attempts: u32 = 0;
    let mut reject_reason: Option<RejectReason> = None;

    let mut status = ChainStatus::Completed;
    let mut checks: Vec<lingbi_book::CheckResult> = Vec::new();
    let mut draft_path: Option<String> = None;
    let mut handoff: Option<HandoffReport> = None;
    let mut review_path: Option<String> = None;

    for (i, def_id) in CHAIN_DEFINITIONS.iter().enumerate() {
        let _ = crate::run::finish_run(
            root,
            &run_id,
            crate::run::RunStatus::Running,
            def_id,
            repair_attempts,
            reject_reason,
        );
        let existing_chapter_hash = crate::hash::chapter_state_hash(root, ch)?;
        let upstream = if i == 0 {
            String::new()
        } else {
            cp.nodes
                .iter()
                .find(|n| n.definition_id == CHAIN_DEFINITIONS[i - 1])
                .map(|n| n.output_hash.clone())
                .unwrap_or_default()
        };
        let params = serde_json::json!({
            "ch": ch,
            "existing_chapter_hash": existing_chapter_hash,
            "provider": if chain::is_ai_node(def_id) { provider_id.clone() } else { String::new() },
            "model": if chain::is_ai_node(def_id) { model_id.clone() } else { String::new() },
        });
        let input_hash = crate::hash::canonical_hash(&[
            ("definition_id", serde_json::json!(def_id)),
            ("definition_version", serde_json::json!(chain::definition_version(def_id))),
            ("book_input_hash", serde_json::json!(book_hash)),
            ("upstream_output_hash", serde_json::json!(upstream)),
            ("params", params),
        ]);

        // 复用 or 失效
        let prev_idx = cp.nodes.iter().position(|n| n.definition_id == *def_id);
        if let Some(idx) = prev_idx {
            let prev = &cp.nodes[idx];
            if prev.status == NodeStatus::Completed && prev.input_hash == input_hash {
                // 复用：重建运行时状态（草稿路径派生自记录 node_id）
                if *def_id == "draft" {
                    draft_path = Some(chain::draft_rel_path(&prev.node_id, ch));
                }
                if *def_id == "machine-check" {
                    // 报告需要机检清单；复用节点时重跑（纯确定性，不影响哈希）
                    let checks_now = lingbi_book::collect_checks(root, "prewrite", Some(ch))?;
                    checks = checks_now;
                }
                if *def_id == "review" {
                    review_path = Some(
                        lingbi_book::review::review_path(root, ch)
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
                continue;
            }
            // 失效：本节点 + 后续全部 Completed 记录 → Invalidated，output_hash 清空
            for n in cp.nodes.iter_mut().skip(idx) {
                if n.status == NodeStatus::Completed {
                    n.status = NodeStatus::Invalidated;
                    n.output_hash.clear();
                }
            }
        }

        // 执行
        let node_id = format!("{def_id}-{}", uuid::Uuid::new_v4());
        let mut record = NodeRecord {
            // 副作用节点（draft/settlement）幂等键 = node_id（崩溃恢复对照）
            node_id: node_id.clone(),
            definition_id: def_id.to_string(),
            definition_version: chain::definition_version(def_id),
            input_hash,
            output_hash: String::new(),
            status: NodeStatus::Running,
            provider: chain::is_ai_node(def_id).then(|| provider_id.clone()),
            model: chain::is_ai_node(def_id).then(|| model_id.clone()),
            usage: None,
            idempotency_key: matches!(*def_id, "draft" | "settlement").then(|| node_id),
            error: None,
            note: None,
        };

        let artifact = match execute_node(
            def_id,
            root,
            ch,
            &record.node_id,
            provider.as_ref(),
            &draft_path,
            &cp.nodes,
        )
        .await
        {
            Ok(a) => a,
            Err(e) => {
                record.status = NodeStatus::Failed;
                record.error = Some(e.message.clone());
                if *def_id == "settlement" && e.message.contains("棘轮") {
                    reject_reason = Some(RejectReason::RatchetRejected);
                }
                status = ChainStatus::Failed;
                upsert(&mut cp.nodes, record);
                break;
            }
        };
        record.status = NodeStatus::Completed;
        record.output_hash = lingbi_storage::sha256_hex(&artifact.bytes);
        record.usage = artifact.usage;
        record.note = artifact.note;
        if let Some(p) = artifact.draft_path {
            draft_path = Some(p);
        }
        if *def_id == "machine-check" {
            checks = artifact.checks;
            review_path = None;
            if !artifact.gate_passed {
                status = ChainStatus::Halted;
                reject_reason = Some(RejectReason::MachineCheckFailed);
                upsert(&mut cp.nodes, record);
                break;
            }
        }
        if *def_id == "review" {
            review_path = Some(
                lingbi_book::review::review_path(root, ch)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        if *def_id == "settlement" {
            draft_path = None;
        }
        upsert(&mut cp.nodes, record);
    }

    // 全链完成后：handoff 报告补齐本节点（8 节点摘要）并落盘——本节点
    // output_hash 以实际落盘文件为准（与 checkpoint 完全一致）。
    if status == ChainStatus::Completed {
        if let Some(idx) = cp.nodes.iter().position(|n| n.definition_id == "handoff") {
            let report = chain::build_handoff_report(root, ch, &cp.nodes)?;
            let json = serde_json::to_vec_pretty(&report).map_err(|e| {
                AppError::new(
                    ErrorCode::BookCorrupted,
                    format!("交付序列化失败: {e}"),
                    false,
                )
            })?;
            let p = crate::checkpoint::handoff_path(root, ch);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    AppError::new(
                        ErrorCode::DiskWriteFailed,
                        format!("创建工作流目录失败: {e}"),
                        false,
                    )
                })?;
            }
            std::fs::write(&p, &json).map_err(|e| {
                AppError::new(
                    ErrorCode::DiskWriteFailed,
                    format!("写交付报告失败: {e}"),
                    false,
                )
            })?;
            cp.nodes[idx].output_hash = lingbi_storage::sha256_hex(&json);
            handoff = Some(report);
        }
    }

    checkpoint::save(root, &cp)?;

    let run_status = match status {
        ChainStatus::Completed => crate::run::RunStatus::Completed,
        ChainStatus::Halted => crate::run::RunStatus::Halted,
        ChainStatus::Failed => crate::run::RunStatus::Failed,
        ChainStatus::Rejected => crate::run::RunStatus::Rejected,
    };
    let current_node = cp
        .nodes
        .last()
        .map(|n| n.definition_id.clone())
        .unwrap_or_default();
    let _ = crate::run::finish_run(
        root,
        &run_id,
        run_status,
        &current_node,
        repair_attempts,
        reject_reason,
    );

    Ok(ChainReport {
        ch,
        status,
        run_id,
        repair_attempts,
        reject_reason,
        book_input_hash: book_hash,
        nodes: cp.nodes,
        checks,
        draft_path,
        review_path,
        handoff,
    })
}

/// 执行单个节点（确定性同步 / AI 异步）。`nodes` = 当前 checkpoint 节点
/// （handoff 报告需全链节点摘要，含本节点之前全部记录）。
async fn execute_node(
    def_id: &str,
    root: &Path,
    ch: i64,
    node_id: &str,
    provider: &dyn AiProvider,
    draft_path: &Option<String>,
    nodes: &[NodeRecord],
) -> Result<NodeArtifact, AppError> {
    match def_id {
        "contract" => exec_contract(root, ch),
        "brief" => exec_brief(root, ch),
        "draft" => chain::exec_draft(root, ch, node_id, provider).await,
        "machine-check" => chain::exec_machine_check(root, ch, draft_path),
        "review" => chain::exec_review(root, ch, provider, draft_path).await,
        "polish" => chain::exec_polish(root, ch, node_id, provider, draft_path).await,
        "settlement" => chain::exec_settlement(root, ch, draft_path),
        "handoff" => chain::exec_handoff(root, ch, nodes),
        _ => Err(AppError::new(
            ErrorCode::BookCorrupted,
            format!("未知节点定义: {def_id}"),
            false,
        )),
    }
}

/// checkpoint 节点规整：只留已知定义、按定义序、每定义一条（后写胜）。
fn normalize_nodes(nodes: &mut Vec<NodeRecord>) {
    let mut out: Vec<NodeRecord> = Vec::new();
    for def_id in CHAIN_DEFINITIONS {
        if let Some(idx) = nodes.iter().rposition(|n| n.definition_id == def_id) {
            out.push(nodes.swap_remove(idx));
        }
    }
    *nodes = out;
}

/// upsert：按 definition_id 替换，否则追加（保持定义序）。
fn upsert(nodes: &mut Vec<NodeRecord>, record: NodeRecord) {
    if let Some(idx) = nodes.iter().position(|n| n.definition_id == record.definition_id) {
        nodes[idx] = record;
    } else {
        nodes.push(record);
    }
    nodes.sort_by_key(|n| {
        CHAIN_DEFINITIONS
            .iter()
            .position(|d| *d == n.definition_id)
            .unwrap_or(usize::MAX)
    });
}
