import assert from "node:assert/strict";
import test from "node:test";
import { formatRatio, formatTimestampNanos, formatTokenE8s, variantLabel } from "../src/app/view-formatters.js";

test("missing values render as dash", () => {
  assert.equal(formatTokenE8s(undefined), "-");
  assert.equal(formatRatio(null), "-");
  assert.equal(formatTimestampNanos(undefined), "-");
});

test("formats token e8s and ratios", () => {
  assert.equal(formatTokenE8s(123_456_789n, "IO"), "1.2346 IO");
  assert.equal(formatRatio({ liquid_icp_per_io_e8s_numerator: 150n, liquid_icp_per_io_e8s_denominator: 100n }), "1.5");
});

test("variant labels are human readable", () => {
  assert.equal(variantLabel({ FailedRetryable: null }), "Failed Retryable");
});
