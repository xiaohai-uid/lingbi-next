# P7 章事务引擎规格（#24）— 8 节点链 + input_hash/invalidate

> 日期：2026-08-08
> 上游：#4 Book Project V3（P1）、P2 引擎移植、P4 命令层（run_workflow 三段同步链）、P5 能力审计（8/9/13/23）
> 接缝模式与 P6 一致（命令层主接缝 + crate 单元测试辅接缝 + e2e 命令路径 + 真实验收），已在 P6 获批并沿用。

## Problem Statement

作者完成一章要经过：起草（AI）、机检、评审、润色、结算、交付。今天这些步骤是断开的：P4 `run_workflow` 只有 Contract→Brief→Machine Check 三段**确定性**同步链，AI 内容走 `create_candidate` 手工通道；链外没有节点级可追溯性（哪个 provider/模型、吃了什么输入、产出什么、花多少 token），没有上游变化失效传播（改了大纲/设定后旧草稿、旧机检、旧评审可能被误用），没有断点续跑（中途失败必须从头再来）。

## Solution

8 节点**章事务引擎**：Contract → Brief → Draft → Machine Check → Review → Polish → Settlement → Handoff。每次运行把每个节点固化为持久化节点记录：`node_id / definition_id / definition_version / input_hash / output_hash / status / provider / model / usage（token） / error`。

重跑语义（checkpoint + invalidate）：

- 按序为每节点计算 input_hash（= 规范化 JSON 哈希：definition + 书仓上下文指纹 + 上游节点 output_hash + 节点参数）。
- 已有记录 `Completed` 且 input_hash 一致 → **复用跳过**（checkpoint 可续，不重复 AI 消耗）。
- 已有记录 input_hash 不一致 → 本节点与后续全部 `Completed` 记录标 `Invalidated`（output_hash 清空，防旧节点复用），重新执行。
- 节点执行失败 → `Failed` + error 文本，链停；修复后重跑从该节点续（checkpoint 可续）。
- 机检状态门禁不过 → 链 halt（后续节点不执行），机检结果如实报告。

AI 节点（Draft/Review/Polish）经 `AiProvider` 抽象执行：provider/model 每次运行解析快照并记入节点（能力 8「运行时模型切换」的 REDESIGN 落点）；切换模型 → 节点 input_hash 变化 → 自动失效重跑。节点 usage 记录 `TokenUsage`（P7 记入节点；P8 做 Run 账本/四级记账）。

## User Stories

1. 作为作者，我想运行「本章工作流」得到完整 8 节点链结果（契约/简报/草稿/机检/评审/润色/结算/交付），以便一章从意图到定稿一次走完。
2. 作为作者，我想看到每个节点的 provider/模型快照、input_hash、output_hash、token 用量，以便审计每次生成、可复现结果。
3. 作为作者，我改了大纲/设定/文风/章计划后重跑工作流，旧草稿、旧机检、旧评审必须作废重来，以便写作始终基于最新意图，绝不复用陈旧节点输出。
4. 作为作者，我切换生成模型后重跑，相关 AI 节点自动失效并重新生成，以便模型切换立即生效（能力 8）。
5. 作为作者，我在运行中途失败（网络断/API 错）后修复问题重跑，已完成且输入未变的节点直接跳过，以便快速续跑而不是从头再来。
6. 作为作者，机检（编号/占位符/摘要/红线）不过时链停在 Machine Check 并给出失败清单，以便先修正计划再继续。
7. 作为作者，评审发现阻塞问题时润色节点按评审意见改写草稿，以便采纳前质量闭环。
8. 作为作者，评审无阻塞问题时润色节点原样透传、如实注明跳过，以便不产生无谓的 AI 消耗。
9. 作为作者，结算节点把最终稿经事务门写入定稿并结算追踪状态（合并事实/bump 修订/重建视图/版本快照），以便与手工采纳同一纪律、无绕过。
10. 作为作者，已结算章（定稿棘轮）不能被链直接覆盖，以便 append-only 棘轮语义保持。
11. 作为作者，交付节点给我最终章路径/标题/字数/全链摘要（handoff 报告），以便确认本章完成并留档。
12. 作为作者，同一输入重复运行不产生重复 AI 消耗（已完成节点跳过），以便 token 不浪费。
13. 作为作者，我可以在 legacy 书仓（无 `.lingbi/`）与托管书仓上同样运行链，以便 P6 迁移后的旧书仓也能用。

