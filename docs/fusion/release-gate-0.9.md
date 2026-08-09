# LingBi Next 0.9.0 发布门禁清单（P13 #55）

> 门禁编排器：`scripts/release-gate.sh`（Git Bash，Windows 门禁机）。任一 FAIL 即阻断发布。
> 验收口径：每项给出证据命令与 PASS 判定；G13（0 P0 data-loss）为聚合项。

| # | 门禁项 | 证据命令（release-gate.sh 内部） | PASS 判定 |
|---|---|---|---|
| G1 | Rust Core | `cargo test --workspace` | 全二进制 `0 failed; 0 ignored`（skipped=0） |
| G2 | React tests | `npx vitest run src/App.test.tsx` | `Tests N passed` 全绿 |
| G3 | 命令层 Windows failed=0 skipped=0 | G1 同源（workspace 含命令层/e2e-desktop 全测试） | 同上 |
| G4 | Desktop E2E | `LINGBI_EDGE_DRIVER=... cargo test -p lingbi-e2e-desktop-real --test windows_desktop_e2e` | `2 passed`（golden path + consumer） |
| G5 | clean-machine | G4 即真实 release 二进制干净路径；无 Git PATH 证明 = P10（`git in PATH: False` 全功能绿） | G4 绿 + P10 审计记录 |
| G6 | V1 migration | `cargo test -p lingbi-e2e-desktop --test migration_command_path` | `test result: ok` |
| G7 | 零界道种 | `LINGBI_REAL_BOOK=... cargo test -p lingbi-e2e-desktop --test real_book_acceptance`（缺省 corpus 022 基线） | `test result: ok`（真实书仓全命令链） |
| G8 | crash recovery | `recovery_crash_failpoints`（+ golden path 崩溃注入段） | `test result: ok` |
| G9 | external modification protection | `workflow_chain_command_path`（冲突守卫） | `test result: ok` |
| G10 | workflow resume | `lingbi-workflow --test run_persistence`（Node Recovery + resume_workflow） | `test result: ok` |
| G11 | TokenLedger | `lingbi-workflow --test output_gate`（ledger 四级记账） | `test result: ok` |
| G12 | OutputGate repair | 同上（Validate→Repair→Validate，max_repair_attempts=2） | `test result: ok` |
| G13 | 0 P0 data-loss | 聚合：G8（崩溃单次提交）+ G9（外部修改冲突不丢字）+ G12（棘轮拒绝保字节）+ 审计 P0 对照 | G8/G9/G12 绿且无 FAIL |
| G14 | 契约校验 | `validate-book-v3.py --self-test` + `--corpus` | `self-test errors: 0` + `corpus total errors: 0` |
| G15 | Skill 仅声明式 | 技能面板声明式测试（vitest `技能` 用例）+ 能力白名单（`capability.test.ts` 仅 git 二进制） | 全绿 |
| G16 | 安全基线 | CSP 严格（`csp: default-src 'self'...`）+ capability 仅 `core:default` + 零 tauri 插件 + 进程白名单断言 | 四项断言全中 |
| G17 | 依赖审计 | `cargo audit` | `0 vulnerabilities`（未安装则跳过并记录） |

## 安全基线审查项（G16 明细）

| 项 | 状态 | 证据 |
|---|---|---|
| IPC allowlist | ✅ | capability 仅 `core:default`（P5-5 能力测试锁定） |
| scoped paths deny 优先 | ✅ | 无 fs 插件；Rust 侧自有路径校验（导出文件名派生自磁盘、拒绝 `..`/分隔符，P4/P5-4） |
| 无 shell/system command | ✅ | 命令层进程白名单 = 仅 git 二进制（P10，capability.test.ts 断言） |
| 严格 CSP | ✅ 本次修复 | `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self' ipc: http://ipc.localhost`——前端零直连 HTTP（AI 全走 Rust IPC），CodeMirror 需 style unsafe-inline |
| 无远程 webview 特权 | ✅ | prod 仅本地资源；devUrl 仅 dev 模式 |
| OS keyring | ✅ | API Key 存 OS keyring（P5-5，provider_status 回显永不含 key） |
| typed 校验 | ✅ | 命令层 DTO + serde 严格类型 |
| 依赖审计 | ✅ G17 | cargo audit 0 vulnerabilities |

## 0 P0 data-loss 对照（G13）

P0 = 用户正文/定稿数据丢失。防线与证据：
1. 崩溃收敛：tx 层 WAL+journal 前向补完（`recovery_crash_failpoints` + golden path 崩溃注入 E2E「恢复 1 条」）
2. 外部修改冲突：CAS 守卫不覆盖外部字节（conflict guidance 测试）
3. 定稿棘轮：已提交章不可直写（run_persistence ratchet + 验收 harness 字节断言）
4. 保存字节等价：`full_chapter_bytes` 互逆（保存未修改 = 字节不变，golden path 断言）

本清单随 0.9.0 发布验收执行并落 ticket 证据。
