import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

const routes = [
  "app/page.tsx",
  "app/download/page.tsx",
  "app/pricing/page.tsx",
  "app/login/page.tsx",
  "app/account/page.tsx",
  "app/account/subscription/page.tsx",
  "app/releases/page.tsx",
  "app/privacy/page.tsx",
  "app/terms/page.tsx",
];

test("required website routes exist", () => {
  for (const route of routes) {
    assert.equal(
      fs.existsSync(path.resolve(process.cwd(), route)),
      true,
      `missing route: ${route}`,
    );
  }
});
