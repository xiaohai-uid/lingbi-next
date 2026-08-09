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
pub mod ledger;
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

        // OutputGate（P8 #25）：review/polish 段 Validate→Repair→Validate。
        // 每轮 repair 后复评（review input 含 draft_hash——润色改写草稿 →
        // 复评自然失效重跑）；次数用尽仍 blocking → 链 Rejected。
        if *def_id == "review" {
            let mut gate_rejected = false;
            loop {
                let outcome = run_node_once(
                    &mut RunCtx {
                        root,
                        run_id: &run_id,
                        ch,
                        book_hash: &book_hash,
                        provider: provider.as_ref(),
                        provider_id: &provider_id,
                        model_id: &model_id,
                        nodes: &mut cp.nodes,
                        draft_path: &mut draft_path,
                        checks: &mut checks,
                        review_path: &mut review_path,
                    },
                    "review",
                    5,
                )
                .await?;
                match apply_outcome(&mut status, &mut reject_reason, "review", outcome) {
                    ApplyOutcome::Reused => {}
                    ApplyOutcome::Record(r) => {
                        upsert(&mut cp.nodes, r);
                        if status != ChainStatus::Completed {
                            break; // review 失败，链停
                        }
                    }
                }
                // blocking 判定：读评审文件（复用节点时同样有效）
                let blocking = review_has_blocking(root, ch)?;
                if !blocking {
                    // 门禁通过：polish 透传（正常执行/复用路径）
                    let outcome = run_node_once(
                        &mut RunCtx {
                            root,
                            run_id: &run_id,
                            ch,
                            book_hash: &book_hash,
                            provider: provider.as_ref(),
                            provider_id: &provider_id,
                            model_id: &model_id,
                            nodes: &mut cp.nodes,
                            draft_path: &mut draft_path,
                            checks: &mut checks,
                            review_path: &mut review_path,
                        },
                        "polish",
                        6,
                    )
                    .await?;
                    match apply_outcome(&mut status, &mut reject_reason, "polish", outcome) {
                        ApplyOutcome::Reused => {}
                        ApplyOutcome::Record(r) => {
                            upsert(&mut cp.nodes, r);
                        }
                    }
                    break;
                }
                if repair_attempts >= MAX_REPAIR_ATTEMPTS {
                    status = ChainStatus::Rejected;
                    reject_reason = Some(RejectReason::BlockingIssuesRemain);
                    gate_rejected = true;
                    break;
                }
                // repair：polish 改写 → 复评
                let outcome = run_node_once(
                    &mut RunCtx {
                        root,
                        run_id: &run_id,
                        ch,
                        book_hash: &book_hash,
                        provider: provider.as_ref(),
                        provider_id: &provider_id,
                        model_id: &model_id,
                        nodes: &mut cp.nodes,
                        draft_path: &mut draft_path,
                        checks: &mut checks,
                        review_path: &mut review_path,
                    },
                    "polish",
                    6,
                )
                .await?;
                match apply_outcome(&mut status, &mut reject_reason, "polish", outcome) {
                    ApplyOutcome::Reused => {}
                    ApplyOutcome::Record(r) => {
                        upsert(&mut cp.nodes, r);
                        if status != ChainStatus::Completed {
                            break;
                        }
                    }
                }
                repair_attempts += 1;
            }
            if gate_rejected || status != ChainStatus::Completed {
                break;
            }
            continue; // review/polish 已由门控循环处理
        }

        // 其他节点正常路径（contract/brief/draft/machine-check/settlement/handoff）
        let outcome = run_node_once(
            &mut RunCtx {
                root,
                run_id: &run_id,
                ch,
                book_hash: &book_hash,
                provider: provider.as_ref(),
                provider_id: &provider_id,
                model_id: &model_id,
                nodes: &mut cp.nodes,
                draft_path: &mut draft_path,
                checks: &mut checks,
                review_path: &mut review_path,
            },
            def_id,
            i,
        )
        .await?;
        match apply_outcome(&mut status, &mut reject_reason, def_id, outcome) {
            ApplyOutcome::Reused => {}
            ApplyOutcome::Record(r) => {
                upsert(&mut cp.nodes, r);
                if status != ChainStatus::Completed {
                    break; // Failed/Halted/Rejected → 链停
                }
            }
        }
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

