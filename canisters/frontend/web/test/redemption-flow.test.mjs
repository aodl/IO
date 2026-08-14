import test from "node:test";
import assert from "node:assert/strict";
import { canonicalSubaccount, prepareRedemption, progressLabel } from "../src/app/redemption.js";

test("redemption uses wallet-selected canonical subaccount, exact nonce and allowance", async () => {
  const selected = new Uint8Array(32).fill(7);
  const ledger = {
    icrc1_fee: async () => 10_000n,
    icrc2_allowance: async () => ({ allowance: 5n, expires_at: [] }),
  };
  const stream = {
    get_caller_redemption_state: async () => ({ Ok: { next_nonce: 9n, last_request_fingerprint: [], last_result: [] } }),
  };
  const request = await prepareRedemption({
    ledger,
    stream,
    owner: "owner",
    streamCanister: "stream",
    selectedSubaccount: selected,
    ioAmountE8s: 1_000_000n,
    minIcpOutE8s: 900_000n,
    maxIcpFeeE8s: 10_000n,
    nowNanos: 1_000_000_000n,
  });
  assert.deepEqual(request.redeem.from_subaccount[0], selected);
  assert.equal(request.redeem.nonce, 9n);
  assert.equal(request.approval.amount, 1_010_000n);
  assert.deepEqual(request.approval.expected_allowance, [5n]);
  assert.equal(progressLabel({ IoInReserve: null }), "IO in reserve");
});

test("arbitrary text and malformed wallet subaccounts are rejected", () => {
  assert.throws(() => canonicalSubaccount("00".repeat(32)));
  assert.throws(() => canonicalSubaccount(new Uint8Array(31)));
});
