//! P4 桌面壳：Book V3 命令的 Tauri 接线（P4 16 命令 + P5-1 增量 2 =
//! create_book / list_chapters，共 18）。
//!
//! 全部命令逻辑在 `lingbi-commands`（workspace 内，可被 e2e/desktop 直接
//! 测试）；本模块只做三件事：会话 root 持有（`open_book` 记录，无参命令
//! 读取）、命令调用、错误映射（`CommandError` → `CommandErrorDto`）。
//! 命令语义/与引擎差异的 fail-closed 声明见 `lingbi-commands` crate 头。

use tauri::State;

use crate::{command_error, CommandErrorDto, DesktopState};

/// 会话根读取：全部无参命令从这里取书仓；未 open_book 即 fail-closed。
fn book_root(state: &State<'_, DesktopState>) -> Result<String, CommandErrorDto> {
    state
        .book_root
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "book session lock", false))?
        .clone()
        .ok_or_else(|| {
            command_error(
                "NO_BOOK_SESSION",
                "no book session; call open_book first",
                false,
            )
        })
}

/// 新建书（Book V3 建仓入口）：book.yaml + 追踪初始化 + 事务层，
/// 幂等（对已存在书仓等价 open_book）。成功后记录会话 root 与最近作品。
#[tauri::command]
pub fn create_book(
    state: State<'_, DesktopState>,
    root: String,
    name: String,
) -> Result<lingbi_commands::BookSessionDto, CommandErrorDto> {
    let dto = lingbi_commands::create_book(&root, &name).map_err(CommandErrorDto::from)?;
    crate::record_recent(&state, &dto.name, &root, "v3");
    *state
        .book_root
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "book session lock", false))? = Some(root);
    Ok(dto)
}

/// 打开书仓（Book V3 唯一入口）。managed 书仓先跑崩溃收敛恢复再幂等
/// 落地事务层；legacy 书仓原样可开。成功后记录会话 root 与最近作品。
#[tauri::command]
pub fn open_book(
    state: State<'_, DesktopState>,
    root: String,
) -> Result<lingbi_commands::BookSessionDto, CommandErrorDto> {
    let dto = lingbi_commands::open_book(&root).map_err(CommandErrorDto::from)?;
    crate::record_recent(&state, &dto.name, &root, "v3");
    *state
        .book_root
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "book session lock", false))? = Some(root);
    Ok(dto)
}

/// 唯一权威状态（`追踪/_tracking-state.json`）的只读视图。
#[tauri::command]
pub fn get_tracking_state(
    state: State<'_, DesktopState>,
) -> Result<lingbi_book::TrackingState, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::get_tracking_state(&root).map_err(CommandErrorDto::from)
}

/// 细纲蓝图（大纲/细纲蓝图 表，引擎 `load_outline`）。
#[tauri::command]
pub fn get_outline(
    state: State<'_, DesktopState>,
) -> Result<Vec<lingbi_commands::OutlineDto>, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::get_outline(&root).map_err(CommandErrorDto::from)
}

/// 建章：`定稿/正文/{ch:03}-{title}.md`（frontmatter 模板 + 空正文），
/// 经事务门落盘（managed）。返回新章；`title` 须为 Windows 合法文件名。
#[tauri::command]
pub fn create_chapter(
    state: State<'_, DesktopState>,
    title: String,
) -> Result<lingbi_commands::ChapterDto, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::create_chapter(&root, &title).map_err(CommandErrorDto::from)
}

/// 章节列表（书仓内全部正文文件，含未结算章；UI 章列表以此为准）。
#[tauri::command]
pub fn list_chapters(
    state: State<'_, DesktopState>,
) -> Result<Vec<lingbi_commands::ChapterMetaDto>, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::list_chapters(&root).map_err(CommandErrorDto::from)
}

/// 读章：frontmatter + 正文（不含 front matter）+ 当前文件哈希
/// （哈希即 save/adopt 的 CAS 期望值来源）。
#[tauri::command]
pub fn open_chapter(
    state: State<'_, DesktopState>,
    ch: i64,
) -> Result<lingbi_commands::ChapterDto, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::open_chapter(&root, ch).map_err(CommandErrorDto::from)
}

/// 存章：正文替换 body 段，frontmatter 原样保留；`expected_hash`
/// （open_chapter 返回）做 CAS——外部修改后存盘被拒、用户字节保留。
#[tauri::command]
pub fn save_chapter(
    state: State<'_, DesktopState>,
    ch: i64,
    content: String,
    expected_hash: Option<String>,
) -> Result<lingbi_commands::TxReceiptDto, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::save_chapter(&root, ch, &content, expected_hash)
        .map_err(CommandErrorDto::from)
}

