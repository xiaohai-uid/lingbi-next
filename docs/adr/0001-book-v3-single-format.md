# ADR-0001: Book Project V3 是唯一项目格式

- 状态：已接受
- 日期：2026-08-08
- 关联：Issue #4（Spec: Book Project V3）、P0 基线（`docs/fusion/p0-baseline.md`）

## 背景

融合前存在三条并行数据血统：
1. **lingbi v1**：`project.json` + `canon/` + `章节内容/` + `章节摘要/`（Flutter 时代）；
2. **lingbi-next Project V2**：JSON 信封 + documents 数组（revision/content_hash）；
3. **storyrepo 书仓**：目录树 + `追踪/_tracking-state.json` 单一权威 + 派生视图（经《零界道种》4 章真实验证）。

三者各定义过"数据真相"，是融合项目"双数据源"矛盾的根本来源。storyrepo 书仓已被生产验证（棘轮、单一权威、原子写），且与 v1 的 mutation protocol 哲学同构（候选→批准→不可变落盘）。

## 决策

**Book Project V3 = 目录树即项目**，作为唯一项目格式：

- 目录树承载书仓全部语义：`大纲/`（mutable，作者意图）、`定稿/`（append-only 棘轮）、`文风/`、`工作区/`（可丢弃）、`追踪/`（tracking state authoritative + 派生视图 derived）；
- 新增 `.lingbi/` 事务层（transactions/candidates/approvals/receipts/checkpoints/versions），承载应用托管状态；
- 四类文件（authoritative / derived / append-only / mutable）写入契约：`contracts/book/book-v3.tree.json`；
- 数据契约：`contracts/book/book-v3.schema.json`（book.yaml + tracking state）；
- **所有 authoritative 写入只经 Rust transaction**（原子写 + expected revision/hash + 回执 + 幂等键），不退回直接写文件；
- 旧格式只作为迁移源（P6 迁移器：source 只读 + hash 不变 + 正文 byte-for-byte）。

## 备选方案（已否决）

- **Project V2 扩展**：JSON 信封无法承载目录树语义（棘轮、派生视图、事务层），且与书仓割裂，否决。
- **双格式并存**：违反"唯一事实来源"，否决。
- **完全新设计**：丢失《零界道种》兼容性与引擎验证成果，否决。

## 后果

- 正向：单一数据源；旧书仓（含《零界道种》）可无损迁入；storyrepo 引擎行为可直接作为 Rust Book Core 的等价性基准（golden corpus）；P2-P4 有明确的行为边界。
- 代价：v1 项目与旧书仓需要一次性迁移器；`.lingbi/` 缺失的书仓 = 合法旧书仓（允许缺席）；Project V2 契约作废（其 revision/hash 纪律下沉为章节/事务级约束）。

## 验证

- 契约校验器（`scripts/validate-book-v3.py`，纯标准库）对 golden corpus **23 个快照 + 22 组棘轮跨步 0 错误**；**反例 8/8 被拒**（缺 book.yaml/缺追踪/编号不连续/front matter 缺失/state_revision 非法/未知文件/章节名非法/越权写定稿）；good 基底通过。
