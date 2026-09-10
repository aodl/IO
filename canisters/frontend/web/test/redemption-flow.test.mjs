import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import {
  canonicalSubaccount,
  consentStageAndProcessRedemption,
  redemptionConsentTerms,
} from "../src/app/redemption.js";
import { mountRedemptionForm } from "../src/ui/redemption-form.js";

const principal = (value = "user") => ({ toText: () => value });
const identity = (value) => ({ getPrincipal: () => principal(value) });
const selectedSubaccount = new Uint8Array(32).fill(7);
const staging = { owner: principal("stream"), subaccount: [new Uint8Array(32).fill(9)] };

function deferred() {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
}

function fixture(overrides = {}) {
  const calls = { consent: 0, transfer: 0, wake: 0 };
  const ledger = {
    icrc1_fee: async () => 10n,
    icrc1_balance_of: async () => 1_000n,
    icrc1_transfer: async () => { calls.transfer += 1; return { Ok: 44n }; },
    ...overrides.ledger,
  };
  const stream = {
    get_redemption_staging_account: async () => staging,
    get_minimum_redemption_io_e8s: async () => 20n,
    process_redemptions: async () => { calls.wake += 1; return { Ok: null }; },
    ...overrides.stream,
  };
  const session = {
    identity: identity("user"),
    network: "local",
    requestTransferConsent: async () => { calls.consent += 1; return true; },
    ...overrides.session,
  };
  return {
    calls,
    args: {
      ledger,
      stream,
      selectedSubaccount,
      ioAmountE8s: 100n,
      session,
      nowNanos: () => 123n,
    },
  };
}

test("canonical wallet subaccount rejects text and the wrong length", () => {
  assert.throws(() => canonicalSubaccount("00".repeat(32)));
  assert.throws(() => canonicalSubaccount(new Uint8Array(31)));
});

for (const [name, amount] of [["malformed", "1.2"], ["zero", 0n], ["negative", -1n]]) {
  test(`${name} input performs no consent, transfer, or wake`, async () => {
    const { args, calls } = fixture();
    await assert.rejects(consentStageAndProcessRedemption({ ...args, ioAmountE8s: amount }), /positive integer/);
    assert.deepEqual(calls, { consent: 0, transfer: 0, wake: 0 });
  });
}

test("below-minimum and insufficient inputs fail before consent or transfer", async () => {
  const below = fixture();
  await assert.rejects(
    consentStageAndProcessRedemption({ ...below.args, ioAmountE8s: 19n }),
    /below the configured minimum/,
  );
  assert.deepEqual(below.calls, { consent: 0, transfer: 0, wake: 0 });

  const unfunded = fixture({ ledger: { icrc1_balance_of: async () => 109n } });
  await assert.rejects(consentStageAndProcessRedemption(unfunded.args), /insufficient/);
  assert.deepEqual(unfunded.calls, { consent: 0, transfer: 0, wake: 0 });
});

test("consent denial performs no transfer or worker wake", async () => {
  const { args, calls } = fixture({
    session: { requestTransferConsent: async () => { calls.consent += 1; return false; } },
  });
  await assert.rejects(consentStageAndProcessRedemption(args), /not granted/);
  assert.deepEqual(calls, { consent: 1, transfer: 0, wake: 0 });
});

test("success sends one ordinary staging transfer then one no-argument wake", async () => {
  const payloads = [];
  const { args, calls } = fixture({
    ledger: {
      icrc1_transfer: async (payload) => { calls.transfer += 1; payloads.push(payload); return { Ok: 44n }; },
    },
  });
  const result = await consentStageAndProcessRedemption(args);
  assert.equal(result.transferBlock, 44n);
  assert.equal(result.wakeError, null);
  assert.deepEqual(calls, { consent: 1, transfer: 1, wake: 1 });
  assert.deepEqual(payloads[0], {
    from_subaccount: [selectedSubaccount],
    to: staging,
    amount: 100n,
    fee: [10n],
    memo: [],
    created_at_time: [123n],
  });
});

test("an exact ledger Duplicate is accepted as the staging receipt", async () => {
  const { args, calls } = fixture({
    ledger: { icrc1_transfer: async () => { calls.transfer += 1; return { Err: { Duplicate: { duplicate_of: 41n } } }; } },
  });
  assert.equal((await consentStageAndProcessRedemption(args)).transferBlock, 41n);
  assert.deepEqual(calls, { consent: 1, transfer: 1, wake: 1 });
});

