import assert from "node:assert/strict";
import test from "node:test";
import { artifactSummary, streamSummary, transformDashboard } from "../src/data/dashboard-transforms.js";

const observedCompleteness = {
  liquid_icp_reserve: { Observed: null },
  non_redeemable_governance_io: { Observed: null },
  protocol_reserve_io: { Observed: null },
  redeemable_io_supply: { Observed: null },
  redemption_rate: { Observed: null },
  total_io_supply: { Observed: null },
  two_year_nns_principal: { Observed: null },
};

test("production transform does not invent fake chart history", () => {
  const view = transformDashboard({
    configured: true,
    outdated: false,
    failures: [],
    status: { version: "0.1.0", schema_version: 1 },
    optional: {},
    dashboard: {
      protocol: { completeness: observedCompleteness, total_io_supply_e8s: [200_000_000n] },
      redemption_rate: [],
      index_health: [],
      release_artifacts: [],
    },
  });
  assert.equal(view.charts.supply.length, 1);
  assert.equal(view.charts.rate.length, 0);
});

test("incomplete protocol snapshot is surfaced", () => {
  const view = transformDashboard({
    configured: true,
    outdated: false,
    failures: [],
    status: null,
    optional: {},
    dashboard: { protocol: { completeness: { ...observedCompleteness, total_io_supply: { Missing: null } } }, index_health: [], release_artifacts: [] },
  });
  assert.equal(view.warnings.some((warning) => warning.includes("Incomplete data")), true);
});

test("summaries use safe text values", () => {
  assert.match(streamSummary({ record_id: "<script>", stream_kind: { JupiterFaucet: null }, amount_e8s: 1n }), /<script>/);
  assert.equal(artifactSummary({ canister_name: "frontend", status: { Matching: null } }), "frontend: Matching");
});
