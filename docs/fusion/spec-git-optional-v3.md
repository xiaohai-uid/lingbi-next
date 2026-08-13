# P10 Git 降级为可选规格（#27）— 应用内部执行

> 日期：2026-08-08
> 上游：#4 Book V3（冻结约束「Git 可选」）、P3 版本快照（.lingbi/versions 已提供内部版本化，无需 git 兜底）

## Problem Statement

章节数据安全不得依赖系统 Git——干净 Windows（无 Git）必须全功能正常。当前代码库**运行时零 git 调用**（已审计：仅 `report.rs` 引擎字节锁定的报告文本提到 git 建议，非功能依赖），但缺两项证据/能力：①「无 Git 全功能正常」没有显式证明（屏蔽 git 的 PATH 下跑真实 E2E）；②「Git 集成可选且应用内执行」没有任何集成点（用户无处启用，也无优雅降级语义）。

## Solution

1. **无 Git 依赖证明**：全仓运行时 git 调用审计（grep 证据）+ 屏蔽 git 的 PATH 下真实 Windows E2E 全绿（黄金路径 + P9 场景）。
2. **可选 Git 快照集成（应用内执行）**：`lingbi-commands::git_snapshot(root)`——检测 git 可用性（`git --version`）→ 书仓为 git 仓库（`git rev-parse --is-inside-work-tree`）→ `git add -A` + `git commit -m "lingbi: {book} rev {revision}"`（cwd = 书仓根，参数单值无 shell，防注入）；非仓库/无 git → 优雅降级 DTO（不报错、不阻断）。Tauri 壳 `book_git_snapshot` + 高级功能区「Git 快照」按钮（结果消息展示）。

## User Stories

1. 作为干净 Windows 用户（无 Git），我打开/创建/写作/保存/重开/导出/运行工作流全部正常——产品不因缺 git 报任何错。
2. 作为启用 Git 的用户，我在书仓内点「Git 快照」即可提交当前进度，不用碰命令行。
3. 作为未建 git 仓库的用户，点「Git 快照」得到明确提示（非 git 仓库），应用不受影响。
4. 作为无 git 环境的用户，点「Git 快照」得到「Git 不可用」提示，应用不受影响。

## Implementation Decisions

### 1. git_snapshot 命令（lingbi-commands）

```rust
pub fn git_snapshot(root: &str) -> Result<GitSnapshotDto, CommandError>
// GitSnapshotDto { available: bool, committed: bool, message: String }
```

- available：`git --version` 退出码 0（std::process::Command，无 shell）。
- 非仓库：`git rev-parse --is-inside-work-tree` 非 0 → committed=false + 「书仓非 git 仓库（可选集成未启用）」。
- 仓库：`git add -A` → `git commit -m "lingbi: {book_name} rev {state_revision}"` → committed=true + commit 短哈希（`git rev-parse --short HEAD`）。提交失败（无可提交变更）→ committed=false + 消息透传 git 输出（非错误）。
- 全部错误映射为 DTO 消息，绝不返回 Err（可选能力，永不阻断）。

### 2. 壳 + UI

- `book_cmds::book_git_snapshot`（薄壳）+ 注册。
- 高级功能 details 内「Git 快照」按钮 + 结果消息（desktop.ts `gitSnapshot()` wrapper；store 动作；App.test mock 断言降级文案）。

### 3. 证明

- 审计：`git grep` 运行时 git 调用 = 零（report.rs 文本除外，字节锁定引擎输出，注释说明）。
- 证明：`PATH` 剔除 git 目录后跑 `cargo test -p lingbi-e2e-desktop-real`（tauri-driver 经 env 传播到子进程）——全绿 = 无 Git 全功能正常。

## Testing Decisions

- **lingbi-commands 单测**：临时 git 仓库（git init + config user）→ snapshot → `git log` 断言提交存在；非仓库目录 → committed=false 降级；不可用（git 缺失）→ available=false（条件跳过——CI 环境有 git）。
- **App.test.tsx**：mock gitSnapshot 返回「非 git 仓库」→ 按钮 + 消息断言。
- **真实 E2E**：屏蔽 git 的 PATH 下 windows_novice_golden_path 全绿（无 Git 证明）。

## Out of Scope

- git 分支/历史浏览 UI → 后续
- 自动快照（每次结算自动 commit）→ 后续（默认关闭，作者显式触发）
- 跨平台 git 配置（user.name/email 缺失时提交失败 → 如实降级）

## Further Notes

- 冻结约束「Git 可选」落点：不依赖（证明）+ 可选（快照命令）+ 应用内执行（壳命令 + UI 按钮，用户不碰 CLI）。
- P3 `.lingbi/versions` Final Revision 快照为内部版本化主力，git 为可选增量。
