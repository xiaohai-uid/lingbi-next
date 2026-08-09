# P5 能力审计矩阵：v1 → Book V3 纵向分片重建（#22）

> 日期：2026-08-08
> 输入：`docs/research/2026-07-27-lingbi-capability-benchmark.md`（v1 31 类能力盘点，`verify/batch1-20260724` 分支）、Book V3 架构（#4 spec + P1-P4 交付）
> 处置枚举：**KEEP**（架构内直接保留并迁入）/ **REDESIGN**（需重构后迁入）/ **DROP**（不搬回）/ **DEFER**（延后到 P7+ 阶段）/ **PARTIAL**（仅保留安全子集）/ **BLOCKED_EXTERNAL**（外部门禁未过，如实不搬回）/ **DISABLED**（明确禁用）

## 0. 处置原则（冻结）

1. **架构先行**：一切能力必须以 Book V3 数据契约（`contracts/book-v3.*`）与 `lingbi-commands` 命令层为唯一接入面；绕过命令层直写 authoritative 的能力一律 REDESIGN。
2. **诚实标注**：v1 中「虚假完成」（UI/Service 存在但主路径不通）与「Windows 降级」（ZVec 等）不得以任何形式带病搬回——DROP 或 REDESIGN 二选一。
3. **外部门禁**：支付/市场情报/许可证等依赖真实服务端的数据与结算能力 = BLOCKED_EXTERNAL（商业门禁未过前不搬回，见 `docs/qa/commercial-audit-2026-08-06.md`）。
4. **风险面收缩**：Skill 任意执行、WebDAV 同步、遥测等默认关闭/禁用，安全基线优先（`docs/security/privacy-baseline.md`）。

## 1. 审计矩阵（31 项全量）

### A. 创作工作台与基础工程

| # | v1 能力 | v1 成熟度 | 处置 | 落点/证据 |
|---|---|---|---|---|
| 1 | 项目与本地文档 | 主路径可用 | **REDESIGN** | v1 ProjectService/DocumentService（JSON 信封）→ Book V3 书仓（目录树）+ `lingbi-commands::open_book`/`create_book`（P4/P5-1）；分片 1 |
| 2 | 长文编辑器 | 部分接线 | **KEEP（改造）** | Quill → CodeMirror 已换（现有 Tauri shell）；保存经 `save_chapter` CAS + 状态可见；分片 1 |
| 3 | 大纲、节拍与故事画布 | 部分接线 | **DEFER** | 引擎「细纲蓝图/总纲」为大纲唯一事实（`get_outline` 已暴露）；画布/节拍矩阵随 P9 产品 UI |
| 4 | 候选稿、安全采纳与冲突保护 | 主路径可用 | **KEEP** | `create|adopt|reject_candidate`（P4）+ P3 事务门 CAS 取代 v1「先删再 rename」非原子路径；分片 2 |
| 5 | 版本、快照、恢复 | 部分接线 | **KEEP（架构内升级）** | `.lingbi/versions/` Final Revision 快照（P3）+ `recover_book_v3` 崩溃收敛（P3）；统一恢复中心随 P11 |
| 6 | 导入、导出与出版编译 | 部分接线 | **REDESIGN** | v1「Word」=HTML 假 DOCX → 真 DOCX/MD/TXT 导出（ImportExportService 已事务化）+ 全项目包随 P11；分片 4 |

### B. AI 生成、模型与上下文

| # | v1 能力 | v1 成熟度 | 处置 | 落点/证据 |
|---|---|---|---|---|
| 7 | 多 Provider、自定义 API 与模型发现 | 部分接线 | **KEEP** | `lingbi-ai` provider_definitions/build_provider + provider_* 命令（现有 shell 已接线）；分片 2 |
| 8 | 运行时模型切换 | 虚假完成 | **REDESIGN** | v1 DI 捕获 provider 致切换失效 → 每次任务解析 provider+model 快照（P7 章事务引擎节点携带 provider/model）；分片 2 先修 UI 一致性 |
| 9 | 任务级模型路由 | 只有 Service+UI | **DEFER** | 路由入任务属 P7/P8（Run 节点携带 provider/model + TokenLedger 四级记账） |
| 10 | AI 对话、续写、润色与 Slash Skills | 主路径可用 | **KEEP（收窄）** | 生成/流式/取消保留（现有 shell generation_*）；Slash 命令面板随 P9；Skill 任意执行禁用（见 #28） |
| 11 | 上下文编译与可解释注入 | 部分接线 | **KEEP** | 引擎四路上下文（`assemble_context`/`build_brief` 字节等价，P2）；上下文检查器随 P9 |
| 12 | 创作罗盘与作者意图 | 只有 Service | **DEFER** | AuthorIntent 概念保留，UI 闭环随 P9 |
| 13 | 生成流水线与章节结算 | 部分接线 | **KEEP（架构对齐）** | v1 prepare→…→settlement 状态机 → P7 章事务 8 节点链（#24）；P4 已提供 run_workflow 同步链与 adopt_candidate 结算 |

### C. 世界观、连续性与知识

