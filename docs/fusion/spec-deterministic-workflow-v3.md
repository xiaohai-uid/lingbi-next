# P8 Deterministic Workflow 收口规格（#25）— Run 持久化/幂等/账本/门禁/恢复

> 日期：2026-08-08
> 上游：#24 P7 章事务引擎（lingbi-workflow 8 节点链 + checkpoint/invalidate）、P3 事务层崩溃收敛（recover_book_v3）、P4 命令层（resume/cancel/get_run_ledger 现状）
> 接缝模式沿用 P6/P7：lingbi-workflow crate 单元测试（辅）+ lingbi-commands/e2e 命令路径（主）。

## Problem Statement

P7 给了 8 节点链 + 节点 checkpoint，但运行本身不可追踪、不可取消、不可记账：

- 没有 **Run 记录**：一次链运行没有 run_id/状态/当前节点/时间戳，崩溃后无法回答「上次跑到哪、什么状态」；`resume_workflow` 只有引擎断点诊断，`cancel_workflow` 是空操作（P4 如实声明「真取消随 P8」）。
- 没有**幂等保障**：结算/追踪副作用若在崩溃窗口重复执行（如正文已落盘但结算未完成），可能重复 bump 修订/污染时间线——P3 恢复层在打开时收敛，但链运行内没有副作用幂等键与注入测试。
- 没有 **TokenLedger**：节点 usage 只存在内存报告里，跨运行不可查（run/node/provider/model 四级）。
- **评审阻塞没有闭环**：P7 评审有 blocking → 润色一次 → 直接结算；没有 Validate→Repair→Validate 循环与修复次数上限，也没有类型化的拒绝原因。
- **取消/预算不传播**：运行中无法取消；token 预算无法限制运行。

## Solution

在 P7 链之上加 Run 层：

1. **Run 持久化**：每次链运行生成 `run_id`，RunRecord（run_id/workflow_version/status/current_node/repair_attempts/reject_reason/created_at/updated_at）落 `工作区/工作流/runs/{run_id}.json`（运行开始写 Running，结束原子改写终态）。
2. **副作用幂等**：draft/settlement 节点记录 `idempotency_key`（= node_id）；结算副作用（正文 CAS 写 + cmd_commit）经「棘轮（章已提交拒绝）+ CAS（期望哈希 = 当前正文）+ P3 恢复收敛」三重保障不重复执行；注入 failpoint（`LINGBI_FAILPOINT=settlement-after-body`）测试崩溃恢复单次提交。
3. **TokenLedger 四级记账**：AI 节点每次执行 append 一行 JSONL（`工作区/工作流/token-ledger.jsonl`：run_id/node_id/definition_id/provider/model/prompt_tokens/completion_tokens/created_at）；查询 API 支持按 run/node/provider/model 过滤。
4. **OutputGate**：review/polish 段 = Validate→Repair→Validate 循环，`max_repair_attempts = 2`；每轮 repair 后强制复评（review 的 input 加入当前草稿文件哈希 draft_hash——草稿变化使复评自然失效重跑）；次数用尽仍有 blocking → 链 Rejected（RejectReason::BlockingIssuesRemain），不结算。
5. **RejectReason typed enum**：`BlockingIssuesRemain / MachineCheckFailed / RatchetRejected / BudgetExceeded / Cancelled`——入 ChainReport 与 RunRecord；机检 halt 带 MachineCheckFailed，结算棘轮失败带 RatchetRejected。
6. **Cancellation/budget Run→Node→Provider 传播**：`RunControl { cancel: Option<CancellationToken>, budget: Option<TokenBudget> }` 传入 run_chain；节点间检查取消（取消 → 节点 Cancelled + 链 Cancelled），AI 节点经 `stream_chat_with_cancel` 把取消传播到 provider 流；预算 = 每次 AI 请求 `max_tokens` 取 min(节点默认, 剩余预算)，节点后累计 usage 超限 → BudgetExceeded。
7. **Node Recovery**：仅 input fingerprint 未变的 Completed 节点可 skip（P7 已实现，P8 以注入崩溃恢复测试 + 复用断言正式收口）。

## User Stories

