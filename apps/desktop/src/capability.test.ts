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
});
