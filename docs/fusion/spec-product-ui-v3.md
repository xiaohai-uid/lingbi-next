# P9 产品 UI 规格（#26）— 黄金路径永远成立 + 小白 4 步模式

> 日期：2026-08-08
> 上游：#21 P4 命令层（UI 唯一接入面）、#24 P7 章事务引擎（start_chain/cancel_chain/get_chain_status 壳命令 P8 #45 已就绪）、P5 产品层（BookEditor/工具面板/黄金路径 E2E）
> 接缝：Tauri 命令（start_chain/cancel_chain/get_chain_status）+ React UI（App.tsx/store）+ App.test.tsx + 真实 Windows E2E（windows_novice_golden_path）。

## Problem Statement

P5-P8 交付了完整的产品后端（命令层 + 章事务引擎 + Run 台账），但 UI 仍只有 P5 的工具面板（契约/简报/机检/诊断只读展示）与 P5-2 的单次生成（book_generate 流式 → 候选稿）。作者面对「运行一章工作流」没有入口；8 节点/哈希/账本等内部机制无处安放；没有「4 步模式」的产品化呈现。黄金路径（打开→创建→写→保存→重开→导出）在无 API Key / AI 宕机 / 引擎异常三场景下必须无条件成立——目前生成入口的失败体验未验证。

## Solution

在 BookEditor 中新增**本章工作流面板**（默认 4 步模式）：

1. **4 步模式（默认）**：`准备本章 → AI 写作 → 检查和优化 → 确认本章`。步骤映射 8 节点链：准备本章 = contract/brief；AI 写作 = draft；检查和优化 = machine-check/review/polish；确认本章 = settlement/handoff。面板显示当前步骤 + 步骤状态（进行中/完成/失败/取消/拒绝）。
2. **高级模式（显式切换）**：展开 8 节点列表（definition_id/status/note/error/reject_reason）；**内部机制默认不可见**——checkpoint hash / input_hash / output_hash / TokenLedger 计数 / mutation / CommitIntent 只在高级模式且折叠展示，不加标签直出。
3. **运行控制**：`运行本章工作流`（start_chain → 轮询 get_chain_status）→ 进行中可 `取消`（cancel_chain）→ 完成/失败/拒绝展示结果（定稿路径/字数/评审阻塞数/修复次数）；无 Key / AI 宕机 / 引擎异常 = 面板内错误展示，**绝不阻断编辑区**（黄金路径无条件成立）。
4. 错误三场景（验收）：无 Key（configured_provider NoApiKey）、AI 宕机（provider 网络错误 → 节点 Failed）、引擎异常（书仓损坏 → 命令错误）——均只影响工作流面板；保存/导出/重开不受影响。

## User Stories

1. 作为小白作者，我打开编辑器就能看到「准备本章→AI 写作→检查和优化→确认本章」四步，点「运行本章工作流」一章自动成稿。
2. 作为小白作者，运行中我能看到当前在第几步、可以取消，完成后看到本章定稿的确认信息。
3. 作为高级作者，我切换「高级模式」能看到 8 个节点的状态与失败原因，但不需要看哈希/账本等内部细节。
4. 作为作者，没配 API Key / AI 全宕 / 书仓异常时，我仍然能写正文、保存、重开、导出——错误只在工作流面板出现。
5. 作为作者，失败/拒绝时我能看到原因（机检未过/评审阻塞未解/预算超限/已取消），知道下一步做什么。

## Implementation Decisions

### 1. 前端接线（lib/desktop.ts + store/useAppStore.ts）

- 新增 wrapper：`startChain(ch) -> { run_id }`、`cancelChain(run_id)`、`getChainStatus(run_id) -> ChainStatusDto`（invoke 壳命令）；类型：`ChainRunDto`（status/run_id/nodes/checks/review_path/handoff）、`ChainNodeDto`（definition_id/status/note/error/provider/model/usage）、`HandoffDto`。
- store 新状态：`chainRunning / chainRunId / chainError / chainRun: ChainRunDto | null / chainNodes: ChainNodeDto[] / chainCurrentStep`；actions：`chainStart(ch)`（start_chain → 轮询 get_chain_status 直至 done → 填充 chainRun）、`chainCancel()`、`chainReset()`。
- 轮询：`setInterval 500ms`，done 即停；组件卸载/切换章时清理。

### 2. 4 步模式（默认）

- BookEditor 右侧面板新增「本章工作流」区块：四步 indicator（进行中高亮/完成打勾/失败红/取消灰）。
- 步骤映射：`准备本章`(contract/brief) → `AI 写作`(draft) → `检查和优化`(machine-check/review/polish) → `确认本章`(settlement/handoff)；由 nodes 里已完成/失败节点推导当前步骤。
- 完成态展示：定稿路径 + 字数 + 评审报告入口（review_path 提示）+ 修复次数（若有）；失败态：reject_reason 文案（机检未过/评审阻塞未解/预算超限/已取消/棘轮）+ 失败节点 note/error。
- 「高级模式」checkbox（默认关）：开 → 8 节点列表 + 每节点（definition_id/status/note/error）+ 折叠区（input/output hash、provider/model、usage）。

### 3. 内部机制不可见

- 默认界面不出现 hash/ledger/mutation/CommitIntent/checkpoint 字样；高级模式以「节点详情」折叠展示（含哈希，标注仅供诊断）。

### 4. 三场景黄金路径（验证）

- 无 Key：未配置 provider → `start_chain` 返回错误（NO_API_KEY 类）→ 面板错误文案「未配置 API Key，请先到设置配置」；编辑区不受影响。
- AI 宕机：provider 网络错 → draft 节点 Failed → 面板显示失败节点与原因；写作不受影响。
- 引擎异常：损坏书仓 → 命令错误映射（面板错误）；写作不受影响。
- 断言：以上三场景下 写正文→Ctrl+S→重开→正文还在→导出 全通（App.test.tsx mock + 真实 E2E 无 Key 场景）。

## Testing Decisions

- **App.test.tsx（vitest + mock desktop）**：4 步模式默认展示；高级模式切换显示 8 节点；无 Key 运行 → 面板错误 + 编辑区可用；运行中显示当前步骤；完成态显示定稿路径/字数。
- **store 单测**：chainStart 轮询至 done 的状态迁移（mock getChainStatus 序列：Running → Completed）。
- **真实 Windows E2E（windows_novice_golden_path 扩展）**：无 Key 场景 → 打开编辑器 → 4 步模式可见 → 点「运行本章工作流」→ 面板出现错误文案 → 继续写正文/保存/重开/导出全通（黄金路径无条件成立验收）。
- 回归：72 组 workspace 全绿 + 既有 golden path 断言不变（时序断言按 P5-4 教训保留单 UI 断言）。

## Out of Scope

- TokenLedger 可视化/成本报表 → 后续
- 并行运行/多章队列 → 后续
- 预算设置 UI（RunControl.budget 壳层已留 None）→ 后续
- 回炉/重写已定稿章 UI → 维持命令层边界
- 主题/无障碍打磨 → P13 前

## Further Notes

- 能力对照：27（中文题材引导流程 REDESIGN → 4 步模式 = 题材引导的落点，本票先做 4 步容器与错误三场景；题材卡内容随小白建书向导）、12（创作罗盘 → 步骤文案传达意图）、10（Slash 命令面板 → 后续）。
- 真实《零界道种》链 UI 演示随 P12。
- 前端测试基建：App.test.tsx 已存在（vitest），mock 走 `lib/desktop.ts` 的 inTauri() 分支。
