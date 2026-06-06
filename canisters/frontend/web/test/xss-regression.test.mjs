import assert from "node:assert/strict";
import test from "node:test";
import { setText } from "../src/dom-helpers.js";

test("data text is not rendered as html", () => {
  const node = { innerHTML: "" };
  setText(node, "<script>alert(1)</script>");
  assert.equal(node.textContent, "<script>alert(1)</script>");
  assert.equal(node.innerHTML, "");
});
