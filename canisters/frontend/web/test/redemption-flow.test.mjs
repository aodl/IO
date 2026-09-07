import test from "node:test";
import assert from "node:assert/strict";
import {
  canonicalSubaccount,
  checkRedemptionReceipt,
  consentStageAndProcessRedemption,
  processRedemptions,
  progressLabel,
  redemptionConsentTerms,
} from "../src/app/redemption.js";
import { mountRedemptionForm } from "../src/ui/redemption-form.js";

const principal = (text) => ({ toText: () => text });
const identity = (text = "user") => ({ getPrincipal: () => principal(text) });
const memoryStore = (initial = []) => {
  const values = new Map(initial);
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => values.set(key, value),
    removeItem: (key) => values.delete(key),
    snapshot: () => [...values.entries()],
  };
};

const deferred = () => {
  let resolve;
  let reject;
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
};

async function withSessionStorageDescriptor(descriptor, action) {
  const original = Object.getOwnPropertyDescriptor(globalThis, "sessionStorage");
  Object.defineProperty(globalThis, "sessionStorage", { configurable: true, ...descriptor });
  try {
    return await action();
  } finally {
    if (original) Object.defineProperty(globalThis, "sessionStorage", original);
    else delete globalThis.sessionStorage;
  }
}

test("arbitrary text and malformed wallet subaccounts are rejected", () => {
  assert.throws(() => canonicalSubaccount("00".repeat(32)));
  assert.throws(() => canonicalSubaccount(new Uint8Array(31)));
});

for (const unavailable of [
  ["throws", { get: () => { throw new Error("storage denied"); } }],
  ["is unusable", { value: {} }],
]) {
  test(`unavailable browser sessionStorage ${unavailable[0]} before consent or transfer`, async () => {
    const calls = { consent: 0, transfer: 0, worker: 0 };
    await withSessionStorageDescriptor(unavailable[1], async () => {
      await assert.rejects(consentStageAndProcessRedemption({
        ledger: {
          icrc1_fee: async () => 10n,
          icrc1_balance_of: async () => 1_000n,
          icrc1_transfer: async () => { calls.transfer += 1; return { Ok: 44n }; },
        },
        stream: {
          get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
          get_minimum_redemption_io_e8s: async () => 20n,
          process_redemptions: async () => { calls.worker += 1; return { Ok: { Pending: null } }; },
        },
        selectedSubaccount: new Uint8Array(32),
        ioAmountE8s: 100n,
        session: {
          identity: identity("no-storage"),
          network: "local",
          requestTransferConsent: async () => { calls.consent += 1; return true; },
        },
      }), /session storage|receipt storage/i);
    });
    assert.deepEqual(calls, { consent: 0, transfer: 0, worker: 0 });
  });
}

test("explicit injected receipt storage permits and persists one normal staging transfer", async () => {
  const storage = memoryStore();
  let consents = 0;
  let transfers = 0;
  const result = await consentStageAndProcessRedemption({
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async () => { transfers += 1; return { Ok: 44n }; },
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: async () => ({ Ok: { Pending: null } }),
    },
    selectedSubaccount: new Uint8Array(32).fill(2),
    ioAmountE8s: 100n,
    session: {
      identity: identity("injected-storage"),
      network: "local",
      requestTransferConsent: async () => { consents += 1; return true; },
    },
    storage,
    nowNanos: () => 123n,
  });
  assert.equal(result.transferBlock, 44n);
  assert.equal(consents, 1);
  assert.equal(transfers, 1);
  const persisted = JSON.parse(storage.snapshot()[0][1]);
  assert.equal(persisted.current.transferBlock, "44");
  assert.equal(persisted.current.createdAtTime, "123");
  assert.equal(persisted.current.status, "staged");
});

test("semantic staging consents, transfers once without a memo, and prompts bounded work", async () => {
  const order = [];
  const selected = new Uint8Array(32).fill(7);
  const staging = { owner: "stream", subaccount: [new Uint8Array(32).fill(9)] };
  const ledger = {
    icrc1_fee: async () => {
      order.push("fee");
      return 10_000n;
    },
    icrc1_balance_of: async () => {
      order.push("balance");
      return 2_000_000n;
    },
    icrc1_transfer: async (args) => {
      order.push("stage");
      assert.deepEqual(args.from_subaccount, [selected]);
      assert.deepEqual(args.to, staging);
      assert.equal(args.amount, 1_000_000n);
      assert.deepEqual(args.fee, [10_000n]);
      assert.deepEqual(args.memo, []);
      assert.deepEqual(args.created_at_time, [123n]);
      return { Ok: 44n };
    },
  };
  const stream = {
    get_redemption_staging_account: async () => {
      order.push("account");
      return staging;
    },
    get_minimum_redemption_io_e8s: async () => {
      order.push("minimum");
      return 20_000n;
    },
    process_redemptions: async (...args) => {
      order.push("process");
      assert.deepEqual(args, []);
      return { Ok: { Pending: null } };
    },
  };
  const session = {
    identity: identity(),
    network: "local",
    requestTransferConsent: async (terms) => {
      order.push("consent");
      assert.equal(terms.action, "icrc1_transfer_to_io_redemption_staging");
      assert.equal(terms.ioAmountE8s, 1_000_000n);
      assert.equal(terms.quoteStatus, "indicative_until_stream_accepts_staged_transfer");
      return true;
    },
  };
  const result = await consentStageAndProcessRedemption({
    ledger,
    stream,
    selectedSubaccount: selected,
    ioAmountE8s: 1_000_000n,
    session,
    storage: memoryStore(),
    nowNanos: () => 123n,
  });
  assert.equal(result.transferBlock, 44n);
  assert.deepEqual(result.progress, { Ok: { Pending: null } });
  assert.deepEqual(order, ["fee", "account", "minimum", "balance", "consent", "stage", "process"]);
  assert.equal(
    progressLabel(result.progress.Ok),
    "IO staged; processing pending",
  );
});

