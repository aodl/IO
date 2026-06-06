import assert from "node:assert/strict";
import test from "node:test";
import { replaceList, setText } from "../src/dom-helpers.js";

test("setText uses textContent and replaces missing values", () => {
  const node = {};
  setText(node, undefined);
  assert.equal(node.textContent, "-");
  setText(node, "<b>unsafe</b>");
  assert.equal(node.textContent, "<b>unsafe</b>");
});

test("replaceList appends text-only list items", () => {
  const created = [];
  globalThis.document = {
    createElement(name) {
      const node = { name, textContent: "" };
      created.push(node);
      return node;
    },
  };
  const list = { children: [], replaceChildren() { this.children = []; }, append(node) { this.children.push(node); } };
  replaceList(list, ["<img>"], (value) => value);
  assert.equal(list.children[0].textContent, "<img>");
});
