import test from "node:test";
import assert from "node:assert/strict";
import {
  canonicalSubaccount,
  consentAndSubmitRedemption,
  prepareRedemption,
  progressLabel,
  redemptionConsentTerms,
} from "../src/app/redemption.js";

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
  assert.equal(progressLabel({ Pending: null }), "Pending external proof or retry");
});

test("arbitrary text and malformed wallet subaccounts are rejected", () => {
  assert.throws(() => canonicalSubaccount("00".repeat(32)));
  assert.throws(() => canonicalSubaccount(new Uint8Array(31)));
});

test("exact flow orders queries, consent, approval and redemption", async () => {
  const order = [];
  const selected = new Uint8Array(32).fill(4);
  const ledger = {
    icrc1_fee: async () => { order.push("fee"); return 10_000n; },
    icrc2_allowance: async () => { order.push("allowance"); return { allowance: 3n, expires_at: [] }; },
    icrc2_approve: async () => { order.push("approve"); return { Ok: 8n }; },
  };
  const stream = {
    get_caller_redemption_state: async () => {
      order.push("nonce");
      return { Ok: { next_nonce: 2n, last_request_fingerprint: [], last_result: [] } };
    },
    redeem: async () => { order.push("redeem"); return { Ok: { Pending: null } }; },
  };
  const request = await prepareRedemption({
    ledger,
    stream,
    owner: "owner",
    streamCanister: "stream",
    selectedSubaccount: selected,
    ioAmountE8s: 100_000n,
    minIcpOutE8s: 50_000n,
    maxIcpFeeE8s: 10_000n,
    nowNanos: 1_000_000_000n,
  });
  const session = {
    network: "local",
    requestApprovalConsent: async (terms) => {
      order.push("consent");
      assert.equal(terms.exactAllowanceE8s, 110_000n);
      assert.equal(terms.currentIoFeeE8s, 10_000n);
      assert.equal(terms.expectedExistingAllowanceE8s, 3n);
      assert.equal(terms.minimumIcpOutputE8s, 50_000n);
      assert.equal(terms.maximumIcpFeeE8s, 10_000n);
      assert.deepEqual(terms.spender, request.approval.spender);
      assert.deepEqual(terms.selectedSourceSubaccount, request.redeem.from_subaccount[0]);
      assert.equal(terms.network, "local");
      return true;
    },
  };
  await consentAndSubmitRedemption({ ledger, stream, request, session });
  assert.deepEqual(order.slice(-3), ["consent", "approve", "redeem"]);
  assert.ok(order.indexOf("fee") < order.indexOf("consent"));
  assert.ok(order.indexOf("allowance") < order.indexOf("consent"));
  assert.ok(order.indexOf("nonce") < order.indexOf("consent"));
});

test("consent denial performs no approval or redemption", async () => {
  let effects = 0;
  await assert.rejects(
    consentAndSubmitRedemption({
      ledger: { icrc2_approve: async () => { effects += 1; } },
      stream: { redeem: async () => { effects += 1; } },
      request: {
        approval: {
          from_subaccount: [new Uint8Array(32)],
          spender: { owner: "stream", subaccount: [] },
          amount: 1n,
          fee: [1n],
          expected_allowance: [0n],
          expires_at: [2n],
          memo: [new Uint8Array()],
          created_at_time: [1n],
        },
        redeem: {
          from_subaccount: [new Uint8Array(32)],
          io_amount_e8s: 1n,
          min_icp_out_e8s: 1n,
          max_icp_fee_e8s: 1n,
          expires_at_nanos: 2n,
          nonce: 0n,
        },
      },
      session: { network: "local", requestApprovalConsent: async () => false },
    }),
    /not granted/,
  );
  assert.equal(effects, 0);
});

test("consent effects derive only from the exact constructed request", () => {
  const source = new Uint8Array(32).fill(6);
  const request = {
    approval: {
      from_subaccount: [source],
      spender: { owner: "submitted-stream", subaccount: [] },
      amount: 2n,
      fee: [1n],
      expected_allowance: [0n],
      expires_at: [3n],
      memo: [new Uint8Array([1])],
      created_at_time: [1n],
    },
    redeem: {
      from_subaccount: [source],
      io_amount_e8s: 1n,
      min_icp_out_e8s: 1n,
      max_icp_fee_e8s: 1n,
      expires_at_nanos: 2n,
      nonce: 0n,
    },
  };
  const terms = redemptionConsentTerms(request, "local", "unrelated-stream");
  assert.deepEqual(terms.spender, request.approval.spender);
  assert.deepEqual(terms.selectedSourceSubaccount, source);
  request.redeem.from_subaccount = [new Uint8Array(32).fill(7)];
  assert.throws(() => redemptionConsentTerms(request, "local"), /subaccounts differ/);
});