test("consent denial performs no transfer or processing", async () => {
  let effects = 0;
  await assert.rejects(consentStageAndProcessRedemption({
    ledger: {
      icrc1_fee: async () => 10_000n,
      icrc1_balance_of: async () => 2_000_000n,
      icrc1_transfer: async () => { effects += 1; },
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20_000n,
      process_redemptions: async () => { effects += 1; },
    },
    selectedSubaccount: new Uint8Array(32),
    ioAmountE8s: 1_000_000n,
    session: { identity: identity(), network: "local", requestTransferConsent: async () => false },
    storage: memoryStore(),
  }), /not granted/);
  assert.equal(effects, 0);
});

test("consent terms describe fees, delayed processing, and claim-bearing staging", () => {
  const selected = new Uint8Array(32).fill(4);
  const staging = { owner: "stream", subaccount: [] };
  const terms = redemptionConsentTerms({
    amount: 2_000_000n,
    sourceSubaccount: selected,
    staging,
    ioFee: 10_000n,
    minimum: 20_000n,
  }, "local");
  assert.equal(terms.ioTransferFeeE8s, 10_000n);
  assert.deepEqual(terms.stagingDestination, staging);
  assert.match(terms.finalPayoutFeePolicy, /canonical_icp_fee/);
  assert.match(terms.delayedProcessingPolicy, /claim_bearing/);
});

test("permissionless fast path passes no monetary arguments", async () => {
  let observed;
  const result = await processRedemptions({
    process_redemptions: async (...args) => {
      observed = args;
      return { Ok: { Idle: null } };
    },
  });
  assert.deepEqual(observed, []);
  assert.deepEqual(result, { Ok: { Idle: null } });
  assert.equal(
    progressLabel({ RateLimited: { retry_at_nanos: 1n } }),
    "Redemption staged — automatic processing runs approximately once per minute",
  );
});

test("below-minimum input performs no consent, transfer, or worker call", async () => {
  const effects = [];
  await assert.rejects(consentStageAndProcessRedemption({
    ledger: {
      icrc1_fee: async () => 10_000n,
      icrc1_balance_of: async () => 1_000_000n,
      icrc1_transfer: async () => effects.push("transfer"),
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20_000n,
      process_redemptions: async () => effects.push("worker"),
    },
    selectedSubaccount: new Uint8Array(32),
    ioAmountE8s: 19_999n,
    session: {
      identity: identity("below-minimum"),
      network: "local",
      requestTransferConsent: async () => effects.push("consent"),
    },
    storage: memoryStore(),
  }), /minimum/i);
  assert.deepEqual(effects, []);
});

test("successful staging receipt survives an optional worker transport failure", async () => {
  let transfers = 0;
  const result = await consentStageAndProcessRedemption({
    ledger: {
      icrc1_fee: async () => 10_000n,
      icrc1_balance_of: async () => 1_000_000n,
      icrc1_transfer: async () => {
        transfers += 1;
        return { Ok: 44n };
      },
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20_000n,
      process_redemptions: async () => { throw new Error("worker unavailable"); },
    },
    selectedSubaccount: new Uint8Array(32),
    ioAmountE8s: 20_000n,
    session: { identity: identity("worker-failure"), network: "local", requestTransferConsent: async () => true },
    storage: memoryStore(),
    nowNanos: () => 456n,
  });
  assert.equal(transfers, 1);
  assert.equal(result.transferBlock, 44n);
  assert.equal(result.processingPending, true);
  assert.match(result.workerError.message, /worker unavailable/);
});

test("exact minimum follows changed configuration and insufficient balance fails before consent", async () => {
  const effects = [];
  const base = {
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 109n,
      icrc1_transfer: async () => effects.push("transfer"),
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 100n,
      process_redemptions: async () => effects.push("worker"),
    },
    selectedSubaccount: new Uint8Array(32).fill(3),
    ioAmountE8s: 100n,
    session: {
      identity: identity("balance"),
      network: "local",
      requestTransferConsent: async () => effects.push("consent"),
    },
    storage: memoryStore(),
  };
  await assert.rejects(consentStageAndProcessRedemption(base), /insufficient/i);
  assert.deepEqual(effects, []);

  base.ledger.icrc1_balance_of = async () => 110n;
  base.ledger.icrc1_transfer = async () => ({ Ok: 9n });
  base.session.requestTransferConsent = async () => true;
  const exact = await consentStageAndProcessRedemption(base);
  assert.equal(exact.transferBlock, 9n);

  base.ioAmountE8s = 149n;
  base.stream.get_minimum_redemption_io_e8s = async () => 150n;
  base.storage = memoryStore();
  await assert.rejects(consentStageAndProcessRedemption(base), /configured minimum of 150/);
});

test("malformed, zero, negative, and over-u128 amounts never transfer", async () => {
  for (const value of ["1.5", 0n, -1n, 1n << 128n]) {
    let transfers = 0;
    await assert.rejects(consentStageAndProcessRedemption({
      ledger: { icrc1_transfer: async () => { transfers += 1; } },
      stream: {},
      selectedSubaccount: new Uint8Array(32),
      ioAmountE8s: value,
      session: { identity: identity("malformed"), network: "local" },
      storage: memoryStore(),
    }), /positive integer|protocol bound/);
    assert.equal(transfers, 0);
  }
});

test("worker protocol error and another source block cannot claim local completion", async () => {
  const staging = { owner: principal("stream"), subaccount: [] };
  const selected = new Uint8Array(32).fill(5);
  const make = (worker) => consentStageAndProcessRedemption({
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async () => ({ Ok: 44n }),
    },
    stream: {
      get_redemption_staging_account: async () => staging,
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: worker,
    },
    selectedSubaccount: selected,
    ioAmountE8s: 100n,
    session: { identity: identity("same-account"), network: "local", requestTransferConsent: async () => true },
    storage: memoryStore(),
    nowNanos: () => 999n,
  });
  const protocolError = await make(async () => ({ Err: { Busy: null } }));
  assert.equal(protocolError.processingPending, true);
  assert.deepEqual(protocolError.progress, { Ok: { Pending: null } });

  const other = await make(async () => ({ Ok: { Completed: {
    source_io_block: 17n,
    source_account: { owner: principal("same-account"), subaccount: [selected] },
  } } }));
  assert.equal(other.transferBlock, 44n);
  assert.equal(other.processingPending, true);
  assert.deepEqual(other.progress, { Ok: { Pending: null } });
});

