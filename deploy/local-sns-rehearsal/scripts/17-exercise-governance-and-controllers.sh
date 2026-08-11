#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# signed Governance lifecycle activation and controller/upgrade proof.
# The maintained `sns upgrade-sns-controlled-canister` command remains recorded
# as blocked only by local chunk-store authorization; the inline proposal below
# still follows the authentic SNS Governance -> Root execution path.
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
require_command_available od
require_command_available didc
root="$(sns_canister_id root)"
vars_file="$(local_vars_file)"
stream="$(toml_string "$vars_file" local io_stream_manager_canister)"
nns_manager="$(toml_string "$vars_file" local io_nns_neuron_manager_canister)"
historian="$(toml_string "$vars_file" local io_historian_canister)"
frontend="$(toml_string "$vars_file" local frontend_canister)"

for canister in "$stream" "$nns_manager" \
  "$historian" \
  "$frontend"; do
  info="$(dfx canister info --network "$network_url" --identity "$identity" "$canister" 2>&1)"
  printf '%s\n' "$info" >> "$log_file"
  controllers="$(printf '%s\n' "$info" | sed -n 's/^Controllers: //p' | xargs)"
  if [ "$controllers" != "$root" ]; then
    record_blocker "local dapp ${canister} controllers are '${controllers}', expected SNS Root only"
    exit 2
  fi
done

if ! phase_is_done 17-upgrade-attempted; then
  raw_hash="$(manifest_artifact_value io_historian raw_wasm_sha256)"
  payload_hash="$(manifest_artifact_value io_historian gz_wasm_sha256)"
  before_hash="$(dfx canister info --network "$network_url" --identity "$identity" "$historian" 2>&1 | sed -n 's/^Module hash: 0x//p')"
  bundle_dir="${IO_LOCAL_SNS_BUNDLE_DIR:-}"
  require_file "${bundle_dir}/manifest.toml"
  root="$(sns_canister_id root)"
  governance="$(sns_canister_id governance)"
  ledger="$(sns_canister_id ledger)"
  index="$(sns_canister_id index)"
  swap="$(sns_canister_id swap)"
  reserve_subaccount="$(runtime_value accounts reserve_subaccount_hex)"
  liquid_subaccount="$(runtime_value accounts liquid_icp_subaccount_hex)"
  fixture="${GENERATED_DIR}/nns-readiness-fixture.toml"
  require_file "$fixture"
  reward_backing_neuron_id="$(toml_number "$fixture" reward_backing_neuron id)"
  two_year_neuron_id="$(toml_number "$fixture" two_year_protected_neuron id)"
  config_file="$(mktemp "${GENERATED_DIR}/historian-observation-config.XXXXXX.did")"
  cat > "$config_file" <<EOF
