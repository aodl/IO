import { formatRatio, formatTimestampNanos, formatTokenE8s, variantLabel } from "../app/view-formatters.js";

function completenessWarnings(protocol) {
  const completeness = protocol?.completeness;
  if (!completeness) return ["Protocol snapshot is incomplete"];
  const missing = Object.entries(completeness)
    .filter(([, value]) => value !== true)
    .map(([key]) => key.replaceAll("_", " "));
  return missing.length ? [`Incomplete data: ${missing.join(", ")}`] : [];
}

function hasVariant(value, key) {
  return value && typeof value === "object" && Object.hasOwn(value, key);
}

function sourceHealthWarnings(sourceHealth) {
  return (sourceHealth ?? [])
    .filter((source) => !hasVariant(source.freshness, "Fresh"))
    .map((source) => {
      const status = variantLabel(source.freshness).toLowerCase();
      return `${source.source ?? "source"} ${status}: ${opt(source.error) ?? "observation unavailable"}`;
    });
}

function singlePointSeries(label, value) {
  const unwrapped = opt(value);
  return unwrapped === null || unwrapped === undefined ? [] : [{ label, value: Number(unwrapped) }];
}

function opt(value) {
  return Array.isArray(value) ? value[0] : value;
}

export function transformDashboard(loadResult) {
  const warnings = [];
  if (!loadResult.configured) warnings.push("Data unavailable: historian is not configured.");
  if (loadResult.outdated) warnings.push("Historian interface may be outdated for this frontend.");
  for (const failure of loadResult.failures ?? []) {
    warnings.push(`${failure.label}: ${failure.error}`);
  }

  const dashboard = loadResult.dashboard;
  const protocol = dashboard?.protocol ?? {};
  const redemptionRate = opt(protocol.redemption_rate);
  warnings.push(...completenessWarnings(protocol));
  warnings.push(...sourceHealthWarnings(dashboard?.source_health));

  const index = opt(dashboard?.index);
  const indexAccounts = index?.accounts ?? [];

  return {
    statusLine: loadResult.status
      ? `Historian ${loadResult.status.version}; schema ${loadResult.status.schema_version}`
      : "Historian data unavailable",
    lastUpdated: `Last updated: ${formatTimestampNanos(opt(protocol.observed_at_timestamp_nanos))}`,
    metrics: {
      redemptionRate: formatRatio(redemptionRate),
      redemptionRateHint: "liquid ICP per IO",
      liquidReserve: formatTokenE8s(opt(protocol.liquid_icp_reserve_e8s), ""),
      redeemableSupply: formatTokenE8s(opt(protocol.redeemable_io_supply_e8s), ""),
      indexHealth: index ? "Observed" : "-",
      indexHealthHint: index ? `${indexAccounts.length} bounded Account histories` : "Observation unavailable",
    },
    charts: {
      rate: singlePointSeries("latest", redemptionRate?.liquid_icp_e8s),
      supply: singlePointSeries("latest", protocol.total_io_supply_e8s),
    },
    lists: {
      artifacts: dashboard?.canisters ?? [],
      sourceHealth: dashboard?.source_health ?? [],
    },
    warnings,
  };
}

export function artifactSummary(record) {
  return `${variantLabel(record.role)}: ${variantLabel(record.module_match)}`;
}

export function sourceHealthSummary(record) {
  return `${record.source ?? "-"}: ${variantLabel(record.freshness)} - ${opt(record.error) ?? ""}`;
}
