#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# signed Governance lifecycle activation and controller/upgrade proof.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 17-exercise-governance-and-controllers)"
touch "$log_file"
if ! phase_is_done 14-discover-sns-canisters; then
  record_blocker "phase 14 canonical SNS discovery must complete first"
  exit 2
fi
network_url="$(local_network_url)"
identity="$(local_identity_name)"
sns="$(sns_cli)"
require_command_available od
root="$(sns_canister_id root)"
stream="$(toml_string "${REHEARSAL_DIR}/local-vars.toml" local io_stream_manager_canister)"
nns_manager="$(toml_string "${REHEARSAL_DIR}/local-vars.toml" local io_nns_neuron_manager_canister)"

for canister in "$stream" "$nns_manager" \
  "$(toml_string "${REHEARSAL_DIR}/local-vars.toml" local io_historian_canister)" \
  "$(toml_string "${REHEARSAL_DIR}/local-vars.toml" local frontend_canister)"; do
  info="$(dfx canister info --network "$network_url" --identity "$identity" "$canister" 2>&1)"
  printf '%s\n' "$info" >> "$log_file"
  controllers="$(printf '%s\n' "$info" | sed -n 's/^Controllers: //p' | xargs)"
  if [ "$controllers" != "$root" ]; then
    record_blocker "local dapp ${canister} controllers are '${controllers}', expected SNS Root only"
    exit 2
  fi
done

if ! phase_is_done 17-upgrade-attempted; then
  before_hash="$(dfx canister status --network "$network_url" --identity "$identity" "$stream" 2>&1 | sed -n 's/^Module hash: 0x//p')"
  set +e
  run_logged "$log_file" "$sns" --identity "$identity" --network "$network_url" \
    upgrade-sns-controlled-canister --sns-neuron-id "$(runtime_value governance sns_neuron_subaccount_hex)" \
    --target-canister-id "$stream" --wasm-path "${REPO_ROOT}/release-artifacts/io_stream_manager.wasm.gz" \
    --proposal-url 'https://example.invalid/io-local-rehearsal/stream-upgrade' \
    --summary 'Local-only exact release stream-manager upgrade through SNS Governance and Root.'
  upgrade_status=$?
  set -e
  after_hash="$(dfx canister status --network "$network_url" --identity "$identity" "$stream" 2>&1 | sed -n 's/^Module hash: 0x//p')"
  inline_proposal_id="none"
  if [ "$upgrade_status" -ne 0 ]; then
    inline_proposal_id="$(submit_inline_sns_upgrade "$log_file" \
      'Upgrade IO stream inline' \
      'Local-only inline exact release Wasm proposal through SNS Governance and Root; this bypasses only the unavailable upload store, not governance.' \
      "$stream" "${REPO_ROOT}/release-artifacts/io_stream_manager.wasm")"
    wait_sns_proposal "$log_file" "$inline_proposal_id"
    after_hash="$(dfx canister status --network "$network_url" --identity "$identity" "$stream" 2>&1 | sed -n 's/^Module hash: 0x//p')"
  fi
  final_controllers="$(dfx canister info --network "$network_url" --identity "$identity" "$stream" 2>&1 | sed -n 's/^Controllers: //p' | xargs)"
  if [ "$final_controllers" != "$root" ]; then
    record_blocker "stream controllers changed during SNS-governed upgrade: ${final_controllers}"
    exit 2
  fi
  mark_phase_done 17-upgrade-attempted "cli_exit_status=${upgrade_status} inline_proposal_id=${inline_proposal_id} before=${before_hash} after=${after_hash} controllers=${final_controllers}; see ${log_file}"
fi

