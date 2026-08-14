import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { cleanGeneratedDirectory } from "../clean-generated.mjs";

test("frontend generation removes every obsolete bundle before building", () => {
  const generated = mkdtempSync(join(tmpdir(), "io-frontend-generated-"));
  try {
    writeFileSync(join(generated, "app.stale000000.js"), "stale");
    writeFileSync(join(generated, "frontend-bundle.json"), "stale");
    mkdirSync(join(generated, "obsolete"));
    writeFileSync(join(generated, "obsolete", "app.other.js"), "stale");

    cleanGeneratedDirectory(generated);

    assert.deepEqual(readdirSync(generated), []);
  } finally {
    rmSync(generated, { recursive: true, force: true });
  }
});
