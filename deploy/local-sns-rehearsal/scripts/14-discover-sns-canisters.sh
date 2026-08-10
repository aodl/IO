#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# canonical, restartable local-only SNS discovery.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 14-discover-sns-canisters)"
: > "$log_file"
if ! phase_is_done 13-propose-and-finalize-sns; then
  record_blocker "phase 13 SNS finalization must complete first"
  exit 2
fi
require_command_available jq
network_url="$(local_network_url)"
identity="$(local_identity_name)"
sns="$(sns_cli)"
discovery_json="${REHEARSAL_DIR}/generated/sns-list.json"
"$sns" --identity "$identity" --network "$network_url" list --json > "$discovery_json" 2>> "$log_file"

selected_json="${REHEARSAL_DIR}/generated/sns-canisters.json"
jq -e '[.[] | select(.name == "IO Local Rehearsal")] | if length == 1 then .[0].sns else error("expected exactly one IO Local Rehearsal SNS") end' \
  "$discovery_json" > "$selected_json"
for role in root governance ledger index swap; do
  principal="$(jq -er --arg role "$role" '.[$role].canister_id' "$selected_json")"
  printf '%s=%s\n' "$role" "$principal" | tee -a "$log_file"
done

planned_file="$(runtime_file)"
for role in root governance ledger index swap; do
  planned="$(toml_string "$planned_file" planned_sns "$role")"
  observed="$(jq -er --arg role "$role" '.[$role].canister_id' "$selected_json")"
  if [ -n "$planned" ] && [ "$planned" != "$observed" ]; then
    record_blocker "planned SNS ${role} ${planned} does not match canonical discovery ${observed}"
    exit 2
  fi
done
mark_phase_done 14-discover-sns-canisters "canonical discovery=${selected_json}"
