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
governance="$(sns_canister_id governance)"
treasury_hex="$(sns_treasury_subaccount_hex "$governance")"
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

query_nat() {
  local did="$1" canister="$2" method="$3" argument="$4" label="$5"
  local response value
  response="$(dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$did" "$canister" "$method" "$argument")" || {
      record_blocker "failed canonical query for ${label}"
      return 2
    }
  printf '%s=%s\n' "$label" "$response" >> "$log_file"
  value="$(printf '%s' "$response" | tr -d '()_ :nat[:space:]')"
  require_nat "$label" "$value"
  printf '%s\n' "$value"
}

sns_balance() {
  local owner="$1" subaccount="$2" label="$3" account
  if [ "$subaccount" = none ]; then
    account="record { owner = principal \"${owner}\"; subaccount = null }"
  else
    account="record { owner = principal \"${owner}\"; subaccount = opt blob \"$(hex_blob_literal "$subaccount")\" }"
  fi
  query_nat "$ledger_did" "$ledger" icrc1_balance_of "(${account})" "$label"
}

icp_balance() {
  local owner="$1" subaccount="$2" label="$3" account
  if [ "$subaccount" = none ]; then
    account="record { owner = principal \"${owner}\"; subaccount = null }"
  else
    account="record { owner = principal \"${owner}\"; subaccount = opt blob \"$(hex_blob_literal "$subaccount")\" }"
  fi
  query_nat "$ledger_did" "$icp_ledger" icrc1_balance_of "(${account})" "$label"
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

if ! phase_is_done 15-treasury-before-reserve; then
  initial_total="$(query_nat "$ledger_did" "$ledger" icrc1_total_supply '()' initial_total_supply_e8s)"
  treasury_before_reserve="$(sns_balance "$governance" "$treasury_hex" treasury_before_reserve_e8s)"
  reserve_before_funding="$(sns_balance "$stream" "$reserve_hex" reserve_before_funding_e8s)"
  if [ "$treasury_before_reserve" -eq 0 ]; then
    record_blocker "canonical SNS treasury Account is unexpectedly zero before reserve funding"
    exit 2
  fi
  mark_phase_done 15-treasury-before-reserve \
    "total_supply_e8s=${initial_total} treasury_balance_e8s=${treasury_before_reserve} reserve_balance_e8s=${reserve_before_funding}"
fi

if ! phase_is_done 15-reserve-funded; then
  action="variant { TransferSnsTreasuryFunds = record { from_treasury = 2 : int32; to_principal = opt principal \"${stream}\"; to_subaccount = opt record { subaccount = blob \"$(hex_blob_literal "$reserve_hex")\" }; memo = opt (1501 : nat64); amount_e8s = ${reserve_amount} : nat64 } }"
  proposal_id="$(submit_sns_proposal "$log_file" 'Fund local IO protocol reserve' 'Local-only exact reserve funding for the IO rehearsal.' "$action")"
  wait_sns_proposal "$log_file" "$proposal_id"
  treasury_after_reserve="$(sns_balance "$governance" "$treasury_hex" treasury_after_reserve_e8s)"
  reserve_after_funding="$(sns_balance "$stream" "$reserve_hex" reserve_after_funding_e8s)"
  total_after_reserve="$(query_nat "$ledger_did" "$ledger" icrc1_total_supply '()' total_after_reserve_e8s)"
  mark_phase_done 15-reserve-funded \
    "proposal_id=${proposal_id} amount_e8s=${reserve_amount} treasury_balance_e8s=${treasury_after_reserve} reserve_balance_e8s=${reserve_after_funding} total_supply_e8s=${total_after_reserve}"
fi

if ! phase_is_done 15-user-funded; then
  treasury_before_user="$(sns_balance "$governance" "$treasury_hex" treasury_before_user_e8s)"
  action="variant { TransferSnsTreasuryFunds = record { from_treasury = 2 : int32; to_principal = opt principal \"${operator}\"; to_subaccount = null; memo = opt (1502 : nat64); amount_e8s = ${user_amount} : nat64 } }"
  proposal_id="$(submit_sns_proposal "$log_file" 'Fund local redemption user' 'Local-only SNS token funding for production redemption proof.' "$action")"
  wait_sns_proposal "$log_file" "$proposal_id"
  treasury_after_user="$(sns_balance "$governance" "$treasury_hex" treasury_after_user_e8s)"
  user_after_funding="$(sns_balance "$operator" none user_after_funding_e8s)"
  total_after_user="$(query_nat "$ledger_did" "$ledger" icrc1_total_supply '()' total_after_user_e8s)"
  mark_phase_done 15-user-funded \
    "proposal_id=${proposal_id} amount_e8s=${user_amount} treasury_before_e8s=${treasury_before_user} treasury_balance_e8s=${treasury_after_user} user_balance_e8s=${user_after_funding} total_supply_e8s=${total_after_user}"
fi

if ! phase_is_done 15-ledger-negatives; then
  transfer_time="$(date +%s%N)"
  transfer="(record { from_subaccount = null; to = record { owner = principal \"${operator}\"; subaccount = null }; amount = 1000000 : nat; fee = opt (10000 : nat); memo = opt blob \"IO duplicate\"; created_at_time = opt (${transfer_time} : nat64) })"
  successful_transfer="$(dfx canister call --network "$network_url" --identity "$identity" --candid "$ledger_did" "$ledger" icrc1_transfer "$transfer")"
  duplicate_transfer="$(dfx canister call --network "$network_url" --identity "$identity" --candid "$ledger_did" "$ledger" icrc1_transfer "$transfer")"
  printf 'duplicate_test_success=%s\nduplicate_test_replay=%s\n' "$successful_transfer" "$duplicate_transfer" >> "$log_file"
  duplicate_block="$(printf '%s' "$successful_transfer" | tr '\n' ' ' | sed -n 's/.*Ok = \([0-9_][0-9_]*\).*/\1/p' | tr -d '_')"
  require_nat "duplicate test block" "$duplicate_block"
  printf '%s' "$duplicate_transfer" | grep -q "duplicate_of = ${duplicate_block}" || {
    record_blocker "ledger duplicate response did not reference original block ${duplicate_block}"
    exit 2
  }
  run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --candid "$ledger_did" "$ledger" icrc1_transfer \
    "(record { from_subaccount = null; to = record { owner = principal \"${operator}\"; subaccount = null }; amount = 1 : nat; fee = opt (1 : nat); memo = null; created_at_time = null })"
  run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --candid "$ledger_did" "$ledger" icrc1_transfer \
    "(record { from_subaccount = null; to = record { owner = principal \"${operator}\"; subaccount = null }; amount = 999999999999999999999 : nat; fee = opt (10000 : nat); memo = null; created_at_time = null })"
  mark_phase_done 15-ledger-negatives \
    "duplicate_block=${duplicate_block} successful transfer, exact duplicate, bad fee and overspend captured"
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
  pre_snapshot="${GENERATED_DIR}/redemption-pre-snapshot.toml"
  caller_state="$(dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" get_caller_redemption_state '()')"
  if ! printf '%s' "$caller_state" | grep -q 'next_nonce = 1' \
    || ! printf '%s' "$caller_state" | grep -q 'last_result = opt record'; then
    if [ ! -f "$pre_snapshot" ]; then
      now_nanos="$(date +%s%N)"
      expires_nanos="$((now_nanos + 800000000000))"
      allowance="$((redeem_amount + 10000))"
      approval_response="$(dfx canister call --network "$network_url" --identity "$identity" \
        --candid "$ledger_did" "$ledger" icrc2_approve \
        "(record { from_subaccount = null; spender = record { owner = principal \"${stream}\"; subaccount = null }; amount = ${allowance} : nat; expected_allowance = null; expires_at = opt (${expires_nanos} : nat64); fee = opt (10000 : nat); memo = opt blob \"IO redemption\"; created_at_time = opt (${now_nanos} : nat64) })")"
      printf 'approval_response=%s\n' "$approval_response" >> "$log_file"
      approval_block="$(printf '%s' "$approval_response" | tr '\n' ' ' | sed -n 's/.*Ok = \([0-9_][0-9_]*\).*/\1/p' | tr -d '_')"
      require_nat "approval block" "$approval_block"

      pre_total="$(query_nat "$ledger_did" "$ledger" icrc1_total_supply '()' pre_pull_total_supply_e8s)"
      pre_reserve="$(sns_balance "$stream" "$reserve_hex" pre_pull_protocol_reserve_e8s)"
      pre_excluded="$(sns_balance "$governance" "$treasury_hex" pre_pull_sns_treasury_e8s)"
      pre_liquid="$(icp_balance "$stream" "$liquid_hex" pre_payout_liquid_icp_e8s)"
      pre_user_io="$(sns_balance "$operator" none pre_pull_user_io_e8s)"
      pre_user_icp="$(icp_balance "$operator" none pre_payout_user_icp_e8s)"
      formula="$(cargo run --quiet -p xtask --manifest-path "${REPO_ROOT}/Cargo.toml" -- \
        calculate_redemption_economics "$pre_total" "$pre_reserve" "$pre_excluded" \
        "$pre_liquid" "$redeem_amount" 10000)"
      expected_d="$(printf '%s\n' "$formula" | sed -n 's/^redeemable_supply_e8s=//p')"
      expected_gross="$(printf '%s\n' "$formula" | sed -n 's/^gross_icp_e8s=//p')"
      expected_net="$(printf '%s\n' "$formula" | sed -n 's/^net_icp_e8s=//p')"
      for value in "$expected_d" "$expected_gross" "$expected_net"; do
        require_nat "independently calculated redemption value" "$value"
      done
      cat > "$pre_snapshot" <<EOF
[redemption]
approval_block = ${approval_block}
expires_at_nanos = ${expires_nanos}
total_io_supply_e8s = ${pre_total}
protocol_reserve_io_e8s = ${pre_reserve}
excluded_io_e8s = ${pre_excluded}
liquid_icp_e8s = ${pre_liquid}
user_io_e8s = ${pre_user_io}
user_icp_e8s = ${pre_user_icp}
redeemable_io_supply_e8s = ${expected_d}
gross_icp_e8s = ${expected_gross}
net_icp_e8s = ${expected_net}
EOF
    fi
    expires_nanos="$(toml_number "$pre_snapshot" redemption expires_at_nanos)"
    redeem_args="(record { from_subaccount = null; io_amount_e8s = ${redeem_amount} : nat; min_icp_out_e8s = 0 : nat; max_io_fee_e8s = 10000 : nat; max_icp_fee_e8s = 10000 : nat; expires_at_nanos = ${expires_nanos} : nat64; nonce = 0 : nat64 })"
    initial_redeem_response="$(dfx canister call --network "$network_url" --identity "$identity" \
      --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" redeem "$redeem_args")"
    printf 'initial_redeem_response=%s\n' "$initial_redeem_response" >> "$log_file"
    for _attempt in 1 2; do
      run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" \
        --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" resume '()'
    done
  fi
  require_file "$pre_snapshot"
  approval_block="$(toml_number "$pre_snapshot" redemption approval_block)"
  expires_nanos="$(toml_number "$pre_snapshot" redemption expires_at_nanos)"
  pre_total="$(toml_number "$pre_snapshot" redemption total_io_supply_e8s)"
  pre_reserve="$(toml_number "$pre_snapshot" redemption protocol_reserve_io_e8s)"
  pre_excluded="$(toml_number "$pre_snapshot" redemption excluded_io_e8s)"
  pre_liquid="$(toml_number "$pre_snapshot" redemption liquid_icp_e8s)"
  pre_user_io="$(toml_number "$pre_snapshot" redemption user_io_e8s)"
  pre_user_icp="$(toml_number "$pre_snapshot" redemption user_icp_e8s)"
  expected_d="$(toml_number "$pre_snapshot" redemption redeemable_io_supply_e8s)"
  expected_gross="$(toml_number "$pre_snapshot" redemption gross_icp_e8s)"
  expected_net="$(toml_number "$pre_snapshot" redemption net_icp_e8s)"
  redeem_args="(record { from_subaccount = null; io_amount_e8s = ${redeem_amount} : nat; min_icp_out_e8s = 0 : nat; max_io_fee_e8s = 10000 : nat; max_icp_fee_e8s = 10000 : nat; expires_at_nanos = ${expires_nanos} : nat64; nonce = 0 : nat64 })"
  replay_response="$(dfx canister call --network "$network_url" --identity "$identity" \
    --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" redeem "$redeem_args")"
  printf 'identical_replay_response=%s\n' "$replay_response" >> "$log_file"
  caller_state="$(dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" get_caller_redemption_state '()')"
  printf '%s\n' "$caller_state" >> "$log_file"
  if ! printf '%s' "$caller_state" | grep -q 'next_nonce = 1' \
    || ! printf '%s' "$caller_state" | grep -q 'last_result = opt record'; then
    record_blocker 'production redemption did not reach Completed'
    exit 2
  fi
  stream_io_block="$(printf '%s' "$caller_state" | sed -n 's/.*io_block = \([0-9_][0-9_]*\).*/\1/p' | head -1 | tr -d '_')"
  stream_icp_block="$(printf '%s' "$caller_state" | sed -n 's/.*icp_block = \([0-9_][0-9_]*\).*/\1/p' | head -1 | tr -d '_')"
  stream_gross="$(printf '%s' "$caller_state" | sed -n 's/.*gross_icp_e8s = \([0-9_][0-9_]*\).*/\1/p' | head -1 | tr -d '_')"
  stream_net="$(printf '%s' "$caller_state" | sed -n 's/.*net_icp_e8s = \([0-9_][0-9_]*\).*/\1/p' | head -1 | tr -d '_')"
  for value in "$stream_io_block" "$stream_icp_block" "$stream_gross" "$stream_net"; do
    require_nat "Stream completed redemption field" "$value"
  done
  if [ "$stream_gross" != "$expected_gross" ] || [ "$stream_net" != "$expected_net" ]; then
    record_blocker "Stream quote does not match independently calculated canonical Account snapshot"
    exit 2
  fi
  for key_value in \
    "io_block:${stream_io_block}" "icp_block:${stream_icp_block}" \
    "gross_icp_e8s:${stream_gross}" "net_icp_e8s:${stream_net}"; do
    key="${key_value%%:*}"
    value="${key_value#*:}"
    replay_value="$(printf '%s' "$replay_response" | sed -n "s/.*${key} = \\([0-9_][0-9_]*\\).*/\\1/p" | head -1 | tr -d '_')"
    if [ "$replay_value" != "$value" ]; then
      record_blocker "identical redemption replay changed ${key}: original=${value} replay=${replay_value}"
      exit 2
    fi
  done
  post_total="$(query_nat "$ledger_did" "$ledger" icrc1_total_supply '()' post_pull_total_supply_e8s)"
  post_reserve="$(sns_balance "$stream" "$reserve_hex" post_pull_protocol_reserve_e8s)"
  post_excluded="$(sns_balance "$governance" "$treasury_hex" post_pull_sns_treasury_e8s)"
  post_liquid="$(icp_balance "$stream" "$liquid_hex" post_payout_liquid_icp_e8s)"
  post_user_io="$(sns_balance "$operator" none post_pull_user_io_e8s)"
  post_user_icp="$(icp_balance "$operator" none post_payout_user_icp_e8s)"
  if [ "$post_reserve" -ne "$((pre_reserve + redeem_amount))" ] \
    || [ "$post_total" -ne "$((pre_total - 10000))" ] \
    || [ "$post_excluded" -ne "$pre_excluded" ] \
    || [ "$post_liquid" -ne "$((pre_liquid - stream_gross))" ] \
    || [ "$post_user_io" -ne "$((pre_user_io - redeem_amount - 10000))" ] \
    || [ "$post_user_icp" -ne "$((pre_user_icp + stream_net))" ]; then
    record_blocker "post-redemption ledger balances do not match fee/value identities"
    exit 2
  fi
  economics="${GENERATED_DIR}/redemption-economics.toml"
  cat > "$economics" <<EOF
