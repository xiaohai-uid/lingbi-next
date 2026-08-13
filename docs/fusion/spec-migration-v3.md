# Spec: V1 → Book V3 一次迁移（P6）

## Problem Statement

LingBi 融合了三套数据血统：lingbi v1 的 `project.json + 章节内容/`、lingbi-next 的 Project V2（JSON 信封）、storyrepo 书仓（目录树 + 追踪单一权威）。Book V3（#4 spec）已是唯一项目格式，但已有用户的旧项目与旧书仓（含真实《零界道种》4 章）还停留在旧形态：Flutter V1 项目没有 `定稿/正文` 目录树，旧 storyrepo 书仓没有 `.lingbi/` 事务层（无法享受崩溃收敛/版本快照/候选稿托管）。迁移必须是**无损的**：source 只读、正文一个字节都不变、结构等价、全程可审计（回执）。

## Solution

一次性迁移管线（五件套）：MigrationInspector → MigrationPlan → MigrationExecutor → MigrationValidator → MigrationReceipt。输入旧源（两类），输出 Book V3 托管书仓（目录树 + `.lingbi/` 事务层），正文 byte-for-byte，回执六字段落盘。

## User Stories

1. 作为零界道种的作者，我想把现有书仓无损迁入新格式，所以 4 章正文与追踪状态原样保留、一个字节不丢，迁移后继续在 V3 里写作。
2. 作为旧版灵笔（Flutter V1）的用户，我想把项目搬进 V3，所以我的章节进入 `定稿/正文` 且正文内容一字不改。
3. 作为迁移者，我想在迁移前先知道源是什么、有多少文档、有什么风险，所以 Inspector 只读扫描并给出 classification 与 warnings。
4. 作为迁移者，我想在真正写盘前看到每个文件将去往哪里、按什么规则，所以 Plan 产出逐文件目标映射与 migration_rule。
5. 作为迁移者，我想确认源在迁移过程中完全没有被动过，所以迁移前后 source_hash 一致（READ ONLY 证据）。
6. 作为审计者，我想核实迁移结果无损，所以 Validator 逐文件对比 source/destination 字节并跑契约校验与 doctor。
7. 作为审计者，我想知道每次迁移的确凿记录，所以回执含 source_hash/classification/destination/destination_hash/migration_rule/warnings 六字段并落盘。
8. 作为开发者，我想复用 V1 项目扫描能力，所以 `flutter_v1::inspect_v1` 继续作为 Inspector 的 V1 分类来源。
9. 作为系统，我想保证迁移写入也走事务纪律，所以正文写入经事务门，`.lingbi/` 由 `ensure_tx_layer` 初始化。

## Implementation Decisions

1. **新 crate `lingbi-migration`**（五件套单一职责；依赖 lingbi-book（tx/tracking/doctor）、lingbi-import-export（inspect_v1）、lingbi-storage（sha256）、lingbi-contracts）。
2. **五件套接口**（公开 API，全部纯函数/显式 I/O，无全局状态）：
   - `MigrationInspector::inspect(source) -> MigrationReport`：只读扫描 → classification + source_hash + 逐文件清单（rel path + bytes 引用）+ warnings。**全程只读**。
   - `MigrationPlan::plan(report, destination) -> MigrationPlan`：逐文件目标映射（rel path + migration_rule）+ 可行性校验（destination 不存在/为空、章号无冲突、标题 sanitize 用 create_chapter 同款非法字符白名单）。
   - `MigrationExecutor::execute(plan) -> MigrationReceipt`：构建目标树——book.yaml（补 id=uuid）、追踪 `_tracking-state.json` 原样搬入、正文经 `tx::write_authoritative`、`tx::ensure_tx_layer` 托管化、回执落 `.lingbi/migration-receipt.json`。
   - `MigrationValidator::validate(source, destination) -> ValidationResult`：逐文件字节对比 + 最小树契约校验（book.yaml/required_dirs/章文件命名/`.lingbi/` 六目录/追踪存在）+ `cmd_doctor` exit 0 + source_hash 双算断言。
   - `MigrationReceipt` 六字段：`source_hash` / `classification` / `destination` / `destination_hash` / `migration_rule`（逐文件规则表）/ `warnings`。
