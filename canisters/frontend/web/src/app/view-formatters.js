export const MISSING = "-";

export function formatTokenE8s(value, symbol = "") {
  if (value === null || value === undefined) return MISSING;
  const numeric = Number(value);
  if (!Number.isFinite(numeric)) return MISSING;
  const whole = numeric / 100_000_000;
  const text = new Intl.NumberFormat("en-US", {
    maximumFractionDigits: whole >= 100 ? 2 : 4,
  }).format(whole);
  return symbol ? `${text} ${symbol}` : text;
}

export function formatRatio(rate) {
  if (!rate || rate.liquid_icp_per_io_e8s_denominator === 0n) return MISSING;
  const numerator = Number(rate.liquid_icp_per_io_e8s_numerator);
  const denominator = Number(rate.liquid_icp_per_io_e8s_denominator);
  if (!Number.isFinite(numerator) || !Number.isFinite(denominator) || denominator === 0) return MISSING;
  return new Intl.NumberFormat("en-US", { maximumFractionDigits: 6 }).format(numerator / denominator);
}

export function formatTimestampNanos(value) {
  if (value === null || value === undefined) return MISSING;
  const numeric = Number(value);
  if (!Number.isFinite(numeric) || numeric <= 0) return MISSING;
  return new Intl.DateTimeFormat("en-US", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(Math.floor(numeric / 1_000_000)));
}

export function variantLabel(value) {
  if (!value || typeof value !== "object") return MISSING;
  const [key] = Object.keys(value);
  return key ? key.replace(/([a-z])([A-Z])/g, "$1 $2") : MISSING;
}

export function statusClass(value) {
  const label = variantLabel(value).toLowerCase();
  if (label.includes("mismatch") || label.includes("failed") || label.includes("broken")) return "bad";
  if (label.includes("unknown") || label.includes("unobserved") || label.includes("retry")) return "warn";
  return "ok";
}
