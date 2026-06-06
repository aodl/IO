import assert from "node:assert/strict";
import test from "node:test";
import { shouldUseFallbackOnly } from "../src/ui/hero-sphere.js";

test("reduced motion uses fallback", () => {
  const win = {
    navigator: { hardwareConcurrency: 8 },
    matchMedia(query) { return { matches: query.includes("prefers-reduced-motion") }; },
  };
  assert.equal(shouldUseFallbackOnly(win), true);
});

test("desktop capable device can use webgl", () => {
  const win = {
    navigator: { hardwareConcurrency: 8 },
    matchMedia() { return { matches: false }; },
  };
  assert.equal(shouldUseFallbackOnly(win), false);
});
