# P12 真实《零界道种》产品级验收规格（#29）

> 日期：2026-08-08
> 上游：#23 P6 迁移器（契约校验器对真实书仓附加文件如实拒绝——无损优先，P12 统一处理）、#26 P9 产品 UI、#28 P11 Windows Consumer E2E、P7 章事务引擎（#42 真实书仓 WSL 副本 ch4 全链验收）

## Problem Statement

1. **契约校验器与真实书仓形态脱节**：`scripts/validate-book-v3.py`（P1 契约，tree.json 33 条规则）对真实《零界道种》书仓**拒绝 22 个文件**——顶层工具文件（README.md/AGENTS.md/状态卡.md/.gitignore/.zcode-plugin//scripts//skills/ 共 17）与作者内容文件（大纲/伏笔登记簿.md、大纲/卷纲/全书蓝图.md、定稿/设定/{世界观,力量体系,时间线}.md 共 5）。真实书仓是格式的权威实例，校验器必须能通过它；同时未知文件拒绝纪律（反例 n6-stray-file）必须保留。
2. **缺全命令链产品级验收**：《零界道种》此前只做过两点验收——P6 迁移（doctor + byte-for-byte + 回执）与 P7 ch4 单链（8 节点）。ticket #29 要求的 **doctor/tracking state/outline/canon/memory/章节顺序/final revisions/AI context build/Contract/Brief/Draft/Review/Settlement/Resume 全项通过**从未在单一流程中串测。

## Solution

### 1. 契约两档（P12-1）

tree.json 增补，校验器分级报告：

- **作者内容树开放**（契约类，参与校验但不再逐文件列举）：`^大纲/.+\.md$`（总纲/章纲/承诺/卷纲/伏笔登记簿/细纲蓝图/信息差）、`^定稿/设定/.+\.md$`（世界观/力量体系/时间线/名册/角色/信息差）——作者创作内容按类别接受，正文/章摘要/book.yaml/追踪契约保持严格。
- **顶层工具文件容忍档**（tolerated，非契约参与）：`README.md`、`AGENTS.md`、`状态卡.md`、`.gitignore`、`^\.zcode-plugin/.+$`、`^scripts/.+$`、`^skills/.+$`——storyrepo 自带工具/插件文件，报告为 ⚠ 不阻断（exit 0）。
- **未知文件仍 ✘ 阻断**：root-junk.txt 等树外文件照旧拒绝（反例 n6-stray-file 保持红）。

### 2. 全命令链验收（P12-2）

`e2e/desktop/tests/real_book_acceptance.rs` 验收 harness（复制不改源）：

```
复制源书仓 → migrate_book（storyrepo-legacy 托管化，回执六字段）
→ 字节等价（定稿逐文件 cmp）→ source READ ONLY（copy 前后源 hash 不变）
→ open_book / get_tracking_state / get_outline / list_chapters（章节顺序）
→ canon+memory（定稿/记忆/章摘要 全读）
→ assemble_context（AI context build：风格宪法+设定+章摘要+追踪）
→ build_contract / build_brief → cmd_doctor exit 0
→ run_chain（ScriptedProvider 注入，最后一章；Draft/Review/Settlement 全节点
  + .lingbi/versions Final Revision 快照 + 追踪结算）
→ resume_workflow（断点诊断 JSON）→ validate-book-v3.py exit 0
```

- **CI 基准**：默认用 `fixtures/storyrepo-oracle/153eb63/steps/000-initial/book`（doctor 通过基准，章节数按追踪数据驱动）。
- **真实运行**：`LINGBI_REAL_BOOK=<UNC 或本地路径>` 环境变量覆盖源——WSL 真实书仓 `//wsl.localhost/Ubuntu/home/a1691/novels/零界道种`，一次证据性运行（源 untouched + 全项通过），证据落 ticket。

## User Stories

1. 作为《零界道种》作者，我的书仓（含 scripts/skills/README/状态卡与全部设定/大纲文件）通过产品全部校验——工具文件不是错误。
2. 作为消费者，复制我的书仓后一条路径跑完：迁移→状态→大纲→设定/记忆→AI 上下文→合同→任务书→链（草稿/评审/定稿）→断点诊断，全程可验收。

## Implementation Decisions

1. **tree.json 增补 + validate-book-v3.py 两档报告**：`tolerated: true` 条目不产生 error（⚠ 警告行），blocking 语义不变；`--self-test` 增加「real-shape 正例」（合成真实书仓形态：good 基底 + 22 类真实文件）断言 0 阻断，n6-stray-file 反例仍拒。
2. **验收 harness 数据驱动**：章节数来自追踪状态（fixture 2 章 / 真实书 4 章同码）；链跑最后一章（working chapter）；AI 节点用 `lingbi_workflow::testing::ScriptedProvider`（如实记录 provider/model，非真实模型产物）。
3. **源 READ ONLY**：harness 复制前对源目录算 manifest hash，全流程后重算断言相等（真实书仓在 WSL 只读路径，Windows 侧经 UNC 复制）。
4. **链在副本上运行**：settlement 会改副本 ch4 正文（脚本化文本）并创建 `.lingbi/versions`——字节等价断言必须先于链执行。

## Testing Decisions

- 接缝主：`e2e/desktop/tests/real_book_acceptance.rs`（命令层 API 直测，P4 唯一边界）；辅：validator `--self-test`（real-shape 正例 + 既有反例全保）。
- 真实运行一次证据（LINGBI_REAL_BOOK 覆盖），doctor/回执/字节/全链断言输出落 ticket #54。

## Out of Scope

- 真实书仓内容变更（任何「改源」行为都不允许——只读验收）。
- flutter-v1 分类的设定/大纲形态（P6 已定义 frontmatter 补全规则，不在此统一）。
- 批量迁移 UI（P9 已明确无 UI）。