/// OutputGate 修复次数上限（Validate→Repair→Validate 循环，P8 #25）。
pub const MAX_REPAIR_ATTEMPTS: u32 = 2;

/// 运行期上下文（run_node_once 的可变共享状态）。
struct RunCtx<'a> {
    root: &'a Path,
    run_id: &'a str,
    ch: i64,
    book_hash: &'a str,
    provider: &'a dyn AiProvider,
    provider_id: &'a str,
    model_id: &'a str,
    nodes: &'a mut Vec<NodeRecord>,
    draft_path: &'a mut Option<String>,
    checks: &'a mut Vec<lingbi_book::CheckResult>,
    review_path: &'a mut Option<String>,
}

/// 单节点执行结果（复用 / 完成 / 失败 / 机检停机）。
enum NodeOutcome {
    Reused,
    Completed(NodeRecord),
    Failed(NodeRecord),
    Halted(NodeRecord),
}

/// 应用执行结果到链状态（status/reject_reason 副作用 + 需要 upsert 的记录）。
enum ApplyOutcome {
    Reused,
    Record(NodeRecord),
}

fn apply_outcome(
    status: &mut ChainStatus,
    reject_reason: &mut Option<RejectReason>,
    def_id: &str,
    outcome: NodeOutcome,
) -> ApplyOutcome {
    match outcome {
        NodeOutcome::Reused => ApplyOutcome::Reused,
        NodeOutcome::Completed(r) => ApplyOutcome::Record(r),
        NodeOutcome::Failed(r) => {
            if def_id == "settlement"
                && r.error.as_deref().map(|e| e.contains("棘轮")).unwrap_or(false)
            {
                *reject_reason = Some(RejectReason::RatchetRejected);
            }
            *status = ChainStatus::Failed;
            ApplyOutcome::Record(r)
        }
        NodeOutcome::Halted(r) => {
            *status = ChainStatus::Halted;
            *reject_reason = Some(RejectReason::MachineCheckFailed);
            ApplyOutcome::Record(r)
        }
    }
}

/// 评审文件 blocking 判定（门控循环用；文件缺失 = 无评审 → 视为无阻塞）。
fn review_has_blocking(root: &Path, ch: i64) -> Result<bool, AppError> {
    let p = lingbi_book::review::review_path(root, ch);
    if !p.exists() {
        return Ok(false);
    }
    let (issues, errors) = lingbi_book::load_review(&p)?;
    if !errors.is_empty() {
        return Err(AppError::new(
            ErrorCode::BookCorrupted,
            format!("评审文件损坏: {}", errors.join("; ")),
            false,
        ));
    }
    Ok(lingbi_book::review::has_blocking(&issues))
}