test("ambiguous transfer retry preserves payload and Duplicate recovers the original block", async () => {
  const store = memoryStore();
  const payloads = [];
  let call = 0;
  const args = {
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async (payload) => {
        payloads.push(payload);
        call += 1;
        if (call === 1) throw new Error("response lost");
        return { Err: { Duplicate: { duplicate_of: 44n } } };
      },
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: async () => ({ Ok: { Pending: null } }),
    },
    selectedSubaccount: new Uint8Array(32).fill(6),
    ioAmountE8s: 100n,
    session: { identity: identity("retry"), network: "local", requestTransferConsent: async () => true },
    storage: store,
    nowNanos: () => 1_234n,
  };
  await assert.rejects(consentStageAndProcessRedemption(args), /preserve the same transfer identity/);
  const recovered = await consentStageAndProcessRedemption({ ...args, nowNanos: () => 9_999n });
  assert.equal(recovered.transferBlock, 44n);
  assert.deepEqual(payloads[1], payloads[0]);
  assert.deepEqual(payloads[0].created_at_time, [1_234n]);

  await consentStageAndProcessRedemption(args);
  assert.equal(call, 2, "reload/submission reuses the known staged receipt without transferring");
});

test("two amounts from one Account retain distinct local source-block identities", async () => {
  const store = memoryStore();
  let nextBlock = 44n;
  const ledger = {
    icrc1_fee: async () => 10n,
    icrc1_balance_of: async () => 10_000n,
    icrc1_transfer: async () => ({ Ok: nextBlock++ }),
  };
  const stream = {
    get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
    get_minimum_redemption_io_e8s: async () => 20n,
    process_redemptions: async () => ({ Ok: { Completed: {
      source_io_block: 17n,
      source_account: { owner: principal("same"), subaccount: [] },
    } } }),
  };
  const common = {
    ledger,
    stream,
    selectedSubaccount: new Uint8Array(32),
    session: { identity: identity("same"), network: "local", requestTransferConsent: async () => true },
    storage: store,
  };
  const first = await consentStageAndProcessRedemption({ ...common, ioAmountE8s: 100n, nowNanos: () => 1n });
  const second = await consentStageAndProcessRedemption({ ...common, ioAmountE8s: 200n, nowNanos: () => 2n });
  assert.deepEqual([first.transferBlock, second.transferBlock], [44n, 45n]);
  assert.equal(first.processingPending, true);
  assert.equal(second.processingPending, true);
});

test("double submit while a transfer is unresolved issues one ledger call", async () => {
  const listeners = {};
  const button = { disabled: false, addEventListener: (name, handler) => { listeners[name] = handler; } };
  const status = { textContent: "" };
  const form = {
    elements: { ioAmount: { value: "100" } },
    querySelector: () => button,
    addEventListener: (name, handler) => { listeners[name] = handler; },
  };
  const document = {
    querySelector: (selector) => ({
      "[data-redemption-form]": form,
      "[data-redemption-status]": status,
      "[data-redemption-process]": button,
    })[selector],
  };
  let resolveTransfer;
  let transfers = 0;
  mountRedemptionForm(document, {
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async () => {
        transfers += 1;
        return new Promise((resolve) => { resolveTransfer = resolve; });
      },
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: async () => ({ Ok: { Pending: null } }),
    },
  }, {
    identity: identity("double-click"),
    selectedSubaccount: new Uint8Array(32),
    network: "local",
    redemptionStorage: memoryStore(),
    requestTransferConsent: async () => true,
  });
  const event = { preventDefault() {} };
  const first = listeners.submit(event);
  const second = listeners.submit(event);
  for (let i = 0; i < 10 && !resolveTransfer; i += 1) await Promise.resolve();
  assert.equal(transfers, 1);
  resolveTransfer({ Ok: 44n });
  await Promise.all([first, second]);
  assert.equal(transfers, 1);
  assert.equal(status.textContent, "IO staged; processing pending");
});

test("lost committed response followed by TooOld remains review-required and cannot retransfer", async () => {
  const store = memoryStore();
  const payloads = [];
  const effectiveDeposits = [];
  let calls = 0;
  let consents = 0;
  const args = {
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async (payload) => {
        payloads.push(payload);
        calls += 1;
        if (calls === 1) {
          effectiveDeposits.push(payload);
          throw new Error("committed response lost");
        }
        if (calls === 2) return { Err: { TooOld: null } };
        effectiveDeposits.push(payload);
        return { Ok: 45n };
      },
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: async () => ({ Ok: { Idle: null } }),
      get_status: async () => ({ last_completed_redemption: [] }),
    },
    selectedSubaccount: new Uint8Array(32).fill(8),
    ioAmountE8s: 100n,
    session: {
      identity: identity("too-old"),
      network: "local",
      requestTransferConsent: async () => { consents += 1; return true; },
    },
    storage: store,
    nowNanos: () => 1_000n,
  };

  await assert.rejects(consentStageAndProcessRedemption(args), /same transfer identity/i);
  await assert.rejects(
    consentStageAndProcessRedemption({ ...args, nowNanos: () => 2_000n }),
    /uncertain|review/i,
  );
  await assert.rejects(
    consentStageAndProcessRedemption({ ...args, nowNanos: () => 3_000n }),
    /uncertain|review/i,
  );
  assert.equal(calls, 2, "review-required retry must stop blind retransmission");
  assert.equal(consents, 1, "an unresolved retry must not become a fresh consented action");
  assert.equal(effectiveDeposits.length, 1);
  assert.deepEqual(payloads[1], payloads[0]);
  assert.deepEqual(payloads[0].created_at_time, [1_000n]);
});

