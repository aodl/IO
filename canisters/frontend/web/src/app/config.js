export const DEFAULT_CONFIG = Object.freeze({
  network: "",
  historianCanisterId: "",
  frontendCanisterId: "",
  assetVersion: "development",
  demoMode: false,
});

export function normalizeConfig(raw = {}) {
  return {
    ...DEFAULT_CONFIG,
    ...raw,
    demoMode: raw.demoMode === true || raw.demoMode === "true",
  };
}

export const runtimeConfig = normalizeConfig(
  typeof __IO_FRONTEND_CONFIG__ === "undefined" ? {} : __IO_FRONTEND_CONFIG__,
);