# The Candid paths cannot be derived from principals, so register each manager explicitly.
if ! phase_is_done 17-stream-function-registered; then
  function_id="$(runtime_value governance stream_lifecycle_function_id)"
  action="variant { AddGenericNervousSystemFunction = record { id = ${function_id} : nat64; name = \"Set IO stream lifecycle\"; description = opt \"Pause or unpause the local IO stream through authenticated SNS Governance.\"; function_type = opt variant { GenericNervousSystemFunction = record { validator_canister_id = opt principal \"${stream}\"; target_canister_id = opt principal \"${stream}\"; validator_method_name = opt \"validate_set_paused\"; target_method_name = opt \"set_paused\"; topic = opt variant { CriticalDappOperations } } } } }"
  proposal_id="$(submit_sns_proposal "$log_file" 'Register IO stream lifecycle' 'Local-only registration of the exact stream validator and execution methods.' "$action")"
  wait_sns_proposal "$log_file" "$proposal_id"
  mark_phase_done 17-stream-function-registered "function_id=${function_id} proposal_id=${proposal_id}"
fi
if ! phase_is_done 17-stream-activated; then
  function_id="$(runtime_value governance stream_lifecycle_function_id)"
  action="variant { ExecuteGenericNervousSystemFunction = record { function_id = ${function_id} : nat64; payload = blob \"DIDL\\00\\01~\\00\" } }"
  proposal_id="$(submit_sns_proposal "$log_file" 'Activate IO stream' 'Local-only authenticated transition from Paused to Ready.' "$action")"
  wait_sns_proposal "$log_file" "$proposal_id"
  status="$(dfx canister call --network "$network_url" --identity "$identity" --query --candid \
    "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" get_status '()')"
  printf '%s\n' "$status" >> "$log_file"
  printf '%s' "$status" | grep -q Ready || { record_blocker 'stream did not enter Ready through SNS Governance'; exit 2; }
  mark_phase_done 17-stream-activated "function_id=${function_id} proposal_id=${proposal_id}"
fi

if ! phase_is_done 17-nns-function-registered; then
  function_id="$(runtime_value governance nns_lifecycle_function_id)"
  action="variant { AddGenericNervousSystemFunction = record { id = ${function_id} : nat64; name = \"Set IO NNS manager lifecycle\"; description = opt \"Pause or unpause the local IO NNS manager through authenticated SNS Governance.\"; function_type = opt variant { GenericNervousSystemFunction = record { validator_canister_id = opt principal \"${nns_manager}\"; target_canister_id = opt principal \"${nns_manager}\"; validator_method_name = opt \"validate_set_paused\"; target_method_name = opt \"set_paused\"; topic = opt variant { CriticalDappOperations } } } } }"
  proposal_id="$(submit_sns_proposal "$log_file" 'Register IO NNS manager lifecycle' 'Local-only registration of the exact NNS manager validator and execution methods.' "$action")"
  wait_sns_proposal "$log_file" "$proposal_id"
  mark_phase_done 17-nns-function-registered "function_id=${function_id} proposal_id=${proposal_id}"
fi
if ! phase_is_done 17-nns-activated; then
  function_id="$(runtime_value governance nns_lifecycle_function_id)"
  action="variant { ExecuteGenericNervousSystemFunction = record { function_id = ${function_id} : nat64; payload = blob \"DIDL\\00\\01~\\00\" } }"
  proposal_id="$(submit_sns_proposal "$log_file" 'Activate IO NNS manager' 'Local-only authenticated transition from Paused to Ready.' "$action")"
  wait_sns_proposal "$log_file" "$proposal_id"
  status="$(dfx canister call --network "$network_url" --identity "$identity" --query --candid \
    "${REPO_ROOT}/canisters/io_nns_neuron_manager/io_nns_neuron_manager.did" "$nns_manager" get_status '()')"
  printf '%s\n' "$status" >> "$log_file"
  printf '%s' "$status" | grep -q Ready || { record_blocker 'NNS manager did not enter Ready through SNS Governance'; exit 2; }
  mark_phase_done 17-nns-activated "function_id=${function_id} proposal_id=${proposal_id}"
fi

mark_phase_done 17-exercise-governance-and-controllers "controllers checked; upgrade result and authenticated lifecycle proposals recorded"
printf 'Governance activation complete; rerun phase 15 for production redemption, then phase 16 for final histories.\n'
