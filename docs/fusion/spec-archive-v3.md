# P14 归档 lingbi 规格（#31）— 1.0.0 验证后执行

> 日期：2026-08-08
> 上游：#30 P13 发布门禁（0.9.x 先行，1.0.0 Authenticode 签名入 Release Gate）、P0 冻结架构（lingbi maintenance-frozen → 发布验证后才 ARCHIVED）

## Problem Statement

lingbi（xiaohai-uid/lingbi，v1 Flutter，1.0.1）已被 lingbi-next（融合重建，Book V3）取代。冻结计划要求：**发布验证后才 ARCHIVED**，且 **1.0.0 代码签名纳入 Release Gate**。当前 lingbi-next 处于 0.9.0 内测（P13 已发布验证，门禁 PASS=17），1.0.0 阻塞于外部签名证书（BLOCKED_EXTERNAL）。归档动作不可逆（GitHub Archive 后仓库只读），不得在 1.0.0 验证前执行。

## Solution

### 1. 本批次（0.9.0 阶段）交付：归档准备包

- 规格本文件 + `docs/fusion/archive/` 归档包：
  - `lingbi-README-ARCHIVED.md`：lingbi README 的 ARCHIVED 替换文（顶部横幅 + 指向 lingbi-next + 交接说明），1.0.0 时原样应用
  - `runbook-archive-lingbi.md`：执行步骤（应用 README 补丁 → 提交 lingbi main → `gh api` archived:true → 更新 lingbi-next README 交接指针）
- 1.0.0 门禁清单（执行前必须全 PASS）：见 §3

### 2. 1.0.0 验证后执行（本批次不做）

按 runbook 顺序：README ARCHIVED → GitHub Archive（`gh api repos/xiaohai-uid/lingbi -X PATCH -f archived=true`）→ lingbi-next 成为唯一正式仓库（README 状态卡更新）。

## 1.0.0 门禁清单（归档执行前置）

- [ ] lingbi-next 1.0.0 发布（Authenticode 签名，G18 签名验证入 gate）
- [ ] `scripts/release-gate.sh` 全项 PASS（0 FAIL）
- [ ] 真实《零界道种》验收仍绿（P12 harness）
- [ ] Windows E2E 2/2 仍绿（P11）
- [ ] lingbi v1 用户交接路径确认：README 指向 lingbi-next 发布页 + 迁移说明（spec-migration-v3.md）

## User Stories

1. 作为维护者，1.0.0 验证后一键归档 lingbi：README 改 ARCHIVED、GitHub Archive、交接指针就位——此后只有一个正式仓库。
2. 作为 v1 用户，打开 lingbi 仓库立即看到「已归档 → 迁移到 lingbi-next」说明与迁移路径。

## Implementation Decisions

1. 归档包在 lingbi-next 仓库维护（`docs/fusion/archive/`），应用时复制进 lingbi 仓库——单一事实源。
2. GitHub Archive 用 `gh api -X PATCH -f archived=true`（仓库设置等价操作），runbook 附回滚说明（unarchive 可行）。
3. 本批次不改 lingbi 仓库任何内容（0.9.0 内测阶段提前归档会让 v1 用户失去指向——违反冻结计划的发布验证前置）。

## Out of Scope

- 1.0.0 签名证书采购与签名执行（BLOCKED_EXTERNAL，用户侧）。
- lingbi 代码仓库内容迁移（v1 作为功能规格基准保留在 git 历史）。
