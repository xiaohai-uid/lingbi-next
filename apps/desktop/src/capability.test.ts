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

  it("command layer has no process execution surface", () => {
    // P5-5 能力审查证据：声明式 Skill 走命令层，命令层无进程/执行调用
    const source = fs.readFileSync(commandsLibPath, "utf8");
    expect(source).not.toContain("std::process");
    expect(source).not.toContain("Command::new");
  });
});
