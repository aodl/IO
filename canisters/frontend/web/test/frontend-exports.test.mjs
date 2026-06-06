import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

test("browser code does not import historian debug declarations or value-moving canisters", () => {
  const files = [
    "canisters/frontend/web/src/app/agent.js",
    "canisters/frontend/web/src/data/historian-loaders.js",
    "canisters/frontend/web/src/main.js",
  ];
  const text = files.map((file) => readFileSync(file, "utf8")).join("\n");
  assert.equal(text.includes("io_historian_debug"), false);
  assert.equal(text.includes("io_stream_manager"), false);
  assert.equal(text.includes("io_nns_neuron_manager"), false);
});
