import assert from "node:assert/strict";
import test from "node:test";
import { artifactSummary, sourceHealthSummary, transformDashboard } from "../src/data/dashboard-transforms.js";

const complete = {
  claim_io_supply: true,
  claim_backing: true,
  active_backing_io: true,
  active_reward_io: true,
  nonredeemable_governance_io: true,
  protocol_reserve_io: true,
  claim_rate: true,
  total_io_supply: true,
};

test("production transform displays one coherent observation, not invented history", () => {
  const view = transformDashboard({
    configured: true,
    outdated: false,
    failures: [],
    status: { version: "0.1.0", schema_version: 3 },
    optional: {},
    dashboard: {
      protocol: {
        completeness: complete,
        total_io_supply_e8s: [200_000_000n],
        claim_rate: [{ backing_numerator_e8s: 50n, claim_denominator_e8s: 100n, available_liquid_e8s: 20n }],
      },
      index: [],
      canisters: [],
    },
  });
  assert.equal(view.charts.supply.length, 1);
  assert.equal(view.metrics.claimRate, "0.5");
  assert.equal(view.metrics.availableLiquid, "0");
});

test("incomplete protocol snapshot is surfaced and never displayed as zero", () => {
  const view = transformDashboard({
    configured: true,
    outdated: false,
    failures: [],
    status: null,
    optional: {},
    dashboard: { protocol: { completeness: { ...complete, total_io_supply: false } } },
  });
  assert.equal(view.warnings.some((warning) => warning.includes("Incomplete data")), true);
  assert.equal(view.metrics.claimSupply, "-");
});

test("source health surfaces stale, missing and retryable errors honestly", () => {
  const view = transformDashboard({
    configured: true,
    outdated: false,
    failures: [],
    status: null,
    optional: {},
    dashboard: {
      protocol: { completeness: complete },
      source_health: [
        { source: "sns-root", freshness: { Stale: null }, error: [] },
        { source: "sns-index", freshness: { Missing: null }, error: [] },
        { source: "protocol", freshness: { ErrorRetryable: null }, error: ["transport"] },
      ],
    },
  });
  assert.equal(view.warnings.some((warning) => warning.includes("sns-root stale")), true);
  assert.equal(view.warnings.some((warning) => warning.includes("sns-index missing")), true);
  assert.equal(view.warnings.some((warning) => warning.includes("protocol error retryable: transport")), true);
});

test("prelaunch no-config state stays explicit", () => {
  const summary = sourceHealthSummary({
    source: "sns-governance",
    freshness: { PrelaunchNotConfigured: null },
    error: [],
  });
  assert.match(summary, /Prelaunch Not Configured/);
});

test("module summary distinguishes mismatch from unavailable", () => {
  assert.equal(
    artifactSummary({ role: { Historian: null }, module_match: { Mismatch: null } }),
    "Historian: Mismatch",
  );
  assert.equal(
    artifactSummary({ role: { SnsRoot: null }, module_match: { Unavailable: null } }),
    "Sns Root: Unavailable",
  );
});