## Implementation Decisions

### 1. 新 crate：`lingbi-workflow`（章事务引擎）

依赖：`lingbi-book`（引擎 build_contract/build_brief/collect_checks/gate/redlines/review::save_review/cmd::cmd_commit/tx）、`lingbi-ai`（AiProvider/TokenUsage/AiError）、`lingbi-storage`（sha256_hex）、`lingbi-contracts`（AppError）、serde/serde_json/sha2/uuid/tokio/futures。

模块：`node.rs`（节点记录类型）、`chain.rs`（8 节点定义 + 执行 + 失效）、`hash.rs`（规范化哈希 + 书仓上下文指纹）、`checkpoint.rs`（持久化）、`lib.rs`（`run_chain` 异步编排）。

### 2. 8 节点定义（definition_id 固定、definition_version 随实现递增）

| # | definition_id | 类型 | 执行 |
|---|---|---|---|
| 1 | `contract` | 确定性 | 引擎 `build_contract(root, state, ch)` → 契约 JSON |
| 2 | `brief` | 确定性 | 引擎 `build_brief(root, state, ch, None, None)` → 简报文本 |
| 3 | `draft` | AI | 简报 + `assemble_context` → provider 流式收集 → 写 `工作区/草稿/{ch:03}-{node_uuid}.md`（复用 P4 DRAFT_DIR 惯例，mutable 不经事务门） |
| 4 | `machine-check` | 确定性 | 状态门禁 `gate(root, "prewrite", Some(ch))`（编号/占位符/摘要/字数弱提示）+ 草稿文本红线/占位符命中（新增 checks.rs 纯文本辅助）；状态门禁不过 → 链 halt；红线命中仅记录（供 Review 输入） |
| 5 | `review` | AI | provider 输出评审 JSON → 解析 `ReviewIssue[]` → `save_review`（`工作区/评审报告/第NNN章.json`）+ `write_report`（.md）+ `has_blocking`；解析失败 → 节点 Failed |
| 6 | `polish` | AI（条件） | 有 blocking 问题 → provider 按评审意见改写草稿 → 新草稿文件；无 blocking → 原样透传（note 注明「无阻塞问题，跳过改写」） |
| 7 | `settlement` | 确定性+事务 | 棘轮守卫（章已在追踪状态 → 拒绝）→ `tx::write_authoritative` CAS 写 `定稿/正文/{ch:03}-{title}.md`（期望哈希 = 当前章文件哈希，managed/legacy 双路径）→ `cmd_commit(root, ch, true)` 结算（合并事实/bump/视图重建/版本快照，precommit 警告透传与 P4 adopt_candidate 一致）→ 删草稿文件 |
| 8 | `handoff` | 确定性 | handoff 报告（最终章相对路径/标题/字数/全链节点摘要）→ `工作区/工作流/handoff-{ch:03}.json` + 返回 |

### 3. 节点记录与哈希

```rust
pub struct NodeRecord {
    pub node_id: String,          // "{definition_id}-{uuid}"
    pub definition_id: String,
    pub definition_version: u32,
    pub input_hash: String,       // sha256(规范化 JSON)
    pub output_hash: String,      // 执行后产物哈希；Invalidated 时清空
    pub status: NodeStatus,       // Pending | Running | Completed | Failed | Invalidated
    pub provider: Option<String>, // AI 节点快照
    pub model: Option<String>,
    pub usage: Option<TokenUsage>,
    pub error: Option<String>,
    pub note: Option<String>,     // 透传/跳过等语义说明
}
```

