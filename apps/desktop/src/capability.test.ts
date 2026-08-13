import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

function resolveFromCwd(relative: string): string {
  const candidates = [
    path.resolve(process.cwd(), relative),
    path.resolve(process.cwd(), `desktop/${relative}`),
    path.resolve(process.cwd(), `apps/desktop/${relative}`),
  ];
  const found = candidates.find((candidate) => fs.existsSync(candidate));
  if (!found) throw new Error(`config not found: ${relative}`);
  return found;
}

const capabilitiesPath = resolveFromCwd("src-tauri/capabilities/default.json");
const tauriConfigPath = resolveFromCwd("src-tauri/tauri.conf.json");
const desktopCargoPath = resolveFromCwd("src-tauri/Cargo.toml");
const commandsLibPath = resolveFromCwd("../../crates/lingbi-commands/src/lib.rs");

describe("Tauri permission boundary", () => {
  it("does not grant shell or process permissions", () => {
    const capabilities = JSON.parse(fs.readFileSync(capabilitiesPath, "utf8"));
    const serialized = JSON.stringify(capabilities);

    expect(serialized).not.toContain("shell:");
    expect(serialized).not.toContain("process:");
    expect(serialized).not.toContain("fs:write");
    expect(capabilities.permissions).toEqual(["core:default"]);
  });

  it("does not enable arbitrary executable plugins in Tauri config", () => {
    const config = JSON.parse(fs.readFileSync(tauriConfigPath, "utf8"));
    expect(config.plugins?.shell?.open).toBeUndefined();
    expect(config.plugins?.process).toBeUndefined();
  });

  it("declares no tauri plugin crates (fs/shell/process surface absent)", () => {
    // P5-5 能力审查证据：桌面依赖仅 tauri 核心，无 plugin-fs/shell/process
    const cargo = fs.readFileSync(desktopCargoPath, "utf8");
    expect(cargo).not.toContain("tauri-plugin-fs");
    expect(cargo).not.toContain("tauri-plugin-shell");
    expect(cargo).not.toContain("tauri-plugin-process");
  });

  it("command layer has no process execution surface (git whitelist only)", () => {
    // P5-5 能力审查证据：命令层无任意进程/执行调用；P10 #27 可选 Git 快照
    // 为唯一窄例外——仅 `git` 二进制 + 固定单值参数（无 shell、无动态命令）
    const source = fs.readFileSync(commandsLibPath, "utf8");
    // 全部 Command::new 必须是 git 二进制（白名单：固定单值参数、无 shell、
    // 无动态命令）；除 git 外不允许任何可执行文件（含测试模块在内）
    const allCalls = (source.match(/Command::new\(/g) ?? []).length;
    const gitCalls = (source.match(/Command::new\("git"\)/g) ?? []).length;
    expect(allCalls).toBeGreaterThanOrEqual(4);
    expect(allCalls).toBe(gitCalls);
    expect(source).not.toContain('Command::new("sh"');
    expect(source).not.toContain('Command::new("cmd"');
    expect(source).not.toContain('Command::new("powershell"');
  });
});