test("later fee and temporary errors preserve ambiguity while definitive initial rejection resets", async () => {
  for (const laterError of [{ BadFee: { expected_fee: 11n } }, { TemporarilyUnavailable: null }]) {
    const store = memoryStore();
    let calls = 0;
    const args = {
      ledger: {
        icrc1_fee: async () => 10n,
        icrc1_balance_of: async () => 1_000n,
        icrc1_transfer: async () => {
          calls += 1;
          if (calls === 1) throw new Error("lost");
          return { Err: laterError };
        },
      },
      stream: {
        get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
        get_minimum_redemption_io_e8s: async () => 20n,
        process_redemptions: async () => ({ Ok: { Idle: null } }),
      },
      selectedSubaccount: new Uint8Array(32).fill(7),
      ioAmountE8s: 100n,
      session: { identity: identity(`ambiguous-${Object.keys(laterError)[0]}`), network: "local", requestTransferConsent: async () => true },
      storage: store,
      nowNanos: () => 1_000n,
    };
    await assert.rejects(consentStageAndProcessRedemption(args), /same transfer identity/i);
    await assert.rejects(consentStageAndProcessRedemption(args), /uncertain|review/i);
    await assert.rejects(consentStageAndProcessRedemption(args), /uncertain|review/i);
    assert.equal(calls, 2);
  }

  const store = memoryStore();
  let calls = 0;
  let consents = 0;
  const definitive = {
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async () => {
        calls += 1;
        return calls === 1 ? { Err: { BadFee: { expected_fee: 11n } } } : { Ok: 55n };
      },
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: async () => ({ Ok: { Pending: null } }),
    },
    selectedSubaccount: new Uint8Array(32).fill(6),
    ioAmountE8s: 100n,
    session: {
      identity: identity("definitive"),
      network: "local",
      requestTransferConsent: async () => { consents += 1; return true; },
    },
    storage: store,
    nowNanos: (() => { let now = 10n; return () => now++; })(),
  };
  await assert.rejects(consentStageAndProcessRedemption(definitive), /rejected.*without an effect/i);
  const recovered = await consentStageAndProcessRedemption(definitive);
  assert.equal(recovered.transferBlock, 55n);
  assert.equal(calls, 2);
  assert.equal(consents, 2);
});

function mountedFixture({
  workerResults = [],
  latestCompletion = () => [],
  getStatus,
  storage = memoryStore(),
  transfer = async () => ({ Ok: 44n }),
  consent = async () => true,
  fee = async () => 10n,
}) {
  const listeners = {};
  const submit = { disabled: false };
  const process = {
    disabled: false,
    addEventListener: (name, handler) => { listeners[`process:${name}`] = handler; },
  };
  const status = { textContent: "" };
  const form = {
    elements: { ioAmount: { value: "100" } },
    querySelector: () => submit,
    addEventListener: (name, handler) => { listeners[`form:${name}`] = handler; },
  };
  const document = {
    querySelector: (selector) => ({
      "[data-redemption-form]": form,
      "[data-redemption-status]": status,
      "[data-redemption-process]": process,
    })[selector],
  };
  const selected = new Uint8Array(32).fill(4);
  const owner = principal("alice");
  let transfers = 0;
  let workerCalls = 0;
  const queue = [...workerResults];
  const stream = {
    get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
    get_minimum_redemption_io_e8s: async () => 20n,
    process_redemptions: async () => {
      workerCalls += 1;
      const result = queue.shift();
      if (result instanceof Error) throw result;
      return result ?? { Ok: { Idle: null } };
    },
    get_status: getStatus ?? (async () => ({ last_completed_redemption: latestCompletion() })),
  };
  const controller = mountRedemptionForm(document, {
    ledger: {
      icrc1_fee: fee,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async (payload) => { transfers += 1; return transfer(payload, transfers); },
    },
    stream,
  }, {
    identity: { getPrincipal: () => owner },
    selectedSubaccount: selected,
    network: "local",
    redemptionStorage: storage,
    requestTransferConsent: consent,
  });
  return {
    listeners,
    status,
    selected,
    owner,
    controller,
    form,
    storage,
    submitControl: submit,
    processControl: process,
    transfers: () => transfers,
    workerCalls: () => workerCalls,
  };
}

async function seedMountedCompletion(storage) {
  const selectedSubaccount = new Uint8Array(32).fill(4);
  const owner = principal("alice");
  await consentStageAndProcessRedemption({
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async () => ({ Ok: 44n }),
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: async () => ({ Ok: { Completed: {
        source_io_block: 44n,
        source_account: { owner, subaccount: [selectedSubaccount] },
      } } }),
    },
    selectedSubaccount,
    ioAmountE8s: 100n,
    session: { identity: { getPrincipal: () => owner }, network: "local", requestTransferConsent: async () => true },
    storage,
    nowNanos: () => 1n,
  });
}

for (const boundary of ["preflight", "consent"]) {
  for (const outcome of ["acknowledged", "ambiguous"]) {
    test(`Check during ${boundary} cannot take ${outcome} submission status ownership`, async () => {
      const storage = memoryStore();
      await seedMountedCompletion(storage);
      const held = deferred();
      let boundaryEntered = false;
      const fixture = mountedFixture({
        storage,
        workerResults: [{ Ok: { Pending: null } }, { Ok: { Pending: null } }],
        fee: async () => {
          if (boundary === "preflight") {
            boundaryEntered = true;
            return held.promise;
          }
          return 10n;
        },
        consent: async () => {
          if (boundary === "consent") {
            boundaryEntered = true;
            return held.promise;
          }
          return true;
        },
        transfer: async () => {
          if (outcome === "ambiguous") throw new Error("B committed; response lost");
          return { Ok: 45n };
        },
      });
      await fixture.controller.ready;
      assert.equal(fixture.status.textContent, "Completed");
      fixture.form.elements.ioAmount.value = "200";
      const submission = fixture.listeners["form:submit"]({ preventDefault() {} });
      for (let turn = 0; turn < 20 && !boundaryEntered; turn += 1) await Promise.resolve();
      assert.equal(boundaryEntered, true);
      assert.equal(fixture.status.textContent, "Awaiting consent to transfer IO into redemption staging");

      await fixture.listeners["process:click"]();
      assert.equal(fixture.workerCalls(), 0, "synthetic Check must not run during submission");
      assert.equal(fixture.status.textContent, "Awaiting consent to transfer IO into redemption staging");
      assert.equal(fixture.processControl.disabled, true);

      held.resolve(boundary === "preflight" ? 10n : true);
      await submission;
      assert.equal(fixture.submitControl.disabled, false);
      assert.equal(fixture.processControl.disabled, false);
      assert.match(
        fixture.status.textContent,
        outcome === "acknowledged" ? /staged.*pending/i : /response unavailable/i,
      );
      const stored = JSON.parse(storage.snapshot()[0][1]);
      assert.equal(stored.current.amount, "200");
      assert.equal(stored.current.status, outcome === "acknowledged" ? "staged" : "ambiguous");
      if (outcome === "acknowledged") assert.equal(stored.current.transferBlock, "45");

      const transfersBeforeCheck = fixture.transfers();
      const workerCallsBeforeCheck = fixture.workerCalls();
      await fixture.listeners["process:click"]();
      assert.equal(fixture.transfers(), transfersBeforeCheck, "post-submit Check never transfers IO");
      assert.equal(fixture.workerCalls(), workerCallsBeforeCheck + 1);
    });
  }
}