3. **classification 两态**：`storyrepo-legacy`（目录树原样搬入 + 托管化，全文件字节不变）/ `flutter-v1`（结构转换）。
4. **flutter-v1 映射规则**：
   - 章节（`章节内容/` 或 `chapters/` 下 md）→ `定稿/正文/{ch:03}-{sanitized-title}.md`；**正文段（frontmatter 之后）byte-for-byte**；源文件缺 V3 六字段 frontmatter → 按 title/chapter/volume/words/status/summary 补全，`migration_rule: frontmatter-补全` 记入回执。
   - 其余 md → `工作区/迁移存档/{原路径}`（mutable 归档，不冒充契约目录），`migration_rule: resource-archived` + warning。
   - `.lingbi/` 源目录跳过（V1 应用元数据不迁入）。
5. **storyrepo-legacy 映射规则**：除 `book.yaml`（补 id 字段，其余键原样）与 `.lingbi/`（生成）外全文件字节不变；`migration_rule: copy-as-is` / `generate-book-id` / `init-tx-layer`。
6. **source READ ONLY 保证**：Inspector 先算 source_hash（按 rel path 排序的逐文件 sha256 拼接）；Executor 代码审查保证只读（无任何写源路径）；Validator 迁移后重算 source_hash 断言相等。
7. **目标存在性**：destination 已存在且非空 → fail-closed（`ProjectPathExists` 语义）；目标为空目录允许（幂等重试路径）。
8. **命令层接线**：`lingbi-commands::migrate_book(source, destination) -> MigrationReceiptDto`（P4 唯一边界；桌面壳包装；**无 UI**，P9 产品 UI 暴露）。迁移器逻辑在 workspace 内 → e2e/desktop 命令路径测试可直测。
9. **删除过时路径**：`flutter_v1::migrate_v1_to_v2`（V2 应用模型输出，无任何调用方）与其 V2 断言测试删除；`inspect_v1` 保留。
10. **契约校验**：迁移后跑 `scripts/validate-book-v3.py`（P1 契约校验器）作为辅接缝证据（本地验收用；CI 以 Rust 侧最小树校验 + doctor 为准）。

## Testing Decisions

- 好测试 = 只测外部行为：给定源目录树 + 目标路径，管线产出合法 V3 托管书仓 + 六字段回执；不测内部实现。
- 主接缝：`lingbi-commands::migrate_book` 命令路径测试（e2e/desktop 风格，P4 先例 `book_v3_command_path`）；辅接缝：`lingbi-migration` crate 单元测试（每件套独立断言）。
- fixture：storyrepo-legacy 源 = corpus `fixtures/storyrepo-oracle/153eb63/steps/000-initial/book/`（字节锁定、doctor 通过基准、真实零界道种同构）；flutter-v1 源 = flutter_v1.rs 既有测试 fixture 形态。
- 断言集：回执六字段齐全；source_hash 迁移前后相等（READ ONLY）；正文逐文件字节相等；doctor exit 0；`.lingbi/` 六目录就位；反例（目标已存在、源缺 book.yaml、章号冲突、非法标题字符）fail-closed。

## Out of Scope

- 章事务引擎/工作流运行时（P7/P8）
- 全项目包导出（P11）
- 真实《零界道种》之外的批量迁移 UI 与交互（P9 产品 UI）
- WebDAV/同步（P13 后置）
- V2 应用模型兼容（V3 唯一格式，`migrate_v1_to_v2` 删除）

## Further Notes

- 验收基准：#23 四条（source READ ONLY + hash 不变 / 正文 byte-for-byte / 《零界道种》迁移后 doctor 通过 / 回执六字段）。
- 迁移目标是**新书仓**（destination 新建），不是就地升级——零界道种真实验收用 WSL 副本（复制不改源，P12 同语义）。
- 字节纪律：目标文件名派生自真实文件名/白名单（P5-4 export 路径穿越教训：不信任源 frontmatter 拼路径）。
