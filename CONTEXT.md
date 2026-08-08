# CONTEXT — LingBi Fusion

本术语表是融合工程（P0-P14）的统一语言。所有 issue 标题、ADR、测试名、提交信息必须使用下列规范词，不得漂移到同义词。

## 项目与格式

- **书仓（Book）**：一本小说的完整目录树，是项目的唯一容器。见 `contracts/book/book-v3.tree.json`。
- **Book V3**：唯一项目格式。旧格式（lingbi v1 project.json、Project V2 JSON 信封、旧书仓）只作为**迁移源**，不再并行定义。见 ADR-0001。
- **追踪状态（tracking state）**：`追踪/_tracking-state.json`，唯一权威状态（chapters/promises/info_gaps/timeline/entity_states/reader_knowledge/state_revision）。只经事务写入。
- **派生视图（derived views）**：`追踪/` 下除 tracking state 外的全部文件（上下文/伏笔/作者真相/读者已知/时间线/角色状态/逐章记录）。禁手改，只由引擎重建。
- **棘轮（ratchet）**：`定稿/` 只增不改的纪律；同路径文件跨事务内容不可变（回炉 revision 是唯一例外）。
- **.lingbi 事务层**：应用托管状态（transactions/candidates/approvals/receipts/checkpoints/versions），仅 Rust transaction 写入；旧书仓允许缺席。
- **Final Revision**：某章正文的一次确定版本；编辑旧章产生新 Final Revision，旧版本快照保留在 `.lingbi/versions/`。

## 章事务与写作管线

- **章事务（chapter transaction）**：Contract → Brief → Draft → Machine Check → Review → Polish → Settlement → Handoff 的 8 节点流水线（高级模式；普通用户只见 4 步：准备本章→AI 写作→检查和优化→确认本章）。
- **合同（contract）**：从细纲蓝图生成的本章写作约束（目标/硬约束/禁区/伏笔待办/结尾要求）。
- **任务书（brief）**：四路上下文 + 五段写作委托。
- **机检（machine check / gate）**：11 项确定性检查，门禁（prewrite/precommit/postcommit）；违反即拦截。
- **候选稿（candidate）**：AI 生成内容未经批准不得进正文；生命周期 pending→reviewing→approved→adopted/rejected。
- **批准（approval）/ 回执（receipt）**：人工批准记录与事务提交凭证；写入只经 transaction。
- **回炉（revision）**：重放式级联（expected_state_revision 并发控制），修改旧章的唯一切入点。
- **批次（batch）**：并行起草计划 + 顺序合并入账。

## 确定性工作流

- **冻结定义（frozen definition）**：Run 启动时的工作流定义快照，运行中修改定义不影响进行中的 Run。
- **输入指纹（input fingerprint）**：节点输入哈希；仅指纹未变的 Completed 节点允许 skip。
- **TokenLedger**：run/node/provider/model 四级记账。
- **OutputGate**：Validate → Repair → Validate，带 max_repair_attempts。
- **RejectReason**：typed enum（reason code/source/target/message/recoverable/suggested action），下游拒绝上游的返工依据。
- **幂等键（idempotency key）**：副作用节点防重复执行；崩溃恢复保证 Candidate/Settlement/Tracking 更新不执行两次。

## 验收与迁移

- **Golden Path**：打开软件→创建书→写正文→自动保存→关闭→重开→正文还在→导出；不配 API Key、AI 全宕、工作流异常均不能阻止。
- **golden corpus**：`fixtures/storyrepo-oracle/<engine_sha>/`，Python 引擎的黑盒行为记录（22 步 + 不可变快照），Rust equivalence tests 的唯一基准；Windows 门禁零 Python/WSL 依赖。
- **迁移器（migration）**：Inspector/Plan/Executor/Validator/Receipt 五件套；source 只读 + hash 不变 + 正文 byte-for-byte。
- **生产级**：工程可靠性 + 安全基线（Tauri 官方能力模型）；签名/支付/法律为外部门禁，如实披露不阻塞本地验收。
