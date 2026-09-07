#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# Restartable local treasury funding and semantic-staging redemption exercise.
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
nns_manager="$(toml_string "$(local_vars_file)" local io_nns_neuron_manager_canister)"
operator="$(runtime_value accounts operator_principal)"
governance="$(sns_canister_id governance)"
treasury_hex="$(sns_treasury_subaccount_hex "$governance")"
reserve_hex="$(runtime_value accounts reserve_subaccount_hex)"
liquid_hex="$(runtime_value accounts liquid_icp_subaccount_hex)"
staging_hex='a1c09f94673b8eeded2ce084800bf2a95bcbd3ea346b97b6c3772ca5630c5d87'
require_hex_32_bytes "reserve subaccount" "$reserve_hex"
require_hex_32_bytes "liquid ICP subaccount" "$liquid_hex"
require_hex_32_bytes "redemption staging subaccount" "$staging_hex"
reserve_amount="$(runtime_value amounts reserve_funding_e8s)"
user_amount="$(runtime_value amounts user_funding_e8s)"
liquid_amount="$(runtime_value amounts liquid_icp_funding_e8s)"
redeem_amount="$(runtime_value amounts redemption_io_e8s)"
for amount in "$reserve_amount" "$user_amount" "$liquid_amount" "$redeem_amount"; do
  require_nat "rehearsal amount" "$amount"
done
ledger_did="${checkout}/rs/ledger_suite/icrc1/ledger/ledger.did"

query_nat() {
  local did="$1" canister="$2" method="$3" argument="$4" label="$5" response value
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
  proposal_id="$(submit_sns_proposal "$log_file" 'Fund local redemption user' 'Local-only SNS token funding for semantic-staging redemption proof.' "$action")"
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
  mark_phase_done 15-ledger-negatives \
    "duplicate_block=${duplicate_block} successful transfer, exact duplicate and bad fee captured"
fi

stream_status="$(dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" get_status '()')"
printf '%s\n' "$stream_status" >> "$log_file"
if ! printf '%s' "$stream_status" | grep -q 'Ready'; then
  mark_phase_done 15-ledger-baseline "reserve and user funding complete; redemption waits for Governance activation"
  exit 0
fi

if ! phase_is_done 15-liquid-icp-funded; then
  liquid_balance="$(icp_balance "$stream" "$liquid_hex" observed_liquid_icp_e8s)"
  if [ "$liquid_balance" -lt "$liquid_amount" ]; then
    sns_testing="$(sns_testing_cli)"
    liquid_delta="$((liquid_amount - liquid_balance))"
    liquid_tokens="$(e8s_to_decimal_tokens "$liquid_delta")"
    transfer_args=(--network "$network_url" transfer-icp)
    if [ -n "${IO_LOCAL_SNS_ICP_TREASURY_IDENTITY:-}" ]; then
      transfer_args+=(--icp-treasury-identity "$IO_LOCAL_SNS_ICP_TREASURY_IDENTITY")
    fi
    transfer_args+=(--amount "$liquid_tokens" --to-principal "$stream" "$liquid_hex")
    run_logged "$log_file" "$sns_testing" "${transfer_args[@]}"
  fi
  mark_phase_done 15-liquid-icp-funded "target_e8s=${liquid_amount} observed_before_e8s=${liquid_balance}"
fi

