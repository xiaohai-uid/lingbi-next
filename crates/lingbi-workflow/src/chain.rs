//! 8 节点链定义与节点执行（P7 章事务引擎，#24）。
//!
//! 链：Contract → Brief → Draft → Machine Check → Review → Polish →
//! Settlement → Handoff。确定性节点直接调引擎（lingbi-book）；AI 节点经
//! `AiProvider` 执行（provider/model 快照每次运行解析，入节点 input_hash，
//! 能力 8）。节点产物 `NodeArtifact` 由 `lib::run_chain` 哈希入记录。
//!
//! 提示词为引擎内部契约（DRAFT_SYSTEM / REVIEW_INSTRUCTION /
//! POLISH_INSTRUCTION），P9 产品 UI 再暴露模板定制。

use crate::node::NodeRecord;
use lingbi_ai::{AiEvent, AiError, AiProvider, ChatMessage, ChatRequest, TokenUsage};
use lingbi_book::state::BODY_DIR;
use lingbi_contracts::{AppError, ErrorCode};
use std::path::Path;

/// 候选稿目录（P4 DRAFT_DIR 惯例：`工作区/草稿/{ch:03}-{uuid}.md`，mutable）。
pub const DRAFT_DIR: &str = "工作区/草稿";

/// 8 节点定义序（固定；变更需递增 CHAIN_VERSION）。
pub const CHAIN_DEFINITIONS: [&str; 8] = [
    "contract",
    "brief",
    "draft",
    "machine-check",
    "review",
    "polish",
    "settlement",
    "handoff",
];

/// 链定义版本（节点序/定义变更时递增，进入 checkpoint 与规格审计）。
pub const CHAIN_VERSION: u32 = 1;

/// AI 节点集合。
pub fn is_ai_node(def_id: &str) -> bool {
    matches!(def_id, "draft" | "review" | "polish")
}

/// 节点实现版本（定义语义变更时递增，进入 input_hash）。
/// review/polish 在 P8（#25）因 input 加入 draft_hash 升至 2（旧 checkpoint
/// 首次加载 input 不匹配 → 全部失效重跑，无需迁移代码）。
pub fn definition_version(def_id: &str) -> u32 {
    match def_id {
        "review" | "polish" => 2,
        _ => 1,
    }
}

/// 节点产物（run_chain 哈希 bytes 入 output_hash）。
#[derive(Default)]
pub struct NodeArtifact {
    /// 产物字节（output_hash 的输入）。
    pub bytes: Vec<u8>,
    /// 本次执行后「当前草稿」相对路径（draft/polish 设置；settlement 清空）。
    pub draft_path: Option<String>,
    pub note: Option<String>,
    pub usage: Option<TokenUsage>,
    /// machine-check：机检清单（报告用）。
    pub checks: Vec<lingbi_book::CheckResult>,
    /// machine-check：状态门禁是否通过（false → 链 halt）。
    pub gate_passed: bool,
}

/// 草稿相对路径：`工作区/草稿/{ch:03}-{uuid}.md`（uuid 取自 node_id）。
pub fn draft_rel_path(node_id: &str, ch: i64) -> String {
    let uuid = node_id
        .split_once('-')
        .map(|(_, rest)| rest)
        .unwrap_or(node_id);
    format!("{DRAFT_DIR}/{ch:03}-{uuid}.md")
}

/// 目标章文件（定稿/正文 下 chapter_num == ch）→ (绝对路径, 文件名)。
pub fn chapter_file_for(root: &Path, ch: i64) -> Option<(std::path::PathBuf, String)> {
    for f in lingbi_book::chapter_files(root) {
        if lingbi_book::chapter_num(&f) == ch {
            let fname = f.clone();
            return Some((root.join(BODY_DIR).join(&f), fname));
        }
    }
    None
}

fn write_file(root: &Path, rel: &str, bytes: &[u8]) -> Result<(), AppError> {
    let p = root.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::new(
                ErrorCode::DiskWriteFailed,
                format!("创建目录失败: {e}"),
                false,
            )
        })?;
    }
    std::fs::write(&p, bytes).map_err(|e| {
        AppError::new(
            ErrorCode::DiskWriteFailed,
            format!("写文件失败: {e}"),
            false,
        )
    })
}

