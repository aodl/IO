#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# canonical local index and archive evidence capture.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 16-exercise-index-and-archives)"
: > "$log_file"
if ! phase_is_done 15-ledger-baseline && ! phase_is_done 15-redemption-complete; then
  record_blocker "phase 15 ledger funding must complete first"
  exit 2
fi
network_url="$(local_network_url)"
identity="$(local_identity_name)"
checkout="$(official_checkout)"
ledger="$(sns_canister_id ledger)"
index="$(sns_canister_id index)"
root="$(sns_canister_id root)"
stream="$(toml_string "${REHEARSAL_DIR}/local-vars.toml" local io_stream_manager_canister)"
operator="$(runtime_value accounts operator_principal)"
reserve_hex="$(runtime_value accounts reserve_subaccount_hex)"
ledger_did="${checkout}/rs/ledger_suite/icrc1/ledger/ledger.did"
index_did="${checkout}/rs/ledger_suite/icrc1/index-ng/index-ng.did"
root_did="${checkout}/rs/sns/root/canister/root.did"

run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$index_did" "$index" ledger_id '()'
run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$index_did" "$index" status '()'
run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$index_did" "$index" get_account_transactions \
  "(record { account = record { owner = principal \"${stream}\"; subaccount = opt blob \"$(hex_blob_literal "$reserve_hex")\" }; start = null; max_results = 100 : nat })"
run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$index_did" "$index" get_account_transactions \
  "(record { account = record { owner = principal \"${operator}\"; subaccount = null }; start = null; max_results = 100 : nat })"
run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$ledger_did" "$ledger" icrc3_get_archives '(record { from = null })'
run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$root_did" "$root" list_sns_canisters '(record {})'
mark_phase_done 16-exercise-index-and-archives "index status, exact account histories and canonical archive discovery captured in ${log_file}"
