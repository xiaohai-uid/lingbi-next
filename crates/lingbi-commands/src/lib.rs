//! P4 核心服务层：Book V3 唯一边界（#21）。
//!
//! 16 个命令构成 UI（Tauri 壳 / 未来任何宿主）触达书仓的唯一通道：
//!
//! ```text
//! open_book / get_tracking_state / get_outline        —— 会话与状态
//! create_chapter / open_chapter / save_chapter        —— 章正文
//! build_contract / build_brief / run_check            —— 写作辅助
//! create_candidate / adopt_candidate / reject_candidate —— 候选稿
//! run_workflow / resume_workflow / cancel_workflow / get_run_ledger —— 工作流
//! ```
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
//! - run/resume/cancel_workflow 在 P4 为同步确定性链（无后台运行）；Run
//!   持久化/幂等/账本收口属 P8（#25），本模块诚实返回同步语义，cancel 为
//!   空操作并说明原因。
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

#[derive(Debug, Clone, Serialize)]
pub struct RunResultDto {
    pub run_id: String,
    pub ch: i64,
    pub contract_json: String,
    pub brief: String,
    pub checks: Vec<CheckResultDto>,
    pub checks_passed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CancelResultDto {
    pub run_id: String,
    pub cancelled: bool,
    pub message: String,
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
    if title.trim().is_empty() {
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
// 工作流命令（P4：同步确定性链；Run 持久化/幂等/账本随 P8 #25）
// ---------------------------------------------------------------------------

/// 运行一次本章确定性工作流：Contract → Brief → Machine Check（P4 无
/// AI 节点，AI 生成节点随 P7 章事务引擎；P4 的 AI 内容经 create_candidate
/// 通道进入）。同步返回全部节点输出；`run_id` 仅本次会话有效（无持久化）。
pub fn run_workflow(root: &str, ch: i64) -> Result<RunResultDto, CommandError> {
    let root_path = Path::new(root);
    let s = load_state(root_path).map_err(CommandError::from)?;
    let contract_json = book_build_contract(root_path, &s, ch).to_json();
    let brief = book_build_brief(root_path, &s, ch, None, None).map_err(CommandError::from)?;
    let checks = collect_checks(root_path, "prewrite", Some(ch)).map_err(CommandError::from)?;
    let checks_passed = checks.iter().all(|c| c.ok);
    Ok(RunResultDto {
        run_id: Uuid::new_v4().to_string(),
        ch,
        contract_json,
        brief,
        checks: checks
            .into_iter()
            .map(|c| CheckResultDto {
                ok: c.ok,
                name: c.name,
                detail: c.detail,
            })
            .collect(),
        checks_passed,
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
    let existing = std::fs::read(path)
        .map_err(|_| CommandError::new("DOCUMENT_NOT_FOUND", "读章节失败", false))?;
    let text = String::from_utf8_lossy(&existing);
    let mut fm_seg = "";
    let mut sep = "";
    if text.starts_with("---") {
        let after_first = text.strip_prefix("---").unwrap_or(&text);
        if let Some(p) = after_first.find("---\n") {
            fm_seg = &text[..p + 3 + 4];
            sep = if content.starts_with('\n') { "" } else { "\n" };
        }
        // 无闭合：整体按纯正文处理（与 body() 宽容一致）
    }
    Ok(format!("{fm_seg}{sep}{content}").into_bytes())
}

/// authoritative 写入口（命令层唯一写通道）：
/// managed → `tx::write_authoritative`（checkpoint+WAL+CAS+回执）；
/// legacy → 引擎路径，CAS 期望哈希不符即拒写（用户字节保留）。
fn write_body(
    root: &str,
    fname: &str,
    bytes: &[u8],
    expected_hash: Option<String>,
    after_revision: Option<i64>,
) -> Result<TxReceiptDto, CommandError> {
    let root_path = Path::new(root);
    let rel = format!("{BODY_DIR}/{fname}");
    if tx::is_managed(root_path) {
        let receipt = tx::write_authoritative(
            root_path,
            &rel,
            bytes,
            expected_hash.as_deref(),
            after_revision,
        )
        .map_err(CommandError::from)?;
        return Ok(TxReceiptDto {
            tx_id: receipt.tx_id.to_string(),
            rel_path: receipt.rel_path,
            after_revision: receipt.after_revision,
            after_content_hash: receipt.after_content_hash,
        });
    }
    // legacy：无事务层，保留引擎字节路径纪律（tmp + rename 原子写，
    // 与 tracking::save_state 的 legacy 分支一致）；CAS 守卫照常保护用户
    // 字节（校验失败即拒写，不产生任何磁盘变更）。
    let path = root_path.join(&rel);
    if let Some(exp) = &expected_hash {
        let cur = std::fs::read(&path)
            .ok()
            .map(|b| lingbi_storage::sha256_hex(&b));
        if cur.as_deref() != Some(exp.as_str()) {
            return Err(CommandError::new(
                "DocumentConflict",
                "外部修改了正文,存盘被拒(用户字节保留);请重新 open_chapter 后再存",
                false,
            ));
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            CommandError::new(
                "DISK_WRITE_FAILED",
                format!("创建正文目录失败: {e}"),
                false,
            )
        })?;
    }
    let tmp = path.with_extension("md.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| {
        CommandError::new("DISK_WRITE_FAILED", format!("写正文失败: {e}"), false)
    })?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        CommandError::new("DISK_WRITE_FAILED", format!("提交正文失败: {e}"), false)
    })?;
    Ok(TxReceiptDto {
        tx_id: Uuid::new_v4().to_string(),
        rel_path: rel,
        after_revision,
        after_content_hash: lingbi_storage::sha256_hex(bytes),
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

    #[test]
    fn run_workflow_returns_sync_chain() {
        let (_dir, root) = init_book();
        create_chapter(&root, "夜访").expect("create");
        let run = run_workflow(&root, 1).expect("run");
        assert!(run.contract_json.contains("\"goal\""));
        assert!(run.brief.contains("## "));
        assert_eq!(run.checks.len(), 11);
        assert!(!run.run_id.is_empty());
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
