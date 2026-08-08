# Spec: Book Project V3 — 唯一项目格式

## Problem Statement

灵笔 Next 融合了三条数据血统：lingbi v1 的 project.json + canon/章节内容、lingbi-next 的 Project V2（JSON 信封 + revision/hash）、storyrepo 书仓（目录树 + 追踪单一权威）。三套格式并行定义过数据真相，是"双数据源"矛盾的根本来源。融合后的应用需要一个**唯一项目格式**：它必须能承载 storyrepo 引擎已验证的全部行为（章事务、机检、追踪、回炉），必须保留 V2 的 revision/hash 纪律，必须为确定性工作流（候选稿/批准/回执/检查点/版本快照）提供托管层，并且旧书仓（含《零界道种》）能在不丢字节的前提下迁入。

## Solution

定义 **Book Project V3（书仓 = 项目）**：项目是一个目录树，不是 JSON 信封。目录树承载 storyrepo 书仓的全部语义（大纲/定稿棘轮/文风/工作区/追踪），新增 `.lingbi/` 事务层承载应用托管状态（事务日志/候选/批准/回执/检查点/版本）。四类文件（authoritative / derived / append-only / mutable）的边界写入契约；所有 authoritative 写入只经 Rust transaction（原子写 + expected revision/hash + 回执）；追踪 `_tracking-state.json` 是唯一权威状态，派生视图只由引擎重建。Book V3 同时是迁移目标：V1 旧项目与旧书仓经一次性迁移器无损迁入。

## User Stories

1. 作为一个首次安装的 Windows 作者，我想创建一本新书，所以打开软件后点"新建书"即可开始写作，不需要理解任何格式概念。
2. 作为作者，我想直接打开我现有的《零界道种》书仓，所以我的 4 章正文和追踪状态原样可读、可继续写。
3. 作为作者，我想在 AI 写完后先看候选稿再决定是否采纳，所以 AI 永远不能直接写进正文。
4. 作为作者，我想第二天重新打开软件后一个字都不丢，所以所有写入都是原子事务，崩溃后自动恢复。
5. 作为作者，我想编辑旧章节，所以系统产生新的 Final Revision 并保留旧版本快照。
6. 作为作者，我想知道每章的字数和全书进度，所以状态卡是自动生成的派生视图。
7. 作为系统，我想在每次提交前运行 11 项机检，所以违反规则（实体未登记、信息差泄密、占位符残留）的章节被门禁拦截。
8. 作为系统，我想在评审拒绝后让上游节点返工，所以 RejectReason 与输入指纹保证只有受影响节点失效。
9. 作为系统，我想在进程崩溃后从断点续跑工作流，所以每个 Run 和副作用节点都有持久化状态与幂等键，已完成节点绝不重跑。
10. 作为系统，我想记账每个节点的 token 消耗，所以 TokenLedger 按 run/node/provider/model 四级记录。
11. 作为开发者，我想确认新实现与 Python 引擎行为一致，所以 golden corpus 提供逐字节期望输出。
12. 作为开发者，我想知道某个文件是哪种性质，所以契约明确标注 authoritative/derived/append-only/mutable，误改即被校验器拒绝。
13. 作为迁移者，我想把 v1 旧项目与旧书仓搬进 V3，所以迁移器保证 source 只读、正文 byte-for-byte、结构文件等价并出具回执。
14. 作为安全审计者，我想确认 Skill 无法直接写正文，所以 Skill 运行时是声明式且无进程执行能力。
15. 作为离线用户，我想在没有网络和 Git 的环境下正常写作，所以数据安全不依赖系统 Git。

## Implementation Decisions

