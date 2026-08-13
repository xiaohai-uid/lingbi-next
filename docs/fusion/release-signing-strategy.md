# LingBi Next 发布签名策略（P13 #56）

> 拍板（2026-08-08 用户）：无签名先 0.9.x 内测，**1.0.0 代码签名纳入 Release Gate**。

## 阶段与信任锚

| 阶段 | 版本 | 签名 | 发布物 | 信任锚 |
|---|---|---|---|---|
| Internal Alpha | 0.9.x | 无 | NSIS + 便携 ZIP | SHA256SUMS + PROVENANCE（构建来源声明） |
| Closed Beta | 0.9.x | 无 | 同上 | 同上 |
| Preview | 0.9.x | 无 | 同上 | 同上 |
| **1.0.0** | 1.0.0 | **Authenticode（EV/OV 代码签名证书）** | 同上 + 签名 | 证书链 + 时间戳 |

## 0.9.x 未签名阶段的约束

- PROVENANCE 必须如实声明 `signed: false`（`scripts/release-artifacts.sh` 生成）。
- 发布物必须附 SHA256SUMS；分发页/公告附校验和与源码 commit。
- Windows SmartScreen 未签名警告为预期行为——内测渠道向用户明示。

## 1.0.0 Release Gate 签名项（发布时执行）

1. 采购 Windows 代码签名证书（EV 优先：硬件令牌 + 立即信任）。
2. `signtool sign /fd SHA256 /tr <RFC3161> /td SHA256 <exe/zip>`（时间戳必须）。
3. 签名后重算 SHA256SUMS（签名改变哈希）。
4. PROVENANCE 更新 `signed: true` + 证书指纹。
5. 门禁新增 G18 签名验证（`signtool verify /pa /v` 全产物）。

## 当前状态

- 版本 0.9.0（三处同步：tauri.conf.json / package.json / src-tauri/Cargo.toml）。
- 产物：NSIS（currentUser 安装）+ zip（便携）。
- 门禁：`scripts/release-gate.sh`（G1-G17，见 `docs/fusion/release-gate-0.9.md`）。
