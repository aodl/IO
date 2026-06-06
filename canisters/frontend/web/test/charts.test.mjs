import assert from "node:assert/strict";
import test from "node:test";
import { renderChart } from "../src/ui/charts.js";

function element(name) {
  return {
    name,
    children: [],
    className: "",
    textContent: "",
    attrs: {},
    replaceChildren() { this.children = []; },
    append(child) { this.children.push(child); },
    setAttribute(key, value) { this.attrs[key] = value; },
  };
}

test("chart renders empty state without enough live history", () => {
  globalThis.document = { createElement: element, createElementNS: (_ns, name) => element(name) };
  const container = element("div");
  renderChart(container, [{ label: "latest", value: 1 }]);
  assert.equal(container.children[0].className, "chart-empty");
});

test("chart renders svg polyline for valid series", () => {
  globalThis.document = { createElement: element, createElementNS: (_ns, name) => element(name) };
  const container = element("div");
  renderChart(container, [{ value: 1 }, { value: 2 }, { value: 3 }]);
  assert.equal(container.children[0].name, "svg");
  assert.equal(container.children[0].children[0].name, "polyline");
});
