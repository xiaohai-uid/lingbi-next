# P11 Windows Consumer E2E 规格（#28）— 干净 Win11 全场景

> 日期：2026-08-08
> 上游：#26 P9 工作流 UI（start_chain/cancel_chain/get_chain_status + 4 步模式）、P5-P10 全部命令与真实 E2E 基建（tauri-driver + Edge WebDriver，windows_novice_golden_path ~11s）

## Problem Statement

单条 golden path（windows_novice_golden_path）已覆盖：建书→写→保存→重开→导出→AI 生成/取消→崩溃恢复→工具面板→P9 工作流错误态→无 Git。但「消费者完整旅程」未串测：**链真正跑通**（UI 建章后 4 步全绿 + 定稿 + Final Revision 快照）、**机检 halt → 补计划 → 续跑**（准备本章的真实闭环）、**第二章 Gate 返工**（评审阻塞 → 自动修复 → 复评通过）、**旧项目迁移 → 打开 → 继续写**、**重启恢复 0 丢失**。

## Solution

在真实 release 二进制上新增第二条 E2E（`windows_consumer_full_scenario`）：消费者完整旅程。

1. **SSE mock 按提示词路由**：请求体含「评审」→ 评审 JSON（首轮 blocking、之后 clean——状态化计数）；含「改写」→ 润色文本；其余 → 草稿文本。链的 draft/review/polish 三节点在真实 UI 流程中可完整执行。
2. **旅程**：
   - 手写 golden path 精简：建书 → 第一章 → 写正文 → 保存。
   - 配置 AI（SSE mock）→ 运行本章工作流：fresh 章（无 summary）→ 机检 halt（检查和优化步骤失败 + 摘要原因）→ 测试经文件系统补章计划（summary——Book V3 = 目录树即项目，作者计划步骤）→ 重跑 → 链完成（4 步全绿 + 定稿路径）→ 断言定稿正文 + `追踪` 结算 + `.lingbi/versions` Final Revision 快照。
   - 第二章：AI 生成 → 取消 → 重试（P5-2 语义复用）→ 运行链 → 评审首轮 blocking → 自动返工（polish 改写）→ 复评通过 → 链完成（run 台账 repair_attempts=1 证据）→ 导出 DOCX/MD 落盘断言。
   - 旧项目迁移：invoke `migrate_book`（legacy fixture → 新书仓）→ UI 打开迁移书 → 继续写 → 保存。
   - 重启（Session 2）：重开 → 全部内容完好（0 丢失）。

## User Stories

1. 作为消费者，我按「建书→手写→AI 工作流→定稿」一路走完，中途机检提示计划不全会告诉我哪一步没准备，补上后能续跑完成。
2. 作为消费者，AI 评审发现问题时应用自动返工，我在面板看到步骤完成。
3. 作为消费者，我导入旧书后能直接打开继续写，重启后一切都在。

## Implementation Decisions

### 1. SSE mock 路由（flow.rs `handle_sse`）

- 读完整请求体（已有 Content-Length 逻辑）→ 按内容路由：
  - `评审`（review 提示词）→ `data: {"choices":[{"delta":{"content":"{\"issues\":[...]}"}}]}`——首轮含 blocking issue、后续空（`AtomicUsize` 全局计数，每会话重置）。
  - `改写`（polish 提示词）→ 润色文本。
  - 其余 → 草稿正文。
  - `慢速` 分支保留（取消测试）。
- `start_sse_server` 返回可重置计数的句柄（或每调用新 server——旅程内多段用同一 server，计数跨段累计，语义按轮次断言）。

### 2. 新 E2E 测试

- `tests/windows_desktop_e2e.rs` 增 `windows_consumer_full_scenario`（同 harness：release binary 缺失 = 硬失败）。
- flow.rs 增 `consumer_full_scenario(binary)`：上述旅程 + 磁盘断言（定稿字节、versions 目录、run 台账 repair_attempts、导出文件、迁移书仓）。
- 时序纪律（P5-4 教训）：每段单 UI 断言 + 磁盘证据兜底。

### 3. 与既有 golden path 的关系

- `windows_novice_golden_path` 保持（手写路径 + P9 错误态）；新测试 = AI 增强完整路径。两测试独立 app 会话。

## Testing Decisions

- 真实 release 二进制（pnpm tauri build）驱动；无 mock 前端。
- 磁盘证据：定稿/正文/00N-*.md 字节、`.lingbi/versions/*.json` 数量、`工作区/工作流/runs/*.json` 的 repair_attempts、`export/` 文件、迁移后书仓 `.lingbi` 事务层。
- 机检 halt → 补计划 → 续跑：断言第一次运行「检查和优化」失败文案 + 第二次 4 步全绿（Node Recovery/续跑语义的真实证据）。
- Gate 返工：断言 run 台账 repair_attempts=1 + 定稿正文 = 润色文本（mock 路由区分）。
- 迁移：fixture 用 `fixtures/storyrepo-oracle/153eb63/steps/000-initial/book/`（P6 同源）→ invoke migrate_book → open_book → 继续写。
- 重启 0 丢失：Session 2 重开 → 第一章/第二章正文 + 导出文件仍在。

## Out of Scope

- 干净 Win11 虚拟机矩阵（真机多版本）→ 发布门禁 P13
- 安装器（NSIS/MSI）流程 → P13
- 性能/内存基线 → P13

## Further Notes

- 「补章计划」经文件系统写 summary：Book V3 = 目录树即项目（作者可编辑任意文件）；UI 暂无 frontmatter 编辑器（P12 统一真实书仓验收时评估是否需要）。
- P11 完成后，`windows_novice_golden_path` + `windows_consumer_full_scenario` = 产品双路径门禁（手写 + AI 增强），后续每阶段回归跑两条。
