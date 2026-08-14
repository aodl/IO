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
vars_file="$(local_vars_file)"
stream_args="$(stream_install_args_file)"
nns_args="$(nns_install_args_file)"
planned_governance="$(runtime_value planned_sns governance)"
treasury_subaccount="$(sns_treasury_subaccount_hex "$planned_governance")"
treasury_blob="$(hex_blob_literal "$treasury_subaccount")"
stream_args_tmp="${stream_args}.tmp"
excluded_assignment_count=0
: > "$stream_args_tmp"
while IFS= read -r line || [ -n "$line" ]; do
  case "$line" in
    *excluded_io_accounts*=*)
      excluded_assignment_count=$((excluded_assignment_count + 1))
      printf '    excluded_io_accounts = vec { record { owner = principal "%s"; subaccount = opt blob "%s" } };\n' \
        "$planned_governance" "$treasury_blob" >> "$stream_args_tmp"
      ;;
    *) printf '%s\n' "$line" >> "$stream_args_tmp" ;;
  esac
done < "$stream_args"
if [ "$excluded_assignment_count" -ne 1 ]; then
  rm -f "$stream_args_tmp"
  record_blocker "stream install args must contain exactly one excluded_io_accounts assignment"
  exit 2
fi
mv "$stream_args_tmp" "$stream_args"
grep -Fq "excluded_io_accounts = vec { record { owner = principal \"${planned_governance}\"; subaccount = opt blob \"${treasury_blob}\" } };" "$stream_args" || {
  record_blocker "stream install args do not contain the canonical SNS treasury Account"
  exit 2
}
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

work_dir="${GENERATED_DIR}/dfx"
mkdir -p "$work_dir"
if [ ! -f "${work_dir}/dfx.json" ]; then
  cp "${REHEARSAL_DIR}/dfx.local.json" "${work_dir}/dfx.json"
fi

declare -A ids
declare -A planned_ids
ids[io_stream_manager]="$(toml_string "$vars_file" local io_stream_manager_canister)"
ids[io_nns_neuron_manager]="$(toml_string "$vars_file" local io_nns_neuron_manager_canister)"
ids[io_historian]="$(toml_string "$vars_file" local io_historian_canister)"
ids[frontend]="$(toml_string "$vars_file" local frontend_canister)"

for canister in io_stream_manager io_nns_neuron_manager io_historian frontend; do
  planned_ids[$canister]="${ids[$canister]}"
done

for canister in io_stream_manager io_nns_neuron_manager io_historian frontend; do
  canister_id="${ids[$canister]}"
  case "$canister_id" in TODO*|"") record_blocker "local dapp ID is unresolved: ${canister}"; exit 2 ;; esac
  if ! (cd "$work_dir" && dfx canister status --network "$network_url" --identity "$identity" "$canister_id") >> "$log_file" 2>&1; then
    allocated_id="$(cd "$work_dir" && dfx canister id --network "$network_url" "$canister" 2>/dev/null || true)"
    if [ -z "$allocated_id" ] \
      || ! (cd "$work_dir" && dfx canister status --network "$network_url" --identity "$identity" "$allocated_id") >> "$log_file" 2>&1; then
      (cd "$work_dir" && run_logged "$log_file" dfx canister create --network "$network_url" --identity "$identity" --no-wallet "$canister") || exit $?
      allocated_id="$(cd "$work_dir" && dfx canister id --network "$network_url" "$canister")"
    fi
    ids[$canister]="$allocated_id"
  fi
done

if [ -n "${IO_LOCAL_SNS_CANISTER_EVIDENCE_FILE:-}" ]; then
  evidence_file="$IO_LOCAL_SNS_CANISTER_EVIDENCE_FILE"
else
  evidence_file=""
fi
for canister in io_stream_manager io_nns_neuron_manager io_historian frontend; do
  planned_id="${planned_ids[$canister]}"
  allocated_id="${ids[$canister]}"
  if [ "$allocated_id" = "$planned_id" ]; then
    continue
  fi
  if [ -z "$evidence_file" ]; then
    record_blocker "${canister} allocated ID ${allocated_id} differs from planned ${planned_id}; use isolated lifecycle inputs so allocated IDs can be recorded safely"
    exit 2
  fi
  for input in "$vars_file" "$(sns_init_file)" "$stream_args" "$nns_args" "$evidence_file"; do
    require_file "$input"
    sed -i "s/${planned_id}/${allocated_id}/g" "$input"
  done
  printf 'allocated_dapp role=%s planned=%s canonical=%s\n' \
    "$canister" "$planned_id" "$allocated_id" >> "$log_file"
done

"${SCRIPT_DIR}/12-provision-local-nns-readiness.sh"
require_file "$stream_args"
require_file "$nns_args"

prior_historian_artifact_commit="0d17a02ddfa8afa5c21f6f886f23fe14377ee0cb"
prior_historian_source_commit="e1f1e1e69c19fe08161706c4fc6345e7e63bf88c"
prior_historian_hash="c7b1d636271e56108a5d7db9be15637e2b9b2d5fda3a627ddf089eabf3707d6c"
prior_historian_path="${GENERATED_DIR}/prior/io_historian.wasm"
mkdir -p "$(dirname "$prior_historian_path")"
if [ ! -f "$prior_historian_path" ]; then
  git -C "$REPO_ROOT" show "${prior_historian_artifact_commit}:release-artifacts/io_historian.wasm" > "$prior_historian_path"
fi
if [ "$(sha256sum "$prior_historian_path" | awk '{print $1}')" != "$prior_historian_hash" ]; then
  record_blocker "prior provenance-correct historian artifact hash mismatch"
  exit 2
fi

for canister in io_stream_manager io_nns_neuron_manager io_historian frontend; do
  canister_id="${ids[$canister]}"
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
      io_historian)
        (cd "$work_dir" && run_logged "$log_file" dfx canister install --network "$network_url" --identity "$identity" --wasm "$prior_historian_path" "$canister_id") || exit $?
        ;;
      *)
        (cd "$work_dir" && run_logged "$log_file" dfx canister install --network "$network_url" --identity "$identity" --wasm "$raw_path" "$canister_id") || exit $?
        ;;
    esac
  elif [ "$canister" = io_historian ] && [ "$observed_hash" = "$prior_historian_hash" ]; then
    :
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
mark_phase_done 12-deploy-local-dapps "current value-moving/frontend dapps and provenance-correct prior historian installed; managers Paused; NNS Root added as co-controller"
printf 'prior historian source=%s artifact_commit=%s raw_hash=%s; current target=%s\n' \
  "$prior_historian_source_commit" "$prior_historian_artifact_commit" "$prior_historian_hash" \
  "$(manifest_artifact_value io_historian raw_wasm_sha256)" >> "$log_file"