[snapshot]
total_io_supply_e8s = ${pre_total}
protocol_reserve_io_e8s = ${pre_reserve}
excluded_io_total_e8s = ${pre_excluded}
redeemable_io_supply_e8s = ${expected_d}
liquid_icp_reserve_e8s = ${pre_liquid}
redemption_io_amount_e8s = ${redeem_amount}
quoted_gross_icp_e8s = ${stream_gross}
io_fee_e8s = 10000
icp_fee_e8s = 10000
quoted_net_icp_e8s = ${stream_net}

[excluded_sns_treasury]
name = "sns-treasury"
owner = "${governance}"
subaccount_hex = "${treasury_hex}"
balance_e8s = ${pre_excluded}
expected_nonzero = true

[stream_result]
approval_block = ${approval_block}
io_block = ${stream_io_block}
icp_block = ${stream_icp_block}
gross_icp_e8s = ${stream_gross}
net_icp_e8s = ${stream_net}
identical_replay = true

[ledger_balances]
io_total_before_e8s = ${pre_total}
io_total_after_e8s = ${post_total}
protocol_reserve_before_e8s = ${pre_reserve}
protocol_reserve_after_e8s = ${post_reserve}
excluded_before_e8s = ${pre_excluded}
excluded_after_e8s = ${post_excluded}
liquid_icp_before_e8s = ${pre_liquid}
liquid_icp_after_e8s = ${post_liquid}
user_io_before_e8s = ${pre_user_io}
user_io_after_e8s = ${post_user_io}
user_icp_before_e8s = ${pre_user_icp}
user_icp_after_e8s = ${post_user_icp}
EOF
  query_ledger icrc1_total_supply '()'
  query_ledger icrc1_balance_of "(record { owner = principal \"${stream}\"; subaccount = opt blob \"$(hex_blob_literal "$reserve_hex")\" })"
  query_ledger icrc1_balance_of "(record { owner = principal \"${operator}\"; subaccount = null })"
  run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$ledger_did" "$icp_ledger" icrc1_balance_of \
    "(record { owner = principal \"${stream}\"; subaccount = opt blob \"$(hex_blob_literal "$liquid_hex")\" })"
  run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$ledger_did" "$icp_ledger" icrc1_balance_of \
    "(record { owner = principal \"${operator}\"; subaccount = null })"
  mark_phase_done 15-redemption-complete \
    "approval_block=${approval_block} io_block=${stream_io_block} icp_block=${stream_icp_block} gross_icp_e8s=${stream_gross} net_icp_e8s=${stream_net} economics=${economics}"
fi
