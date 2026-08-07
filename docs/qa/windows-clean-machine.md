# Windows Clean-Machine Acceptance (Task 17) & Installer Tests (Task 18)

Date: 2026-08-07

## Why a second layer exists

GitHub's Windows runner is CI, not a normal person's computer. Consumer
acceptance happens on a **clean Windows 11 machine**:

- clean Windows 11 VM, **or** a dedicated Windows 11 self-hosted runner
- the machine must have NO: Rust, Cargo, Node, pnpm, Flutter, Go,
  Visual Studio, or any dev tooling
- the only artifact the user receives is the LingBi installer

## The acceptance path (0 次命令行)

```text
安装
启动
创建作品（只输作品名，自动 Documents/LingBi/<作品名>）
输入中文
配置 AI（选服务 + 粘贴 API Key）
生成
采纳
关闭
重启
导出 DOCX/MD/TXT
卸载
重新安装
重新打开原项目（章节/正文/AI 配置都在）
```

Whole process requirements:

```text
0 次命令行
0 个环境变量
0 次手工复制 DLL
0 次编辑 JSON/YAML
```

Any step fails → `Windows Novice Product Gate = FAIL`.

## Installer test matrix (Task 18)

The installer is NSIS (`installMode: currentUser` — no administrator
rights, per-user install; see `apps/desktop/src-tauri/tauri.conf.json`).

```text
首次安装        → 安装完成，可从开始菜单/桌面启动
重复安装        → 提示已安装，选择修复/覆盖不破坏数据
升级安装        → 旧版本升级后数据完整
卸载            → 应用移除
卸载后用户小说仍保留
重新安装        → 可打开旧小说
```

**绝对禁止：卸载 LingBi → 删除用户小说。**

Data-safety design that makes this hold:

```text
用户小说      → {Documents}/LingBi/<作品名>/   (user data, never touched
                                               by the installer)
应用二进制    → per-user app install dir        (uninstaller scope)
AI 配置(Key) → OS keyring / Credential Manager (survives reinstall)
最近项目      → app data dir                    (survives reinstall)
```

The NSIS uninstaller only removes what it installed. If an acceptance
run shows user novels deleted on uninstall, the gate is FAIL and the
installer must be fixed before any release.

## How to run acceptance

1. Prepare the clean machine (VM snapshot or dedicated runner).
2. Copy the signed/unsigned installer to it (network share or USB).
3. Run `tool/windows/clean_machine_acceptance.ps1 -Installer <path>` on
   the machine (double-click / run from PowerShell — the script itself is
   the evidence log, not a dev tool requirement).
4. The script records each step with a PASS/FAIL line and writes
   `acceptance-evidence.txt`; it verifies data survival after uninstall
   by checking `{Documents}/LingBi` before/after.

Evidence label for this layer: `WINDOWS_CLEAN_MACHINE`.
Never claim clean-machine PASS from a GitHub runner run.

## Current status

```text
NOT YET RUN — requires a clean Windows 11 VM / self-hosted runner.
Windows Product Gate remains FAIL until this layer passes.
```
