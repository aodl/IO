#!/usr/bin/env bash
set -euo pipefail

# optional local-only dry-run helper; this does not deploy, install, upgrade, or call canisters
# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-local-sns.sh
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

evidence="${1:-${REHEARSAL_DIR}/canister-ids.local.toml}"
output="${2:-${REHEARSAL_DIR}/generated/local-production-wiring.toml}"
require_file "$evidence"

if [ -n "${IO_LOCAL_SNS_REHEARSAL_XTASK:-}" ]; then
  IO_LOCAL_SNS_EVIDENCE="$evidence" "${IO_LOCAL_SNS_REHEARSAL_XTASK}" local_sns_evidence_tests
else
  cd "${REPO_ROOT}"
  cargo run -p xtask -- validate_local_sns_ledger
fi

sns_root="$(toml_string "$evidence" sns_canisters root)"
sns_governance="$(toml_string "$evidence" sns_canisters governance)"
sns_ledger="$(toml_string "$evidence" sns_canisters ledger)"
sns_index="$(toml_string "$evidence" sns_canisters index)"
io_stream_manager="$(toml_string "$evidence" io_dapp_canisters io_stream_manager)"
io_nns_neuron_manager="$(toml_string "$evidence" io_dapp_canisters io_nns_neuron_manager)"
fee="$(toml_number "$evidence" ledger_evidence transaction_fee_e8s)"

mkdir -p "$(dirname "$output")"
cat > "$output" <<EOF
# Human-readable local evidence-derived wiring.
# Not accepted by production_wiring validators.
# Do not use as install args.
# Generated from official local SNS rehearsal evidence.
[environment]
mode = "LocalOfficialSnsDryRun"
io_ledger_role = "FutureCanonicalSnsIoLocalRehearsal"
production_active = false

[principals]
sns_root = "${sns_root}"
sns_governance = "${sns_governance}"
sns_ledger = "${sns_ledger}"
sns_index = "${sns_index}"
io_ledger = "${sns_ledger}"
io_index = "${sns_index}"

[fees]
io_ledger_transfer_fee_e8s = ${fee}
allow_zero_fees_for_mock_or_local = false

[deployment_targets]
io_stream_manager = "${io_stream_manager}"
io_nns_neuron_manager = "${io_nns_neuron_manager}"
mutation_target_principals = []
mutation_target_nns_neuron_ids = []
EOF

printf 'wrote local-only dry-run wiring: %s\n' "$output"
