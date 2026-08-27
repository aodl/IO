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
  assert.equal(
    formatRatio({ backing_numerator_e8s: 150n, claim_denominator_e8s: 100n }),
    "1.5",
  );
});

test("variant labels are human readable", () => {
  assert.equal(variantLabel({ FailedRetryable: null }), "Failed Retryable");
});
