# Book V3 事务层（P3 #20）设计与无绕过审计

> 状态：已实现（commit 0872162 @ fusion/book-v3）。本文件是 #20 的验收依据之一：
> 「Book V3 所有 authoritative 写入仅经 transaction（代码审查无绕过）」的审计结论。

## 1. 分层

| Crate | 模块 | 职责 |
|---|---|---|
| `lingbi-storage` | `book_v3.rs` | 共享记录类型（`BookV3Receipt`/`BookV3Checkpoint`）、`.lingbi/` 六目录契约、`write_json_atomic` 写入口 |
| `lingbi-book` | `tx.rs` | **事务门**：修订守卫 + CAS + 日志生命周期（唯一权威写通道） |
| `lingbi-recovery` | `book_v3.rs` | 崩溃收敛（哈希证据，无回滚） |

## 2. 事务门语义（托管书仓，`root/.lingbi/` 存在）

每次权威写入（`lingbi_book::tx::write_authoritative`）：

1. **checkpoint**（`.lingbi/checkpoints/{tx}.json`）——写前审计记录；恢复不依赖（原子写保证全有或全无）。
2. **WAL 日志 begin**（`.lingbi/transactions/{tx}.json`，`DocumentTransaction` phase=Created）——先写日志后 apply（spec #4）。
3. **期望哈希 CAS 原子写**（tmp+fsync+rename；磁盘与期望不符 → `DocumentConflict`，外部字节保留）。
4. phase=ContentWritten。
5. **回执**（`.lingbi/receipts/{tx}.json`：after_revision + after_content_hash）——append-only 历史。
6. phase=Completed → 清理日志（残留日志 = 未决事务 = 恢复对象）。

**修订守卫**（仅追踪状态）：磁盘 `state_revision` 必须等于 `s.state_revision - 1`（引擎语义：每次内容变更保存前 bump 恰好一次），或本轮保存与「load 规范化后的磁盘状态」逐字节一致（幂等重存，批次预刷新路径）。否则 `DocumentConflict`——拦截并发丢更新，保留用户字节。哈希 CAS 兜底修订号未变、内容被改的情形。已验证全部调用方：`cmd_commit`（bump→save）、`batch::merge_batch` 预刷新（幂等）+ 每章（bump）、`revision::cascade_revision`（bump）、`init_state`（首次写，无前置镜像）。

**legacy 书仓**（无 `.lingbi/`，spec「旧书仓允许缺席」）：调用方分支回引擎原字节路径（`tracking::save_state` tmp+rename、`learn` fs::write）——P2 回放门禁（`replay_equivalence.rs` 22 步）字节锁定，`.lingbi/` 绝不自动产生。权威写入不存在第三条路径。

## 3. 崩溃收敛（`recover_book_v3`）

原子写 ⇒ 文件全有或全无，无需回滚；按哈希证据收敛（幂等，可在每次打开时调用）：

- 目标内容 == `after_hash` → 前向补完（补回执 → Completed → 清理）。
- 目标内容 == `before_hash` 或文件缺失 → 事务未生效 → 清理。
- 目标内容为其他字节 → 崩溃窗口内外部修改 → **保留用户字节**，日志标记 Failed 留档。
- IndexUpdated/Completed 残留 → 清理；Failed → 不动；日志损坏 → 记 errors，保留文件。
- 目标读取的真实 IO 错误（非 NotFound）→ 记 errors，跳过（日志保留，下轮再试）。

## 4. 无绕过审计（2026-08-08，`git grep` 全仓）

Book V3 权威路径 = `book.yaml`、`定稿/**`、`文风/金句库/**`、`文风/审查反例库.md`、`追踪/_tracking-state.json`、`.lingbi/**`。

