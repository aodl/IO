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
stream="$(toml_string "$(local_vars_file)" local io_stream_manager_canister)"
operator="$(runtime_value accounts operator_principal)"
reserve_hex="$(runtime_value accounts reserve_subaccount_hex)"
governance="$(sns_canister_id governance)"
treasury_hex="$(sns_treasury_subaccount_hex "$governance")"
ledger_did="${checkout}/rs/ledger_suite/icrc1/ledger/ledger.did"
index_did="${checkout}/rs/ledger_suite/icrc1/index-ng/index-ng.did"
root_did="${checkout}/rs/sns/root/canister/root.did"

run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$index_did" "$index" ledger_id '()'
required_synced_blocks=1
economics="${GENERATED_DIR}/redemption-economics.toml"
if [ -f "$economics" ]; then
  required_synced_blocks="$(( $(toml_number "$economics" stream_result io_block) + 1 ))"
fi
num_blocks_synced=0
for _attempt in 1 2 3 4 5 6 7 8 9 10; do
  index_status="$(dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$index_did" "$index" status '()')"
  printf '%s\n' "$index_status" >> "$log_file"
  num_blocks_synced="$(printf '%s' "$index_status" | sed -n \
    's/.*num_blocks_synced = \([0-9_][0-9_]*\).*/\1/p' | head -1 | tr -d '_')"
  require_nat "SNS index synchronized block count" "$num_blocks_synced"
  if [ "$num_blocks_synced" -ge "$required_synced_blocks" ]; then
    break
  fi
  sleep 1
done
if [ "$num_blocks_synced" -lt "$required_synced_blocks" ]; then
  record_blocker "SNS index synchronized ${num_blocks_synced} blocks; required ${required_synced_blocks}"
  exit 2
fi
run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$index_did" "$index" get_account_transactions \
  "(record { account = record { owner = principal \"${stream}\"; subaccount = opt blob \"$(hex_blob_literal "$reserve_hex")\" }; start = null; max_results = 100 : nat })"
run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$index_did" "$index" get_account_transactions \
  "(record { account = record { owner = principal \"${operator}\"; subaccount = null }; start = null; max_results = 100 : nat })"
treasury_history="${GENERATED_DIR}/treasury-account-history.log"
dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$index_did" "$index" get_account_transactions \
  "(record { account = record { owner = principal \"${governance}\"; subaccount = opt blob \"$(hex_blob_literal "$treasury_hex")\" }; start = null; max_results = 100 : nat })" \
  > "$treasury_history"
printf 'treasury_account_history=%s\n' "$treasury_history" >> "$log_file"
cat "$treasury_history" >> "$log_file"
run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$ledger_did" "$ledger" icrc3_get_archives '(record { from = null })'
run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$root_did" "$root" list_sns_canisters '(record {})'
mark_phase_done 16-exercise-index-and-archives \
  "num_blocks_synced=${num_blocks_synced} required_synced_blocks=${required_synced_blocks} exact protocol/user/treasury account histories and canonical archive discovery captured in ${log_file}; treasury_history=${treasury_history}"
