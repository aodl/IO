import test from "node:test";
import assert from "node:assert/strict";
import {
  connectWalletAdapter,
  injectedTestingAdapter,
  resolveWalletSession,
} from "../src/app/wallet-adapter.js";

const identity = { getPrincipal: () => "principal" };
const consent = async () => true;

test("supported adapter supplies identity, canonical subaccount, network and consent", async () => {
  const session = await connectWalletAdapter({
    connect: async () => ({
      identity,
      selectedSubaccount: new Uint8Array(32).fill(7),
      network: "local",
      requestApprovalConsent: consent,
    }),
  }, "local");
  assert.equal(session.identity, identity);
  assert.equal(session.selectedSubaccount.length, 32);
  assert.equal(session.network, "local");
  assert.equal(await session.requestApprovalConsent({}), true);
});

test("adapter rejects malformed subaccount, wrong network and missing consent", async () => {
  const connect = (value) => connectWalletAdapter({ connect: async () => value }, "local");
  await assert.rejects(connect({ identity, selectedSubaccount: new Uint8Array(31), network: "local", requestApprovalConsent: consent }));
  await assert.rejects(connect({ identity, selectedSubaccount: new Uint8Array(32), network: "ic", requestApprovalConsent: consent }));
  await assert.rejects(connect({ identity, selectedSubaccount: new Uint8Array(32), network: "local" }));
});

test("injected session is explicitly labeled local/testing", async () => {
  const window = {
    ioRedemptionSession: {
      identity,
      selectedSubaccount: new Uint8Array(32),
      network: "local",
      requestApprovalConsent: consent,
    },
  };
  const adapter = injectedTestingAdapter(window, "local");
  const session = await adapter.connect();
  assert.equal(session.adapterKind, "injected-local-testing");
  assert.equal((await resolveWalletSession(window, "local")).adapterKind, "injected-local-testing");
});
