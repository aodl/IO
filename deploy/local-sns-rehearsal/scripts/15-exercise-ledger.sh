#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# restartable local treasury funding and production redemption exercise.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 15-exercise-ledger)"
touch "$log_file"
if ! phase_is_done 14-discover-sns-canisters; then
  record_blocker "phase 14 canonical SNS discovery must complete first"
  exit 2
fi
require_command_available dfx
network_url="$(local_network_url)"
identity="$(local_identity_name)"
checkout="$(official_checkout)"
ledger="$(sns_canister_id ledger)"
icp_ledger="$(runtime_value nns icp_ledger)"
stream="$(toml_string "$(local_vars_file)" local io_stream_manager_canister)"
operator="$(runtime_value accounts operator_principal)"
reserve_hex="$(runtime_value accounts reserve_subaccount_hex)"
liquid_hex="$(runtime_value accounts liquid_icp_subaccount_hex)"
require_hex_32_bytes "reserve subaccount" "$reserve_hex"
require_hex_32_bytes "liquid ICP subaccount" "$liquid_hex"
reserve_amount="$(runtime_value amounts reserve_funding_e8s)"
user_amount="$(runtime_value amounts user_funding_e8s)"
liquid_amount="$(runtime_value amounts liquid_icp_funding_e8s)"
redeem_amount="$(runtime_value amounts redemption_io_e8s)"
for amount in "$reserve_amount" "$user_amount" "$liquid_amount" "$redeem_amount"; do
  require_nat "rehearsal amount" "$amount"
done
ledger_did="${checkout}/rs/ledger_suite/icrc1/ledger/ledger.did"

query_ledger() {
  run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" \
    --query --candid "$ledger_did" "$ledger" "$@"
}