fn read_file(root: &Path, rel: &str) -> Result<String, AppError> {
    std::fs::read_to_string(root.join(rel)).map_err(|_| {
        AppError::new(
            ErrorCode::DocumentNotFound,
            format!("缺文件: {rel}"),
            false,
        )
    })
}

fn map_ai_error(e: AiError) -> AppError {
    let (code, retryable) = match e {
        AiError::NoApiKey => (ErrorCode::AiNoApiKey, false),
        AiError::AuthFailed => (ErrorCode::AiAuthFailed, false),
        AiError::RateLimited => (ErrorCode::AiRateLimited, true),
        AiError::Timeout => (ErrorCode::AiTimeout, true),
        AiError::Network => (ErrorCode::AiNetworkError, true),
        AiError::Server(_) => (ErrorCode::AiServerError, true),
        AiError::InvalidResponse => (ErrorCode::AiInvalidResponse, true),
        AiError::Cancelled => (ErrorCode::AiCancelled, false),
    };
    AppError::new(code, e.to_string(), retryable)
}

/// 收集 provider 流：正文 + Usage（节点级 token 记账）。
/// cancel 给定 → 经 stream_chat_with_cancel 传播到 provider 流（P8 #25）。
pub async fn run_ai(
    provider: &dyn AiProvider,
    request: ChatRequest,
    cancel: Option<&lingbi_ai::CancellationToken>,
) -> Result<(String, TokenUsage), AppError> {
    use futures::StreamExt;
    let mut stream = match cancel {
        Some(token) => provider.stream_chat_with_cancel(request, token.clone()),
        None => provider.stream_chat(request),
    };
    let mut text = String::new();
    let mut usage = TokenUsage {
        prompt_tokens: 0,
        completion_tokens: 0,
    };
    while let Some(event) = stream.next().await {
        match event.map_err(map_ai_error)? {
            AiEvent::ContentDelta(delta) => text.push_str(&delta),
            AiEvent::Usage(u) => usage = u,
            AiEvent::ReasoningDelta(_) => {}
            AiEvent::Completed => break,
        }
    }
    Ok((text, usage))
}

// ---------------------------------------------------------------------------
// 节点执行器
// ---------------------------------------------------------------------------

/// contract（确定性）：引擎 build_contract。
pub fn exec_contract(root: &Path, ch: i64) -> Result<NodeArtifact, AppError> {
    let s = lingbi_book::load_state(root)?;
    let json = lingbi_book::build_contract(root, &s, ch).to_json();
    Ok(NodeArtifact {
        bytes: json.into_bytes(),
        note: Some("写作合同已生成".to_owned()),
        ..Default::default()
    })
}

/// brief（确定性）：引擎 build_brief（五段任务书 = 合同 + 四路上下文）。
pub fn exec_brief(root: &Path, ch: i64) -> Result<NodeArtifact, AppError> {
    let s = lingbi_book::load_state(root)?;
    let brief = lingbi_book::build_brief(root, &s, ch, None, None)?;
    Ok(NodeArtifact {
        bytes: brief.into_bytes(),
        note: Some("任务书已生成".to_owned()),
        ..Default::default()
    })
}

const DRAFT_SYSTEM: &str = "你是一位资深网络小说作家。根据任务书直接写本章正文：只输出正文文本，不要解释、不要标题、不要 markdown 代码块。";

/// draft（AI）：任务书 → provider → 草稿落 `工作区/草稿/`（P4 候选稿惯例）。
pub async fn exec_draft(
    root: &Path,
    ch: i64,
    node_id: &str,
    provider: &dyn AiProvider,
    cancel: Option<&lingbi_ai::CancellationToken>,
    max_tokens: Option<u32>,
) -> Result<NodeArtifact, AppError> {
    let s = lingbi_book::load_state(root)?;
    let brief = lingbi_book::build_brief(root, &s, ch, None, None)?;
    let request = ChatRequest {
        messages: vec![
            ChatMessage {
                role: "system".to_owned(),
                content: DRAFT_SYSTEM.to_owned(),
            },
            ChatMessage {
                role: "user".to_owned(),
                content: brief,
            },
        ],
        temperature: 0.7,
        max_tokens: max_tokens.unwrap_or(4096),
    };
    let (text, usage) = run_ai(provider, request, cancel).await?;
    let rel = draft_rel_path(node_id, ch);
    write_file(root, &rel, text.as_bytes())?;
    Ok(NodeArtifact {
        bytes: text.into_bytes(),
        draft_path: Some(rel),
        usage: Some(usage),
        note: Some("草稿已生成".to_owned()),
        ..Default::default()
    })
}

