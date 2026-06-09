import assert from "node:assert/strict";
import test from "node:test";
import { artifactSummary, sourceHealthSummary, streamSummary, transformDashboard } from "../src/data/dashboard-transforms.js";

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

test("source health warnings surface stale incomplete and missing observations", () => {
  const view = transformDashboard({
    configured: true,
    outdated: false,
    failures: [],
    status: null,
    optional: {},
    dashboard: {
      protocol: { completeness: observedCompleteness },
      index_health: [],
      release_artifacts: [],
      source_health: [
        { source_id: "release-artifacts", freshness: { Stale: null }, summary: "observed artifact manifest is stale" },
        { source_id: "protocol-snapshot", freshness: { Incomplete: null }, summary: "missing fields are not zero protocol value" },
        { source_id: "icp-index-health", freshness: { Missing: null }, summary: "index health missing" },
      ],
    },
  });
  assert.equal(view.warnings.some((warning) => warning.includes("release-artifacts stale")), true);
  assert.equal(view.warnings.some((warning) => warning.includes("protocol-snapshot incomplete")), true);
  assert.equal(view.warnings.some((warning) => warning.includes("icp-index-health missing")), true);
});

test("source health shows sns not launched without treating it as fake protocol data", () => {
  const view = transformDashboard({
    configured: true,
    outdated: false,
    failures: [],
    status: null,
    optional: {},
    dashboard: {
      protocol: { completeness: observedCompleteness },
      index_health: [],
      release_artifacts: [],
      source_health: [
        {
          source_id: "sns-governance-freshness",
          freshness: { PrelaunchNotApplicable: null },
          summary: "SNS governance is not launched; missing SNS observations are not an error",
        },
      ],
    },
  });
  assert.equal(view.warnings.some((warning) => warning.includes("sns-governance-freshness prelaunch")), true);
  assert.equal(view.metrics.redemptionRate, "-");
  assert.equal(view.charts.rate.length, 0);
});

test("source health can state value-moving canisters are not deployed", () => {
  const summary = sourceHealthSummary({
    source_id: "future-io-sns-index-health",
    freshness: { PrelaunchNotApplicable: null },
    summary: "value-moving canisters are not deployed/not allocated",
  });
  assert.match(summary, /not deployed\/not allocated/);
});

test("summaries use safe text values", () => {
  assert.match(streamSummary({ record_id: "<script>", stream_kind: { JupiterFaucet: null }, amount_e8s: 1n }), /<script>/);
  assert.equal(artifactSummary({ canister_name: "frontend", status: { Matching: null } }), "frontend: Matching");
});