/// 执行单个节点（复用判定 + 执行 + 记录；P8 加入 draft_hash/ledger）。
async fn run_node_once(
    ctx: &mut RunCtx<'_>,
    def_id: &str,
    i: usize,
) -> Result<NodeOutcome, AppError> {
    let existing_chapter_hash = crate::hash::chapter_state_hash(ctx.root, ctx.ch)?;
    let upstream = if i == 0 {
        String::new()
    } else {
        ctx.nodes
            .iter()
            .find(|n| n.definition_id == CHAIN_DEFINITIONS[i - 1])
            .map(|n| n.output_hash.clone())
            .unwrap_or_default()
    };
    // review 的输入含当前草稿文件哈希（draft_hash）：润色改写草稿 → 复评
    // 自然失效重跑（OutputGate 的 Validate→Repair→Validate 闭环依据）
    let draft_hash = if def_id == "review" {
        match ctx.draft_path {
            Some(rel) => std::fs::read(ctx.root.join(rel))
                .map(|b| lingbi_storage::sha256_hex(&b))
                .unwrap_or_else(|_| "absent".to_owned()),
            None => "absent".to_owned(),
        }
    } else {
        String::new()
    };
    let params = serde_json::json!({
        "ch": ctx.ch,
        "existing_chapter_hash": existing_chapter_hash,
        "draft_hash": draft_hash,
        "provider": if chain::is_ai_node(def_id) { ctx.provider_id } else { "" },
        "model": if chain::is_ai_node(def_id) { ctx.model_id } else { "" },
    });
    let input_hash = crate::hash::canonical_hash(&[
        ("definition_id", serde_json::json!(def_id)),
        ("definition_version", serde_json::json!(chain::definition_version(def_id))),
        ("book_input_hash", serde_json::json!(ctx.book_hash)),
        ("upstream_output_hash", serde_json::json!(upstream)),
        ("params", params),
    ]);

    // 复用 or 失效（Node Recovery：仅 input fingerprint 未变的 Completed 可 skip）
    let prev_idx = ctx.nodes.iter().position(|n| n.definition_id == def_id);
    if let Some(idx) = prev_idx {
        let prev = &ctx.nodes[idx];
        if prev.status == NodeStatus::Completed && prev.input_hash == input_hash {
            // 复用：重建运行时状态（草稿路径派生自记录 node_id）
            if def_id == "draft" {
                *ctx.draft_path = Some(chain::draft_rel_path(&prev.node_id, ctx.ch));
            }
            if def_id == "machine-check" {
                // 报告需要机检清单；复用节点时重跑（纯确定性，不影响哈希）
                *ctx.checks = lingbi_book::collect_checks(ctx.root, "prewrite", Some(ctx.ch))?;
            }
            if def_id == "review" {
                *ctx.review_path = Some(
                    lingbi_book::review::review_path(ctx.root, ctx.ch)
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            return Ok(NodeOutcome::Reused);
        }
        // 失效：本节点 + 后续全部 Completed 记录 → Invalidated，output_hash 清空
        for n in ctx.nodes.iter_mut().skip(idx) {
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
        provider: chain::is_ai_node(def_id).then(|| ctx.provider_id.to_owned()),
        model: chain::is_ai_node(def_id).then(|| ctx.model_id.to_owned()),
        usage: None,
        idempotency_key: matches!(def_id, "draft" | "settlement").then(|| node_id),
        error: None,
        note: None,
    };

    let artifact = match execute_node(
        def_id,
        ctx.root,
        ctx.ch,
        &record.node_id,
        ctx.provider,
        ctx.draft_path,
        ctx.nodes,
    )
    .await
    {
        Ok(a) => a,
        Err(e) => {
            record.status = NodeStatus::Failed;
            record.error = Some(e.message.clone());
            return Ok(NodeOutcome::Failed(record));
        }
    };
    record.status = NodeStatus::Completed;
    record.output_hash = lingbi_storage::sha256_hex(&artifact.bytes);
    record.usage = artifact.usage;
    record.note = artifact.note;
    if let Some(p) = artifact.draft_path {
        *ctx.draft_path = Some(p);
    }
    if def_id == "machine-check" {
        *ctx.checks = artifact.checks;
        *ctx.review_path = None;
        if !artifact.gate_passed {
            return Ok(NodeOutcome::Halted(record));
        }
    }
    if def_id == "review" {
        *ctx.review_path = Some(
            lingbi_book::review::review_path(ctx.root, ctx.ch)
                .to_string_lossy()
                .into_owned(),
        );
    }
    if def_id == "settlement" {
        *ctx.draft_path = None;
    }
    // TokenLedger（P8 #25）：AI 节点执行后 append（复用/透传不重复记账）
    if let Some(usage) = &record.usage {
        let _ = crate::ledger::append_ledger(
            ctx.root,
            &crate::ledger::TokenLedgerEntry {
                run_id: ctx.run_id.to_owned(),
                node_id: record.node_id.clone(),
                definition_id: record.definition_id.clone(),
                provider: record.provider.clone().unwrap_or_default(),
                model: record.model.clone().unwrap_or_default(),
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        );
    }
    Ok(NodeOutcome::Completed(record))
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