test("page interruption after dispatch preserves uncertainty before the catch can run", async () => {
  const firstStore = memoryStore();
  const payloads = [];
  const effects = [];
  let consents = 0;
  const neverReturns = deferred();
  const selectedSubaccount = new Uint8Array(32).fill(12);
  const session = {
    identity: identity("interrupted"),
    network: "local",
    requestTransferConsent: async () => { consents += 1; return true; },
  };
  const stream = {
    get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
    get_minimum_redemption_io_e8s: async () => 20n,
    process_redemptions: async () => ({ Ok: { Pending: null } }),
    get_status: async () => ({ last_completed_redemption: [] }),
  };
  const first = consentStageAndProcessRedemption({
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async (payload) => {
        payloads.push(payload);
        effects.push(payload);
        return neverReturns.promise;
      },
    },
    stream,
    selectedSubaccount,
    ioAmountE8s: 100n,
    session,
    storage: firstStore,
    nowNanos: () => 1_000n,
  });
  for (let turn = 0; turn < 20 && payloads.length === 0; turn += 1) await Promise.resolve();
  assert.equal(payloads.length, 1, "the first request must have reached the ledger mock");

  const restoredStore = memoryStore(firstStore.snapshot());
  let restoredCalls = 0;
  const restoredArgs = {
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async (payload) => {
        payloads.push(payload);
        restoredCalls += 1;
        if (restoredCalls === 1) return { Err: { TooOld: null } };
        effects.push(payload);
        return { Ok: 45n };
      },
    },
    stream,
    selectedSubaccount,
    ioAmountE8s: 100n,
    session,
    storage: restoredStore,
    nowNanos: () => 2_000n,
  };
  await assert.rejects(consentStageAndProcessRedemption(restoredArgs), /uncertain|review/i);
  await assert.rejects(consentStageAndProcessRedemption(restoredArgs), /uncertain|review/i);
  assert.equal(restoredCalls, 1, "review-required state must stop a blind third dispatch");
  assert.equal(effects.length, 1);
  assert.equal(consents, 1);
  assert.deepEqual(payloads[1], payloads[0]);
  void first;
});

test("pre-dispatch durable state is conservative and interrupted Duplicate recovers it", async () => {
  const firstStore = memoryStore();
  const held = deferred();
  const payloads = [];
  let durableAtDispatch;
  let consents = 0;
  const selectedSubaccount = new Uint8Array(32).fill(14);
  const session = {
    identity: identity("interrupted-duplicate"),
    network: "local",
    requestTransferConsent: async () => { consents += 1; return true; },
  };
  const stream = {
    get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
    get_minimum_redemption_io_e8s: async () => 20n,
    process_redemptions: async () => ({ Ok: { Pending: null } }),
  };
  const first = consentStageAndProcessRedemption({
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async (payload) => {
        payloads.push(payload);
        durableAtDispatch = JSON.parse(firstStore.snapshot()[0][1]);
        return held.promise;
      },
    },
    stream,
    selectedSubaccount,
    ioAmountE8s: 100n,
    session,
    storage: firstStore,
    nowNanos: () => 5_000n,
  });
  for (let turn = 0; turn < 20 && !durableAtDispatch; turn += 1) await Promise.resolve();
  assert.equal(durableAtDispatch.current.status, "submitted");
  assert.equal(durableAtDispatch.current.hadAmbiguousSubmission, true);
  assert.equal(durableAtDispatch.current.dispatchEpoch, 1);

  const restoredStore = memoryStore(firstStore.snapshot());
  const recovered = await consentStageAndProcessRedemption({
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async (payload) => {
        payloads.push(payload);
        return { Err: { Duplicate: { duplicate_of: 44n } } };
      },
    },
    stream,
    selectedSubaccount,
    ioAmountE8s: 100n,
    session,
    storage: restoredStore,
    nowNanos: () => 9_999n,
  });
  assert.equal(recovered.transferBlock, 44n);
  assert.deepEqual(payloads[1], payloads[0]);
  assert.equal(consents, 1);
  void first;
});

test("late first-dispatch rejection cannot downgrade a newer acknowledged retry", async () => {
  const store = memoryStore();
  const firstReply = deferred();
  const payloads = [];
  let calls = 0;
  let consents = 0;
  const common = {
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async (payload) => {
        payloads.push(payload);
        calls += 1;
        return calls === 1 ? firstReply.promise : { Ok: 44n };
      },
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: async () => ({ Ok: { Pending: null } }),
    },
    selectedSubaccount: new Uint8Array(32).fill(15),
    ioAmountE8s: 100n,
    session: {
      identity: identity("late-rejection"),
      network: "local",
      requestTransferConsent: async () => { consents += 1; return true; },
    },
    storage: store,
    nowNanos: () => 7_000n,
  };
  const first = consentStageAndProcessRedemption(common);
  for (let turn = 0; turn < 20 && calls === 0; turn += 1) await Promise.resolve();
  const retry = await consentStageAndProcessRedemption(common);
  assert.equal(retry.transferBlock, 44n);
  firstReply.resolve({ Err: { BadFee: { expected_fee: 11n } } });
  const late = await first;
  assert.equal(late.transferBlock, 44n);
  const restored = await consentStageAndProcessRedemption(common);
  assert.equal(restored.transferBlock, 44n);
  assert.equal(calls, 2);
  assert.equal(consents, 1);
  assert.deepEqual(payloads[1], payloads[0]);
});

