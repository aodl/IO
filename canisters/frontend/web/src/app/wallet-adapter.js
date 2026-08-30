function requireSession(session, expectedNetwork) {
  if (!session?.identity || typeof session.identity.getPrincipal !== "function") {
    throw new Error("wallet adapter must supply an authenticated identity");
  }
  if (!(session.selectedSubaccount instanceof Uint8Array) || session.selectedSubaccount.length !== 32) {
    throw new Error("wallet adapter must supply one canonical 32-byte effective subaccount");
  }
  if (typeof session.network !== "string" || session.network !== expectedNetwork) {
    throw new Error("wallet adapter network does not match the configured network");
  }
  if (typeof session.requestTransferConsent !== "function") {
    throw new Error("wallet adapter must supply explicit transfer consent");
  }
  return Object.freeze({
    identity: session.identity,
    selectedSubaccount: new Uint8Array(session.selectedSubaccount),
    network: session.network,
    requestTransferConsent: session.requestTransferConsent.bind(session),
    adapterKind: session.adapterKind || "wallet",
  });
}

export async function connectWalletAdapter(adapter, expectedNetwork) {
  if (!adapter || typeof adapter.connect !== "function") return null;
  return requireSession(await adapter.connect(), expectedNetwork);
}

export async function resolveWalletSession(window, expectedNetwork) {
  return connectWalletAdapter(window.ioWalletAdapter, expectedNetwork);
}
