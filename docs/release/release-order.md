# LingBi Release Order & Platform Positioning

Date: 2026-08-07

## 1. Release order (Task 24)

The product advances strictly in this order. Never skip a stage; never
substitute a cheaper signal for the required one.

```text
Rust Correctness
        ↓
Windows CI
        ↓
Windows Real Desktop E2E
        ↓
Windows Clean Machine
        ↓
Novice UX
        ↓
Flutter FFI
        ↓
Signed Installer
        ↓
Public Beta
        ↓
Cloud / Billing
        ↓
Commercial GA
```

Not:

```text
Rust tests
↓
Linux Tauri build
↓
继续堆功能
```

The release gate definitions live in `docs/qa/release-gates.md`.

## 2. Platform positioning (Task 25)

```text
Windows = P0 正式产品平台      (product gate, signed installer, clean machine)
macOS   = P1 兼容平台          (build + unit + desktop smoke before any
                               public macOS release; then signing +
                               notarization + clean-machine acceptance)
Linux   = Core/开发兼容平台    (portability job only; not a consumer
                               release target)
```

Windows Product Gate 不通过 → 禁止 Public Beta，无论其他平台多绿。

## 3. Public Beta final gate (Task 26)

The docs may only be changed to `Public Beta` after ALL of the following:

```text
Windows Core CI PASS
Windows Tauri Build PASS
Windows Desktop E2E PASS          (passed > 0, failed = 0, skipped = 0)
Windows FFI PASS                  (FFI passed > 0, failed = 0, skipped = 0)
Windows clean-machine PASS

项目创建 PASS
多章节 PASS
中文路径 PASS
人工保存 PASS
真正 Streaming PASS
真正 Cancel PASS
Candidate PASS
Mutation crash recovery PASS
关闭重开 PASS
外部编辑保护 PASS
DOCX/MD/TXT export PASS

Flutter ZIP safety PASS
Flutter fallback collision PASS

0 P0
0 known data-loss bug
```

Code signing: 正式 Public Beta 安装包必须完成 Windows code signing
(`docs/security/signing-policy.md`). 没有代码签名 → 只能
Internal Alpha / Closed Testing。不要给普通用户发一个会触发明显安全
警告的“正式 Public Beta”安装包。

## 4. Temporarily forbidden work (Task 27)

Until Tasks 1–26 are complete, the following are NOT the bottleneck and
are forbidden:

```text
支付接入 / 真实 Billing Provider
Cloud manuscript sync
多人协作
高级 Agent / Agent Shell
Skill Marketplace 扩展
新的 Vector/RAG 系统
市场情报
新的 Flutter FFI 模块
复杂模型路由
macOS 正式发布
Linux 正式发布
```

## 5. The one user path that matters

Before reporting test counts, prove this path end to end on a clean
Windows machine:

```text
官网下载安装 LingBi → 双击安装 → 启动 → 创建小说（只输作品名）→
不配置 AI 也能写 → 想用 AI 时只选服务 + 粘贴 API Key → 连接成功 →
生成时实时看到文字 → 随时取消 → 满意后采纳 → 正文安全写入 → 关机 →
第二天打开 → 小说完全存在 → AI 配置仍然存在 → 继续写 → 正常导出
```

任何一步失败 → `Windows Novice Product Gate = FAIL`。