test("delayed mounted Check completion for A cannot erase ambiguous B", async () => {
  const delayedStatus = deferred();
  let statusCalls = 0;
  const payloads = [];
  const effects = [];
  let consents = 0;
  const fixture = mountedFixture({
    workerResults: [{ Ok: { Pending: null } }, { Ok: { Idle: null } }],
    getStatus: async () => {
      statusCalls += 1;
      if (statusCalls <= 2) return { last_completed_redemption: [] };
      return delayedStatus.promise;
    },
    consent: async () => { consents += 1; return true; },
    transfer: async (payload, call) => {
      payloads.push(payload);
      if (call === 1) {
        effects.push(payload);
        return { Ok: 44n };
      }
      if (call === 2) {
        effects.push(payload);
        throw new Error("B committed; response lost");
      }
      if (payload.created_at_time[0] === payloads[1].created_at_time[0]) {
        return { Err: { Duplicate: { duplicate_of: 45n } } };
      }
      effects.push(payload);
      return { Ok: 46n };
    },
  });
  await fixture.controller.ready;
  await fixture.listeners["form:submit"]({ preventDefault() {} });
  const oldCheck = fixture.listeners["process:click"]();
  for (let turn = 0; turn < 20 && statusCalls < 3; turn += 1) await Promise.resolve();

  fixture.form.elements.ioAmount.value = "200";
  await fixture.listeners["form:submit"]({ preventDefault() {} });
  assert.match(fixture.status.textContent, /response unavailable/i);
  delayedStatus.resolve({
    last_completed_redemption: [{
      source_io_block: 44n,
      source_account: { owner: fixture.owner, subaccount: [fixture.selected] },
    }],
  });
  await oldCheck;
  assert.doesNotMatch(fixture.status.textContent, /^Completed$/);

  await fixture.listeners["form:submit"]({ preventDefault() {} });
  assert.equal(fixture.transfers(), 3);
  assert.equal(consents, 2);
  assert.equal(effects.length, 2, "B retry must retain its original transfer identity");
  assert.deepEqual(payloads[2], payloads[1]);
});

test("delayed mounted Check for A preserves acknowledged B and cannot repaint it complete", async () => {
  const delayedStatus = deferred();
  let statusCalls = 0;
  const fixture = mountedFixture({
    workerResults: [{ Ok: { Pending: null } }, { Ok: { Idle: null } }, { Ok: { Pending: null } }],
    getStatus: async () => {
      statusCalls += 1;
      if (statusCalls === 3) return delayedStatus.promise;
      return { last_completed_redemption: [] };
    },
    transfer: async (_payload, call) => ({ Ok: call === 1 ? 44n : 45n }),
  });
  await fixture.controller.ready;
  await fixture.listeners["form:submit"]({ preventDefault() {} });
  const oldCheck = fixture.listeners["process:click"]();
  for (let turn = 0; turn < 20 && statusCalls < 3; turn += 1) await Promise.resolve();
  fixture.form.elements.ioAmount.value = "200";
  await fixture.listeners["form:submit"]({ preventDefault() {} });
  delayedStatus.resolve({
    last_completed_redemption: [{
      source_io_block: 44n,
      source_account: { owner: fixture.owner, subaccount: [fixture.selected] },
    }],
  });
  await oldCheck;
  assert.doesNotMatch(fixture.status.textContent, /^Completed$/);
  const retained = await checkRedemptionReceipt({
    stream: {
      process_redemptions: async () => ({ Ok: { Pending: null } }),
      get_status: async () => ({ last_completed_redemption: [] }),
    },
    selectedSubaccount: fixture.selected,
    session: { identity: { getPrincipal: () => fixture.owner }, network: "local" },
    storage: fixture.storage,
  });
  assert.equal(retained.transferBlock, 45n);
  assert.equal(retained.processingPending, true);
});

test("initial restore delayed across submission cannot repaint or erase the new receipt", async () => {
  const store = memoryStore();
  const selectedSubaccount = new Uint8Array(32).fill(4);
  const owner = principal("alice");
  await consentStageAndProcessRedemption({
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 1_000n,
      icrc1_transfer: async () => ({ Ok: 44n }),
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: async () => ({ Ok: { Pending: null } }),
      get_status: async () => ({ last_completed_redemption: [] }),
    },
    selectedSubaccount,
    ioAmountE8s: 100n,
    session: { identity: { getPrincipal: () => owner }, network: "local", requestTransferConsent: async () => true },
    storage: store,
    nowNanos: () => 1n,
  });
  const delayedRestore = deferred();
  let statusCalls = 0;
  const fixture = mountedFixture({
    storage: store,
    workerResults: [{ Ok: { Pending: null } }],
    getStatus: async () => {
      statusCalls += 1;
      return statusCalls === 1 ? delayedRestore.promise : { last_completed_redemption: [] };
    },
    transfer: async () => ({ Ok: 45n }),
  });
  fixture.form.elements.ioAmount.value = "200";
  await fixture.listeners["form:submit"]({ preventDefault() {} });
  delayedRestore.resolve({
    last_completed_redemption: [{
      source_io_block: 44n,
      source_account: { owner: fixture.owner, subaccount: [fixture.selected] },
    }],
  });
  await fixture.controller.ready;
  assert.doesNotMatch(fixture.status.textContent, /^Completed$/);
  const retained = await checkRedemptionReceipt({
    stream: {
      process_redemptions: async () => ({ Ok: { Pending: null } }),
      get_status: async () => ({ last_completed_redemption: [] }),
    },
    selectedSubaccount: fixture.selected,
    session: { identity: { getPrincipal: () => fixture.owner }, network: "local" },
    storage: store,
  });
  assert.equal(retained.transferBlock, 45n);
});