1. 作为作者，我想看到每次运行有 run_id/状态/当前节点/时间戳，以便崩溃后知道上次跑到哪。
2. 作为作者，运行在结算窗口崩溃后，重跑不会重复提交（修订只 bump 一次、时间线不污染），以便放心重试。
3. 作为作者，我想按 run/node/provider/model 查 token 消耗，以便审计与成本控制。
4. 作为作者，评审有阻塞问题时系统自动修复→复评循环，修复次数用尽仍不过则拒绝结算并给出类型化原因，以便质量门禁真实生效。
5. 作为作者，我可以取消进行中的运行，取消会传播到 provider 流，以便停止无谓消耗。
6. 作为作者，我可以设 token 预算，超预算运行停止并给出原因，以便控制成本。
7. 作为作者，输入未变的已完成节点重跑时跳过（Node Recovery），以便续跑不浪费。

## Implementation Decisions

### 1. RunRecord 与运行台账

```rust
pub struct RunRecord {
    pub run_id: String,               // uuid
    pub workflow_version: u32,        // = CHAIN_VERSION
    pub status: RunStatus,            // Running | Completed | Failed | Cancelled | Rejected
    pub current_node: String,         // 执行中节点 definition_id（终态 = 最后执行节点）
    pub repair_attempts: u32,         // OutputGate 已执行修复次数
    pub reject_reason: Option<RejectReason>,
    pub created_at: String,           // RFC3339
    pub updated_at: String,
}
```

落 `工作区/工作流/runs/{run_id}.json`（原子写，工作区 mutable 契约）。ChainReport 增加 `run_id` / `repair_attempts` / `reject_reason`；ChainStatus 增加 `Rejected`。

### 2. 副作用幂等（draft/settlement）

- NodeRecord 增加 `idempotency_key: Option<String>`（draft/settlement = node_id；审计 + 恢复对照）。
- 结算三重保障：棘轮（章已提交 → 拒绝）→ `tx::write_authoritative` CAS（期望哈希 = 当前正文；崩溃窗口内已写同字节 → 幂等重写）→ `cmd_commit`（章已提交守卫，不重复 bump）。
- 注入测试 failpoint：`LINGBI_FAILPOINT=settlement-after-body` 使结算在正文落盘后、cmd_commit 前模拟崩溃返回；断言：重跑后 `state_revision` 只 +1、时间线无重复、正文字节一致。
- Node Recovery 语义（P7）：仅 input fingerprint 未变的 Completed 可 skip——崩溃重跑时已完成节点按 input_hash 复用，副作用节点经幂等键审计。

### 3. TokenLedger（四级）

- 文件：`工作区/工作流/token-ledger.jsonl`，每 AI 节点执行 append 一行（含 re-execution，诚实记账）。
- 查询：`query_token_ledger(root, run_id, definition_id, provider, model 过滤) -> Vec<TokenLedgerEntry>`（None = 不过滤；组合过滤）。

### 4. OutputGate（review/polish 循环）

- review 节点 input params 增加 `draft_hash`（当前草稿文件哈希）——润色改写草稿 → 复评 input 变化 → 自然失效重跑（无需强制）。
- 运行器对 review/polish 段做门控循环：review → blocking? → (attempts < max) → polish 改写（attempts++）→ 复评；attempts >= max 仍 blocking → 链 Rejected + `RejectReason::BlockingIssuesRemain`（不结算）。
- polish 无 blocking 时透传（P7 语义保留）。

### 5. RejectReason

```rust
pub enum RejectReason {
    BlockingIssuesRemain, // OutputGate 修复次数用尽
    MachineCheckFailed,   // 机检状态门禁不过（链 Halted）
    RatchetRejected,      // 定稿棘轮（settlement 节点失败）
    BudgetExceeded,       // token 预算超限
    Cancelled,            // 用户取消
}
```

入 ChainReport.reject_reason 与 RunRecord.reject_reason（序列化字符串）。

### 6. 取消/预算传播

```rust
pub struct RunControl {
    pub cancel: Option<CancellationToken>,   // lingbi-ai
    pub budget: Option<TokenBudget>,         // { max_completion_tokens: u64 }
}
```