(opt record {
  stream_manager = principal "${stream}";
  nns_manager = principal "${nns_manager}";
  sns_root = principal "${root}";
  sns_governance = principal "${governance}";
  sns_ledger = principal "${ledger}";
  sns_index = principal "${index}";
  icp_ledger = principal "$(runtime_value nns icp_ledger)";
  nns_governance = principal "$(runtime_value nns governance)";
  reward_backing_neuron_id = ${reward_backing_neuron_id} : nat64;
  two_year_neuron_id = ${two_year_neuron_id} : nat64;
  protocol_io_reserve = record { owner = principal "${stream}"; subaccount = opt blob "$(hex_blob_literal "$reserve_subaccount")" };
  liquid_icp_reserve = record { owner = principal "${stream}"; subaccount = opt blob "$(hex_blob_literal "$liquid_subaccount")" };
  excluded_io_accounts = vec { record { name = "sns-governance"; account = record { owner = principal "${governance}"; subaccount = null } } };
  history_accounts = vec {
    record { name = "protocol-reserve"; account = record { owner = principal "${stream}"; subaccount = opt blob "$(hex_blob_literal "$reserve_subaccount")" } };
    record { name = "sns-governance"; account = record { owner = principal "${governance}"; subaccount = null } };
  };
  expected_modules = vec {
    record { role = variant { StreamManager }; canister_id = principal "${stream}"; wasm_sha256 = blob "$(hex_blob_literal "$(manifest_artifact_value io_stream_manager raw_wasm_sha256)")" };
    record { role = variant { NnsManager }; canister_id = principal "${nns_manager}"; wasm_sha256 = blob "$(hex_blob_literal "$(manifest_artifact_value io_nns_neuron_manager raw_wasm_sha256)")" };
    record { role = variant { Historian }; canister_id = principal "${historian}"; wasm_sha256 = blob "$(hex_blob_literal "$raw_hash")" };
    record { role = variant { Frontend }; canister_id = principal "${frontend}"; wasm_sha256 = blob "$(hex_blob_literal "$(manifest_artifact_value frontend raw_wasm_sha256)")" };
    record { role = variant { SnsGovernance }; canister_id = principal "${governance}"; wasm_sha256 = blob "$(hex_blob_literal "$(toml_string "${bundle_dir}/manifest.toml" artifacts sns_governance_sha256)")" };
    record { role = variant { SnsRoot }; canister_id = principal "${root}"; wasm_sha256 = blob "$(hex_blob_literal "$(toml_string "${bundle_dir}/manifest.toml" artifacts sns_root_sha256)")" };
    record { role = variant { SnsLedger }; canister_id = principal "${ledger}"; wasm_sha256 = blob "$(hex_blob_literal "$(toml_string "${bundle_dir}/manifest.toml" artifacts sns_ledger_sha256)")" };
    record { role = variant { SnsIndex }; canister_id = principal "${index}"; wasm_sha256 = blob "$(hex_blob_literal "$(toml_string "${bundle_dir}/manifest.toml" artifacts sns_index_sha256)")" };
    record { role = variant { SnsSwap }; canister_id = principal "${swap}"; wasm_sha256 = blob "$(hex_blob_literal "$(toml_string "${bundle_dir}/manifest.toml" artifacts sns_swap_sha256)")" };
  };
  reward_share_capable_governance_sha256 = opt blob "$(hex_blob_literal "$(toml_string "${bundle_dir}/manifest.toml" artifacts sns_governance_sha256)")";
  refresh_interval_seconds = 60 : nat64;
})
EOF
  upgrade_arg_hex="$(didc encode --defs "${REPO_ROOT}/canisters/io_historian/io_historian.did" --types '(opt ObservationConfig)' < "$config_file")"
  inline_proposal_id="$(submit_inline_sns_upgrade "$log_file" \
    'Upgrade and configure IO historian' \
    'Local-only exact gzip release Wasm plus typed observation configuration through SNS Governance and Root. Inline payload avoids only the unavailable chunk-store bootstrap and remains an authentic governance proposal.' \
    "$historian" "${REPO_ROOT}/release-artifacts/io_historian.wasm.gz" "$upgrade_arg_hex")"
  wait_sns_proposal "$log_file" "$inline_proposal_id"
  after_hash="$(dfx canister info --network "$network_url" --identity "$identity" "$historian" 2>&1 | sed -n 's/^Module hash: 0x//p')"
  if [ "$before_hash" = "$after_hash" ] || [ "$after_hash" != "$raw_hash" ]; then
    record_blocker "SNS-controlled historian upgrade did not change to the exact current release module: before=${before_hash} after=${after_hash} expected=${raw_hash}"
    exit 2
  fi
  final_controllers="$(dfx canister info --network "$network_url" --identity "$identity" "$historian" 2>&1 | sed -n 's/^Controllers: //p' | xargs)"
  if [ "$final_controllers" != "$root" ]; then
    record_blocker "historian controllers changed during SNS-governed upgrade: ${final_controllers}"
    exit 2
  fi
  mark_phase_done 17-upgrade-attempted "target=${historian} path=inline-governance-root proposal_id=${inline_proposal_id} before=${before_hash} payload_gzip_sha256=${payload_hash} after=${after_hash} release_manifest_raw_sha256=${raw_hash} typed_observation_config=true controllers=${final_controllers}; see ${log_file}"
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
  fixture="${GENERATED_DIR}/nns-readiness-fixture.toml"
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
  printf '%s' "$neuron_info_compact" | grep -q "stakee8s = ${seeded_principal}" || {
    record_blocker 'reward-backing neuron observation no longer matches seeded principal'
    exit 2
  }
  printf '%s' "$neuron_info_compact" | grep -q "dissolvedelayseconds = ${approved_delay}" || {
    record_blocker 'reward-backing neuron observation no longer matches approved dissolve delay'
    exit 2
  }
  mark_phase_done 17-nns-activated "function_id=${function_id} proposal_id=${proposal_id} lifecycle=Ready baseline_reconciled=true reward_backing_neuron_id=${two_week_neuron_id} seeded_principal_e8s=${seeded_principal} dissolve_delay_seconds=${approved_delay} jupiter_staging_e8s=${jupiter_balance} two_week_staging_e8s=${two_week_balance}"
fi

mark_phase_done 17-exercise-governance-and-controllers "controllers checked; upgrade result and authenticated lifecycle proposals recorded"
printf 'Governance activation complete; rerun phase 15 for production redemption, then phase 16 for final histories.\n'