**input_hash** = sha256（规范化 JSON，键序固定）：`{definition_id, definition_version, book_input_hash, upstream_output_hash, params}`；`params` = `{ch, existing_chapter_hash}`（章文件当前哈希，无章为 `"absent"`），AI 节点 params 追加 `{provider, model}`（每次运行解析的快照）。

**book_input_hash（书仓上下文指纹）** = 排序 manifest（相对路径 + sha256）再整体哈希，覆盖：`book.yaml`、`大纲/**`、`文风/**`、`定稿/设定/**`、`定稿/记忆/**`、`追踪/_tracking-state.json`；排除 `.lingbi/**`、`.git/**`、`工作区/**`。prev 章正文不入指纹（其影响经追踪状态/章摘要体现，正文编辑由 `existing_chapter_hash` 覆盖）。

### 4. 重跑语义（checkpoint + invalidate）

1. 加载该章 checkpoint（`工作区/工作流/第NNN章.json`，不存在 = 首次运行）。
2. 对每节点按序：计算 input_hash → 记录存在且 `Completed` 且 input_hash 一致 → 跳过复用；否则标 `Invalidated`（本节点 + 后续所有 `Completed` 记录，output_hash 清空）→ 执行。
3. 执行成功 → `Completed` + output_hash/usage；失败 → `Failed` + error，链停。
4. 全链完成后落盘 checkpoint；handoff 报告落 `工作区/工作流/handoff-{ch:03}.json`。

checkpoint 存 `工作区/`（mutable 契约，managed/legacy 通用，可丢弃可重建）——与 P4 候选稿/评审报告同类别，不经事务门。

### 5. 接缝

- **主接缝**：`lingbi-commands::run_chain(root, ch, Arc<dyn AiProvider>)`（异步命令，P4 命令边界唯一化）+ e2e/desktop 命令路径测试直测。
- **辅接缝**：`lingbi-workflow` crate API 单元测试（FakeProvider 注入）。
- **Tauri 壳**：`book_cmds::run_chain` 由设置/密钥构建 provider（复用 `generation_settings` + keyring 模式，与 `book_generate` 同构），命令层不接触密钥。

### 6. 删除与保留

- **删除** P4 `run_workflow`（三段确定性链是 8 节点链的严格子集，无 UI 调用方，不留兼容层）。
- **保留** `resume_workflow`（引擎 diagnose）/`cancel_workflow`/`get_run_ledger` 现状——真取消与 Run 持久化属 P8（#25）。
- **保留** P4 手工通道 `create_candidate/adopt_candidate/reject_candidate`（作者手动采纳路径，能力 26 单人确认）；链内 settlement 与 P4 采纳同一事务纪律。

### 7. 诚实边界（如实记录）

- 链内结算 = 自动采纳：机检状态门禁 + 评审阻塞门禁已前置于链内，settlement 的 precommit 警告透传（与 P4 force 采纳一致）；AI 内容自动进入 append-only 定稿的风险在 P9 UI 提供「运行到评审为止」等交互模式（引擎预留参数，P9 定义）。
- 重写已定稿章：链不覆盖（settlement 棘轮拒绝），走回炉（revision 级联）+ 手工候选通道。
- 并发运行/取消传播/流式 UI 事件：P8/P9。
- 草稿文件的手工外部编辑不自动感知（input 以记录 output_hash 为准）；P9 UI 显式重跑。

## Testing Decisions

**好测试 = 只测外部行为**：跑链（喂 FakeProvider）→ 断言书仓产物（定稿正文字节/追踪修订/checkpoint/handoff）与记录字段（哈希非空且确定性、status 序列），不测内部调用顺序。

