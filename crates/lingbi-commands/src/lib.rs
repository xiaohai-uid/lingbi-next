//! P4 核心服务层：Book V3 唯一边界（#21）。
//!
//! 18 个命令（P4 16 + P5-1 增量 2）构成 UI（Tauri 壳 / 未来任何宿主）触达
//! 书仓的唯一通道：
//!
//! ```text
//! create_book / open_book / get_tracking_state / get_outline —— 会话与状态
//! create_chapter / list_chapters / open_chapter / save_chapter —— 章正文
//! build_contract / build_brief / run_check            —— 写作辅助
//! create_candidate / adopt_candidate / reject_candidate —— 候选稿
//! run_chain / resume_workflow / cancel_workflow / get_run_ledger —— 工作流
//! ```
//!
//! `run_chain`（P7 #24）= 8 节点章事务链（lingbi-workflow），异步收
//! `Arc<dyn AiProvider>`；`resume/cancel_workflow/get_run_ledger` 保持 P4
//! 语义，Run 持久化/幂等/账本收口属 P8（#25）。
//!
//! 纪律（与引擎/既有 crate 的差异，fail-closed 声明）：
//! - 本 crate 是薄包装：领域逻辑全部复用 `lingbi-book` / `lingbi-recovery`，
//!   不复制引擎逻辑；命令输出（contract/brief/diagnose）为引擎字节等价
//!   JSON/Markdown，期望值来自 golden corpus。
//! - authoritative 写入只经 `lingbi-book::tx` 事务门（managed 书仓）或
//!   引擎 legacy 路径（`.lingbi/` 缺席的旧书仓）——与 `save_state` 的
//!   双路径一致，不引入第三条写路径。
//! - 候选稿生命周期为 P4 最小版（`工作区/草稿/` mutable 文件，不经事务
//!   门）；P7 章事务引擎（#24）以 `.lingbi/candidates` 状态机接管，本模块
//!   不提前实现 P7 语义。
//! - run_chain（P7）为异步 8 节点链，AI 节点经注入的 `AiProvider` 执行；
//!   resume/cancel_workflow 保持 P4 同步语义，Run 持久化/幂等/账本收口属
//!   P8（#25），cancel 为空操作并说明原因。
//! - 建章 frontmatter 模板为应用语义（引擎无「建章」命令，正文由外部写
//!   入）；字段名与引擎解析器一致，commit 时 `merge_chapter_facts` 照常
//!   消化，不触碰引擎既有字节路径。
//! - 标题仅允许 Windows 合法文件名字符（含中文；拒绝控制字符）；
//!   非法字符 fail-closed（INVALID_TITLE）。
//! - 定稿棘轮：已结算章节拒绝直写（CHAPTER_COMMITTED，save_chapter 与
//!   adopt_candidate 前置守卫）——旧章编辑走回炉重放级联（revision），
//!   P7 章事务引擎暴露 UI 通道；P4 不做隐式重写。
//! - 候选稿 id 为白名单格式 `{ch:03}-{uuid}`（防路径穿越）；
//!   错误码约定：命令层守卫用 SCREAMING_SNAKE（INVALID_TITLE/
//!   CHAPTER_COMMITTED/NO_BOOK_SESSION 等），引擎/事务门派生错误透传
//!   `AppError` 的 Debug 格式（PascalCase，如 DocumentConflict/
//!   BookNotFound）——与桌面壳 V2 命令层同构，前端按两套拼写分派。
//! - 采纳候选不更新正文 frontmatter 的 status 字段（引擎结算只合并追踪，
//!   不重写正文头）；状态卡等派生视图由引擎重建，语义与引擎一致。

use lingbi_book::{
    body, chapter_files, chapter_num, collect_checks, diagnose, frontmatter, latest_chapter,
    load_state, tx,
};
use lingbi_book::{build_brief as book_build_brief, build_contract as book_build_contract};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// 候选稿目录（spec 工作区 = mutable，可丢弃，不经事务门）。
pub const DRAFT_DIR: &str = "工作区/草稿";
/// 引擎正文目录（state::BODY_DIR = "定稿/正文"）。
const BODY_DIR: &str = "定稿/正文";

// ---------------------------------------------------------------------------
// 错误
// ---------------------------------------------------------------------------

/// 命令层错误（自包含；宿主壳映射为自己的错误 DTO）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl CommandError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }
}

impl From<lingbi_contracts::AppError> for CommandError {
    fn from(error: lingbi_contracts::AppError) -> Self {
        Self {
            code: format!("{:?}", error.code),
            message: error.message,
            retryable: error.retryable,
        }
    }
}

// ---------------------------------------------------------------------------
// DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct BookSessionDto {
    pub name: String,
    pub root: String,
    /// `.lingbi/` 事务层是否启用（false = 合法旧书仓，写路径走引擎 legacy）。
    pub managed: bool,
    pub state_revision: i64,
    pub chapter_count: usize,
    /// 启动恢复收敛的记录数（managed 书仓；含前向补完与清理）。
    pub recovered: usize,
    /// 启动恢复检出的外部修改/IO 错误数（0 = 干净）。
    pub recovery_failed: usize,
    pub recovery_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChapterDto {
    pub ch: i64,
    pub title: String,
    pub frontmatter: HashMap<String, String>,
    /// 正文（不含 front matter；引擎 `body()` 语义，含前导空行）。
    pub body: String,
    pub content_hash: String,
}

