import assert from "node:assert/strict";
import test from "node:test";
import { loadHistorianDashboard, missingRequiredMethods, REQUIRED_HISTORIAN_METHODS } from "../src/data/historian-loaders.js";

test("required historian methods are fixed to production read model", () => {
  assert.deepEqual(REQUIRED_HISTORIAN_METHODS, ["get_dashboard_state", "get_public_status"]);
});

test("historian not configured returns explicit unavailable state", async () => {
  const result = await loadHistorianDashboard(null, { historianCanisterId: "" });
  assert.equal(result.configured, false);
  assert.match(result.failures[0].error, /not configured/);
});

test("all required methods missing marks outdated deployment", async () => {
  const actor = {};
  assert.deepEqual(missingRequiredMethods(actor), REQUIRED_HISTORIAN_METHODS);
  const result = await loadHistorianDashboard(actor, { historianCanisterId: "aaaaa-aa" });
  assert.equal(result.outdated, true);
});

test("required dashboard queries return canonical source state", async () => {
  const actor = {
    get_dashboard_state: async () => ({ protocol: {}, source_health: [], index: [] }),
    get_public_status: async () => ({ version: "0.1.0", schema_version: 3 }),
  };
  const result = await loadHistorianDashboard(actor, { historianCanisterId: "aaaaa-aa" });
  assert.equal(result.dashboard.protocol !== undefined, true);
  assert.deepEqual(result.optional, {});
  assert.equal(result.failures.length, 0);
});
