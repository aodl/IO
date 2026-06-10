#!/usr/bin/env bash
set -euo pipefail

# optional local-only dry-run helper; this does not deploy, install, upgrade, or call canisters

evidence_file="${1:-deploy/local-sns-rehearsal/canister-ids.local.toml}"

if [ ! -f "$evidence_file" ]; then
  printf 'missing local evidence file: %s\n' "$evidence_file" >&2
  exit 2
fi

cargo run -p xtask -- validate_local_sns_ledger

toml_value() {
  section="$1"
  key="$2"
  awk -F '=' -v section="[$section]" -v key="$key" '
    $0 == section { in_section = 1; next }
    /^\[/ { in_section = 0 }
    in_section && $1 ~ "^[[:space:]]*" key "[[:space:]]*$" {
      value = $2
      sub(/^[[:space:]]*"/, "", value)
      sub(/"[[:space:]]*$/, "", value)
      print value
      exit
    }
  ' "$evidence_file"
}

sns_governance="$(toml_value sns_canisters governance)"
sns_ledger="$(toml_value sns_canisters ledger)"
sns_index="$(toml_value sns_canisters index)"
io_stream_manager="$(toml_value io_dapp_canisters io_stream_manager)"
io_nns_neuron_manager="$(toml_value io_dapp_canisters io_nns_neuron_manager)"

printf 'Local SNS evidence validated from %s\n' "$evidence_file"
printf 'Use these local SNS principals only for DryRun or LocalOfficialSns mode.\n'
printf 'io_stream_manager local dapp: %s\n' "$io_stream_manager"
printf 'io_nns_neuron_manager local dapp: %s\n' "$io_nns_neuron_manager"
printf 'io_stream_manager SNS constructor wiring:\n'
printf '  io_ledger_principal_text = opt "%s"\n' "$sns_ledger"
printf '  io_index_principal_text = opt "%s"\n' "$sns_index"
printf '  io_sns_ledger_principal_text = opt "%s"\n' "$sns_ledger"
printf '  io_sns_index_principal_text = opt "%s"\n' "$sns_index"
printf '  sns_governance_principal_text = opt "%s"\n' "$sns_governance"
