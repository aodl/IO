#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# restartable local-only dapp canister creation/install and NNS Root handoff.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 12-deploy-local-dapps)"
: > "$log_file"
if phase_is_done 12-deploy-local-dapps; then
  printf 'phase 12 already completed: %s\n' "$(cat "$(phase_done_file 12-deploy-local-dapps)")"
  exit 0
fi
if ! phase_is_done 11-build-local-io-canisters; then
  record_blocker "phase 11 exact release verification must complete first"
  exit 2
fi

require_command_available dfx
require_command_available jq
network_url="$(local_network_url)"
identity="$(local_identity_name)"
sns="$(sns_cli)"
vars_file="${REHEARSAL_DIR}/local-vars.toml"
require_file "$vars_file"
stream_args="${REHEARSAL_DIR}/install-args.local/io_stream_manager.did"
nns_args="${REHEARSAL_DIR}/install-args.local/io_nns_neuron_manager.did"
require_file "$stream_args"
require_file "$nns_args"
bundle_dir="${IO_LOCAL_SNS_BUNDLE_DIR:-}"
if [ -z "$bundle_dir" ]; then
  record_blocker "set IO_LOCAL_SNS_BUNDLE_DIR to the reviewed same-source Governance/Root bundle"
  exit 2
fi
require_file "${bundle_dir}/manifest.toml"
governance_hash="$(toml_string "${bundle_dir}/manifest.toml" artifacts sns_governance_source_sha256)"
require_lower_sha256 "SNS-W Governance compressed/source hash" "$governance_hash"
governance_blob="$(hex_blob_literal "$governance_hash")"
governance_sed_blob="$(printf '%s' "$governance_blob" | sed 's/\\/\\\\/g; s/&/\\\&/g')"
if ! grep -q 'expected_sns_governance_module_hash = blob' "$stream_args"; then
  record_blocker "stream install args omit expected_sns_governance_module_hash"
  exit 2
fi
sed -i "s|    expected_sns_governance_module_hash = blob .*;|    expected_sns_governance_module_hash = blob \"${governance_sed_blob}\";|" "$stream_args"
grep -Fq "expected_sns_governance_module_hash = blob \"${governance_blob}\";" "$stream_args" || {
  record_blocker "stream install args do not contain exact SNS-W Governance source hash"
  exit 2
}

work_dir="${REHEARSAL_DIR}/generated/dfx"
mkdir -p "$work_dir"
if [ ! -f "${work_dir}/dfx.json" ]; then
  cp "${REHEARSAL_DIR}/dfx.local.json" "${work_dir}/dfx.json"
fi

declare -A ids
ids[io_stream_manager]="$(toml_string "$vars_file" local io_stream_manager_canister)"
ids[io_nns_neuron_manager]="$(toml_string "$vars_file" local io_nns_neuron_manager_canister)"
ids[io_historian]="$(toml_string "$vars_file" local io_historian_canister)"
ids[frontend]="$(toml_string "$vars_file" local frontend_canister)"

for canister in io_stream_manager io_nns_neuron_manager io_historian frontend; do
  canister_id="${ids[$canister]}"
  case "$canister_id" in TODO*|"") record_blocker "local dapp ID is unresolved: ${canister}"; exit 2 ;; esac
  if ! (cd "$work_dir" && dfx canister status --network "$network_url" --identity "$identity" "$canister_id") >> "$log_file" 2>&1; then
    (cd "$work_dir" && run_logged "$log_file" dfx canister create --network "$network_url" --identity "$identity" --no-wallet "$canister") || exit $?
    allocated_id="$(cd "$work_dir" && dfx canister id --network "$network_url" "$canister")"
    if [ "$allocated_id" != "$canister_id" ]; then
      record_blocker "${canister} allocated ID ${allocated_id} does not match planned ${canister_id}"
      exit 2
    fi
  fi
  raw_path="${REPO_ROOT}/$(manifest_artifact_value "$canister" raw_wasm_path)"
  expected_hash="$(manifest_artifact_value "$canister" raw_wasm_sha256)"
  status="$(cd "$work_dir" && dfx canister status --network "$network_url" --identity "$identity" "$canister_id" 2>&1)"
  observed_hash="$(printf '%s\n' "$status" | sed -n 's/^Module hash: 0x//p' | tr -d '[:space:]')"
  if [ -z "$observed_hash" ]; then
    case "$canister" in
      io_stream_manager)
        (cd "$work_dir" && run_logged "$log_file" dfx canister install --network "$network_url" --identity "$identity" --wasm "$raw_path" --argument-file "$stream_args" "$canister_id") || exit $?
        ;;
      io_nns_neuron_manager)
        (cd "$work_dir" && run_logged "$log_file" dfx canister install --network "$network_url" --identity "$identity" --wasm "$raw_path" --argument-file "$nns_args" "$canister_id") || exit $?
        ;;
      *)
        (cd "$work_dir" && run_logged "$log_file" dfx canister install --network "$network_url" --identity "$identity" --wasm "$raw_path" "$canister_id") || exit $?
        ;;
    esac
  elif [ "$observed_hash" != "$expected_hash" ]; then
    record_blocker "${canister} module hash ${observed_hash} does not match exact release ${expected_hash}"
    exit 2
  fi
done

for canister in io_stream_manager io_nns_neuron_manager; do
  response="$(cd "$work_dir" && dfx canister call --network "$network_url" --identity "$identity" --query --candid "${REPO_ROOT}/canisters/${canister}/${canister}.did" "${ids[$canister]}" get_status '()')"
  printf '%s\n' "$response" >> "$log_file"
  if ! printf '%s' "$response" | grep -q 'Paused'; then
    record_blocker "${canister} did not install Paused"
    exit 2
  fi
done

run_logged "$log_file" "$sns" --identity "$identity" --network "$network_url" prepare-canisters add-nns-root \
  "${ids[io_stream_manager]}" "${ids[io_nns_neuron_manager]}" "${ids[io_historian]}" "${ids[frontend]}"
mark_phase_done 12-deploy-local-dapps "release dapps installed Paused and NNS Root added as co-controller"
