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
require_file "${REHEARSAL_DIR}/sns_init.local.yaml"

governance_source="$(toml_string "${bundle_dir}/manifest.toml" artifacts sns_governance_source_commit)"
root_source="$(toml_string "${bundle_dir}/manifest.toml" artifacts sns_root_source_commit)"
if [ "$governance_source" != "$PINNED_IC_COMMIT" ] || [ "$root_source" != "$PINNED_IC_COMMIT" ]; then
  record_blocker "candidate Governance/Root sources ${governance_source}/${root_source} do not both match pinned ${PINNED_IC_COMMIT}"
  exit 2
fi
if ! phase_is_done 13-candidate-published; then
  run_logged "$log_file" "$sns" --identity "$identity" --network "$network_url" add-sns-wasm-for-tests --wasm-file "${bundle_dir}/wasms/sns_governance.wasm.gz" governance
  run_logged "$log_file" "$sns" --identity "$identity" --network "$network_url" add-sns-wasm-for-tests --wasm-file "${bundle_dir}/wasms/sns_root.wasm.gz" root
  mark_phase_done 13-candidate-published "source=${PINNED_IC_COMMIT} governance+root published through SNS-W"
fi

proposal_file="${REHEARSAL_DIR}/generated/create-sns-proposal.json"
nns_neuron_id="${IO_LOCAL_SNS_NNS_NEURON_ID:-11129307823670308035}"
if ! phase_is_done 13-create-sns-proposed; then
  run_logged "$log_file" "$sns" --identity "$identity" --network "$network_url" propose \
    --neuron-id "$nns_neuron_id" --skip-confirmation --save-to "$proposal_file" \
    "${REHEARSAL_DIR}/sns_init.local.yaml"
  mark_phase_done 13-create-sns-proposed "nns_neuron_id=${nns_neuron_id} proposal=${proposal_file}"
fi

developer_principal="$(toml_string "${REHEARSAL_DIR}/local-vars.toml" local developer_neuron_principal)"
run_logged "$log_file" "$sns_testing" --network "$network_url" swap-complete \
  --sns-name 'IO Local Rehearsal' --follow-principal-neurons "$developer_principal"
mark_phase_done 13-propose-and-finalize-sns "candidate Governance/Root SNS created and swap finalized"
