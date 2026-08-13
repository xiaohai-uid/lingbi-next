# 归档 lingbi 执行手册（P14 #31）— 1.0.0 验证后执行

> 前置门禁（全 PASS 才可执行，见 `docs/fusion/spec-archive-v3.md` §3）：
> lingbi-next 1.0.0 签名发布 + release-gate.sh 0 FAIL + 零界道种验收绿 + Windows E2E 2/2 绿。
> 本手册操作对 lingbi 仓库（xiaohai-uid/lingbi）为只读外的唯一次写操作——执行前确认门禁。

## 步骤

```bash
# 1) 应用 README ARCHIVED 补丁（源文件：lingbi-next docs/fusion/archive/lingbi-README-ARCHIVED.md）
#    lingbi README 顶部加归档横幅；「当前发布状态」节替换为指向 lingbi-next
#    的交接说明（保留 v1 能力证据与开发验证节原样）。

# 2) 提交到 lingbi main
cd <lingbi-checkout>
git checkout main && git pull
#    （将 lingbi-README-ARCHIVED.md 内容按 §1 合并进 README.md）
git add README.md && git commit -m "docs: ARCHIVED — 继任为 lingbi-next（迁移说明见其 spec-migration-v3）"
git push

# 3) GitHub Archive（仓库只读）
gh api repos/xiaohai-uid/lingbi -X PATCH -f archived=true

# 4) lingbi-next README 状态卡更新：标注唯一正式仓库 + lingbi 已归档链接
#    （README.md 项目状态节）

# 5) 回滚说明（如需）：gh api repos/xiaohai-uid/lingbi -X PATCH -f archived=false
#    （README 变更保留在 git 历史，可 revert）
```

## 执行后验证

- [ ] `gh repo view xiaohai-uid/lingbi --json isArchived` = true
- [ ] lingbi README 顶部可见 ARCHIVED 横幅与 lingbi-next 链接
- [ ] lingbi-next README 标注唯一正式仓库

## 时间线记录

| 日期 | 事件 |
|---|---|
| 2026-08-08 | 准备包完成（spec + README 补丁 + 本手册）；0.9.0 已发布验证（门禁 PASS=17），1.0.0 阻塞于外部签名证书 → 归档动作挂门禁待命 |