test("two mounted checks resolving in reverse order cannot regress completion", async () => {
  const statuses = [deferred(), deferred()];
  let statusCalls = 0;
  const fixture = mountedFixture({
    workerResults: [{ Ok: { Pending: null } }, { Ok: { Idle: null } }, { Ok: { Idle: null } }],
    getStatus: async () => {
      statusCalls += 1;
      if (statusCalls <= 2) return { last_completed_redemption: [] };
      return statuses[statusCalls - 3].promise;
    },
  });
  await fixture.controller.ready;
  await fixture.listeners["form:submit"]({ preventDefault() {} });
  const first = fixture.listeners["process:click"]();
  const second = fixture.listeners["process:click"]();
  for (let turn = 0; turn < 20 && statusCalls < 4; turn += 1) await Promise.resolve();
  statuses[1].resolve({
    last_completed_redemption: [{
      source_io_block: 44n,
      source_account: { owner: fixture.owner, subaccount: [fixture.selected] },
    }],
  });
  await second;
  assert.equal(fixture.status.textContent, "Completed");
  statuses[0].resolve({ last_completed_redemption: [] });
  await first;
  assert.equal(fixture.status.textContent, "Completed");
});

test("direct worker completion consumes the prior receipt after latest status moves on", async () => {
  const store = memoryStore();
  const selectedSubaccount = new Uint8Array(32).fill(13);
  const owner = principal("prior-worker");
  let nextBlock = 44n;
  let workerResult = { Ok: { Pending: null } };
  let latest = [];
  const common = {
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 10_000n,
      icrc1_transfer: async () => ({ Ok: nextBlock++ }),
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: async () => workerResult,
      get_status: async () => ({ last_completed_redemption: latest }),
    },
    selectedSubaccount,
    session: { identity: { getPrincipal: () => owner }, network: "local", requestTransferConsent: async () => true },
    storage: store,
  };
  await consentStageAndProcessRedemption({ ...common, ioAmountE8s: 100n, nowNanos: () => 1n });
  await consentStageAndProcessRedemption({ ...common, ioAmountE8s: 200n, nowNanos: () => 2n });
  workerResult = { Ok: { Completed: {
    source_io_block: 44n,
    source_account: { owner, subaccount: [selectedSubaccount] },
  } } };
  latest = [{ source_io_block: 900n, source_account: { owner: principal("other"), subaccount: [] } }];
  const checked = await checkRedemptionReceipt({
    stream: common.stream,
    selectedSubaccount,
    session: common.session,
    storage: store,
  });
  assert.equal(checked.transferBlock, 45n, "B remains the displayed current receipt");

  workerResult = { Ok: { Pending: null } };
  const third = await consentStageAndProcessRedemption({ ...common, ioAmountE8s: 300n, nowNanos: () => 3n });
  assert.equal(third.transferBlock, 46n, "resolved prior slot permits a distinct C intent");
});

test("worker-current and status-prior evidence resolve both retained receipts", async () => {
  const store = memoryStore();
  const selectedSubaccount = new Uint8Array(32).fill(16);
  const owner = principal("dual-completion");
  let nextBlock = 44n;
  let workerResult = { Ok: { Pending: null } };
  let latest = [];
  const common = {
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 10_000n,
      icrc1_transfer: async () => ({ Ok: nextBlock++ }),
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: async () => workerResult,
      get_status: async () => ({ last_completed_redemption: latest }),
    },
    selectedSubaccount,
    session: { identity: { getPrincipal: () => owner }, network: "local", requestTransferConsent: async () => true },
    storage: store,
  };
  await consentStageAndProcessRedemption({ ...common, ioAmountE8s: 100n, nowNanos: () => 1n });
  await consentStageAndProcessRedemption({ ...common, ioAmountE8s: 200n, nowNanos: () => 2n });
  workerResult = { Ok: { Completed: {
    source_io_block: 45n,
    source_account: { owner, subaccount: [selectedSubaccount] },
  } } };
  latest = [{
    source_io_block: 44n,
    source_account: { owner, subaccount: [selectedSubaccount] },
  }];
  const completed = await checkRedemptionReceipt({
    stream: common.stream,
    selectedSubaccount,
    session: common.session,
    storage: store,
  });
  assert.equal(completed.processingPending, false);
  assert.equal(completed.transferBlock, 45n, "newer completion remains the bounded cached receipt");
});

test("prior completion evidence with a mismatched subaccount cannot release its slot", async () => {
  const store = memoryStore();
  const selectedSubaccount = new Uint8Array(32).fill(17);
  const owner = principal("prior-mismatch");
  let nextBlock = 44n;
  let workerResult = { Ok: { Pending: null } };
  const common = {
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 10_000n,
      icrc1_transfer: async () => ({ Ok: nextBlock++ }),
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: async () => workerResult,
      get_status: async () => ({ last_completed_redemption: [] }),
    },
    selectedSubaccount,
    session: { identity: { getPrincipal: () => owner }, network: "local", requestTransferConsent: async () => true },
    storage: store,
  };
  await consentStageAndProcessRedemption({ ...common, ioAmountE8s: 100n, nowNanos: () => 1n });
  await consentStageAndProcessRedemption({ ...common, ioAmountE8s: 200n, nowNanos: () => 2n });
  workerResult = { Ok: { Completed: {
    source_io_block: 44n,
    source_account: { owner, subaccount: [new Uint8Array(32).fill(99)] },
  } } };
  await checkRedemptionReceipt({
    stream: common.stream,
    selectedSubaccount,
    session: common.session,
    storage: store,
  });
  await assert.rejects(
    consentStageAndProcessRedemption({ ...common, ioAmountE8s: 300n, nowNanos: () => 3n }),
    /two retained staged redemptions/i,
  );
});

