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
  if (typeof session.requestApprovalConsent !== "function") {
    throw new Error("wallet adapter must supply explicit approval consent");
  }
  return Object.freeze({
    identity: session.identity,
    selectedSubaccount: new Uint8Array(session.selectedSubaccount),
    network: session.network,
    requestApprovalConsent: session.requestApprovalConsent.bind(session),
    adapterKind: session.adapterKind || "wallet",
  });
}

export async function connectWalletAdapter(adapter, expectedNetwork) {
  if (!adapter || typeof adapter.connect !== "function") return null;
  return requireSession(await adapter.connect(), expectedNetwork);
}

export function injectedTestingAdapter(window, expectedNetwork) {
  const injected = window.ioRedemptionSession;
  if (!injected) return null;
  return {
    connect: async () => requireSession({
      ...injected,
      network: injected.network || expectedNetwork,
      adapterKind: "injected-local-testing",
    }, expectedNetwork),
  };
}

export async function resolveWalletSession(window, expectedNetwork) {
  const adapter = window.ioWalletAdapter || injectedTestingAdapter(window, expectedNetwork);
  return connectWalletAdapter(adapter, expectedNetwork);
}