/// machine-check（确定性）：prewrite 状态门禁（编号/占位符/摘要/字数弱提示）
/// + 草稿文本红线/占位符命中（仅记录，供 Review 输入）。状态门禁不过 → 链 halt。
pub fn exec_machine_check(
    root: &Path,
    ch: i64,
    draft_path: &Option<String>,
) -> Result<NodeArtifact, AppError> {
    let checks = lingbi_book::collect_checks(root, "prewrite", Some(ch))?;
    let g = lingbi_book::gate(root, "prewrite", Some(ch))?;
    let draft_text = match draft_path {
        Some(rel) => std::fs::read_to_string(root.join(rel)).unwrap_or_default(),
        None => String::new(),
    };
    let mut hits: Vec<String> = Vec::new();
    for h in lingbi_book::placeholder_hits(&draft_text) {
        hits.push(format!("占位符「{h}」"));
    }
    for (name, frag) in lingbi_book::redline_hits(&draft_text, Some(root))? {
        hits.push(format!("红线「{name}」命中: {frag}"));
    }
    let payload = serde_json::json!({
        "checks": checks.iter().map(|c| serde_json::json!({"ok": c.ok, "name": c.name, "detail": c.detail})).collect::<Vec<_>>(),
        "draft_hits": hits,
        "gate_passed": g.passed,
    });
    let bytes = serde_json::to_vec(&payload)
        .map_err(|e| AppError::new(ErrorCode::BookCorrupted, format!("机检序列化失败: {e}"), false))?;
    let note = if !g.passed {
        Some(format!(
            "状态门禁未过({} 项): {}",
            g.fails.len(),
            g.fails.join("; ")
        ))
    } else if !hits.is_empty() {
        Some(format!("草稿命中 {} 项: {}", hits.len(), hits.join("; ")))
    } else {
        Some("机检通过".to_owned())
    };
    Ok(NodeArtifact {
        bytes,
        checks,
        gate_passed: g.passed,
        note,
        ..Default::default()
    })
}

const REVIEW_INSTRUCTION: &str = "请评审以下草稿，输出严格 JSON（不要 markdown 代码块）：{\"issues\": [{\"severity\": \"high|medium|low|critical\", \"category\": \"continuity|setting|character|timeline|ai_flavor|logic|pacing|other\", \"location\": \"...\", \"description\": \"...\", \"evidence\": \"...\", \"fix_hint\": \"...\"}]}";

/// review（AI）：草稿 + 任务书 → provider → 评审 JSON → save_review +
/// write_report（`工作区/评审报告/`）。解析失败 → 节点 Failed（可重试）。
pub async fn exec_review(
    root: &Path,
    ch: i64,
    provider: &dyn AiProvider,
    draft_path: &Option<String>,
    cancel: Option<&lingbi_ai::CancellationToken>,
    max_tokens: Option<u32>,
) -> Result<NodeArtifact, AppError> {
    let s = lingbi_book::load_state(root)?;
    let brief = lingbi_book::build_brief(root, &s, ch, None, None)?;
    let draft = match draft_path {
        Some(rel) => read_file(root, rel)?,
        None => {
            return Err(AppError::new(
                ErrorCode::DocumentNotFound,
                "无草稿可评审(链未产稿)".to_owned(),
                false,
            ))
        }
    };
    let request = ChatRequest {
        messages: vec![ChatMessage {
            role: "user".to_owned(),
            content: format!("{REVIEW_INSTRUCTION}\n\n任务书:\n{brief}\n\n草稿:\n{draft}"),
        }],
        temperature: 0.2,
        max_tokens: max_tokens.unwrap_or(4096),
    };
    let (text, usage) = run_ai(provider, request, cancel).await?;
    let (issues, errors) = lingbi_book::parse_review_json(&text)?;
    if !errors.is_empty() {
        return Err(AppError::new(
            ErrorCode::AiInvalidResponse,
            format!("评审解析失败: {}", errors.join("; ")),
            true,
        ));
    }
    lingbi_book::review::save_review(root, ch, &issues)?;
    lingbi_book::review::write_report(root, ch, &issues)?;
    let blocking = lingbi_book::review::has_blocking(&issues);
    let note = if blocking {
        format!(
            "{} 项阻塞问题(待润色处理)",
            issues.iter().filter(|i| i.blocking).count()
        )
    } else {
        "无阻塞问题".to_owned()
    };
    Ok(NodeArtifact {
        bytes: text.into_bytes(),
        usage: Some(usage),
        note: Some(note),
        ..Default::default()
    })
}