| 写点 | 路径类别 | 结论 |
|---|---|---|
| `tracking::save_state`（cmd_commit/batch/revision/init 全部经此） | `追踪/_tracking-state.json` authoritative | 经事务门（managed）/引擎路径（legacy）✓ |
| `learn::harvest_quotes` | `文风/金句库/{ch}.md` authoritative/append-only | 经事务门（managed）/引擎路径（legacy）✓ |
| `learn::absorb_review` | `文风/审查反例库.md` authoritative/append-only | 经事务门（managed）/引擎路径（legacy）✓ |
| `cmd_commit` → `tx::write_version_snapshot` | `.lingbi/versions/…` authoritative | 经事务门（首次写无 CAS，照常 journal+回执）✓ |
| `lingbi-commands::write_body`（P4 #21 命令层唯一写入口：create/save_chapter、adopt_candidate） | `定稿/正文/**` authoritative | managed 经 `tx::write_authoritative`；legacy 走引擎纪律 tmp+rename（与 save_state legacy 分支同构）✓ |
| `views::write_views`、`tracking::init_state` 占位卡 | `追踪/*`（除状态卡）derived | 派生视图，契约允许直接写 ✓ |
| `review::save_review/write_report`、`cmd::cmd_batch_create` | `工作区/**` mutable | 非权威，直接写 ✓ |
| `lingbi-storage` 各 repo | `.lingbi/transactions\|candidates\|approvals\|receipts` | 事务层自身（原子写）✓ |
| `lingbi-application` document/generation/mutation 流 | `chapters/{uuid}.md`、`.lingbi/project.json`、`documents.json` | **V2 应用模型**（P4/P5 由 Tauri 命令迁到 V3 时替换），非 Book V3 权威路径，不在本票范围 |
| `lingbi-import-export::flutter_v1` | `.lingbi/project.json` | V1 导入（V2 应用模型），P6 迁移器接入 V3 时走事务门 |

**结论：Book V3 权威路径上不存在绕过事务的直接写**。未来 定稿/正文 写入（P7 章事务引擎的 candidate adoption）须经 `tx::write_authoritative`（CAS 期望哈希取自采纳前读取），此通道已就绪并在测试中覆盖。

## 5. 范围声明（对 #20 What-to-build 的逐项对应）

- **AtomicFileStore / expected revision+hash / DocumentTransaction / Receipt / snapshot / recovery**——已接入（见 §2/§3；input/output hash 由日志 `before_hash`/`after_hash` 记录，spec #4 的哈希纪律）。
- **Candidate / Approval / CommitIntent**——既有 `lingbi-mutation::MutationEngine` 管线（`.lingbi/candidates|approvals|intents` + `CommitIntent.idempotency_key`）已事务化，供 AI 候选稿采纳流使用；其目标路径当前为 V2 模型（`chapters/{uuid}.md`），**接入 Book V3 定稿/正文 属 P7 章事务引擎（#24，节点链 Candidate/Settlement/Tracking）**——#20 落地六目录 + 通用权威写通道，不在本票提前实现 P7 语义。
- **idempotency_key（spec #4）**——恢复**从不重放内容**（原子写 + 哈希证据收敛，只补回执/清理），重复执行在结构上不可能；候选采纳流由 `CommitIntent.idempotency_key` 承担显式幂等键。
- **`.lingbi/` 六目录落地**——`tx::ensure_tx_layer`（幂等）；应用建书/迁移时调用（P4 `open_book`/建书命令接入）。
- **crash 注入恢复测试**——`book_v3_recovery.rs` 12 例按崩溃相位手工构造磁盘现场（journal 前后/内容写后/回执前后/外部修改/损坏日志/目标 IO 错误/幂等）。
- **external modification 检测**——`book_v3_tx.rs` 修订号改、同修订字节改、损坏、CAS 追加冲突共 4 例，均验证用户字节保留。

## 6. 验收对照

- [x] Book V3 所有 authoritative 写入仅经 transaction（§4 审计，无绕过）
- [x] crash 注入恢复测试绿（25 例新测试全绿；workspace 225 绿，仅 pre-existing e2e-desktop-real 需发布构建）
- [x] external modification 检测绿（4 例冲突测试，用户字节保留）
- [x] P2 回放门禁不破（legacy 字节路径未动；`replay_equivalence.rs` 22 步绿）