query_ledger icrc1_total_supply '()'
query_ledger icrc1_fee '()'
query_ledger icrc1_balance_of "(record { owner = principal \"${stream}\"; subaccount = opt blob \"$(hex_blob_literal "$reserve_hex")\" })"
query_ledger icrc1_balance_of "(record { owner = principal \"${operator}\"; subaccount = null })"
run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$ledger_did" "$icp_ledger" icrc1_balance_of \
  "(record { owner = principal \"${stream}\"; subaccount = opt blob \"$(hex_blob_literal "$liquid_hex")\" })"
run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$ledger_did" "$icp_ledger" icrc1_balance_of \
  "(record { owner = principal \"${operator}\"; subaccount = null })"

if ! phase_is_done 15-reserve-funded; then
  action="variant { TransferSnsTreasuryFunds = record { from_treasury = 2 : int32; to_principal = opt principal \"${stream}\"; to_subaccount = opt record { subaccount = blob \"$(hex_blob_literal "$reserve_hex")\" }; memo = opt (1501 : nat64); amount_e8s = ${reserve_amount} : nat64 } }"
  proposal_id="$(submit_sns_proposal "$log_file" 'Fund local IO protocol reserve' 'Local-only exact reserve funding for the IO rehearsal.' "$action")"
  wait_sns_proposal "$log_file" "$proposal_id"
  mark_phase_done 15-reserve-funded "proposal_id=${proposal_id} amount_e8s=${reserve_amount}"
fi

if ! phase_is_done 15-user-funded; then
  action="variant { TransferSnsTreasuryFunds = record { from_treasury = 2 : int32; to_principal = opt principal \"${operator}\"; to_subaccount = null; memo = opt (1502 : nat64); amount_e8s = ${user_amount} : nat64 } }"
  proposal_id="$(submit_sns_proposal "$log_file" 'Fund local redemption user' 'Local-only SNS token funding for production redemption proof.' "$action")"
  wait_sns_proposal "$log_file" "$proposal_id"
  mark_phase_done 15-user-funded "proposal_id=${proposal_id} amount_e8s=${user_amount}"
fi

if ! phase_is_done 15-ledger-negatives; then
  transfer_time="$(date +%s%N)"
  transfer="(record { from_subaccount = null; to = record { owner = principal \"${operator}\"; subaccount = null }; amount = 1000000 : nat; fee = opt (10000 : nat); memo = opt blob \"IO duplicate\"; created_at_time = opt (${transfer_time} : nat64) })"
  run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --candid "$ledger_did" "$ledger" icrc1_transfer "$transfer"
  run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --candid "$ledger_did" "$ledger" icrc1_transfer "$transfer"
  run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --candid "$ledger_did" "$ledger" icrc1_transfer \
    "(record { from_subaccount = null; to = record { owner = principal \"${operator}\"; subaccount = null }; amount = 1 : nat; fee = opt (1 : nat); memo = null; created_at_time = null })"
  run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --candid "$ledger_did" "$ledger" icrc1_transfer \
    "(record { from_subaccount = null; to = record { owner = principal \"${operator}\"; subaccount = null }; amount = 999999999999999999999 : nat; fee = opt (10000 : nat); memo = null; created_at_time = null })"
  mark_phase_done 15-ledger-negatives "successful transfer, exact duplicate, bad fee and overspend captured"
fi

durable_redemption_complete=0
caller_checkpoint="$(dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" get_caller_redemption_state '()')"
if phase_is_done 15-redemption-complete \
  || { printf '%s' "$caller_checkpoint" | grep -q 'next_nonce = 1' \
    && printf '%s' "$caller_checkpoint" | grep -q 'last_result = opt record'; }; then
  durable_redemption_complete=1
fi
if [ "$durable_redemption_complete" -ne 1 ]; then
  liquid_balance="$(dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$ledger_did" "$icp_ledger" icrc1_balance_of \
    "(record { owner = principal \"${stream}\"; subaccount = opt blob \"$(hex_blob_literal "$liquid_hex")\" })" \
    | tr -d '()_ :nat[:space:]')"
  require_nat "observed liquid ICP balance" "$liquid_balance"
  if [ "$liquid_balance" -lt "$liquid_amount" ]; then
    sns_testing="$(sns_testing_cli)"
    liquid_delta="$((liquid_amount - liquid_balance))"
    liquid_tokens="$(e8s_to_decimal_tokens "$liquid_delta")"
    run_logged "$log_file" "$sns_testing" --network "$network_url" transfer-icp --amount "$liquid_tokens" \
      --to-principal "$stream" "$liquid_hex"
  fi
  mark_phase_done 15-liquid-icp-funded "target_e8s=${liquid_amount} observed_before_e8s=${liquid_balance}"
fi

query_ledger icrc1_total_supply '()'
query_ledger icrc1_balance_of "(record { owner = principal \"${stream}\"; subaccount = opt blob \"$(hex_blob_literal "$reserve_hex")\" })"
query_ledger icrc1_balance_of "(record { owner = principal \"${operator}\"; subaccount = null })"

stream_status="$(dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" get_status '()')"
printf '%s\n' "$stream_status" >> "$log_file"
if ! printf '%s' "$stream_status" | grep -q 'Ready'; then
  printf 'stream remains Paused; funding evidence is complete and redemption will resume after phase 17 activation\n'
  mark_phase_done 15-ledger-baseline "reserve, user and liquid ICP funded; redemption pending Governance activation"
  exit 0
fi

if ! phase_is_done 15-redemption-complete; then
  caller_state="$(dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" get_caller_redemption_state '()')"
  if ! printf '%s' "$caller_state" | grep -q 'next_nonce = 1' \
    || ! printf '%s' "$caller_state" | grep -q 'last_result = opt record'; then
    now_nanos="$(date +%s%N)"
    expires_nanos="$((now_nanos + 800000000000))"
    allowance="$((redeem_amount + 10000))"
    run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" \
      --candid "$ledger_did" "$ledger" icrc2_approve \
      "(record { from_subaccount = null; spender = record { owner = principal \"${stream}\"; subaccount = null }; amount = ${allowance} : nat; expected_allowance = null; expires_at = opt (${expires_nanos} : nat64); fee = opt (10000 : nat); memo = opt blob \"IO redemption\"; created_at_time = opt (${now_nanos} : nat64) })"
    redeem_args="(record { from_subaccount = null; io_amount_e8s = ${redeem_amount} : nat; min_icp_out_e8s = 0 : nat; max_io_fee_e8s = 10000 : nat; max_icp_fee_e8s = 10000 : nat; expires_at_nanos = ${expires_nanos} : nat64; nonce = 0 : nat64 })"
    run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" \
      --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" redeem "$redeem_args"
    for _attempt in 1 2; do
      run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" \
        --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" resume '()'
    done
    run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" \
      --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" redeem "$redeem_args"
    caller_state="$(dfx canister call --network "$network_url" --identity "$identity" --query \
      --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" get_caller_redemption_state '()')"
  fi
  printf '%s\n' "$caller_state" >> "$log_file"
  if ! printf '%s' "$caller_state" | grep -q 'next_nonce = 1' \
    || ! printf '%s' "$caller_state" | grep -q 'last_result = opt record'; then
    record_blocker 'production redemption did not reach Completed'
    exit 2
  fi
  query_ledger icrc1_total_supply '()'
  query_ledger icrc1_balance_of "(record { owner = principal \"${stream}\"; subaccount = opt blob \"$(hex_blob_literal "$reserve_hex")\" })"
  query_ledger icrc1_balance_of "(record { owner = principal \"${operator}\"; subaccount = null })"
  run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$ledger_did" "$icp_ledger" icrc1_balance_of \
    "(record { owner = principal \"${stream}\"; subaccount = opt blob \"$(hex_blob_literal "$liquid_hex")\" })"
  run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$ledger_did" "$icp_ledger" icrc1_balance_of \
    "(record { owner = principal \"${operator}\"; subaccount = null })"
  mark_phase_done 15-redemption-complete "production ICRC-2 redemption invoked and resumed; inspect canonical result in ${log_file}"
fi