const POLISH_INSTRUCTION: &str = "根据评审意见改写草稿，解决所有 blocking 问题，保持文风一致。只输出改写后的正文，不要解释、不要 markdown 代码块。";

/// polish（AI，条件）：评审有 blocking → provider 改写 → 新草稿文件；
/// 无 blocking → 原样透传（note 注明，不产生 AI 消耗）。
pub async fn exec_polish(
    root: &Path,
    ch: i64,
    node_id: &str,
    provider: &dyn AiProvider,
    draft_path: &Option<String>,
    cancel: Option<&lingbi_ai::CancellationToken>,
    max_tokens: Option<u32>,
) -> Result<NodeArtifact, AppError> {
    let current = match draft_path {
        Some(rel) => read_file(root, rel)?,
        None => {
            return Err(AppError::new(
                ErrorCode::DocumentNotFound,
                "无草稿可润色(链未产稿)".to_owned(),
                false,
            ))
        }
    };
    let review_file = lingbi_book::review::review_path(root, ch);
    let (issues, errors) = lingbi_book::load_review(&review_file)?;
    if !errors.is_empty() {
        return Err(AppError::new(
            ErrorCode::AiInvalidResponse,
            format!("评审文件损坏: {}", errors.join("; ")),
            true,
        ));
    }
    if !lingbi_book::review::has_blocking(&issues) {
        // 透传：output_hash = 当前草稿哈希，不新建草稿文件
        return Ok(NodeArtifact {
            bytes: current.into_bytes(),
            note: Some("无阻塞问题，跳过改写".to_owned()),
            ..Default::default()
        });
    }
    let review_json = serde_json::json!({
        "issues": issues.iter().map(|i| serde_json::json!({
            "severity": i.severity, "category": i.category, "location": i.location,
            "description": i.description, "evidence": i.evidence, "fix_hint": i.fix_hint,
            "blocking": i.blocking,
        })).collect::<Vec<_>>()
    });
    let request = ChatRequest {
        messages: vec![ChatMessage {
            role: "user".to_owned(),
            content: format!(
                "{POLISH_INSTRUCTION}\n\n评审:\n{review_json}\n\n草稿:\n{current}"
            ),
        }],
        temperature: 0.5,
        max_tokens: max_tokens.unwrap_or(4096),
    };
    let (text, usage) = run_ai(provider, request, cancel).await?;
    let rel = draft_rel_path(node_id, ch);
    write_file(root, &rel, text.as_bytes())?;
    Ok(NodeArtifact {
        bytes: text.into_bytes(),
        draft_path: Some(rel),
        usage: Some(usage),
        note: Some("按评审意见改写".to_owned()),
        ..Default::default()
    })
}