- `run_chain(root, ch, provider, control: &RunControl)`——P7 签名扩展（更新既有调用方/测试）。
- 节点间：`control.cancel.is_cancelled()` → 节点 Cancelled + 链 Cancelled（当前节点状态 Cancelled，error=None，reject_reason=Cancelled）。
- AI 节点：`stream_chat_with_cancel(request, cancel)`；预算：请求 max_tokens = min(4096, 剩余)，节点后累计 usage 超限 → 节点 Failed + 链 Failed + reject_reason=BudgetExceeded。

### 7. 命令层/壳收口

- `lingbi-commands::run_chain` 更新为收 RunControl；`cancel_workflow`（P4 空操作）删除——取消由调用方持有 token 实现。
- Tauri 壳：`start_chain`（spawn tokio 任务 + 存 Arc<CancellationToken> + 立即返回 run_id）/ `cancel_chain(run_id)`（cancel token）/ `get_chain_status(run_id)`（读 run 台账；进程重启后仍可查）——镜像 book_generation_tokens 模式。`book_cmds::run_chain`（P7 同步命令）删除。
- `resume_workflow`（引擎 diagnose）保留；`get_run_ledger`（tx 回执台账）保留——Run 台账为新增维度，不混入。

### 8. 兼容/迁移

- definition_version：review/polish 因 input 格式变化（draft_hash）升到 2，其余 1——旧 checkpoint 首次加载时 input 不匹配 → 全部失效重跑（诚实、无需迁移代码）。

## Testing Decisions

- **好测试 = 只测外部行为**：跑链（ScriptedProvider）→ 断言 run 台账/ledger 文件/链终态与原因/书仓修订号，不测内部循环。
- **lingbi-workflow 单元测试**：
  1. Run 台账：Running 写入 → 终态原子改写；run_id 唯一；current_node 反映执行进度。
  2. 崩溃幂等：failpoint 注入（settlement-after-body）→ 链 Failed → 重跑 → 修订仅 +1、正文字节一致、时间线无重复（注入测试，acceptance ①）。
  3. Node Recovery：失败重跑已完成节点复用（node_id 不变断言，P7 回归）。
  4. TokenLedger：3 节点执行 → 3 行 JSONL；四级过滤各返回正确子集；复评/重跑追加行。
  5. OutputGate：blocking → polish → 复评干净 → 结算；持续 blocking → attempts=2 后 Rejected + BlockingIssuesRemain + 未结算；复评 input 变化证据（draft_hash）。
  6. RejectReason：机检 halt → MachineCheckFailed；棘轮 → RatchetRejected；预算 → BudgetExceeded；取消 → Cancelled。
  7. 取消传播：慢 provider（挂起流）+ cancel → 链 Cancelled + 无结算。
  8. 预算：budget < 一次调用 usage → BudgetExceeded + 无结算。
- **lingbi-commands**：run_chain 收 RunControl 后既有单测更新；RunDto 含 run_id/reject_reason。
- **e2e/desktop 命令路径**：start_chain 语义经 lingbi-commands::run_chain 直测（run 台账/ledger/门禁）；壳层 start/cancel/status 用 Tauri 单测替代（真实 Windows E2E 交互随 P9 UI）。
- **回归**：P7 全部测试更新后保持绿；真实 Windows E2E（带 Edge driver env）绿。

## Out of Scope

- 工作流 UI（进度/取消按钮/节点可视化）→ **P9 #26**
- 并行运行（多 run 并发互斥）→ P9 产品语义再定
- 预算粒度（按节点/按 provider 独立预算、每日预算）→ 后续
- Run 保留策略/归档 → 后续
- 重写已定稿章（回炉链路）→ 维持 P7 边界

## Further Notes

- DeterminFlow 借鉴语义落点：D TokenLedger（本票 JSONL 四级）、E OutputGate（本票 Validate→Repair→Validate + max_repair_attempts + RejectReason）、F Node Recovery（P7 已实现，本票收口）——AGPL 仅借鉴思想，无代码引入。
- 能力对照：9（任务级模型路由 → Run 携带 provider/model + TokenLedger 记账，本票完成记账维度）、13（生成流水线 → 链 + 门禁闭环）、23（变更传播 → Node Recovery input 指纹）。
- 真实《零界道种》运行级验收随 P12。
