# P13 发布门禁 + 安全基线 + 签名策略规格（#30）— 0.9.x 先行

> 日期：2026-08-08
> 上游：#4 Book V3（P1 契约）、#22 P5 产品层重建（能力审计 KEEP/REDESIGN/DROP/DEFER + 安全边界）、#27 P10 Git 可选（命令层进程白名单）、#28/#29 P11/P12（Windows E2E + 真实书仓验收）

## Problem Statement

功能已全部落地（P5-P12），但没有**可重复执行的发布门禁**：门禁清单（Rust Core/React/命令层 Windows/Desktop E2E/clean-machine/V1 migration/零界道种/crash recovery/external modification/workflow resume/TokenLedger/OutputGate repair/0 P0 data-loss）散落在各票证据里，无法一键全验；Tauri 安全基线有一项实缺口（`csp: null`）；版本仍 0.1.0（未签名阶段应走 0.9.x 内测语义）；发布物只有 NSIS 安装包，无便携 ZIP/SHA256SUMS/PROVENANCE。

## Solution

### 1. 发布门禁清单（P13-1）

`scripts/release-gate.sh`（Git Bash 可跑，Windows 门禁机）——逐项运行并输出 PASS/FAIL 清单，任一失败 exit 非零：

| # | 门禁项 | 证据命令 |
|---|---|---|
| G1 | Rust Core | `cargo test --workspace` 全绿（含 failed=0 skipped=0） |
| G2 | React tests | `vitest run src/App.test.tsx` 全绿 |
| G3 | 命令层 Windows failed=0 skipped=0 | workspace 测试输出断言（G1 同源） |
| G4 | Desktop E2E（真实 release 二进制） | `windows_novice_golden_path` + `windows_consumer_full_scenario`（LINGBI_EDGE_DRIVER） |
| G5 | clean-machine | E2E 即干净机器语义（本机真实二进制；无 Git PATH 证明 P10 已覆盖） |
| G6 | V1 migration | `migration_command_path.rs`（flutter-v1 + storyrepo-legacy） |
| G7 | 零界道种 | `real_book_acceptance`（LINGBI_REAL_BOOK 真实书仓） |
| G8 | crash recovery | `recovery_crash_failpoints.rs` + golden path 崩溃注入段 |
| G9 | external modification protection | 冲突守卫测试（conflict guidance） |
| G10 | workflow resume | `run_persistence.rs`（Node Recovery）+ `resume_workflow` |
| G11 | TokenLedger | `output_gate.rs` ledger 测试 |
| G12 | OutputGate repair | `output_gate.rs`（Validate→Repair→Validate） |
| G13 | 0 P0 data-loss | 上述崩溃/冲突/棘轮全绿 = P0 清单无违反（审计文档对照） |
| G14 | 契约校验 | `validate-book-v3.py --self-test` + `--corpus` |
| G15 | Skill 仅声明式 | 技能面板声明式测试（P5-5）+ 能力白名单测试（仅 git 二进制） |
| G16 | 安全基线 | 见 §2 审查项（脚本断言 CSP/能力文件/插件清单） |
| G17 | 依赖审计 | `cargo audit`（若可用）+ 审计报告落 release 目录 |

门禁清单文档 `docs/fusion/release-gate-0.9.md`：逐项映射到证据与验收口径。

### 2. Tauri 安全基线（P13-1）

审查项与现状：IPC allowlist = capability 仅 `core:default`（P5-5 能力测试锁定）✓；scoped paths 无 fs 插件（Rust 侧自有路径校验，deny 优先语义 P4/P5-4）✓；无 shell/system command（命令层进程白名单仅 git，P10）✓；**严格 CSP 缺失（`csp: null`）→ 修复**：前端零直连 HTTP（全走 IPC），CSP 可严格化；无远程 webview 特权（prod 仅本地资源，devUrl 仅 dev）✓；OS keyring 存 API Key（P5-5）✓；typed 校验（DTO + serde）✓；依赖审计见 G17。

### 3. Skill v1.0 仅声明式（P13-1 复核）

P5-5 已实现（声明式只读卡 + 无执行能力）；门禁 G15 锁定，无需新实现。

### 4. 版本与发布物（P13-2）

- 版本 0.1.0 → **0.9.0**（tauri.conf.json + package.json + src-tauri/Cargo.toml 同步）。
- bundle targets：nsis + **zip**（便携版）。
- `scripts/release-artifacts.sh`：build → 收集 NSIS/ZIP → 生成 **SHA256SUMS**（逐文件 sha256）+ **PROVENANCE**（源码 commit/构建环境/版本/未签名声明/门禁摘要）。
- 签名策略文档 `docs/fusion/release-signing-strategy.md`：未签名 → 0.9.x Internal Alpha/Closed Beta/Preview；**1.0.0 将 Windows 代码签名（Authenticode）纳入 Release Gate**（用户已拍板）。

## User Stories

1. 作为发布工程师，一条命令跑完全部门禁，输出逐项 PASS/FAIL 与证据路径，失败即阻断发布。
2. 作为用户，下载 0.9.0 内测包时可校验 SHA256SUMS 与 PROVENANCE（未签名阶段信任锚 = 校验和 + 构建来源声明）。

## Implementation Decisions

1. **门禁 = 脚本 + 文档双件**：脚本跑全部证据命令并汇总（测试输出解析断言 failed=0 skipped=0）；文档逐项映射验收口径（审计可对照）。
2. **CSP 严格化**：`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self' ipc: http://ipc.localhost`——CodeMirror 内联样式需 style 'unsafe-inline'；AI 请求全走 Rust 侧（connect-src 无需外域）；改后全量 E2E 回归验证不破坏功能。
3. **版本同步三处**：tauri.conf.json / package.json / src-tauri/Cargo.toml（+ NSIS 产物名自动随版本）。
4. **SHA256SUMS/PROVENANCE 由脚本生成**（每次发布重新生成，不提交产物）；PROVENANCE 含门禁 G1-G17 摘要。

## Testing Decisions

- 门禁脚本本身 = 已有测试的编排器（不新增产品测试）；CSP 变更用 E2E 全量验证。
- 发布物脚本在本次验收中真实执行一次（0.9.0 产物 + SHA256SUMS + PROVENANCE 证据落 ticket）。

## Out of Scope

- 真实代码签名证书购买与签名执行（1.0.0 Release Gate 项，本次只定策略）。
- 自动更新通道/发布平台集成。
