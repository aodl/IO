import { formatRatio, formatTimestampNanos, formatTokenE8s, variantLabel } from "../app/view-formatters.js";

function isObserved(value) {
  return value && typeof value === "object" && Object.hasOwn(value, "Observed");
}

function completenessWarnings(protocol) {
  const completeness = protocol?.completeness;
  if (!completeness) return ["Protocol snapshot is incomplete"];
  const missing = Object.entries(completeness)
    .filter(([, value]) => !isObserved(value))
    .map(([key]) => key.replaceAll("_", " "));
  return missing.length ? [`Incomplete data: ${missing.join(", ")}`] : [];
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
  const redemptionRate = opt(dashboard?.redemption_rate) ?? opt(protocol.redemption_rate);
  warnings.push(...completenessWarnings(protocol));

  const indexHealth = dashboard?.index_health ?? [];
  const broken = indexHealth.filter((entry) => entry.invariant_broken_count > 0n || entry.lag_suspected || entry.scan_incomplete);

  return {
    statusLine: loadResult.status
      ? `Historian ${loadResult.status.version}; schema ${loadResult.status.schema_version}`
      : "Historian data unavailable",
    lastUpdated: `Last updated: ${formatTimestampNanos(protocol.last_updated_timestamp_nanos)}`,
    metrics: {
      redemptionRate: formatRatio(redemptionRate),
      redemptionRateHint: "liquid ICP per IO",
      liquidReserve: formatTokenE8s(opt(protocol.liquid_icp_reserve_e8s), ""),
      redeemableSupply: formatTokenE8s(opt(protocol.redeemable_io_supply_e8s), ""),
      indexHealth: broken.length ? `${broken.length} warning` : indexHealth.length ? "Observed" : "-",
      indexHealthHint: indexHealth.length ? `${indexHealth.length} account scans` : "No scan records",
    },
    charts: {
      rate: singlePointSeries("latest", redemptionRate?.liquid_icp_per_io_e8s_numerator),
      supply: singlePointSeries("latest", protocol.total_io_supply_e8s),
    },
    lists: {
      streams: loadResult.optional?.streams?.records ?? [],
      redemptions: loadResult.optional?.redemptions?.records ?? [],
      rewards: loadResult.optional?.rewards?.records ?? [],
      artifacts: dashboard?.release_artifacts ?? [],
    },
    warnings,
  };
}

export function streamSummary(record) {
  return `${record.record_id ?? "-"}: ${variantLabel(record.stream_kind)} ${formatTokenE8s(record.amount_e8s, "ICP")}`;
}

export function redemptionSummary(record) {
  return `${record.record_id ?? "-"}: ${formatTokenE8s(record.io_amount_e8s, "IO")} ${variantLabel(record.phase)}`;
}

export function rewardSummary(record) {
  return `${record.record_id ?? "-"}: ${formatTokenE8s(record.reward_amount_e8s, "IO")} ${variantLabel(record.status)}`;
}

export function artifactSummary(record) {
  return `${record.canister_name ?? "-"}: ${variantLabel(record.status)}`;
}