test("mounted lost response then TooOld preserves one unresolved transfer identity", async () => {
  const payloads = [];
  const effectiveDeposits = [];
  let consents = 0;
  const fixture = mountedFixture({
    workerResults: [],
    consent: async () => { consents += 1; return true; },
    transfer: async (payload, call) => {
      payloads.push(payload);
      if (call === 1) {
        effectiveDeposits.push(payload);
        throw new Error("committed response lost");
      }
      if (call === 2) return { Err: { TooOld: null } };
      effectiveDeposits.push(payload);
      return { Ok: 45n };
    },
  });
  await fixture.controller.ready;
  const submit = () => fixture.listeners["form:submit"]({ preventDefault() {} });

  await submit();
  assert.match(fixture.status.textContent, /same transfer/i);
  await submit();
  assert.match(fixture.status.textContent, /uncertain|review/i);
  await submit();
  assert.match(fixture.status.textContent, /uncertain|review/i);

  assert.equal(fixture.transfers(), 2);
  assert.equal(consents, 1);
  assert.equal(effectiveDeposits.length, 1);
  assert.deepEqual(payloads[1], payloads[0]);
});

test("mounted manual check remains a no-transfer global worker prompt without a local receipt", async () => {
  const fixture = mountedFixture({ workerResults: [{ Ok: { Completed: { source_io_block: 17n } } }] });
  await fixture.controller.ready;
  assert.equal(fixture.workerCalls(), 0, "mount status restoration must not prompt global work");
  await fixture.listeners["process:click"]();
  assert.equal(fixture.workerCalls(), 1);
  assert.equal(fixture.transfers(), 0);
  assert.match(fixture.status.textContent, /^Global redemption worker: Completed$/);
});

test("mounted manual check matches the retained block and effective Account and catches errors", async () => {
  const selected = new Uint8Array(32).fill(4);
  const alice = principal("alice");
  const otherSubaccount = new Uint8Array(32).fill(9);
  const fixture = mountedFixture({
    workerResults: [
      { Ok: { Pending: null } },
      { Ok: { Completed: { source_io_block: 17n, source_account: { owner: principal("bob"), subaccount: [] } } } },
      { Ok: { Completed: { source_io_block: 17n, source_account: { owner: alice, subaccount: [selected] } } } },
      { Ok: { Completed: { source_io_block: 44n, source_account: { owner: alice, subaccount: [otherSubaccount] } } } },
      { Err: { Busy: null } },
      { Ok: { RateLimited: { retry_at_nanos: 123n } } },
      { Ok: { Idle: null } },
      new Error("worker transport unavailable"),
      { Err: { Generic: { code: 7n } } },
      { Ok: { Completed: { source_io_block: 44n, source_account: { owner: alice, subaccount: [selected] } } } },
    ],
  });
  await fixture.controller.ready;
  await fixture.listeners["form:submit"]({ preventDefault() {} });
  assert.equal(fixture.transfers(), 1);
  assert.match(fixture.status.textContent, /staged.*pending/i);

  for (let i = 0; i < 8; i += 1) {
    await fixture.listeners["process:click"]();
    assert.doesNotMatch(fixture.status.textContent, /^Completed$/);
    assert.match(fixture.status.textContent, /staged.*pending/i);
    assert.equal(fixture.transfers(), 1);
  }
  await fixture.listeners["process:click"]();
  assert.equal(fixture.status.textContent, "Completed");
  assert.equal(fixture.transfers(), 1, "manual checks never stage another transfer");
});

test("automatic latest completion is cached before a deliberate same-amount redemption", async () => {
  const store = memoryStore();
  const selected = new Uint8Array(32).fill(2);
  const owner = principal("automatic");
  let transferBlock = 44n;
  let latest = [];
  let consents = 0;
  let transfers = 0;
  let now = 100n;
  const common = {
    ledger: {
      icrc1_fee: async () => 10n,
      icrc1_balance_of: async () => 10_000n,
      icrc1_transfer: async () => { transfers += 1; return { Ok: transferBlock++ }; },
    },
    stream: {
      get_redemption_staging_account: async () => ({ owner: "stream", subaccount: [] }),
      get_minimum_redemption_io_e8s: async () => 20n,
      process_redemptions: async () => ({ Ok: { Idle: null } }),
      get_status: async () => ({ last_completed_redemption: latest }),
    },
    selectedSubaccount: selected,
    ioAmountE8s: 100n,
    session: {
      identity: { getPrincipal: () => owner },
      network: "local",
      requestTransferConsent: async () => { consents += 1; return true; },
    },
    storage: store,
    nowNanos: () => now++,
  };
  const staged = await consentStageAndProcessRedemption(common);
  assert.equal(staged.transferBlock, 44n);
  latest = [{ source_io_block: 44n, source_account: { owner, subaccount: [selected] } }];

  const recognizedAfterReload = await consentStageAndProcessRedemption(common);
  assert.equal(recognizedAfterReload.processingPending, false);
  assert.equal(recognizedAfterReload.transferBlock, 44n);
  assert.equal(transfers, 1);

  latest = [];
  const deliberateSecond = await consentStageAndProcessRedemption(common);
  assert.equal(deliberateSecond.transferBlock, 45n);
  assert.equal(deliberateSecond.processingPending, true);
  assert.equal(transfers, 2);
  assert.equal(consents, 2);
});

test("mounted form reload recognizes a still-available own latest completion without transferring", async () => {
  const storage = memoryStore();
  let latest = [];
  const first = mountedFixture({
    storage,
    workerResults: [{ Ok: { Pending: null } }],
    latestCompletion: () => latest,
  });
  await first.controller.ready;
  await first.listeners["form:submit"]({ preventDefault() {} });
  assert.equal(first.transfers(), 1);
  latest = [{
    source_io_block: 44n,
    source_account: { owner: first.owner, subaccount: [first.selected] },
  }];

  const reloaded = mountedFixture({ storage, workerResults: [], latestCompletion: () => latest });
  await reloaded.controller.ready;
  assert.equal(reloaded.status.textContent, "Completed");
  assert.equal(reloaded.transfers(), 0);
});
