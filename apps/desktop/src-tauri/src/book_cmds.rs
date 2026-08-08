//! P4 桌面壳：Book V3 命令的 Tauri 接线（P4 16 命令 + P5-1 增量 2 =
//! create_book / list_chapters，共 18；P5-2 增 AI 生成 book_generate /
//! book_generate_cancel，第 19-20 命令）。
//!
//! 命令逻辑在 `lingbi-commands`（workspace 内，可被 e2e/desktop 直接测）；
//! 本模块只做：会话 root 持有、命令调用、错误映射，以及 P5-2 的 AI 生成
//! 管道（provider 流式 → 候选稿落盘，复用 lingbi-ai 与 create_candidate）。
//! 命令语义/与引擎差异的 fail-closed 声明见 `lingbi-commands` crate 头。

use lingbi_ai::{AiEvent, ChatMessage, ChatRequest};
use tauri::{Emitter, Manager, State};
use uuid::Uuid;

use crate::{
    command_error, CommandErrorDto, BookGenerationEventDto, BookGenerateStartDto, DesktopState,
};

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

/// 生成候选稿（Book V3，P5-2）：真实 provider 流式（book-generation-event
/// 事件），完成后内容落 `工作区/草稿`（lingbi-commands::create_candidate）。
/// 上下文 = 引擎任务书（build_brief 字节等价）+ 用户写作要求。
/// 运行时模型快照：每次任务经 `configured_provider` 重新解析 provider+
/// model（v1「DI 捕获 provider 致切换失效」在 V2 管道已修，Book 版沿用）。
#[tauri::command]
pub async fn book_generate(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    ch: i64,
    instruction: String,
) -> Result<BookGenerateStartDto, CommandErrorDto> {
    let root = book_root(&state)?;
    let provider = crate::configured_provider(&state).await?;
    let settings = crate::generation_settings(&state)
        .await
        .map_err(CommandErrorDto::from)?;
    // 引擎任务书（含四路上下文）作为生成上下文，字节等价输出
    let brief = lingbi_commands::build_brief(&root, ch).map_err(CommandErrorDto::from)?;
    let prompt = format!("{brief}\n\n写作要求：{instruction}");

    let task_id = Uuid::new_v4();
    let cancel = lingbi_ai::CancellationToken::new();
    state
        .book_generation_tokens
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "book generation lock", false))?
        .insert(task_id, cancel.clone());

    let task_app = app.clone();
    let task_provider = provider.clone();
    let task_cancel = cancel.clone();
    let task_root = root.clone();
    tokio::spawn(async move {
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: "user".to_owned(),
                content: prompt,
            }],
            temperature: settings.temperature,
            max_tokens: settings.max_tokens,
        };
        let mut stream = task_provider.stream_chat_with_cancel(request, task_cancel.clone());
        let mut content = String::new();
        let mut stream_error: Option<lingbi_ai::AiError> = None;
        use futures::StreamExt;
        while let Some(event) = stream.next().await {
            match event {
                Ok(AiEvent::ContentDelta(delta)) => {
                    content.push_str(&delta);
                    let _ = task_app.emit(
                        "book-generation-event",
                        BookGenerationEventDto::Delta {
                            task_id,
                            content: delta,
                        },
                    );
                }
                Ok(AiEvent::Completed) => break,
                Ok(_) => {}
                Err(error) => {
                    stream_error = Some(error);
                    break;
                }
            }
        }
        let cancelled = task_cancel.is_cancelled();
        let desktop = task_app.state::<DesktopState>();
        let _ = desktop
            .book_generation_tokens
            .lock()
            .map(|mut tokens| tokens.remove(&task_id));
        // 取消优先：token 已取消（含流中途取消）→ Cancelled 事件，部分
        // 内容不落候选（镜像 V2 generation 管道先判取消的语义）。
        if cancelled {
            let _ = task_app.emit(
                "book-generation-event",
                BookGenerationEventDto::Cancelled { task_id },
            );
        } else if let Some(error) = stream_error {
            // typed 错误映射（AiError → ErrorCode + retryable，与 V2 同表）
            let _ = task_app.emit(
                "book-generation-event",
                BookGenerationEventDto::Error {
                    task_id,
                    error: CommandErrorDto::from(ai_error_mapping(error)),
                },
            );
        } else if content.trim().is_empty() {
            let _ = task_app.emit(
                "book-generation-event",
                BookGenerationEventDto::Error {
                    task_id,
                    error: CommandErrorDto::from(lingbi_contracts::AppError::new(
                        lingbi_contracts::ErrorCode::AiInvalidResponse,
                        "AI 未返回任何内容".to_owned(),
                        false,
                    )),
                },
            );
        } else {
            // 候选稿落盘（工作区/草稿，mutable 非权威）
            match lingbi_commands::create_candidate(&task_root, ch, &content) {
                Ok(candidate) => {
                    let _ = task_app.emit(
                        "book-generation-event",
                        BookGenerationEventDto::Candidate {
                            task_id,
                            candidate_id: candidate.id,
                            ch,
                            content,
                        },
                    );
                }
                Err(error) => {
                    let _ = task_app.emit(
                        "book-generation-event",
                        BookGenerationEventDto::Error {
                            task_id,
                            error: CommandErrorDto::from(error),
                        },
                    );
                }
            }
        }
    });
    Ok(BookGenerateStartDto { task_id })
}