test("transport ambiguity warns explicitly and the application performs no retry or wake", async () => {
  const { args, calls } = fixture({
    ledger: { icrc1_transfer: async () => { calls.transfer += 1; throw new Error("response lost"); } },
  });
  await assert.rejects(
    consentStageAndProcessRedemption(args),
    /may have succeeded.*check wallet or ledger history/i,
  );
  assert.deepEqual(calls, { consent: 1, transfer: 1, wake: 0 });
});

test("an unrecognized post-dispatch response is treated as unknown, never as absence", async () => {
  const { args, calls } = fixture({
    ledger: { icrc1_transfer: async () => { calls.transfer += 1; return null; } },
  });
  await assert.rejects(
    consentStageAndProcessRedemption(args),
    /may have succeeded.*check wallet or ledger history/i,
  );
  assert.deepEqual(calls, { consent: 1, transfer: 1, wake: 0 });
});

for (const [name, processRedemptions] of [
  ["transport failure", async () => { throw new Error("unavailable"); }],
  ["protocol error", async () => ({ Err: { Busy: null } })],
]) {
  test(`worker wake ${name} cannot relabel a confirmed staging transfer as failed`, async () => {
    const { args, calls } = fixture({
      stream: { process_redemptions: async () => { calls.wake += 1; return processRedemptions(); } },
    });
    const result = await consentStageAndProcessRedemption(args);
    assert.equal(result.transferBlock, 44n);
    assert.ok(result.wakeError);
    assert.deepEqual(calls, { consent: 1, transfer: 1, wake: 1 });
  });
}

test("wallet terms expose the canonical staging facts and no quote promise", () => {
  const terms = redemptionConsentTerms({
    amount: 100n,
    sourceSubaccount: selectedSubaccount,
    staging,
    ioFee: 10n,
    minimum: 20n,
  }, "local");
  assert.equal(terms.ioAmountE8s, 100n);
  assert.equal(terms.ioTransferFeeE8s, 10n);
  assert.equal(terms.quoteStatus, "indicative_until_stream_accepts_staged_transfer");
  assert.deepEqual(terms.stagingDestination, staging);
});

function mountedFixture(overrides = {}) {
  const pendingConsent = overrides.pendingConsent;
  let transfers = 0;
  const listeners = {};
  const submit = { disabled: false };
  const status = { textContent: "" };
  const form = {
    elements: { ioAmount: { value: "100" } },
    querySelector: () => submit,
    addEventListener: (event, listener) => { listeners[event] = listener; },
  };
  const document = {
    querySelector: (selector) => ({
      "[data-redemption-form]": form,
      "[data-redemption-status]": status,
    })[selector] ?? null,
  };
  const actors = {
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async () => { transfers += 1; return overrides.transferResult ?? { Ok: 44n }; },
    },
    stream: {
      get_redemption_staging_account: async () => staging,
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: async () => ({ Ok: null }),
    },
  };
  const session = {
    identity: identity("mounted"),
    network: "local",
    selectedSubaccount,
    requestTransferConsent: async () => pendingConsent ? pendingConsent.promise : true,
  };
  mountRedemptionForm(document, actors, session);
  return { listeners, submit, status, transfers: () => transfers };
}

test("mounted form suppresses same-page double submission while consent is pending", async () => {
  const pendingConsent = deferred();
  const mounted = mountedFixture({ pendingConsent });
  const event = { preventDefault() {} };
  const first = mounted.listeners.submit(event);
  const second = mounted.listeners.submit(event);
  assert.equal(mounted.submit.disabled, true);
  pendingConsent.resolve(true);
  await Promise.all([first, second]);
  assert.equal(mounted.transfers(), 1);
  assert.match(mounted.status.textContent, /IO staged.*automatic/i);
});

test("mounted ambiguity remains explicit and disables another submission on that page", async () => {
  const mounted = mountedFixture({ transferResult: Promise.reject(new Error("lost")) });
  await mounted.listeners.submit({ preventDefault() {} });
  assert.equal(mounted.transfers(), 1);
  assert.equal(mounted.submit.disabled, true);
  assert.match(mounted.status.textContent, /outcome is unknown.*may have succeeded/i);
  await mounted.listeners.submit({ preventDefault() {} });
  assert.equal(mounted.transfers(), 1);
});

test("normal frontend has no Check button or browser receipt database", async () => {
  const template = await readFile(new URL("../index.template.html", import.meta.url), "utf8");
  const source = await readFile(new URL("../src/app/redemption.js", import.meta.url), "utf8");
  assert.doesNotMatch(template, /data-redemption-process|Check staged redemptions/);
  assert.doesNotMatch(source, /sessionStorage|localStorage|attemptStorage|previousReceipt|dispatchEpoch/);
});