/// 本章写作合同（引擎 `build_contract` 字节等价 JSON）。
#[tauri::command]
pub fn build_contract(
    state: State<'_, DesktopState>,
    ch: i64,
) -> Result<String, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::build_contract(&root, ch).map_err(CommandErrorDto::from)
}

/// 五段任务书（引擎 `build_brief` 字节等价 Markdown）。
#[tauri::command]
pub fn build_brief(state: State<'_, DesktopState>, ch: i64) -> Result<String, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::build_brief(&root, ch).map_err(CommandErrorDto::from)
}

/// 11 项机检（引擎 `collect_checks`）。stage 默认 `prewrite`。
#[tauri::command]
pub fn run_check(
    state: State<'_, DesktopState>,
    stage: Option<String>,
) -> Result<Vec<lingbi_commands::CheckResultDto>, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::run_check(&root, stage).map_err(CommandErrorDto::from)
}

/// 建候选稿：内容落 `工作区/草稿/{ch:03}-{uuid}.md`（mutable，可丢弃）。
/// P4 最小版；P7（#24）以 .lingbi/candidates 状态机接管。
#[tauri::command]
pub fn create_candidate(
    state: State<'_, DesktopState>,
    ch: i64,
    content: String,
) -> Result<lingbi_commands::CandidateDto, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::create_candidate(&root, ch, &content).map_err(CommandErrorDto::from)
}

/// 采纳候选稿：正文落盘（CAS）→ 结算（合并追踪 + bump + 派生视图 +
/// Final Revision 快照）→ 删草稿。precommit gate 警告随 stdout 透传。
#[tauri::command]
pub fn adopt_candidate(
    state: State<'_, DesktopState>,
    candidate_id: String,
) -> Result<lingbi_commands::AdoptResultDto, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::adopt_candidate(&root, &candidate_id).map_err(CommandErrorDto::from)
}

/// 拒绝候选稿：删 `工作区/草稿/` 下的候选文件（mutable，可丢弃）。
#[tauri::command]
pub fn reject_candidate(
    state: State<'_, DesktopState>,
    candidate_id: String,
) -> Result<(), CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::reject_candidate(&root, &candidate_id).map_err(CommandErrorDto::from)
}

/// 运行一次本章确定性工作流：Contract → Brief → Machine Check
/// （P4 同步链，无 AI 节点；AI 内容经 create_candidate 通道进入）。
#[tauri::command]
pub fn run_workflow(
    state: State<'_, DesktopState>,
    ch: i64,
) -> Result<lingbi_commands::RunResultDto, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::run_workflow(&root, ch).map_err(CommandErrorDto::from)
}

/// 断点诊断（引擎 `diagnose` 字节等价 JSON）：status / next_step / 新鲜度。
#[tauri::command]
pub fn resume_workflow(state: State<'_, DesktopState>) -> Result<String, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::resume_workflow(&root).map_err(CommandErrorDto::from)
}

/// 取消工作流。P4 同步链无后台任务可取消——空操作并如实说明
/// （真取消随 P8 #25）。与书仓会话无关（全局命令），不要求 open_book。
#[tauri::command]
pub fn cancel_workflow(
    run_id: String,
) -> Result<lingbi_commands::CancelResultDto, CommandErrorDto> {
    lingbi_commands::cancel_workflow(&run_id).map_err(CommandErrorDto::from)
}

/// 运行台账：`.lingbi/` 事务层 append-only 历史（checkpoints + receipts）。
#[tauri::command]
pub fn get_run_ledger(
    state: State<'_, DesktopState>,
) -> Result<lingbi_commands::RunLedgerDto, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::get_run_ledger(&root).map_err(CommandErrorDto::from)
}

/// 桌面壳测试：错误映射（命令逻辑与 fail-closed 语义测试在
/// lingbi-commands；壳的 book_root 会话守卫未单测——tauri::State 在
/// 2.x 无 from_owned 构造 API，且 E2E 未穿越 Tauri 边界；P5 接 UI 时
/// 以 tauri-driver 或 mock app 补边界测试）。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_error_maps_to_dto() {
        let err = lingbi_commands::CommandError::new("INVALID_TITLE", "x", false);
        let dto = CommandErrorDto::from(err);
        assert_eq!(dto.code, "INVALID_TITLE");
        assert_eq!(dto.message, "x");
        assert!(!dto.retryable);
    }
}
