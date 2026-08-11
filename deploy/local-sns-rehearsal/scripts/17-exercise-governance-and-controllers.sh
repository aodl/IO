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
historian="$(toml_string "${REHEARSAL_DIR}/local-vars.toml" local io_historian_canister)"

for canister in "$stream" "$nns_manager" \
  "$historian" \
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
  target_hash="$(manifest_artifact_value io_historian raw_wasm_sha256)"
  payload_hash="$(manifest_artifact_value io_historian gz_wasm_sha256)"
  before_hash="$(dfx canister info --network "$network_url" --identity "$identity" "$historian" 2>&1 | sed -n 's/^Module hash: 0x//p')"
  set +e
  run_logged "$log_file" "$sns" --identity "$identity" --network "$network_url" \
    upgrade-sns-controlled-canister --sns-neuron-id "$(runtime_value governance sns_neuron_subaccount_hex)" \
    --target-canister-id "$historian" --wasm-path "${REPO_ROOT}/release-artifacts/io_historian.wasm.gz" \
    --proposal-url 'https://forum.dfinity.org/t/io-local-rehearsal/0' \
    --summary 'Local-only prior-to-current exact release historian upgrade through SNS Governance and Root.'
  upgrade_status=$?
  set -e
  after_hash="$(dfx canister info --network "$network_url" --identity "$identity" "$historian" 2>&1 | sed -n 's/^Module hash: 0x//p')"
  inline_proposal_id="none"
  if [ "$upgrade_status" -ne 0 ]; then
    inline_proposal_id="$(submit_inline_sns_upgrade "$log_file" \
      'Upgrade IO historian inline' \
      'Local-only inline exact gzip release Wasm proposal through SNS Governance and Root; the inline payload avoids only the unavailable upload store and remains an authentic governance proposal.' \
      "$historian" "${REPO_ROOT}/release-artifacts/io_historian.wasm.gz")"
    wait_sns_proposal "$log_file" "$inline_proposal_id"
    after_hash="$(dfx canister info --network "$network_url" --identity "$identity" "$historian" 2>&1 | sed -n 's/^Module hash: 0x//p')"
  fi
  if [ "$before_hash" = "$after_hash" ] || [ "$after_hash" != "$target_hash" ]; then
    record_blocker "SNS-controlled historian upgrade did not change to the exact current release module: before=${before_hash} after=${after_hash} expected=${target_hash}"
    exit 2
  fi
  final_controllers="$(dfx canister info --network "$network_url" --identity "$identity" "$historian" 2>&1 | sed -n 's/^Controllers: //p' | xargs)"
  if [ "$final_controllers" != "$root" ]; then
    record_blocker "historian controllers changed during SNS-governed upgrade: ${final_controllers}"
    exit 2
  fi
  mark_phase_done 17-upgrade-attempted "target=${historian} cli_exit_status=${upgrade_status} proposal_id=${inline_proposal_id} before=${before_hash} payload_gzip_sha256=${payload_hash} after=${after_hash} release_manifest_raw_sha256=${target_hash} controllers=${final_controllers}; see ${log_file}"
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
  printf '%s' "$status" | grep -q Ready || {
    record_blocker 'NNS manager activation proposal executed through SNS Governance but readiness remained Paused despite the source-shaped staging and protected-neuron fixture; inspect the canonical readiness error'
    exit 2
  }
  printf '%s' "$status" | grep -q 'two_week_maturity_baseline_reconciled = true' || {
    record_blocker 'NNS manager entered Ready without the required recorded reward-backing baseline'
    exit 2
  }
  fixture="${REHEARSAL_DIR}/generated/nns-readiness-fixture.toml"
  require_file "$fixture"
  two_week_neuron_id="$(toml_number "$fixture" reward_backing_neuron id)"
  seeded_principal="$(toml_number "$fixture" reward_backing_neuron seeded_principal_e8s)"
  approved_delay="$(toml_number "$fixture" reward_backing_neuron dissolve_delay_seconds)"
  icp_ledger="$(runtime_value nns icp_ledger)"
  checkout="$(official_checkout)"
  ledger_did="${checkout}/rs/ledger_suite/icrc1/ledger/ledger.did"
  governance_did="${checkout}/rs/nns/governance/canister/governance.did"
  jupiter_balance="$(dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$ledger_did" "$icp_ledger" icrc1_balance_of \
    "(record { owner = principal \"${nns_manager}\"; subaccount = null })" | tr -d '()_ :nat[:space:]')"
  two_week_balance="$(dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$ledger_did" "$icp_ledger" icrc1_balance_of \
    "(record { owner = principal \"${nns_manager}\"; subaccount = opt blob \"$(hex_blob_literal 0303030303030303030303030303030303030303030303030303030303030303)\" })" | tr -d '()_ :nat[:space:]')"
  if [ "$jupiter_balance" -lt 20000 ] || [ "$two_week_balance" -lt 10000 ]; then
    record_blocker "NNS manager Ready observation has insufficient staging balances: Jupiter=${jupiter_balance} two-week=${two_week_balance}"
    exit 2
  fi
  neuron_info="$(dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$governance_did" "$(runtime_value nns governance)" get_neuron_info \
    "(${two_week_neuron_id} : nat64)")"
  printf '%s\n' "$neuron_info" >> "$log_file"
  neuron_info_compact="$(printf '%s' "$neuron_info" | tr -d '_')"
  printf '%s' "$neuron_info_compact" | grep -q "cached_neuron_stake_e8s = ${seeded_principal}" || {
    record_blocker 'reward-backing neuron observation no longer matches seeded principal'
    exit 2
  }
  printf '%s' "$neuron_info_compact" | grep -q "dissolve_delay_seconds = ${approved_delay}" || {
    record_blocker 'reward-backing neuron observation no longer matches approved dissolve delay'
    exit 2
  }
  mark_phase_done 17-nns-activated "function_id=${function_id} proposal_id=${proposal_id} lifecycle=Ready baseline_reconciled=true reward_backing_neuron_id=${two_week_neuron_id} seeded_principal_e8s=${seeded_principal} dissolve_delay_seconds=${approved_delay} jupiter_staging_e8s=${jupiter_balance} two_week_staging_e8s=${two_week_balance}"
fi

mark_phase_done 17-exercise-governance-and-controllers "controllers checked; upgrade result and authenticated lifecycle proposals recorded"
printf 'Governance activation complete; rerun phase 15 for production redemption, then phase 16 for final histories.\n'