/// AiError → AppError（typed code + retryable，与 V2 generation_service 同表）。
fn ai_error_mapping(error: lingbi_ai::AiError) -> lingbi_contracts::AppError {
    let (code, retryable) = match error {
        lingbi_ai::AiError::NoApiKey => (lingbi_contracts::ErrorCode::AiNoApiKey, false),
        lingbi_ai::AiError::AuthFailed => (lingbi_contracts::ErrorCode::AiAuthFailed, false),
        lingbi_ai::AiError::RateLimited => (lingbi_contracts::ErrorCode::AiRateLimited, true),
        lingbi_ai::AiError::Timeout => (lingbi_contracts::ErrorCode::AiTimeout, true),
        lingbi_ai::AiError::Network => (lingbi_contracts::ErrorCode::AiNetworkError, true),
        lingbi_ai::AiError::Server(_) => (lingbi_contracts::ErrorCode::AiServerError, true),
        lingbi_ai::AiError::InvalidResponse => {
            (lingbi_contracts::ErrorCode::AiInvalidResponse, false)
        }
        lingbi_ai::AiError::Cancelled => (lingbi_contracts::ErrorCode::AiCancelled, false),
    };
    lingbi_contracts::AppError::new(code, error.to_string(), retryable)
}

/// 取消 Book 生成任务（P5-2）：令牌传播到 provider 流（真取消）。
#[tauri::command]
pub fn book_generate_cancel(
    state: State<'_, DesktopState>,
    task_id: String,
) -> Result<(), CommandErrorDto> {
    let task_id = Uuid::parse_str(&task_id)
        .map_err(|_| command_error("INVALID_UUID", "invalid task id", false))?;
    if let Some(cancel) = state
        .book_generation_tokens
        .lock()
        .map_err(|_| command_error("LOCK_ERROR", "book generation lock", false))?
        .remove(&task_id)
    {
        cancel.cancel();
    }
    Ok(())
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

/// 导出章节（P5-4）：md/txt/docx 到 书仓 export/ 目录。
#[tauri::command]
pub fn export_chapter(
    state: State<'_, DesktopState>,
    ch: i64,
    format: String,
) -> Result<lingbi_commands::ExportResultDto, CommandErrorDto> {
    let root = book_root(&state)?;
    lingbi_commands::export_chapter(&root, ch, &format).map_err(CommandErrorDto::from)
}

/// 声明式技巧卡（P5-5）：只读静态数据，无执行能力。
#[tauri::command]
pub fn list_skills() -> Vec<lingbi_commands::SkillCardDto> {
    lingbi_commands::list_skills()
}

/// 迁移旧书仓/旧项目到 V3 托管书仓（P6）：source 只读 + 正文 byte-for-byte，
/// 返回六字段回执。
#[tauri::command]
pub fn migrate_book(
    source: String,
    destination: String,
) -> Result<lingbi_commands::MigrationReceiptDto, CommandErrorDto> {
    lingbi_commands::migrate_book(&source, &destination).map_err(CommandErrorDto::from)
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
