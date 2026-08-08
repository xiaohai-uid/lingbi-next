# P0 冻结与基线（2026-08-08）

冻结架构后的第一条机器证据基线。所有测试结果均为当日实际运行记录。

## 三仓 SHA 与标签

| 仓库 | 冻结 SHA | 标签 | 说明 |
|------|----------|------|------|
| xiaohai-uid/lingbi（v1） | `91e9fb03c8c4403c9b49ea5fe43711f5f1fa1d2c` | `maintenance-frozen`（已推送） | 只收数据安全修复 + Fusion 阻断修复；暂不归档 |
| xiaohai-uid/lingbi-next | `66a5172ced56a22f5e31381573521faef62ca0cd` | `pre-fusion-tauri-shell`（已推送） | 当前 Tauri 主线回退点；Fusion 分支 `fusion/book-v3` 自此分出 |
| xiaohai-uid/storyrepo | `153eb632b1a99bcce5b055946b357a765de3009d` | —（WSL 真仓 HEAD） | 引擎移植基准（Reference Implementation） |

## 测试基线

### lingbi v1 @ 91e9fb0（Flutter 3.44.6, `flutter test`）
- **1537 通过 / 3 跳过 / 3 失败**
- 失败：
  1. `test/e2e_novel_generation_test.dart`（加载失败，网络/live provider 依赖）
  2. `test/e2e_workflow_test.dart`（全链路 AI 用例，需真实 provider）
  3. `test/release_metadata_contract_test.dart`（release 契约 vs 本地 SDK 底线）
- 结论：离线确定性测试全绿；网络依赖与发布契约项为已知外部条件。

### lingbi-next @ 66a5172（`cargo test --workspace`）
- **67 单元测试通过（0 失败）**；`windows_novice_golden_path` FAILED（需先 `pnpm tauri build` 产出 release 二进制，环境项）
- 真实 Windows 桌面 E2E 状态：**CI 门禁 opt-in**（`fix/ci-gate-desktop-e2e-optin` 分支已建、未合入 main；默认不阻塞 CI）

### storyrepo 引擎 @ 153eb63（Python 3.12.10, `pytest`）
- **212 通过 / 1 失败 / 1 跳过**
- 失败：`test_resume.py::TestDiagnose::test_committed_stale_card_warns`——断言"状态卡新鲜=False"而冻结版实现返回 True（引擎自带测试与实现漂移，冻结 SHA 如实记录；corpus 为黑盒行为记录不受影响）
- 已知仓库垃圾（非本阶段处理，建议后续清理）：跟踪了损坏路径文件 `C:\Users\a1691\Documents\Obsidian Vault/00_Command/AI/Logs/2026-08-07.md`（Windows 无法 checkout/archive）；`engine/tests/tests/` 重复目录

## Golden Corpus（P0 关键交付物）

- 位置：`fixtures/storyrepo-oracle/153eb63/`（`steps/000-initial/book` 初始不可变快照 + 22 步 × (stdout + exitcode + 整仓快照) + MANIFEST.json）
- 生成器：`scripts/gen-storyrepo-oracle.py`（黑盒：`python -m storyrepo.cli` 子进程 + cwd=书仓根 + PYTHONPATH=冻结引擎）
- 覆盖命令：doctor(prewrite/precommit/postcommit) / contract / context / brief / review / commit / check / status / stats / resume / revision / learn / batch create / report
- **22 步全部 exit=0**；覆盖副作用：追踪状态合并（imported_through_chapter=2, state_revision=3）、派生视图重建（上下文/伏笔/作者真相/时间线/角色状态/读者已知/逐章记录）、评审报告落盘、金句收割、批次计划
- **字节锁定**：生成时统一 LF（引擎 Windows CRLF 副作用归一化），仓库既有 `fixtures/** text eol=lf` 规则保证任何平台检出一致；**全新克隆验证 0 mismatches**
- Rust Book Core equivalence tests 只读此 corpus；正式 Windows 门禁零 Python/WSL 依赖
- corpus 快照含时间戳文件名（`工作区/批次-<ts>.json`），消费方按 glob 定位

## 后续阶段衔接

- P1 以此为基线定义唯一 Book Project V3 schema（书仓结构 + `.lingbi/` 事务目录 + 文件分类规则）
- P2 按 22 步 equivalence 移植 Rust Book Core