/// settlement（确定性+事务）：棘轮（已提交章拒绝）→ 正文重组 → tx 事务门
/// CAS 写 `定稿/正文/` → cmd_commit 结算（合并事实/bump/视图/版本快照）→
/// 删草稿。与 P4 adopt_candidate 同一事务纪律（precommit 警告透传）。
pub fn exec_settlement(
    root: &Path,
    ch: i64,
    draft_path: &Option<String>,
) -> Result<NodeArtifact, AppError> {
    let rel = draft_path.as_ref().ok_or_else(|| {
        AppError::new(
            ErrorCode::MutationConflict,
            "无草稿可结算(链未产稿)".to_owned(),
            false,
        )
    })?;
    let s = lingbi_book::load_state(root)?;
    if s.chapters.contains_key(&ch.to_string()) {
        return Err(AppError::new(
            ErrorCode::MutationConflict,
            format!("章 {ch} 已提交——定稿棘轮:已结算正文不可直接改写;重写走回炉(revision)"),
            false,
        ));
    }
    let content = read_file(root, rel)?;
    let (path, fname) = chapter_file_for(root, ch).ok_or_else(|| {
        AppError::new(
            ErrorCode::BookNotFound,
            format!("章 {ch} 无正文文件"),
            false,
        )
    })?;
    let bytes = lingbi_book::replace_body(&path, &content)?;
    let expected = std::fs::read(&path).ok().map(|b| lingbi_storage::sha256_hex(&b));
    lingbi_book::tx::ensure_tx_layer(root)?;
    lingbi_book::tx::write_authoritative(
        root,
        &format!("{BODY_DIR}/{fname}"),
        &bytes,
        expected.as_deref(),
        None,
    )?;
    // 注入 failpoint（测试/恢复演练）：正文已事务落盘、结算前模拟崩溃。
    // 生产路径不设置该环境变量；重跑时 CAS（期望 = 当前正文）幂等重写 +
    // cmd_commit 章已提交守卫 → 修订只 bump 一次。
    if std::env::var("LINGBI_FAILPOINT").as_deref() == Ok("settlement-after-body") {
        return Err(AppError::new(
            ErrorCode::DiskWriteFailed,
            "failpoint: settlement-after-body 模拟崩溃(正文已落盘,结算未完成)".to_owned(),
            false,
        ));
    }
    let out = lingbi_book::cmd::cmd_commit(root, ch, true)?;
    if out.exit != 0 {
        return Err(AppError::new(
            ErrorCode::MutationConflict,
            format!("结算失败: {}", out.stdout.trim()),
            false,
        ));
    }
    let _ = std::fs::remove_file(root.join(rel)); // 草稿可丢弃，尽力删除
    Ok(NodeArtifact {
        bytes,
        note: Some("已结算:正文事务落盘 + 追踪合并".to_owned()),
        ..Default::default()
    })
}

/// 构建 handoff 报告（确定性）：章路径/标题/字数 + 全链节点摘要。
/// 文件写入由 run_chain 在链完成后统一执行（含本节点记录，8 节点摘要）。
pub fn build_handoff_report(
    root: &Path,
    ch: i64,
    nodes: &[NodeRecord],
) -> Result<crate::HandoffReport, AppError> {
    let (path, fname) = chapter_file_for(root, ch).ok_or_else(|| {
        AppError::new(
            ErrorCode::BookNotFound,
            format!("章 {ch} 无正文文件(结算未完成)"),
            false,
        )
    })?;
    let fm = lingbi_book::frontmatter(&path)?;
    let text = lingbi_book::body(&path)?;
    Ok(crate::HandoffReport {
        ch,
        chapter_path: format!("{BODY_DIR}/{fname}"),
        title: fm.get("title").cloned().unwrap_or_default(),
        words: lingbi_book::hanzi(&text),
        nodes: nodes
            .iter()
            .map(|n| crate::HandoffNode {
                definition_id: n.definition_id.clone(),
                status: format!("{:?}", n.status),
                input_hash: n.input_hash.clone(),
                output_hash: n.output_hash.clone(),
            })
            .collect(),
    })
}

/// handoff（确定性）：交付报告产物字节（文件由 run_chain 统一落盘）。
pub fn exec_handoff(root: &Path, ch: i64, nodes: &[NodeRecord]) -> Result<NodeArtifact, AppError> {
    let report = build_handoff_report(root, ch, nodes)?;
    let json = serde_json::to_vec_pretty(&report)
        .map_err(|e| AppError::new(ErrorCode::BookCorrupted, format!("交付序列化失败: {e}"), false))?;
    Ok(NodeArtifact {
        bytes: json,
        note: Some("本章已完成交付".to_owned()),
        ..Default::default()
    })
}