- **`lingbi-workflow` 单元测试**（tokio + FakeProvider，包一层计数 provider 断言 AI 调用次数）：
  1. 8 节点全链跑通：Completed + 定稿正文 = 最终草稿字节 + handoff 落盘 + checkpoint 落盘（managed 与 legacy 各一）。
  2. input/output_hash 记录齐全；同输入两次运行哈希相同（确定性）。
  3. checkpoint 可续：跑完重跑（输入不变）→ 8 节点全部跳过复用，AI 调用次数 0。
  4. 上游失效：改 `大纲/章纲.md` → 重跑 → contract 起全链重跑（AI 调用 ≥2）；仅改 provider/model 快照 → contract/brief 复用、draft 起失效重跑。
  5. 失败续跑：draft 节点 `with_error` → 链 Failed + error 落盘；换好 provider 重跑 → 从 draft 续（contract/brief 复用）。
  6. 机检状态门禁不过（如摘要空）→ 链 halt，review 之后节点不执行。
  7. Review 解析失败（非法 JSON）→ review 节点 Failed。
  8. 评审有 blocking → polish 改写（草稿文件变化）；无 blocking → 透传（note 注明）。
  9. 定稿棘轮：章已提交 → settlement 拒绝，正文不被覆盖。
  10. 草稿红线命中 → machine-check 记录命中项（链不 halt）。
- **`lingbi-commands::run_chain` 单元测试**：命令返回 8 节点 DTO + checkpoint 落盘 + 错误映射（CommandError）。
- **e2e/desktop 命令路径测试**（tokio + FakeProvider，先例：book_v3_command_path.rs）：`create_book → create_chapter → （写章计划）→ run_chain` → 断言定稿/正文/`{ch:03}-{title}.md` 字节、追踪 state_revision bump、checkpoint/handoff 落盘、二次运行（输入不变）节点复用、改大纲后重跑失效重跑。
- **checks.rs 纯文本辅助**（redline_hits/placeholder_hits）随 P2 既有检查表重构出公共函数，单元测试覆盖空文本/命中/反例库合并。
- 全部 AI 节点用 FakeProvider；真实 provider 接线仅存 Tauri 壳（真实网络 E2E 在 e2e/desktop-real，本票不碰）。

## Out of Scope

- Run 持久化（Run ID）/幂等/账本（TokenLedger 四级记账）/OutputGate/RejectReason/Node Recovery → **P8 #25**
- 工作流 UI（进度/取消按钮/节点可视化/「运行到评审为止」模式）→ **P9 #26**
- 并发运行、真取消传播（Run→Node→Provider）→ **P8**
- 流式 UI 事件（book-generation-event 接入链）→ **P9**
- 重写已定稿章（回炉 + 手工候选通道，P4 已有）
- 任务级模型路由（按任务选模型策略）→ **P8**
- Git 相关 → **P10**

## Further Notes

- 能力对照：**8**（运行时模型切换 REDESIGN → 节点携带 provider/model 快照 + 快照入 input_hash）、**9**（任务级模型路由 DEFER → 节点携带，路由策略 P8）、**13**（生成流水线 KEEP → 8 节点链取代 v1 状态机）、**23**（变更传播 KEEP → book_input_hash 指纹失效 + existing_chapter_hash）。
- DeterminFlow 借鉴语义分派：TokenLedger（P8 账本）/OutputGate/RejectReason/Node Recovery（P8）——P7 只做节点级 checkpoint 与失效。
- 与 P4 纪律一致：目标文件名只从磁盘文件名/白名单派生；settlement 写路径复用 tx 门禁（无第三条权威写路径，见 `docs/security/book-v3-transaction-layer.md` §4 未来章节）。
- 真实《零界道种》链验收：P12（#29）product-level acceptance 时用真实书仓跑链（确定性节点真实字节 + 注入 provider），本票真实验收 = WSL 副本书仓全链跑通 + 定稿落盘 + doctor 门禁证据。