| # | v1 能力 | v1 成熟度 | 处置 | 落点/证据 |
|---|---|---|---|---|
| 14 | Canon 人物/地点/设定/情节点 | 主路径可用 | **KEEP（映射）** | v1 Canon 四类 → Book V3 `定稿/设定/`（名册/角色/信息差 S-总表）+ 引擎逐章记录派生视图；实体链接随 P9 |
| 15 | 人物关系图 | 部分接线 | **DEFER** | 关系提取/可视化依赖实体图谱，随 P9/P14 |
| 16 | 伏笔生命周期 | 部分接线 | **KEEP** | 引擎伏笔（foreshadow_todo/contract ending_requirement）+ 追踪伏笔状态（P2）；逾期/回收报警随 P9 |
| 17 | 多叙事线与红线 | 部分接线 | **KEEP** | 引擎红线机检（checks.redlines，P2）已是 precommit gate；跨章监控随 P9 |
| 18 | 反幻觉与一致性监督 | 部分接线 | **KEEP（引擎化）** | 引擎机检 11 项（实体未登记/信息差泄密/占位符/摘要等，P2）取代 v1 独立监督服务；采纳前 gate（adopt → cmd_commit precommit gate） |
| 19 | 向量知识库/RAG | 虚假完成 | **DROP** | v1 ZVec Windows 恒空、vectorSearch 退化普通查询——带病能力不搬回；引擎以规则化知识（设定/承诺/信息差/金句库）承担一致性，无向量依赖 |
| 20 | 参考书采集与四维分析 | 部分接线 | **DROP（抓取）/ DEFER（文件导入）** | 联网抓取风险面收缩（robots/版权不可控）；本地文件导入 + 风格抽象随 P14 |
| 21 | 风格蒸馏与绑定 | 部分接线 | **KEEP（引擎化）** | 引擎 `文风/风格宪法`（mutable）+ 金句库/审查反例库（append-only，P2 learn）+ 去 AI 味反例吸收；盲评门槛随 P9 |

### D. 审稿、修订与质量控制

| # | v1 能力 | v1 成熟度 | 处置 | 落点/证据 |
|---|---|---|---|---|
| 22 | 六维审稿 | 部分接线 | **KEEP（引擎化）** | 引擎机检 + 评审报告（review，P2）+ 反例吸收；开发/行级审稿随 P9 |
| 23 | 变更传播 | 部分接线 | **KEEP（引擎化）** | v1 vector 依赖不可靠 → 引擎回炉 revision 重放式级联（P2，expected_revision 并发控制）+ 输入指纹失效随 P7/P8 |
| 24 | 去 AI 味 | 主路径可用 | **KEEP（引擎化）** | 引擎红线 + 反例库机检动态合并（P2 learn）；改写建议随 P9 |
| 25 | 意图澄清 | 主路径可用 | **KEEP** | 引擎 `resume::diagnose`（断点诊断，P2）+ 契约/任务书确定性生成；指令追问随 P9 |
| 26 | 工作流审批 | 部分接线 | **KEEP（收敛为单人确认）** | 单人 = `adopt_candidate`（采纳即确认，P4）；团队角色/权限随 P14 |

### E. 专业扩展与生态

| # | v1 能力 | v1 成熟度 | 处置 | 落点/证据 |
|---|---|---|---|---|
| 27 | 中文题材引导流程 | 虚假完成 | **REDESIGN** | v1 题材卡不落盘 → 小白 4 步模式（P9 产品 UI：#26）；ProjectBrief 落 `大纲/` |
| 28 | Skills、市场与蒸馏 | 部分接线 | **DISABLED（执行）/ DEFER（蒸馏）** | Skill 任意代码执行 = 安全风险面，`terminal/DISABLED`（#22 预判）；声明式 Skill + 市场签名链随 P13/P14；蒸馏 SKILL.md 概念保留 |
| 29 | 中文网文市场情报 | 虚假完成 | **BLOCKED_EXTERNAL** | 依赖真实服务端数据/结算，商业门禁未过前不搬回（#22 预判）；不伪造数据 |
| 30 | 短故事、短剧改编和平行世界 | 部分接线 | **DEFER** | P2 专业扩展；真实工作室任务验证节省时间 ≥40% 前不占主导航 |
| 31 | 联网搜索、同步、商业授权与公益额度 | 部分接线 | **PARTIAL（WebDAV）/ BLOCKED_EXTERNAL（支付/许可证）/ DROP（遥测）** | WebDAV 仅保留冲突模型设计（#22 预判，不搬回实现）；支付/许可证 BLOCKED_EXTERNAL；遥测默认关闭 → v1 默认开启属隐私违规，DROP |

## 2. 纵向分片计划（进入时再拆票）

| 片 | 范围 | 对应能力 | 拆票 |
|---|---|---|---|
| 1 | 项目/编辑/保存/重启 | 1/2/5 | P5-1 |
| 2 | AI 配置/流式/取消/Candidate | 7/8/10/13 | P5-2 |
| 3 | 设定/上下文/评审 | 11/14/16/18/21/22/24/25 | P5-3 |
| 4 | 导入导出/恢复 | 6/5 恢复 | P5-4 |
| 5 | Skills/高级 | 27/28（声明式子集）/10 Slash | P5-5 |

