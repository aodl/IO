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
if ! phase_is_done 14-neuron-cap-set; then
  governance="$(sns_canister_id governance)"
  parameters="$(dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$(official_checkout)/rs/sns/governance/canister/governance.did" \
    "$governance" get_nervous_system_parameters '(null)')"
  printf '%s\n' "$parameters" >> "$log_file"
  if printf '%s' "$parameters" | grep -Eq 'max_number_of_neurons = opt \(?1_000'; then
    proposal_id=already-set
  else
    action='variant { ManageNervousSystemParameters = record { max_number_of_neurons = opt (1_000 : nat64) } }'
    proposal_id="$(submit_sns_proposal "$log_file" 'Cap local SNS neurons' 'Set the reviewed IO launch bound of at most 1,000 SNS neurons.' "$action")"
    wait_sns_proposal "$log_file" "$proposal_id"
    parameters="$(dfx canister call --network "$network_url" --identity "$identity" --query \
      --candid "$(official_checkout)/rs/sns/governance/canister/governance.did" \
      "$governance" get_nervous_system_parameters '(null)')"
    printf '%s\n' "$parameters" >> "$log_file"
    printf '%s' "$parameters" | grep -Eq 'max_number_of_neurons = opt \(?1_000' || {
      record_blocker 'SNS Governance max_number_of_neurons is not the reviewed 1,000 bound'
      exit 2
    }
  fi
  mark_phase_done 14-neuron-cap-set "proposal_id=${proposal_id} max_number_of_neurons=1000"
fi
mark_phase_done 14-discover-sns-canisters "canonical discovery=${selected_json}"