/// 章节列表项（P5-1：UI 章列表 = 书仓内全部章文件，含未结算章）。
#[derive(Debug, Clone, Serialize)]
pub struct ChapterMetaDto {
    pub ch: i64,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TxReceiptDto {
    pub tx_id: String,
    pub rel_path: String,
    pub after_revision: Option<i64>,
    pub after_content_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutlineDto {
    pub ch: i64,
    pub title: Option<String>,
    pub goal: Option<String>,
    pub hard: Option<String>,
    pub forbidden: Option<String>,
    pub roles: Option<String>,
    pub suspense: Option<String>,
    pub foreshadow: Option<String>,
    pub twist: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResultDto {
    pub ok: bool,
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CandidateDto {
    /// 候选稿 id（`{ch:03}-{uuid}`，工作区/草稿 文件名基底）。
    pub id: String,
    pub ch: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdoptResultDto {
    pub ch: i64,
    /// 结算（cmd_commit）exit code：0 = 采纳并结算；非 0 = 正文已落、结算未过。
    pub exit: i32,
    pub stdout: String,
}

/// P7 章事务链运行结果（8 节点 DTO；对应 lingbi-workflow::ChainReport）。
#[derive(Debug, Clone, Serialize)]
pub struct ChainRunDto {
    pub ch: i64,
    /// Completed / Halted（机检门禁不过）/ Failed（节点失败）/ Rejected（OutputGate 拒绝）。
    pub status: String,
    /// 本次运行 run_id（台账 `工作区/工作流/runs/{run_id}.json`，重启后可查）。
    pub run_id: String,
    /// OutputGate 已执行修复次数。
    pub repair_attempts: u32,
    /// 类型化拒绝原因（停机/拒绝/失败时）。
    pub reject_reason: Option<String>,
    pub book_input_hash: String,
    pub nodes: Vec<ChainNodeDto>,
    pub checks: Vec<CheckResultDto>,
    pub draft_path: Option<String>,
    pub review_path: Option<String>,
    pub handoff: Option<HandoffDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChainNodeDto {
    pub node_id: String,
    pub definition_id: String,
    pub definition_version: u32,
    pub input_hash: String,
    pub output_hash: String,
    pub status: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub usage: Option<UsageDto>,
    pub error: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageDto {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// 交付报告（handoff 节点产物摘要）。
#[derive(Debug, Clone, Serialize)]
pub struct HandoffDto {
    pub ch: i64,
    pub chapter_path: String,
    pub title: String,
    pub words: usize,
    pub nodes: Vec<HandoffNodeDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HandoffNodeDto {
    pub definition_id: String,
    pub status: String,
    pub input_hash: String,
    pub output_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CancelResultDto {
    pub run_id: String,
    pub cancelled: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResultDto {
    pub format: String,
    pub path: String,
}

/// P6 迁移回执 DTO（六字段，对应 lingbi-migration::MigrationReceipt）。
#[derive(Debug, Clone, Serialize)]
pub struct MigrationReceiptDto {
    pub source_hash: String,
    pub classification: String,
    pub destination: String,
    pub destination_hash: String,
    pub migration_rule: Vec<FileRuleDto>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileRuleDto {
    pub source: String,
    pub destination: String,
    pub rule: String,
}

/// 声明式写作技巧卡（P5-5）：只读静态数据——无文件系统/进程/执行能力，
/// UI 仅渲染展示。完整小白 4 步模式随 P9（#26）产品 UI。
#[derive(Debug, Clone, Serialize)]
pub struct SkillCardDto {
    pub id: String,
    pub title: String,
    pub description: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunLedgerDto {
    /// `.lingbi/checkpoints/*.json`（append-only 历史，来自事务门写前审计）。
    pub checkpoints: Vec<Value>,
    /// `.lingbi/receipts/*.json`（append-only 提交回执）。
    pub receipts: Vec<Value>,
}

// ---------------------------------------------------------------------------
// 会话命令
// ---------------------------------------------------------------------------

/// 打开书仓（Book V3 唯一入口）。managed 书仓先跑崩溃收敛恢复
/// （`lingbi-recovery::recover_book_v3`，哈希证据驱动、从不清除用户字节），
/// 再幂等落地事务层；legacy 书仓（`.lingbi/` 缺席）原样可开。
pub fn open_book(root: &str) -> Result<BookSessionDto, CommandError> {
    let root_path = Path::new(root);
    let managed = tx::is_managed(root_path);
    let (recovered, recovery_failed, recovery_errors) = if managed {
        let report = lingbi_recovery::recover_book_v3(root_path).map_err(CommandError::from)?;
        tx::ensure_tx_layer(root_path).map_err(CommandError::from)?;
        (
            report.completed + report.cleaned,
            report.failed,
            report.errors,
        )
    } else {
        (0, 0, Vec::new())
    };
    let s = load_state(root_path).map_err(CommandError::from)?;
    let name = root_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_owned());
    Ok(BookSessionDto {
        name,
        root: root.to_owned(),
        managed,
        state_revision: s.state_revision,
        chapter_count: s.chapters.len(),
        recovered,
        recovery_failed,
        recovery_errors,
    })
}

/// 唯一权威状态（`追踪/_tracking-state.json`）的只读视图。
pub fn get_tracking_state(root: &str) -> Result<lingbi_book::TrackingState, CommandError> {
    load_state(Path::new(root)).map_err(CommandError::from)
}

/// 新建书（P5-1，spec user story 1「新建书」）：在 `root` 下创建完整
/// Book V3 书仓——book.yaml（引擎默认配置窗口）+ 追踪状态初始化 +
/// `.lingbi/` 事务层（新建即托管，与 P3 审计「应用建书时 ensure_tx_layer」
/// 一致）。对已存在书仓幂等（init_state 无操作 + open_book 语义）。
/// 返回会话 DTO；书仓内其它内容（大纲/文风等）由作者经命令层逐步创建。
pub fn create_book(root: &str, name: &str) -> Result<BookSessionDto, CommandError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::new("INVALID_BOOK_NAME", "书名不能为空", false));
    }
    if name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || name.chars().any(char::is_control)
    {
        return Err(CommandError::new(
            "INVALID_BOOK_NAME",
            format!("书名含 Windows 非法字符: {name}"),
            false,
        ));
    }
    let root_path = Path::new(root);
    std::fs::create_dir_all(root_path).map_err(|e| {
        CommandError::new(
            "DISK_WRITE_FAILED",
            format!("创建书仓目录失败: {e}"),
            false,
        )
    })?;
    // 先落地事务层，再初始化——book.yaml 与追踪状态首写都经事务门
    // （managed 下 init_state → save_state 走 write_authoritative_state，
    // book.yaml 经 write_authoritative 首次写无 CAS），维持 P3 审计
    // 「权威路径无绕过事务」结论（建仓写 = 创建时一次性写，同样入账）。
    tx::ensure_tx_layer(root_path).map_err(CommandError::from)?;
    let yaml = format!(
        "---\nname: {name}\nstatus: writing\ntotal_chapters: {}\nmin_words_per_chapter: {}\nmax_words_per_chapter: {}\ntarget_words: {}\n---\n",
        lingbi_book::config::DEFAULT_TOTAL_CHAPTERS,
        lingbi_book::config::DEFAULT_MIN_WORDS,
        lingbi_book::config::DEFAULT_MAX_WORDS,
        lingbi_book::config::DEFAULT_TARGET_WORDS,
    );
    if !root_path.join("book.yaml").exists() {
        tx::write_authoritative(root_path, "book.yaml", yaml.as_bytes(), None, None)
            .map_err(CommandError::from)?;
    }
    lingbi_book::tracking::init_state(root_path).map_err(CommandError::from)?;
    // 书名以入参为准（root basename 可能 ≠ 书名，如自定义保存位置）
    let mut dto = open_book(root)?;
    dto.name = name.to_owned();
    Ok(dto)
}

/// 细纲蓝图（大纲/细纲蓝图 表，引擎 `load_outline`）。
pub fn get_outline(root: &str) -> Result<Vec<OutlineDto>, CommandError> {
    Ok(lingbi_book::load_outline(Path::new(root))
        .into_iter()
        .map(|r| OutlineDto {
            ch: r.ch,
            title: r.title,
            goal: r.goal,
            hard: r.hard,
            forbidden: r.forbidden,
            roles: r.roles,
            suspense: r.suspense,
            foreshadow: r.foreshadow,
            twist: r.twist,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// 章正文命令
// ---------------------------------------------------------------------------

/// 建章：`定稿/正文/{ch:03}-{title}.md`（frontmatter 模板 + 空正文），
/// 经事务门落盘（managed）。返回新章；`title` 须为 Windows 合法文件名。
pub fn create_chapter(root: &str, title: &str) -> Result<ChapterDto, CommandError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(CommandError::new("INVALID_TITLE", "章标题不能为空", false));
    }
    if title.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || title.chars().any(char::is_control)
    {
        return Err(CommandError::new(
            "INVALID_TITLE",
            format!("标题含 Windows 非法字符: {title}"),
            false,
        ));
    }
    let root_path = Path::new(root);
    let ch = latest_chapter(root_path) + 1;
    let fname = format!("{ch:03}-{title}.md");
    // 引擎 frontmatter 字段名（title/chapter/volume/words/status），
    // commit 时 merge_chapter_facts 照常消化；正文留空由作者/候选填充。
    // 文件格式与引擎正文一致：frontmatter 段 + 空行 + 正文（空正文即 ""）。
    let fm = format!(
        "---\ntitle: {title}\nchapter: {ch:03}\nvolume: 1\nwords: 0\nstatus: 草稿\n---\n\n"
    );
    write_body(root, &fname, fm.as_bytes(), None, None)?;
    Ok(ChapterDto {
        ch,
        title: title.to_owned(),
        frontmatter: HashMap::from([
            ("title".to_owned(), title.to_owned()),
            ("chapter".to_owned(), format!("{ch:03}")),
            ("volume".to_owned(), "1".to_owned()),
            ("words".to_owned(), "0".to_owned()),
            ("status".to_owned(), "草稿".to_owned()),
        ]),
        body: String::new(),
        content_hash: lingbi_storage::sha256_hex(fm.as_bytes()),
    })
}

/// 读章：frontmatter + 正文（不含 front matter）+ 当前文件哈希
/// （哈希即 save/adopt 的 CAS 期望值来源）。
pub fn open_chapter(root: &str, ch: i64) -> Result<ChapterDto, CommandError> {
    let path = chapter_file(Path::new(root), ch)?;
    let fm = frontmatter(&path).map_err(CommandError::from)?;
    let body = body(&path).map_err(CommandError::from)?;
    let bytes = std::fs::read(&path).map_err(|e| {
        CommandError::new(
            "DOCUMENT_NOT_FOUND",
            format!("读章节失败: {e}"),
            false,
        )
    })?;
    Ok(ChapterDto {
        ch,
        title: fm.get("title").cloned().unwrap_or_default(),
        frontmatter: fm,
        body,
        content_hash: lingbi_storage::sha256_hex(&bytes),
    })
}

/// 章节列表（P5-1）：书仓内全部正文文件（`定稿/正文/NNN-*.md`，章号序），
/// 含未结算章——UI 章列表以此为准（tracking 仅含已结算章，不适合作列表）。
pub fn list_chapters(root: &str) -> Result<Vec<ChapterMetaDto>, CommandError> {
    let root_path = Path::new(root);
    let mut out: Vec<ChapterMetaDto> = chapter_files(root_path)
        .into_iter()
        .map(|f| {
            let ch = chapter_num(&f);
            let path = root_path.join(BODY_DIR).join(&f);
            let title = frontmatter(&path)
                .map(|fm| fm.get("title").cloned().unwrap_or_default())
                .unwrap_or_default();
            ChapterMetaDto { ch, title }
        })
        .collect();
    out.sort_by_key(|c| c.ch);
    Ok(out)
}

/// 存章：正文替换 body 段，frontmatter 原样保留；`expected_hash`
/// （open_chapter 返回）做 CAS——外部修改后存盘被拒、用户字节保留。
/// managed 书仓经事务门（journal+回执），legacy 书仓走引擎路径。
/// 定稿棘轮：已结算（追踪已合并）的章节拒绝直接改写——旧章编辑须走
/// 回炉重放级联（storyrepo revision / P7 章事务引擎 UI 通道）。
pub fn save_chapter(
    root: &str,
    ch: i64,
    content: &str,
    expected_hash: Option<String>,
) -> Result<TxReceiptDto, CommandError> {
    ratchet_guard(Path::new(root), ch)?;
    let path = chapter_file(Path::new(root), ch)?;
    let fname = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| CommandError::new("DOCUMENT_NOT_FOUND", "缺章文件", false))?;
    let bytes = full_chapter_bytes(&path, content)?;
    write_body(root, &fname, &bytes, expected_hash, None)
}

// ---------------------------------------------------------------------------
// 写作辅助命令
// ---------------------------------------------------------------------------

/// 本章写作合同（引擎 `build_contract` 字节等价 JSON）。
pub fn build_contract(root: &str, ch: i64) -> Result<String, CommandError> {
    let root_path = Path::new(root);
    let s = load_state(root_path).map_err(CommandError::from)?;
    Ok(book_build_contract(root_path, &s, ch).to_json())
}

/// 五段任务书（引擎 `build_brief` 字节等价 Markdown）。
pub fn build_brief(root: &str, ch: i64) -> Result<String, CommandError> {
    let root_path = Path::new(root);
    let s = load_state(root_path).map_err(CommandError::from)?;
    book_build_brief(root_path, &s, ch, None, None).map_err(CommandError::from)
}

/// 11 项机检（引擎 `collect_checks`）。stage 默认 `prewrite`。
pub fn run_check(root: &str, stage: Option<String>) -> Result<Vec<CheckResultDto>, CommandError> {
    let stage = stage.unwrap_or_else(|| "prewrite".to_owned());
    let checks = collect_checks(Path::new(root), &stage, None).map_err(CommandError::from)?;
    Ok(checks
        .into_iter()
        .map(|c| CheckResultDto {
            ok: c.ok,
            name: c.name,
            detail: c.detail,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// 候选稿命令（P4 最小版；P7 #24 以 .lingbi/candidates 状态机接管）
// ---------------------------------------------------------------------------

/// 建候选稿：内容落 `工作区/草稿/{ch:03}-{uuid}.md`（mutable，非权威，
/// 不经事务门——候选稿可丢弃，spec 工作区契约）。返回候选稿 id。
pub fn create_candidate(root: &str, ch: i64, content: &str) -> Result<CandidateDto, CommandError> {
    // 候选是对已有章的（重）写：章必须存在
    let _ = chapter_file(Path::new(root), ch)?;
    let id = format!("{ch:03}-{}", Uuid::new_v4());
    let dir = Path::new(root).join(DRAFT_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| {
        CommandError::new(
            "DISK_WRITE_FAILED",
            format!("创建草稿目录失败: {e}"),
            false,
        )
    })?;
    std::fs::write(dir.join(format!("{id}.md")), content.as_bytes()).map_err(|e| {
        CommandError::new(
            "DISK_WRITE_FAILED",
            format!("写候选稿失败: {e}"),
            false,
        )
    })?;
    Ok(CandidateDto {
        id,
        ch,
        content_hash: lingbi_storage::sha256_hex(content.as_bytes()),
    })
}

/// 采纳候选稿：正文落盘（CAS 期望 = 当前正文哈希）→ 结算（cmd_commit：
/// 合并追踪事实 + bump revision + 派生视图重建 + Final Revision 快照）→
/// 删草稿。采纳即用户显式确认，precommit gate 警告随 stdout 透传不阻断。
/// 结算失败（exit != 0）时正文已按事务落盘、草稿保留，供定点修复后重试。
/// 定稿棘轮：目标章已结算则拒绝（二次采纳会绕回炉直写已提交正文）。
pub fn adopt_candidate(root: &str, candidate_id: &str) -> Result<AdoptResultDto, CommandError> {
    let root_path = Path::new(root);
    let ch = draft_chapter(candidate_id)?;
    ratchet_guard(root_path, ch)?;
    let draft = root_path.join(DRAFT_DIR).join(format!("{candidate_id}.md"));
    let content = std::fs::read_to_string(&draft).map_err(|_| {
        CommandError::new(
            "DOCUMENT_NOT_FOUND",
            format!("候选稿不存在: {candidate_id}"),
            false,
        )
    })?;
    // 1. 正文落盘：期望 = 当前正文哈希（外部修改即冲突，用户字节保留）
    let path = chapter_file(root_path, ch)?;
    let fname = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| CommandError::new("DOCUMENT_NOT_FOUND", "缺章文件", false))?;
    let expected = std::fs::read(&path)
        .ok()
        .map(|b| lingbi_storage::sha256_hex(&b));
    let bytes = full_chapter_bytes(&path, &content)?;
    write_body(root, &fname, &bytes, expected, None)?;
    // 2. 结算（合并追踪 + bump + 派生视图 + Final Revision 快照）
    let out = lingbi_book::cmd::cmd_commit(root_path, ch, true).map_err(CommandError::from)?;
    if out.exit == 0 {
        std::fs::remove_file(&draft).map_err(|e| {
            CommandError::new(
                "DISK_WRITE_FAILED",
                format!("删候选稿失败: {e}"),
                false,
            )
        })?;
    }
    Ok(AdoptResultDto {
        ch,
        exit: out.exit,
        stdout: out.stdout,
    })
}

/// 拒绝候选稿：删 `工作区/草稿/` 下的候选文件（mutable，可丢弃）。
pub fn reject_candidate(root: &str, candidate_id: &str) -> Result<(), CommandError> {
    let _ = draft_chapter(candidate_id)?;
    let draft = Path::new(root)
        .join(DRAFT_DIR)
        .join(format!("{candidate_id}.md"));
    match std::fs::remove_file(&draft) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(CommandError::new(
            "DOCUMENT_NOT_FOUND",
            format!("候选稿不存在: {candidate_id}"),
            false,
        )),
        Err(e) => Err(CommandError::new(
            "DISK_WRITE_FAILED",
            format!("删候选稿失败: {e}"),
            false,
        )),
    }
}

// ---------------------------------------------------------------------------
// 工作流命令（P7 #24：8 节点章事务链；Run 持久化/幂等/账本随 P8 #25）
// ---------------------------------------------------------------------------

/// 运行一次本章 8 节点链（Contract→Brief→Draft→Machine Check→Review→
/// Polish→Settlement→Handoff，lingbi-workflow）。AI 节点经注入的
/// `AiProvider` 执行；provider/model 快照每次运行解析并记入节点
/// （能力 8——切换模型即失效重跑）。checkpoint 落 `工作区/工作流/`，
/// 重跑复用输入未变的已完成节点；返回 8 节点 DTO + 机检清单 + 交付报告。
pub async fn run_chain(
    root: &str,
    ch: i64,
    provider: std::sync::Arc<dyn lingbi_ai::AiProvider>,
) -> Result<ChainRunDto, CommandError> {
    let report = lingbi_workflow::run_chain(Path::new(root), ch, provider)
        .await
        .map_err(CommandError::from)?;
    Ok(ChainRunDto {
        ch: report.ch,
        status: format!("{:?}", report.status),
        run_id: report.run_id,
        repair_attempts: report.repair_attempts,
        reject_reason: report.reject_reason.map(|r| format!("{:?}", r)),
        book_input_hash: report.book_input_hash,
        nodes: report
            .nodes
            .iter()
            .map(|n| ChainNodeDto {
                node_id: n.node_id.clone(),
                definition_id: n.definition_id.clone(),
                definition_version: n.definition_version,
                input_hash: n.input_hash.clone(),
                output_hash: n.output_hash.clone(),
                status: format!("{:?}", n.status),
                provider: n.provider.clone(),
                model: n.model.clone(),
                usage: n.usage.as_ref().map(|u| UsageDto {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                }),
                error: n.error.clone(),
                note: n.note.clone(),
            })
            .collect(),
        checks: report
            .checks
            .into_iter()
            .map(|c| CheckResultDto {
                ok: c.ok,
                name: c.name,
                detail: c.detail,
            })
            .collect(),
        draft_path: report.draft_path,
        review_path: report.review_path,
        handoff: report.handoff.map(|h| HandoffDto {
            ch: h.ch,
            chapter_path: h.chapter_path,
            title: h.title,
            words: h.words,
            nodes: h
                .nodes
                .into_iter()
                .map(|n| HandoffNodeDto {
                    definition_id: n.definition_id,
                    status: n.status,
                    input_hash: n.input_hash,
                    output_hash: n.output_hash,
                })
                .collect(),
        }),
    })
}

/// 断点诊断（引擎 `diagnose` 字节等价 JSON）：status / next_step /
/// 各文件新鲜度——崩溃或中断后续跑前先看它。
pub fn resume_workflow(root: &str) -> Result<String, CommandError> {
    Ok(diagnose(Path::new(root), None).to_json())
}

/// 取消工作流。P4 工作流为同步确定性链：调用方拿到结果时运行已结束，
/// 无后台任务可取消——返回空操作并如实说明；真取消
/// （Run→Node→Provider 传播）随 P8（#25）。
pub fn cancel_workflow(run_id: &str) -> Result<CancelResultDto, CommandError> {
    Ok(CancelResultDto {
        run_id: run_id.to_owned(),
        cancelled: false,
        message: "P4 同步工作流无进行中后台运行;真取消随 P8(#25)".to_owned(),
    })
}

/// 导出章节（P5-4）：`export/{ch:03}-{title}.{md,txt,docx}`（export/ 为
/// 派生目录，命令层接口；docx 为真 DOCX——ImportExportService 生成）。
/// 内容 = 引擎 body 语义（正文 + 章标题，不含 front matter 段）。
///
/// 导出文件名派生自磁盘章文件名（而非 frontmatter title）：create_chapter
/// 已校验标题字符，且真实文件名在 Windows 上不可能含路径分隔符/精确
/// `..` 组件 → 拼接天然防穿越；frontmatter 是用户可手改字段，不用于路径。
pub fn export_chapter(root: &str, ch: i64, format: &str) -> Result<ExportResultDto, CommandError> {
    let chapter = open_chapter(root, ch)?;
    let format = format.to_ascii_lowercase();
    if !matches!(format.as_str(), "md" | "txt" | "docx") {
        return Err(CommandError::new(
            "UNSUPPORTED_EXPORT_FORMAT",
            format!("unsupported export format: {format}"),
            false,
        ));
    }
    // 磁盘章文件名（chapter_file 已按 {ch:03}- 前缀定位）→ stem 即安全基名。
    let disk_fname = chapter_file(Path::new(root), ch)?
        .file_name()
        .ok_or_else(|| CommandError::new("DOCUMENT_NOT_FOUND", "章文件名缺失", false))?
        .to_string_lossy()
        .into_owned();
    let base = Path::new(root)
        .join("export")
        .join(disk_fname.trim_end_matches(".md"));
    // ImportExportService 的导出方法为纯内容/路径（new 惰性无副作用）
    let service = lingbi_import_export::ImportExportService::new(
        std::sync::Arc::new(lingbi_application::DocumentApplicationService::new(root)),
    );
    let path = match format.as_str() {
        "md" => service
            .export_markdown(&chapter.body, &base.with_extension("md"))
            .map_err(CommandError::from)?,
        "txt" => service
            .export_txt(&chapter.body, &base.with_extension("txt"))
            .map_err(CommandError::from)?,
        "docx" => service
            .export_docx(&chapter.title, &chapter.body, &base.with_extension("docx"))
            .map_err(CommandError::from)?,
        _ => unreachable!("validated above"),
    };
    Ok(ExportResultDto {
        format,
        path: path.to_string_lossy().into_owned(),
    })
}

/// 迁移旧书仓/旧项目到 Book V3 托管书仓（P6）：五件套管线（inspect →
/// plan → execute → validate），source 只读、正文 byte-for-byte，返回
/// 六字段回执（同时落 `.lingbi/migration-receipt.json`）。支持
/// storyrepo-legacy 与 flutter-v1 两类源。
pub fn migrate_book(source: &str, destination: &str) -> Result<MigrationReceiptDto, CommandError> {
    let receipt = lingbi_migration::migrate_book(Path::new(source), Path::new(destination))
        .map_err(CommandError::from)?;
    Ok(MigrationReceiptDto {
        source_hash: receipt.source_hash,
        classification: receipt.classification,
        destination: receipt.destination,
        destination_hash: receipt.destination_hash,
        migration_rule: receipt
            .migration_rule
            .iter()
            .map(|rule| FileRuleDto {
                source: rule.source.clone(),
                destination: rule.destination.clone(),
                rule: rule.rule.clone(),
            })
            .collect(),
        warnings: receipt.warnings,
    })
}

/// 预置声明式技巧卡（P5-5 声明式子集，审计 #27 题材引导 REDESIGN /
/// #28 Skill 执行 DISABLED）：题材引导精简版等只读卡片。纯静态数据表，
/// 无任何执行面——调用方仅渲染展示。
pub fn list_skills() -> Vec<SkillCardDto> {
    vec![
        SkillCardDto {
            id: "theme-guide-lite".to_owned(),
            title: "题材引导（精简版）".to_owned(),
            description: "从一句话设定开始，逐步收拢题材方向与卖点。".to_owned(),
            steps: vec![
                "一句话设定：谁、在什么世界、要做什么".to_owned(),
                "核心卖点：读者为什么点进来".to_owned(),
                "冲突引擎：每章至少一个悬念或转折".to_owned(),
            ],
        },
        SkillCardDto {
            id: "chapter-skeleton".to_owned(),
            title: "章节骨架".to_owned(),
            description: "一章一目标：开场、推进、钩子三段式。".to_owned(),
            steps: vec![
                "开场钩子：前 200 字抓住读者".to_owned(),
                "目标推进：本章信息/成长/冲突至少前进一步".to_owned(),
                "结尾悬念：留下一个想读下去的钩子".to_owned(),
            ],
        },
    ]
}

/// 运行台账：`.lingbi/` 事务层的 append-only 历史（写前 checkpoints +
/// 提交 receipts）。legacy 书仓（无事务层）返回空台账。
pub fn get_run_ledger(root: &str) -> Result<RunLedgerDto, CommandError> {
    Ok(RunLedgerDto {
        checkpoints: read_json_dir(&Path::new(root).join(".lingbi/checkpoints")),
        receipts: read_json_dir(&Path::new(root).join(".lingbi/receipts")),
    })
}

// ---------------------------------------------------------------------------
// 内部 helper
// ---------------------------------------------------------------------------

/// 章正文文件（`定稿/正文/{ch:03}-*.md`）；不存在 → DocumentNotFound。
fn chapter_file(root: &Path, ch: i64) -> Result<PathBuf, CommandError> {
    chapter_files(root)
        .into_iter()
        .find(|f| chapter_num(f) == ch)
        .map(|f| root.join(BODY_DIR).join(f))
        .ok_or_else(|| CommandError::new("DOCUMENT_NOT_FOUND", format!("章 {ch} 无正文"), false))
}

/// 候选稿 id 前缀解析 + 白名单校验：`{ch:03}-{uuid}`（uuid 为 8-4-4-4-12
/// 十六进制段）。拒绝任何其它字符（`/` `\` `.` `..` 等）——id 只由
/// `create_candidate` 的 `Uuid::new_v4()` 生成，白名单防路径穿越
/// （reject/adopt 会把 id 拼进文件路径）。格式非法 fail-closed。
fn draft_chapter(candidate_id: &str) -> Result<i64, CommandError> {
    let ch = chapter_num(candidate_id);
    let mut parts = candidate_id.split('-');
    let prefix = parts.next().unwrap_or("");
    // 章前缀：3-4 位纯数字（{ch:03} 对 1000+ 章输出 4 位）
    if !(3..=4).contains(&prefix.len())
        || !prefix.chars().all(|c| c.is_ascii_digit())
        || ch < 1
    {
        return Err(CommandError::new(
            "INVALID_CANDIDATE_ID",
            format!("候选稿 id 非法: {candidate_id}"),
            false,
        ));
    }
    let uuid_lens = [8usize, 4, 4, 4, 12];
    for (i, seg) in parts.enumerate() {
        let expected = uuid_lens.get(i).copied().unwrap_or(0);
        if seg.len() != expected || !seg.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CommandError::new(
                "INVALID_CANDIDATE_ID",
                format!("候选稿 id 非法: {candidate_id}"),
                false,
            ));
        }
    }
    Ok(ch)
}

/// 定稿棘轮守卫：章节已在追踪中（已结算）→ 拒绝直写正文。
/// spec 决策 #2「定稿/ 只增不改，回炉 revision 是唯一例外」——
/// 已提交章的编辑走 storyrepo revision 重放式级联（P7 章事务引擎
/// 暴露 UI 通道），命令层在 P4 不做隐式重写。
fn ratchet_guard(root: &Path, ch: i64) -> Result<(), CommandError> {
    let s = load_state(root).map_err(CommandError::from)?;
    if s.chapters.contains_key(&ch.to_string()) {
        return Err(CommandError::new(
            "CHAPTER_COMMITTED",
            format!(
                "章 {ch} 已提交(rev {})——定稿棘轮:已结算正文不可直接改写;旧章编辑走回炉重放级联(revision),P7 章事务引擎将暴露 UI 通道",
                s.state_revision
            ),
            false,
        ));
    }
    Ok(())
}

/// 重组章文件 = 原 frontmatter 段（原样保留）+ 正文。frontmatter 段以
/// `---\n` 结尾；正文若以换行开头（open_chapter 返回的引擎格式 body）则不
/// 再加分隔，否则补一个空行——两种输入都落成引擎文件格式
/// （frontmatter + 空行 + 正文）。
/// 两种宽容输入与引擎 `body()` 严格互逆（保存未修改 = 字节不变）：
/// - 无闭合 `---\n` 的文件（引擎 body() 视为整文件）→ 按纯正文整体替换，
///   不注入空行、不丢弃原字节；
/// - 非 `---` 开头的纯正文文件 → 原样整体替换。
fn full_chapter_bytes(path: &Path, content: &str) -> Result<Vec<u8>, CommandError> {
    lingbi_book::replace_body(path, content).map_err(CommandError::from)
}

/// authoritative 写入口（命令层唯一写通道）：
/// legacy 书仓（`.lingbi/` 缺席）首次写入即托管化——`ensure_tx_layer`
/// 幂等落地六目录（P5-4 #35 验收「legacy 打开→保存→.lingbi 自动就位」），
/// 此后统一经 `tx::write_authoritative`（checkpoint+WAL+CAS+回执）。
/// CAS 期望哈希不符即拒写（用户字节保留）。
fn write_body(
    root: &str,
    fname: &str,
    bytes: &[u8],
    expected_hash: Option<String>,
    after_revision: Option<i64>,
) -> Result<TxReceiptDto, CommandError> {
    let root_path = Path::new(root);
    let rel = format!("{BODY_DIR}/{fname}");
    tx::ensure_tx_layer(root_path).map_err(CommandError::from)?;
    let receipt = tx::write_authoritative(
        root_path,
        &rel,
        bytes,
        expected_hash.as_deref(),
        after_revision,
    )
    .map_err(CommandError::from)?;
    Ok(TxReceiptDto {
        tx_id: receipt.tx_id.to_string(),
        rel_path: receipt.rel_path,
        after_revision: receipt.after_revision,
        after_content_hash: receipt.after_content_hash,
    })
}

/// 目录内全部 *.json 的解析结果（事务层 append-only 历史；缺目录/损坏
/// 文件跳过——台账只读，损坏项由恢复层留档处理）。
fn read_json_dir(dir: &Path) -> Vec<Value> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                out.push(value);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// 空书仓（追踪状态 + 占位卡 + 最小 book.yaml；无 .lingbi —— legacy 起步）。
    fn init_book() -> (TempDir, String) {
        let dir = TempDir::new().expect("tempdir");
        lingbi_book::tracking::init_state(dir.path()).expect("init");
        // 引擎书仓必需 book.yaml（corpus 快照字节格式）；缺它 checks 的
        // 字数窗口项按引擎语义 fail-closed（BookNotFound）。
        std::fs::write(
            dir.path().join("book.yaml"),
            "---\nname: 测试书\nstatus: writing\ntotal_chapters: 0\nmin_words_per_chapter: 100\nmax_words_per_chapter: 300\ntarget_words: 200000\n---\n",
        )
        .expect("book.yaml");
        let root = dir.path().to_string_lossy().into_owned();
        (dir, root)
    }

    fn managed(root: &str) {
        tx::ensure_tx_layer(Path::new(root)).expect("ensure tx layer");
    }

    // ---- open_book / 会话 ----

    #[test]
    fn create_book_builds_managed_book_and_reopens() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("我的小说").to_string_lossy().into_owned();
        let dto = create_book(&root, "我的小说").expect("create");
        assert!(dto.managed);
        assert_eq!(dto.name, "我的小说");
        assert_eq!(dto.state_revision, 1);
        // 书仓结构齐全：book.yaml + 追踪 + 事务层六目录
        assert!(Path::new(&root).join("book.yaml").exists());
        assert!(Path::new(&root).join("追踪/_tracking-state.json").exists());
        for sub in lingbi_storage::book_v3::TX_LAYER_DIRS {
            assert!(Path::new(&root).join(".lingbi").join(sub).is_dir());
        }
        // 幂等：再次建书 = 打开既有书仓
        let dto2 = create_book(&root, "我的小说").expect("recreate");
        assert!(dto2.managed);
        assert_eq!(dto2.state_revision, 1);
        // 建章可用
        let ch = create_chapter(&root, "开局").expect("chapter");
        assert_eq!(ch.ch, 1);
        assert!(Path::new(&root).join("定稿/正文/001-开局.md").exists());
    }

    #[test]
    fn create_book_rejects_empty_and_illegal_names() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().to_string_lossy().into_owned();
        let err = create_book(&root, "  ").expect_err("empty");
        assert_eq!(err.code, "INVALID_BOOK_NAME");
        let err = create_book(&root, "a/b").expect_err("illegal");
        assert_eq!(err.code, "INVALID_BOOK_NAME");
        let err = create_book(&root, "a\nb").expect_err("control");
        assert_eq!(err.code, "INVALID_BOOK_NAME");
    }

    #[test]
    fn open_book_legacy_book_without_lingbi() {
        let (_dir, root) = init_book();
        let dto = open_book(&root).expect("open");
        assert!(!dto.managed);
        assert_eq!(dto.state_revision, 1);
        assert_eq!(dto.chapter_count, 0);
        assert_eq!(dto.recovered, 0);
        assert!(!Path::new(&root).join(".lingbi").exists());
    }

    #[test]
    fn open_book_managed_runs_recovery_and_ensures_tx_layer() {
        let (_dir, root) = init_book();
        managed(&root);
        let dto = open_book(&root).expect("open");
        assert!(dto.managed);
        assert_eq!(dto.recovered, 0); // 无残留日志,收敛为无操作
        for dir in lingbi_storage::book_v3::TX_LAYER_DIRS {
            assert!(Path::new(&root).join(".lingbi").join(dir).is_dir());
        }
    }

    #[test]
    fn open_book_missing_book_fails_closed() {
        let err = open_book("C:/不存在/书仓").expect_err("err");
        assert_eq!(err.code, "BookNotFound");
    }

    // ---- 章正文 ----

    #[test]
    fn create_then_open_chapter_roundtrip() {
        let (_dir, root) = init_book();
        let created = create_chapter(&root, "开局").expect("create");
        assert_eq!(created.ch, 1);
        assert!(created.body.is_empty());
        let opened = open_chapter(&root, 1).expect("open");
        assert_eq!(opened.title, "开局");
        // 引擎 body() 语义：frontmatter 段后的全部（含前导空行）
        assert!(opened.body.trim().is_empty());
        assert_eq!(opened.content_hash, created.content_hash);
        assert!(Path::new(&root).join("定稿/正文/001-开局.md").exists());
    }

    #[test]
    fn create_chapter_rejects_windows_illegal_title() {
        let (_dir, root) = init_book();
        let err = create_chapter(&root, "a/b").expect_err("err");
        assert_eq!(err.code, "INVALID_TITLE");
        // 控制字符同样拒绝（Windows 非法文件名）
        let err = create_chapter(&root, "a\nb").expect_err("err");
        assert_eq!(err.code, "INVALID_TITLE");
        let err = create_chapter(&root, "  ").expect_err("err");
        assert_eq!(err.code, "INVALID_TITLE");
    }

    #[test]
    fn open_unknown_chapter_fails_closed() {
        let (_dir, root) = init_book();
        let err = open_chapter(&root, 1).expect_err("err");
        assert_eq!(err.code, "DOCUMENT_NOT_FOUND");
    }

    #[test]
    fn save_chapter_without_expected_hash_allowed_for_fresh_write() {
        let (_dir, root) = init_book();
        create_chapter(&root, "夜访").expect("create");
        // expected_hash=None：不校验既有内容（创建后首次填充）
        let receipt = save_chapter(&root, 1, "无期望哈希保存", None).expect("save");
        assert_eq!(receipt.after_content_hash.len(), 64);
    }

    #[test]
    fn candidate_id_whitelist_blocks_path_traversal() {
        let (_dir, root) = init_book();
        // 章前缀必须 3-4 位纯数字；uuid 段必须 8-4-4-4-12 十六进制。
        // 任何路径成分（/ \ . ..）与格式偏差 → INVALID_CANDIDATE_ID，
        // 且不会触碰磁盘（reject/adopt 在解析阶段即失败）。
        for bad in [
            "001-../../../../追踪/上下文",
            "001-../../../定稿/正文/001-夜访",
            "001-../../../book.yaml",
            "1-00000000-0000-0000-0000-000000000000",
            "001-00000000-0000-0000-0000-00000000000",
            "001-zz000000-0000-0000-0000-000000000000",
            "001",
            "",
        ] {
            let err = reject_candidate(&root, bad).expect_err("reject");
            assert_eq!(err.code, "INVALID_CANDIDATE_ID", "id: {bad}");
        }
        // 书仓内容完好（无文件被删）
        assert!(Path::new(&root).join("追踪/上下文.md").exists());
    }

    #[test]
    fn ratchet_guard_rejects_writes_to_committed_chapter() {
        let (_dir, root) = init_book();
        create_chapter(&root, "夜访").expect("create");
        // 采纳并结算 ch1
        let candidate = create_candidate(&root, 1, "深夜,阿米推开了门。").expect("create");
        let adopted = adopt_candidate(&root, &candidate.id).expect("adopt");
        assert_eq!(adopted.exit, 0);
        // 已提交章：save_chapter 与 adopt_candidate 均拒绝（CHAPTER_COMMITTED）
        let err = save_chapter(&root, 1, "改写已提交章", None).expect_err("save");
        assert_eq!(err.code, "CHAPTER_COMMITTED");
        let candidate2 = create_candidate(&root, 1, "再次采纳").expect("create candidate");
        let err = adopt_candidate(&root, &candidate2.id).expect_err("adopt");
        assert_eq!(err.code, "CHAPTER_COMMITTED");
        // 已提交正文未被改动
        let reopened = open_chapter(&root, 1).expect("open");
        assert!(reopened.body.contains("阿米推开了门"));
        assert!(!reopened.body.contains("再次采纳"));
    }

    #[test]
    fn adopt_candidate_managed_book_settles_with_receipts() {
        let (_dir, root) = init_book();
        managed(&root);
        create_chapter(&root, "夜访").expect("create");
        let candidate = create_candidate(&root, 1, "managed 书仓候选正文").expect("create");
        let adopted = adopt_candidate(&root, &candidate.id).expect("adopt");
        assert_eq!(adopted.exit, 0, "stdout: {}", adopted.stdout);
        // managed：正文事务 + 追踪结算都留回执
        let ledger = get_run_ledger(&root).expect("ledger");
        assert!(!ledger.receipts.is_empty());
        let reopened = open_chapter(&root, 1).expect("open");
        assert!(reopened.body.contains("managed 书仓候选正文"));
    }

    #[test]
    fn list_chapters_enumerates_all_body_files_including_unsettled() {
        let (_dir, root) = init_book();
        create_chapter(&root, "夜访").expect("create");
        create_chapter(&root, "拜访").expect("create");
        let opened = open_chapter(&root, 1).expect("open");
        save_chapter(&root, 1, "正文", Some(opened.content_hash)).expect("save");
        let list = list_chapters(&root).expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].ch, 1);
        assert_eq!(list[0].title, "夜访");
        assert_eq!(list[1].ch, 2);
        assert_eq!(list[1].title, "拜访");
        // 未结算章也在列表（tracking 只有已结算章）
        let tracking = get_tracking_state(&root).expect("tracking");
        assert!(tracking.chapters.is_empty());
    }

    #[test]
    fn save_chapter_keeps_frontmatter_and_returns_receipt() {
        let (_dir, root) = init_book();
        create_chapter(&root, "夜访").expect("create");
        let opened = open_chapter(&root, 1).expect("open");
        let receipt =
            save_chapter(&root, 1, "第一段正文", Some(opened.content_hash)).expect("save");
        assert_eq!(receipt.rel_path, "定稿/正文/001-夜访.md");
        assert_eq!(receipt.after_content_hash.len(), 64);
        let reopened = open_chapter(&root, 1).expect("open");
        assert_eq!(reopened.body, "\n第一段正文");
        assert_eq!(reopened.frontmatter.get("title").map(String::as_str), Some("夜访"));
    }

    #[test]
    fn save_chapter_stale_expected_hash_conflict_preserves_user_bytes() {
        let (_dir, root) = init_book();
        create_chapter(&root, "夜访").expect("create");
        let opened = open_chapter(&root, 1).expect("open");
        // 外部修改正文
        std::fs::write(
            Path::new(&root).join("定稿/正文/001-夜访.md"),
            "外部写入的字节\n",
        )
        .expect("external write");
        let err = save_chapter(&root, 1, "作者的新正文", Some(opened.content_hash))
            .expect_err("conflict");
        assert_eq!(err.code, "DocumentConflict");
        // 用户字节保留（外部写入未被覆盖）
        let disk = std::fs::read_to_string(Path::new(&root).join("定稿/正文/001-夜访.md"))
            .expect("read");
        assert!(disk.contains("外部写入的字节"));
    }

    #[test]
    fn save_chapter_managed_writes_transaction_layer() {
        let (_dir, root) = init_book();
        managed(&root);
        create_chapter(&root, "夜访").expect("create");
        let opened = open_chapter(&root, 1).expect("open");
        save_chapter(&root, 1, "正文", Some(opened.content_hash)).expect("save");
        // 事务层留下回执（append-only 历史）：建章 1 + 存章 1
        let receipts = read_json_dir(&Path::new(&root).join(".lingbi/receipts"));
        assert_eq!(receipts.len(), 2);
        assert!(receipts.iter().any(|r| r["rel_path"] == "定稿/正文/001-夜访.md"));
    }

    // ---- 写作辅助 ----

    #[test]
    fn build_contract_and_brief_deterministic() {
        let (_dir, root) = init_book();
        create_chapter(&root, "夜访").expect("create");
        let contract = build_contract(&root, 1).expect("contract");
        assert!(contract.contains("\"goal\""));
        assert!(contract.contains("追踪状态"));
        let brief = build_brief(&root, 1).expect("brief");
        assert!(brief.contains("## "));
    }

    #[test]
    fn run_check_returns_11_checks() {
        let (_dir, root) = init_book();
        let checks = run_check(&root, None).expect("check");
        assert_eq!(checks.len(), 11);
        // 空书仓无内容可查：11 项全部通过（引擎语义）
        assert!(checks.iter().all(|c| c.ok), "空书仓应全过: {:?}", checks);
    }

    // ---- 候选稿 ----

    #[test]
    fn candidate_lifecycle_adopt_writes_body_and_settles() {
        let (_dir, root) = init_book();
        create_chapter(&root, "夜访").expect("create");
        let candidate = create_candidate(&root, 1, "深夜,阿米推开了门。\n\n她看见桌上有一封信。")
            .expect("create candidate");
        assert_eq!(candidate.ch, 1);
        // 采纳：正文落盘 + 结算
        let adopted = adopt_candidate(&root, &candidate.id).expect("adopt");
        assert_eq!(adopted.exit, 0, "stdout: {}", adopted.stdout);
        // 草稿已删
        let err = reject_candidate(&root, &candidate.id).expect_err("gone");
        assert_eq!(err.code, "DOCUMENT_NOT_FOUND");
        // 正文含候选内容、追踪已合并
        let reopened = open_chapter(&root, 1).expect("open");
        assert!(reopened.body.contains("阿米推开了门"));
        let tracking = load_state(Path::new(&root)).expect("tracking");
        assert_eq!(tracking.state_revision, 2); // init rev1 + commit bump
        assert!(tracking.chapters.contains_key("1"));
    }

    #[test]
    fn candidate_reject_removes_draft() {
        let (_dir, root) = init_book();
        create_chapter(&root, "夜访").expect("create");
        let candidate = create_candidate(&root, 1, "候选正文").expect("create");
        reject_candidate(&root, &candidate.id).expect("reject");
        // 正文未被改动（引擎 body 语义含前导空行）
        let reopened = open_chapter(&root, 1).expect("open");
        assert!(reopened.body.trim().is_empty());
    }

    #[test]
    fn adopt_unknown_candidate_fails_closed() {
        let (_dir, root) = init_book();
        let err = adopt_candidate(&root, "001-00000000-0000-0000-0000-000000000000")
            .expect_err("err");
        assert_eq!(err.code, "DOCUMENT_NOT_FOUND");
    }

    // ---- 工作流 ----

    #[tokio::test]
    async fn run_chain_returns_eight_nodes_and_settles() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let root = dir.path();
        lingbi_workflow::testing::init_book(root, true, "第一章", Some("主角踏上修行之路。"));
        let provider = std::sync::Arc::new(lingbi_workflow::testing::ScriptedProvider::new(
            lingbi_workflow::testing::standard_script(
                lingbi_workflow::testing::REVIEW_OK,
                lingbi_workflow::testing::POLISH,
                lingbi_workflow::testing::DRAFT,
            ),
        ));
        let run = run_chain(&root.to_string_lossy(), 1, provider)
            .await
            .expect("run_chain");
        assert_eq!(run.status, "Completed");
        assert_eq!(run.nodes.len(), 8);
        assert!(run.nodes.iter().all(|n| !n.input_hash.is_empty()));
        let draft = run.nodes.iter().find(|n| n.definition_id == "draft").unwrap();
        assert_eq!(draft.provider.as_deref(), Some("scripted"));
        assert!(run.handoff.is_some());
        assert!(root.join("定稿/正文/001-第一章.md").exists());
    }

    #[test]
    fn resume_workflow_returns_engine_json() {
        let (_dir, root) = init_book();
        let json = resume_workflow(&root).expect("resume");
        assert!(json.contains("\"status\""));
        assert!(json.contains("\"next_step\""));
        assert!(json.contains("\"files\""));
    }

    #[test]
    fn cancel_workflow_idempotent_sync_semantics() {
        let out = cancel_workflow("run-1").expect("cancel");
        assert!(!out.cancelled);
        assert!(out.message.contains("P8"));
    }

    #[test]
    fn export_chapter_writes_md_txt_docx() {
        let (_dir, root) = init_book();
        create_chapter(&root, "夜访").expect("create");
        let opened = open_chapter(&root, 1).expect("open");
        save_chapter(&root, 1, "正文内容", Some(opened.content_hash)).expect("save");
        for format in ["md", "txt", "docx"] {
            let result = export_chapter(&root, 1, format).expect(format);
            assert_eq!(result.format, format);
            let path = Path::new(&result.path);
            assert!(path.exists(), "export file missing: {}", path.display());
            let bytes = std::fs::read(path).expect("read export");
            assert!(!bytes.is_empty());
            if format != "docx" {
                let text = String::from_utf8_lossy(&bytes);
                assert!(text.contains("正文内容"), "{format} content");
            }
        }
        // 非法格式 fail-closed
        let err = export_chapter(&root, 1, "pdf").expect_err("err");
        assert_eq!(err.code, "UNSUPPORTED_EXPORT_FORMAT");
    }

    #[test]
    fn export_chapter_ignores_hostile_frontmatter_title() {
        // 路径穿越守卫：frontmatter title 是用户可手改字段——手改出
        // 分隔符/`..` 序列不得影响导出路径（文件名派生自磁盘章文件名）。
        let (_dir, root) = init_book();
        create_chapter(&root, "夜访").expect("create");
        let opened = open_chapter(&root, 1).expect("open");
        save_chapter(&root, 1, "正文内容", Some(opened.content_hash)).expect("save");
        let file = Path::new(&root).join("定稿/正文/001-夜访.md");
        let poisoned = std::fs::read_to_string(&file)
            .expect("read")
            .replace("title: 夜访", "title: ..\\..\\..\\evil");
        std::fs::write(&file, poisoned).expect("poison frontmatter");
        let result = export_chapter(&root, 1, "md").expect("export");
        let path = Path::new(&result.path);
        let export_dir = Path::new(&root).join("export");
        assert!(
            path.starts_with(&export_dir),
            "export escapes export/: {}",
            path.display()
        );
        assert!(path.exists(), "export file missing: {}", path.display());
    }

    #[test]
    fn legacy_book_first_save_adopts_tx_layer() {
        // P5-4：legacy 书仓（无 .lingbi）首次写入即托管化
        let (_dir, root) = init_book();
        assert!(!Path::new(&root).join(".lingbi").exists());
        create_chapter(&root, "夜访").expect("create");
        // 首次写（create_chapter → write_body）后事务层就位
        assert!(Path::new(&root).join(".lingbi/transactions").is_dir());
        let ledger = get_run_ledger(&root).expect("ledger");
        assert!(!ledger.receipts.is_empty());
        // 回执指向正文（事务化写入路径）
        assert!(ledger
            .receipts
            .iter()
            .any(|r| r["rel_path"] == "定稿/正文/001-夜访.md"));
    }

    #[test]
    fn list_skills_returns_declarative_readonly_cards() {
        // P5-5：声明式子集——卡片为纯静态数据，字段非空、id 唯一
        let cards = list_skills();
        assert!(!cards.is_empty(), "at least one prebuilt card");
        let mut ids = std::collections::HashSet::new();
        for card in &cards {
            assert!(!card.id.is_empty(), "card id");
            assert!(!card.title.is_empty(), "card title: {}", card.id);
            assert!(!card.description.is_empty(), "card description: {}", card.id);
            assert!(!card.steps.is_empty(), "card steps: {}", card.id);
            assert!(ids.insert(&card.id), "duplicate card id: {}", card.id);
        }
    }

    #[test]
    fn run_ledger_lists_receipts_and_checkpoints() {
        let (_dir, root) = init_book();
        managed(&root);
        create_chapter(&root, "夜访").expect("create");
        let opened = open_chapter(&root, 1).expect("open");
        save_chapter(&root, 1, "正文", Some(opened.content_hash)).expect("save");
        let ledger = get_run_ledger(&root).expect("ledger");
        assert_eq!(ledger.receipts.len(), 2); // 建章 + 存章
        assert_eq!(ledger.checkpoints.len(), 2); // 每事务写前 checkpoint 留档
    }
}
