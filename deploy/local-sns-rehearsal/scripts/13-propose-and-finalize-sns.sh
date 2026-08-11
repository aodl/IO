#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# restartable local-only SNS-W publication, proposal, and swap finalization.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 13-propose-and-finalize-sns)"
: > "$log_file"
if phase_is_done 13-propose-and-finalize-sns; then
  printf 'phase 13 already completed: %s\n' "$(cat "$(phase_done_file 13-propose-and-finalize-sns)")"
  exit 0
fi
if ! phase_is_done 12-deploy-local-dapps; then
  record_blocker "phase 12 dapp deployment must complete first"
  exit 2
fi

network_url="$(local_network_url)"
identity="$(local_identity_name)"
sns="$(sns_cli)"
sns_testing="$(sns_testing_cli)"
bundle_dir="${IO_LOCAL_SNS_BUNDLE_DIR:-}"
if [ -z "$bundle_dir" ]; then
  record_blocker "set IO_LOCAL_SNS_BUNDLE_DIR to the reviewed same-source Governance/Root bundle"
  exit 2
fi
require_file "${bundle_dir}/manifest.toml"
require_file "${bundle_dir}/wasms/sns_governance.wasm.gz"
require_file "${bundle_dir}/wasms/sns_root.wasm.gz"
sns_init="$(sns_init_file)"

governance_source="$(toml_string "${bundle_dir}/manifest.toml" artifacts sns_governance_source_commit)"
root_source="$(toml_string "${bundle_dir}/manifest.toml" artifacts sns_root_source_commit)"
if [ "$governance_source" != "$PINNED_IC_COMMIT" ] || [ "$root_source" != "$PINNED_IC_COMMIT" ]; then
  record_blocker "candidate Governance/Root sources ${governance_source}/${root_source} do not both match pinned ${PINNED_IC_COMMIT}"
  exit 2
fi
if ! phase_is_done 13-candidate-published; then
  nns_neuron_id="${IO_LOCAL_SNS_NNS_NEURON_ID:-11129307823670308035}"
  governance_hash="$(toml_string "${bundle_dir}/manifest.toml" artifacts sns_governance_source_sha256)"
  root_hash="$(toml_string "${bundle_dir}/manifest.toml" artifacts sns_root_source_sha256)"
  governance_proposal="$(publish_sns_wasm_via_nns "$log_file" sns_governance Governance \
    "${bundle_dir}/wasms/sns_governance.wasm.gz" "$governance_hash" "$nns_neuron_id")"
  root_proposal="$(publish_sns_wasm_via_nns "$log_file" sns_root Root \
    "${bundle_dir}/wasms/sns_root.wasm.gz" "$root_hash" "$nns_neuron_id")"
  mark_phase_done 13-candidate-published "source=${PINNED_IC_COMMIT} governance_proposal=${governance_proposal} root_proposal=${root_proposal} exact compressed hashes verified in SNS-W"
fi

proposal_file="${GENERATED_DIR}/create-sns-proposal.json"
nns_neuron_id="${IO_LOCAL_SNS_NNS_NEURON_ID:-11129307823670308035}"
if ! phase_is_done 13-create-sns-proposed; then
  run_logged "$log_file" "$sns" --identity "$identity" --network "$network_url" propose \
    --neuron-id "$nns_neuron_id" --skip-confirmation --save-to "$proposal_file" \
    "$sns_init"
  mark_phase_done 13-create-sns-proposed "nns_neuron_id=${nns_neuron_id} proposal=${proposal_file}"
fi

governance_did="$(official_checkout)/rs/sns/governance/canister/governance.did"
metadata_ready=0
for _attempt in 1 2 3 4 5 6 7 8 9 10; do
  metadata="$(dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$governance_did" "$(runtime_value planned_sns governance)" get_metadata '(record {})' 2>&1)" || true
  printf '%s\n' "$metadata" >> "$log_file"
  if printf '%s' "$metadata" | grep -Fq 'name = opt "IO Local Rehearsal"'; then
    metadata_ready=1
    break
  fi
  sleep 1
done
if [ "$metadata_ready" -ne 1 ]; then
  record_blocker "created SNS Governance metadata did not become queryable"
  exit 2
fi

developer_principal="$(toml_string "$(local_vars_file)" local developer_neuron_principal)"
swap_completed=0
for _attempt in 1 2 3 4 5; do
  if run_logged "$log_file" "$sns_testing" --network "$network_url" swap-complete \
    --sns-name 'IO Local Rehearsal' --follow-principal-neurons "$developer_principal"; then
    swap_completed=1
    break
  fi
  sleep 2
done
if [ "$swap_completed" -ne 1 ]; then
  record_blocker "created SNS did not become ready for swap completion"
  exit 2
fi
mark_phase_done 13-propose-and-finalize-sns "candidate Governance/Root SNS created and swap finalized"