1. **项目 = 目录树**。Book V3 不是 JSON 信封；V2 的 `revision`/`content_hash` 纪律下沉为章节级与事务级约束（见 4/5）。
2. **目录契约**（`contracts/book-v3.tree.json`）：每个路径声明类别与规则：
   - `book.yaml` — authoritative/mutable，仅经事务写；可含 `id`（uuid）、name/status/total_chapters/字数窗口/target_words。
   - `大纲/`（总纲/卷纲/章纲/细纲蓝图/承诺总表/信息差）— mutable，作者意图。
   - `定稿/`（正文/NNN-标题.md、设定/名册+角色+信息差 S-总表、记忆/章摘要）— **append-only 棘轮**：只增不改，正文仅经事务写入；同路径文件内容跨事务不可变（回炉 revision 是唯一例外，且走显式重放）。
   - `文风/`（风格宪法 mutable；金句库、审查反例库 append-only）。
   - `工作区/`（草稿/评审报告/批次计划）— mutable，可丢弃，gitignored 语义。
   - `追踪/_tracking-state.json` — authoritative，唯一权威状态，仅事务写入。
   - `追踪/` 其余视图（上下文/伏笔/作者真相/读者已知/时间线/角色状态/逐章记录）— derived，禁手改，只由引擎重建。
   - `.lingbi/` — 应用托管事务层（authoritative，仅 Rust transaction 写入）：`transactions/`（WAL 日志，append-only）、`candidates/`、`approvals/`、`receipts/`、`checkpoints/`、`versions/`（每章 Final Revision 快照）。
3. **ID 规则**：book id = uuid（book.yaml，旧书仓迁移时生成）；章节 id = 三位章号（`001`），正文文件 `NNN-标题.md`；承诺 P-NNN、信息差 S-NNN 沿用；事务记录 id = uuid。
4. **revision/hash 规则**：`_tracking-state.json.state_revision` 单调递增，每事务 +1；正文提交记 `input_hash`/`output_hash`（SHA-256）；`.lingbi/transactions` 为 WAL，先写日志后 apply；每事务带 `idempotency_key`（崩溃重放不重复执行）。
5. **crash recovery 规则**：所有 authoritative 写入 = tmp + fsync + rename（原子）；启动扫描未决事务：已完成 apply 的跳过、未完成的按日志重放或回滚；checkpoints 供工作流断点恢复；external modification 检测（hash 不符即拒绝覆盖）。
6. **旧书仓兼容**：corpus（storyrepo-oracle）的 23 个快照全部满足核心树契约（.lingbi/ 允许缺席 = 未启用事务层的合法旧书仓）；V1 项目经迁移器生成 V3 目录 + .lingbi/。
7. **测试接缝（单一）**：契约校验器（Python，P1 交付）→ P2 `lingbi-book` crate 的 `BookValidator` 复用同一 schema；最终单一接缝在 crate 公开 API（open_book/save_chapter/run_check…）。
8. **schema 形态**：`contracts/book-v3.schema.json`（book.yaml 与 _tracking-state.json 的 JSON Schema）+ `contracts/book-v3.tree.json`（目录树契约）。

## Testing Decisions

- 好测试 = 只测外部行为：给定书仓目录树，校验器只回答"合法/不合法 + 违反哪条规则"，不测内部实现。
- P1 测试：契约校验器对 **golden corpus 全部 23 个快照**（000-initial + 22 步）验证全绿（引擎真实行为 ⊇ schema）；**棘轮跨步检查**（定稿文件跨步不可变，revision 步骤除外）；**反例集**（缺 book.yaml / 缺追踪 / 正文编号不连续 / front matter 缺字段 / state_revision 非法 / 工作区写定稿 / 未知文件类别）全部被拒。
- P2+ 测试：Rust equivalence tests 读同一 corpus 与 schema；负例复用 P1 反例集。
- 先例：lingbi-next `rust_opens_shared_project_v2_fixture`（fixtures 字节锁定的既有纪律）、storyrepo 引擎 214 测试。

## Out of Scope

- 章事务引擎/工作流运行时实现（P2/P7/P8）
- V1 → V3 迁移器实现（P6，本 spec 只定义目标形态）
- `.lingbi/` 事务层的具体存储编码细节（P3 实现时定）
- Go Cloud、WebDAV、技能市场（P5/P13 后置）

## Further Notes

- P0 已冻结三仓 SHA 并生成 golden corpus（22 步全 exit=0，LF 字节锁定，全新克隆 0 mismatches）——本 spec 的验收基准。
- 旧 lingbi 项目格式与旧书仓仅作为"可迁移源"，不再并行定义（评审修正 #3/10：source 只读 + hash 不变 + byte-for-byte 正文）。
- 里程碑：P1 交付 schema + 校验器 + 反例集；P2 起所有 Rust 模块以本契约为行为边界。