每片验收 = 真实 Windows release 二进制（tauri-driver + Edge WebDriver）E2E 驱动该片主路径；验收标准见 `e2e/desktop-real/README.md`（passed>0 / failed=0 / skipped=0）。

## 3. 验收对照（#22）

- [x] 审计矩阵落盘（31 项全量 KEEP/REDESIGN/DROP/DEFER + PARTIAL/BLOCKED_EXTERNAL/DISABLED 标注）
- [x] 分片 1（项目/编辑/保存/重启）：真实 Windows E2E 绿（windows_novice_golden_path，release + tauri-driver + Edge WebDriver 151）
- [x] 分片 3（#34 设定/上下文/评审）：真实 Windows E2E 绿——工具面板（大纲/合同/任务书/机检/诊断）
- [x] 分片 4（#35 导入导出/恢复）：真实 Windows E2E 绿——导出 DOCX/MD/TXT 落盘 + crash journal 注入 → 重开恢复报告「恢复 1 条」+ 正文完好；legacy 首次写入托管化经事务门
- [x] 分片 5（#36 Skills/高级）：真实 Windows E2E 绿——高级设置 0.3/512 保存 → 重启回显预填 + provider_status 读回一致（keyring 持久化）；技能面板声明式只读卡展示 + P9 占位；能力审查证据（capability 仅 core:default、零 tauri 插件、命令层无进程调用）
- [x] P6（#23 V1→V3 一次迁移）：五件套迁移器（lingbi-migration crate + migrate_book 命令，无 UI）——storyrepo-legacy（字节原样 + 托管化 + 回执六字段）与 flutter-v1（章节转 定稿/正文 + frontmatter 补全 + 资源归档）两分类；**真实《零界道种》4 章迁移验收**：doctor exit 0 + 正文 byte-for-byte（4 章全 IDENTICAL）+ source READ ONLY（source_hash 前后一致）+ 回执落盘；契约校验器对真实书仓附加文件（README/skills/scripts 等未知类别）如实拒绝——无损优先，P12 完整产品级验收统一处理
- [ ] 每个纵向分片真实 Windows E2E 验收（P5-1 起逐片）
- [x] PARTIAL/BLOCKED 能力如实标注不搬回（#19 RAG DROP、#29 市场情报 BLOCKED_EXTERNAL、#28 terminal DISABLED、#31 WebDAV PARTIAL/遥测 DROP）
- [x] P7 章事务引擎（#24，能力 8/13/23）：8 节点链引擎（lingbi-workflow，#40）+ 命令层 run_chain（#41）+ 真实《零界道种》全链验收（#42，WSL 副本 ch4：8 节点 Completed、定稿落盘字节断言、追踪 rev 7→8 结算、checkpoint/handoff/评审报告落盘、doctor exit 0、改大纲重跑失效 + 定稿棘轮拒绝证据；源书仓 untouched；AI 节点注入 provider 如实记录）——规格 `docs/fusion/spec-chapter-transaction-v3.md`；链重跑语义：输入未变复用（checkpoint 可续）、上游变化 invalidate（能力 23）、provider/model 快照入 input_hash（能力 8）
- [x] P9 产品 UI（#26，能力 3/12/27 落点）：本章工作流面板——4 步模式默认（准备本章/AI 写作/检查和优化/确认本章，映射 8 节点链）+ 高级模式 8 节点（哈希/usage 仅高级折叠诊断）+ 运行/取消/轮询（start_chain/cancel_chain/get_chain_status）+ 三场景黄金路径（无 Key/AI 宕机/引擎异常只影响面板，写作永不阻断）——规格 `docs/fusion/spec-product-ui-v3.md`；真实 Windows E2E 扩展（4 步模式 + 错误态 + golden path 全通，11s）；前端测试 33 项全绿
- [x] P10 Git 降级为可选（#27）：运行时零 git 调用审计 + 屏蔽 git PATH 真实 E2E 全绿（干净 Windows 无 Git 全功能正常）+ 可选 Git 快照（git_snapshot 应用内执行，非仓库/无 git 优雅降级，能力测试白名单仅 git 二进制）——规格 `docs/fusion/spec-git-optional-v3.md`
- [x] P8 Deterministic Workflow 收口（#25，能力 9/13/23）：Run 持久化（#43 runs/{run_id}.json + 崩溃恢复注入测试单次提交）+ TokenLedger 四级记账（#44 JSONL + 查询 API）+ OutputGate Validate→Repair→Validate（#44 max_repair_attempts=2，Rejected + BlockingIssuesRemain）+ RejectReason 五值（#43/#44/#45）+ 取消/预算传播（#45 RunControl → stream_chat_with_cancel + BudgetExceeded）+ 壳收口 start_chain/cancel_chain/get_chain_status——规格 `docs/fusion/spec-deterministic-workflow-v3.md`；P4 cancel_workflow 删除