if ! phase_is_done 15-redemption-complete; then
  pre_snapshot="${GENERATED_DIR}/redemption-pre-snapshot.toml"
  if [ ! -f "$pre_snapshot" ]; then
    staging_account="$(dfx canister call --network "$network_url" --identity "$identity" --query \
      --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" \
      "$stream" get_redemption_staging_account '()')"
    printf 'semantic_staging_account=%s\n' "$staging_account" >> "$log_file"
    pre_total="$(query_nat "$ledger_did" "$ledger" icrc1_total_supply '()' pre_staging_total_supply_e8s)"
    pre_reserve="$(sns_balance "$stream" "$reserve_hex" pre_staging_protocol_reserve_e8s)"
    pre_excluded="$(sns_balance "$governance" "$treasury_hex" pre_staging_sns_treasury_e8s)"
    pre_liquid="$(icp_balance "$stream" "$liquid_hex" pre_payout_liquid_icp_e8s)"
    pre_user_io="$(sns_balance "$operator" none pre_staging_user_io_e8s)"
    pre_user_icp="$(icp_balance "$operator" none pre_payout_user_icp_e8s)"
    stage_response="$(dfx canister call --network "$network_url" --identity "$identity" \
      --candid "$ledger_did" "$ledger" icrc1_transfer \
      "(record { from_subaccount = null; to = record { owner = principal \"${stream}\"; subaccount = opt blob \"$(hex_blob_literal "$staging_hex")\" }; amount = ${redeem_amount} : nat; fee = opt (10000 : nat); memo = null; created_at_time = null })")"
    printf 'staging_transfer_response=%s\n' "$stage_response" >> "$log_file"
    source_block="$(printf '%s' "$stage_response" | tr '\n' ' ' | sed -n 's/.*Ok = \([0-9_][0-9_]*\).*/\1/p' | tr -d '_')"
    require_nat "semantic staging source block" "$source_block"
    staged_total="$(query_nat "$ledger_did" "$ledger" icrc1_total_supply '()' staged_total_supply_e8s)"
    staged_reserve="$(sns_balance "$stream" "$reserve_hex" staged_protocol_reserve_e8s)"
    staging_balance="$(sns_balance "$stream" "$staging_hex" staged_redemption_balance_e8s)"
    if [ "$staged_total" -ne "$((pre_total - 10000))" ] \
      || [ "$staged_reserve" -ne "$pre_reserve" ] \
      || [ "$staging_balance" -lt "$redeem_amount" ]; then
      record_blocker 'semantic staging transfer did not remain claim-bearing outside the formal reserve'
      exit 2
    fi
    formula="$(cargo run --quiet -p xtask --manifest-path "${REPO_ROOT}/Cargo.toml" -- \
      calculate_redemption_economics "$staged_total" "$staged_reserve" "$pre_excluded" \
      "$pre_liquid" "$redeem_amount" 10000)"
    expected_d="$(printf '%s\n' "$formula" | sed -n 's/^redeemable_supply_e8s=//p')"
    expected_gross="$(printf '%s\n' "$formula" | sed -n 's/^gross_icp_e8s=//p')"
    expected_net="$(printf '%s\n' "$formula" | sed -n 's/^net_icp_e8s=//p')"
    for value in "$expected_d" "$expected_gross" "$expected_net"; do
      require_nat "independently calculated redemption value" "$value"
    done
    cat > "$pre_snapshot" <<EOF
[redemption]
source_io_block = ${source_block}
total_io_supply_before_e8s = ${pre_total}
total_io_supply_staged_e8s = ${staged_total}
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

  source_block="$(toml_number "$pre_snapshot" redemption source_io_block)"
  completed_status=''
  for attempt in $(seq 1 32); do
    process_response="$(dfx canister call --network "$network_url" --identity "$identity" \
      --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" \
      "$stream" process_redemptions '()')"
    printf 'process_redemptions_attempt=%s response=%s\n' "$attempt" "$process_response" >> "$log_file"
    run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" \
      --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" resume '()'
    run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" \
      --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" resume_reward_backing '()'
    run_logged "$log_file" dfx canister call --network "$network_url" --identity "$identity" \
      --candid "${REPO_ROOT}/canisters/io_nns_neuron_manager/io_nns_neuron_manager.did" "$nns_manager" resume '()'
    completed_status="$(dfx canister call --network "$network_url" --identity "$identity" --query \
      --candid "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" "$stream" get_status '()')"
    if printf '%s' "$completed_status" | grep -q 'last_completed_redemption = opt record'; then
      break
    fi
    sleep 2
  done
  if ! printf '%s' "$completed_status" | grep -q 'last_completed_redemption = opt record'; then
    record_blocker 'semantic-staging redemption did not complete after bounded automatic-worker prompts'
    exit 2
  fi
  printf '%s\n' "$completed_status" >> "$log_file"
  compact="$(printf '%s' "$completed_status" | tr '\n' ' ')"
  result_source="$(printf '%s' "$compact" | sed -n 's/.*source_io_block = \([0-9_][0-9_]*\).*/\1/p' | tr -d '_')"
  payout_block="$(printf '%s' "$compact" | sed -n 's/.*icp_payout_block = \([0-9_][0-9_]*\).*/\1/p' | tr -d '_')"
  sweep_block="$(printf '%s' "$compact" | sed -n 's/.*reserve_sweep_block = \([0-9_][0-9_]*\).*/\1/p' | tr -d '_')"
  stream_gross="$(printf '%s' "$compact" | sed -n 's/.*gross_icp_e8s = \([0-9_][0-9_]*\).*/\1/p' | tr -d '_')"
  stream_net="$(printf '%s' "$compact" | sed -n 's/.*net_icp_e8s = \([0-9_][0-9_]*\).*/\1/p' | tr -d '_')"
  for value in "$result_source" "$payout_block" "$sweep_block" "$stream_gross" "$stream_net"; do
    require_nat "completed semantic-staging redemption field" "$value"
  done
  if [ "$result_source" != "$source_block" ]; then
    record_blocker 'completed redemption source block differs from the canonically staged transfer'
    exit 2
  fi
  pre_total="$(toml_number "$pre_snapshot" redemption total_io_supply_before_e8s)"
  pre_reserve="$(toml_number "$pre_snapshot" redemption protocol_reserve_io_e8s)"
  pre_excluded="$(toml_number "$pre_snapshot" redemption excluded_io_e8s)"
  pre_liquid="$(toml_number "$pre_snapshot" redemption liquid_icp_e8s)"
  pre_user_io="$(toml_number "$pre_snapshot" redemption user_io_e8s)"
  pre_user_icp="$(toml_number "$pre_snapshot" redemption user_icp_e8s)"
  expected_d="$(toml_number "$pre_snapshot" redemption redeemable_io_supply_e8s)"
  expected_gross="$(toml_number "$pre_snapshot" redemption gross_icp_e8s)"
  expected_net="$(toml_number "$pre_snapshot" redemption net_icp_e8s)"
  if [ "$stream_gross" != "$expected_gross" ] || [ "$stream_net" != "$expected_net" ]; then
    record_blocker 'Stream frozen quote differs from the independently calculated post-staging B/C snapshot'
    exit 2
  fi
  post_total="$(query_nat "$ledger_did" "$ledger" icrc1_total_supply '()' post_sweep_total_supply_e8s)"
  post_reserve="$(sns_balance "$stream" "$reserve_hex" post_sweep_protocol_reserve_e8s)"
  post_staging="$(sns_balance "$stream" "$staging_hex" post_sweep_staging_e8s)"
  post_excluded="$(sns_balance "$governance" "$treasury_hex" post_sweep_sns_treasury_e8s)"
  post_liquid="$(icp_balance "$stream" "$liquid_hex" post_payout_liquid_icp_e8s)"
  post_user_io="$(sns_balance "$operator" none post_staging_user_io_e8s)"
  post_user_icp="$(icp_balance "$operator" none post_payout_user_icp_e8s)"
  if [ "$post_total" -ne "$((pre_total - 20000))" ] \
    || [ "$post_reserve" -ne "$((pre_reserve + redeem_amount - 10000))" ] \
    || [ "$post_staging" -ne 0 ] \
    || [ "$post_excluded" -ne "$pre_excluded" ] \
    || [ "$post_liquid" -ne "$((pre_liquid - stream_gross))" ] \
    || [ "$post_user_io" -ne "$((pre_user_io - redeem_amount - 10000))" ] \
    || [ "$post_user_icp" -ne "$((pre_user_icp + stream_net))" ]; then
    record_blocker 'semantic-staging redemption balances violate payout or sweep conservation'
    exit 2
  fi
  economics="${GENERATED_DIR}/redemption-economics.toml"
  cat > "$economics" <<EOF
[snapshot]
total_io_supply_before_e8s = ${pre_total}
total_io_supply_after_staging_e8s = $((pre_total - 10000))
protocol_reserve_io_e8s = ${pre_reserve}
excluded_io_total_e8s = ${pre_excluded}
redeemable_io_supply_e8s = ${expected_d}
total_backing_icp_e8s = ${pre_liquid}
redemption_io_amount_e8s = ${redeem_amount}
quoted_gross_icp_e8s = ${stream_gross}
initial_io_transfer_fee_e8s = 10000
reserve_sweep_io_fee_e8s = 10000
icp_payout_fee_e8s = 10000
quoted_net_icp_e8s = ${stream_net}

[stream_result]
source_io_block = ${source_block}
icp_payout_block = ${payout_block}
reserve_sweep_block = ${sweep_block}
gross_icp_e8s = ${stream_gross}
net_icp_e8s = ${stream_net}

[ledger_balances]
io_total_before_e8s = ${pre_total}
io_total_after_e8s = ${post_total}
protocol_reserve_before_e8s = ${pre_reserve}
protocol_reserve_after_e8s = ${post_reserve}
staging_after_e8s = ${post_staging}
liquid_icp_before_e8s = ${pre_liquid}
liquid_icp_after_e8s = ${post_liquid}
user_io_before_e8s = ${pre_user_io}
user_io_after_e8s = ${post_user_io}
user_icp_before_e8s = ${pre_user_icp}
user_icp_after_e8s = ${post_user_icp}
EOF
  mark_phase_done 15-redemption-complete \
    "source_io_block=${source_block} icp_payout_block=${payout_block} reserve_sweep_block=${sweep_block} gross_icp_e8s=${stream_gross} net_icp_e8s=${stream_net} economics=${economics}"
fi
