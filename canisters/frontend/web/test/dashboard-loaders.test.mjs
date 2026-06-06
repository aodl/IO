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

test("partial optional query failure preserves required data", async () => {
  const actor = {
    get_dashboard_state: async () => ({ protocol: {}, release_artifacts: [], index_health: [] }),
    get_public_status: async () => ({ version: "0.1.0", schema_version: 1 }),
    list_streams: async () => ({ records: [{ record_id: "stream:1" }] }),
    list_redemptions: async () => { throw new Error("redemptions unavailable"); },
    list_rewards: async () => ({ records: [] }),
  };
  const result = await loadHistorianDashboard(actor, { historianCanisterId: "aaaaa-aa" });
  assert.equal(result.dashboard.protocol !== undefined, true);
  assert.equal(result.optional.streams.records.length, 1);
  assert.equal(result.failures.some((failure) => failure.label === "redemptions"), true);
});
