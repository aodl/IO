import test from "node:test";
import assert from "node:assert/strict";
import {
  canonicalSubaccount,
  consentPushAndSettleRedemption,
  prepareRedemption,
  progressLabel,
  redemptionConsentTerms,
} from "../src/app/redemption.js";

function prepared(selected = new Uint8Array(32).fill(7)) {
  return {
    caller: "owner",
    account: { owner: "owner", subaccount: [selected] },
    reserve: { owner: "stream", subaccount: [] },
    request: {
      from_subaccount: [selected],
      io_amount_e8s: 1_000_000n,
      min_icp_out_e8s: 900_000n,
      max_io_fee_e8s: 10_000n,
      max_icp_fee_e8s: 10_000n,
      expires_at_nanos: 2_000_000_000n,
      nonce: 9n,
    },
    prepared_at_nanos: 1_000_000_000n,
    push_memo: new Uint8Array(32).fill(3),
    gross_icp_e8s: 1_000_000n,
    net_icp_e8s: 990_000n,
    snapshot: { io_fee_e8s: 10_000n, icp_fee_e8s: 10_000n },
  };
}

test("preparation uses the wallet subaccount, exact nonce, and no allowance", async () => {
  const selected = new Uint8Array(32).fill(7);
  const order = [];
  const expected = prepared(selected);
  const ledger = { icrc1_fee: async () => { order.push("fee"); return 10_000n; } };
  const stream = {
    get_caller_redemption_state: async () => {
      order.push("nonce");
      return { Ok: { next_nonce: 9n, pending: [], last_request_fingerprint: [], last_result: [] } };
    },
    prepare_redemption: async (args) => {
      order.push("prepare");
      assert.deepEqual(args.from_subaccount[0], selected);
      assert.equal(args.nonce, 9n);
      return { Ok: expected };
    },
  };
  assert.equal(await prepareRedemption({
    ledger, stream, selectedSubaccount: selected, ioAmountE8s: 1_000_000n,
    minIcpOutE8s: 900_000n, maxIcpFeeE8s: 10_000n, nowNanos: 1_000_000_000n,
  }), expected);
  assert.deepEqual(order, ["fee", "nonce", "prepare"]);
  assert.equal(progressLabel({ Pending: null }), "Payout owed — waiting for exact recovery");
});

test("arbitrary text and malformed wallet subaccounts are rejected", () => {
  assert.throws(() => canonicalSubaccount("00".repeat(32)));
  assert.throws(() => canonicalSubaccount(new Uint8Array(31)));
});

test("exact flow consents, performs one ICRC-1 push, and settles its block", async () => {
  const order = [];
  const quote = prepared();
  const ledger = {
    icrc1_transfer: async (args) => {
      order.push("push");
      assert.deepEqual(args.from_subaccount, quote.account.subaccount);
      assert.deepEqual(args.to, quote.reserve);
      assert.equal(args.amount, quote.request.io_amount_e8s);
      assert.deepEqual(args.memo, [quote.push_memo]);
      return { Ok: 44n };
    },
  };
  const stream = {
    settle_redemption: async (block) => {
      order.push("settle");
      assert.equal(block, 44n);
      return { Ok: { Pending: null } };
    },
  };
  const session = {
    network: "local",
    requestTransferConsent: async (terms) => {
      order.push("consent");
      assert.equal(terms.action, "icrc1_push_for_io_redemption");
      assert.equal(terms.exactNetIcpE8s, 990_000n);
      return true;
    },
  };
  await consentPushAndSettleRedemption({ ledger, stream, prepared: quote, session });
  assert.deepEqual(order, ["consent", "push", "settle"]);
});

test("consent denial performs no push or settlement", async () => {
  let effects = 0;
  await assert.rejects(consentPushAndSettleRedemption({
    ledger: { icrc1_transfer: async () => { effects += 1; } },
    stream: { settle_redemption: async () => { effects += 1; } },
    prepared: prepared(),
    session: { network: "local", requestTransferConsent: async () => false },
  }), /not granted/);
  assert.equal(effects, 0);
});

test("consent terms are derived from the prepared immutable quote", () => {
  const quote = prepared();
  const terms = redemptionConsentTerms(quote, "local");
  assert.equal(terms.ioAmountE8s, quote.request.io_amount_e8s);
  assert.deepEqual(terms.reserveDestination, quote.reserve);
  assert.deepEqual(terms.exactMemo, quote.push_memo);
  assert.deepEqual(terms.sourceSubaccount, quote.account.subaccount[0]);
});
