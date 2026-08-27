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
  const rawNumerator = rate?.backing_numerator_e8s;
  const rawDenominator = rate?.claim_denominator_e8s;
  if (rawNumerator === undefined || rawDenominator === undefined || rawDenominator === 0n) return MISSING;
  const numerator = Number(rawNumerator);
  const denominator = Number(rawDenominator);
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
