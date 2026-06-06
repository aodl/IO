export const REQUIRED_HISTORIAN_METHODS = Object.freeze([
  "get_dashboard_state",
  "get_public_status",
]);

const OPTIONAL_QUERIES = Object.freeze([
  ["streams", "list_streams", { limit: [10n], start_after: [] }],
  ["redemptions", "list_redemptions", { limit: [10n], start_after: [] }],
  ["rewards", "list_rewards", { limit: [10n], start_after: [] }],
]);

export function missingRequiredMethods(actor) {
  return REQUIRED_HISTORIAN_METHODS.filter((method) => typeof actor?.[method] !== "function");
}

async function callQuery(actor, label, method, args = []) {
  try {
    if (typeof actor?.[method] !== "function") {
      return { label, ok: false, error: `Missing historian method ${method}` };
    }
    return { label, ok: true, value: await actor[method](...args) };
  } catch (error) {
    return { label, ok: false, error: error?.message || String(error) };
  }
}

export async function loadHistorianDashboard(actor, config) {
  if (!config?.historianCanisterId || !actor) {
    return {
      configured: false,
      outdated: false,
      dashboard: null,
      status: null,
      optional: {},
      failures: [{ label: "historian", error: "Historian canister is not configured" }],
    };
  }

  const missing = missingRequiredMethods(actor);
  if (missing.length === REQUIRED_HISTORIAN_METHODS.length) {
    return {
      configured: true,
      outdated: true,
      dashboard: null,
      status: null,
      optional: {},
      failures: missing.map((method) => ({ label: method, error: `Missing ${method}` })),
    };
  }

  const requiredResults = await Promise.all([
    callQuery(actor, "dashboard", "get_dashboard_state"),
    callQuery(actor, "status", "get_public_status"),
  ]);
  const optionalResults = await Promise.all(
    OPTIONAL_QUERIES.map(([label, method, request]) => callQuery(actor, label, method, [request])),
  );

  const failures = [...requiredResults, ...optionalResults].filter((result) => !result.ok);
  const optional = Object.fromEntries(
    optionalResults.filter((result) => result.ok).map((result) => [result.label, result.value]),
  );

  return {
    configured: true,
    outdated: missing.length > 0,
    dashboard: requiredResults.find((result) => result.label === "dashboard" && result.ok)?.value ?? null,
    status: requiredResults.find((result) => result.label === "status" && result.ok)?.value ?? null,
    optional,
    failures,
  };
}
