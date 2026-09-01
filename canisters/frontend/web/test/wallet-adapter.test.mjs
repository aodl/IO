import test from "node:test";
import assert from "node:assert/strict";
import {
  connectWalletAdapter,
  resolveWalletSession,
} from "../src/app/wallet-adapter.js";

const identity = { getPrincipal: () => "principal" };
const consent = async () => true;

test("supported adapter supplies identity, canonical subaccount, network and transfer consent", async () => {
  const session = await connectWalletAdapter({
    connect: async () => ({
      identity,
      selectedSubaccount: new Uint8Array(32).fill(7),
      network: "local",
      requestTransferConsent: consent,
    }),
  }, "local");
  assert.equal(session.identity, identity);
  assert.equal(session.selectedSubaccount.length, 32);
  assert.equal(session.network, "local");
  assert.equal(await session.requestTransferConsent({}), true);
});

test("adapter rejects malformed subaccount, wrong network and missing consent", async () => {
  const connect = (value) => connectWalletAdapter({ connect: async () => value }, "local");
  await assert.rejects(connect({ identity, selectedSubaccount: new Uint8Array(31), network: "local", requestTransferConsent: consent }));
  await assert.rejects(connect({ identity, selectedSubaccount: new Uint8Array(32), network: "ic", requestTransferConsent: consent }));
  await assert.rejects(connect({ identity, selectedSubaccount: new Uint8Array(32), network: "local" }));
});

test("production resolution uses only ioWalletAdapter", async () => {
  const window = {
    ioWalletAdapter: {
      connect: async () => ({
        identity,
        selectedSubaccount: new Uint8Array(32),
        network: "local",
        requestTransferConsent: consent,
      }),
    },
  };
  assert.equal((await resolveWalletSession(window, "local")).adapterKind, "wallet");
  assert.equal(await resolveWalletSession({}, "local"), null);
});
