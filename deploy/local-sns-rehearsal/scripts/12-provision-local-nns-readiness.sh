#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# Provisions canonical local ICP balances and two source-shaped local NNS neurons.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 12-provision-local-nns-readiness)"
touch "$log_file"
if phase_is_done 12-provision-local-nns-readiness; then
  printf 'phase 12 local NNS readiness already completed: %s\n' \
    "$(cat "$(phase_done_file 12-provision-local-nns-readiness)")"
  exit 0
fi

require_command_available dfx
require_command_available cargo
network_url="$(local_network_url)"
identity="$(local_identity_name)"
checkout="$(official_checkout)"
vars_file="$(local_vars_file)"
runtime="$(runtime_file)"
nns_manager="$(toml_string "$vars_file" local io_nns_neuron_manager_canister)"
operator="$(runtime_value accounts operator_principal)"
icp_ledger="$(runtime_value nns icp_ledger)"
nns_governance="$(runtime_value nns governance)"
ledger_did="${checkout}/rs/ledger_suite/icrc1/ledger/ledger.did"
governance_did="${checkout}/rs/nns/governance/canister/governance.did"
governance_test_did="${REHEARSAL_DIR}/nns-governance-test.did"
nns_args="$(nns_install_args_file)"
for file in "$vars_file" "$runtime" "$ledger_did" "$governance_did" "$governance_test_did" "$nns_args"; do
  require_file "$file"
done

expected_fee=10000
jupiter_fee_float=20000
two_week_fee_float=10000
two_week_stake=100000000
two_year_stake=100000000
two_week_nonce=42001
two_year_nonce=42002
approved_delay=252460800
two_week_staging_hex="0303030303030303030303030303030303030303030303030303030303030303"

observed_fee="$(dfx canister call --network "$network_url" --identity "$identity" --query \
  --candid "$ledger_did" "$icp_ledger" icrc1_fee '()' | tr -d '()_ :nat[:space:]')"
if [ "$observed_fee" != "$expected_fee" ]; then
  record_blocker "canonical local ICP fee ${observed_fee} differs from configured ${expected_fee}"
  exit 2
fi

query_balance() {
  local owner="$1"
  local subaccount="$2"
  local account
  if [ -n "$subaccount" ]; then
    account="(record { owner = principal \"${owner}\"; subaccount = opt blob \"$(hex_blob_literal "$subaccount")\" })"
  else
    account="(record { owner = principal \"${owner}\"; subaccount = null })"
  fi
  dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$ledger_did" "$icp_ledger" icrc1_balance_of "$account" \
    | tr -d '()_ :nat[:space:]'
}

transfer_delta() {
  local label="$1"
  local owner="$2"
  local subaccount="$3"
  local required="$4"
  local balance delta account response
  balance="$(query_balance "$owner" "$subaccount")"
  require_nat "${label} balance" "$balance"
  if [ "$balance" -lt "$required" ]; then
    delta="$((required - balance))"
    if [ -n "$subaccount" ]; then
      account="record { owner = principal \"${owner}\"; subaccount = opt blob \"$(hex_blob_literal "$subaccount")\" }"
    else
      account="record { owner = principal \"${owner}\"; subaccount = null }"
    fi
    response="$(dfx canister call --network "$network_url" --identity "$identity" \
      --candid "$ledger_did" "$icp_ledger" icrc1_transfer \
      "(record { from_subaccount = null; to = ${account}; amount = ${delta} : nat; fee = opt (${expected_fee} : nat); memo = null; created_at_time = null })")"
    printf '%s transfer_delta=%s response=%s\n' "$label" "$delta" "$response" >> "$log_file"
    printf '%s' "$response" | grep -q 'Ok' || {
      record_blocker "${label} canonical local ICP transfer failed"
      exit 2
    }
  fi
  balance="$(query_balance "$owner" "$subaccount")"
  if [ "$balance" -lt "$required" ]; then
    record_blocker "${label} balance ${balance} remains below ${required}"
    exit 2
  fi
  printf '%s balance_e8s=%s required_e8s=%s\n' "$label" "$balance" "$required" >> "$log_file"
}

claim_and_shape_neuron() {
  local role="$1"
  local nonce="$2"
  local stake="$3"
  local subaccount claim_response neuron_id now update_arg update_response info info_compact
  subaccount="$(cd "$REPO_ROOT" && cargo run -q -p xtask -- nns_neuron_staking_subaccount "$operator" "$nonce")"
  require_hex_32_bytes "${role} staking subaccount" "$subaccount"
  transfer_delta "${role} neuron stake" "$nns_governance" "$subaccount" "$stake"
  claim_response="$(dfx canister call --network "$network_url" --identity "$identity" \
    --candid "$governance_did" "$nns_governance" claim_or_refresh_neuron_from_account \
    "(record { controller = opt principal \"${operator}\"; memo = ${nonce} : nat64 })")"
  printf '%s claim=%s\n' "$role" "$claim_response" >> "$log_file"
  neuron_id="$(printf '%s' "$claim_response" | tr '\n' ' ' \
    | sed -n 's/.*NeuronId = record { id = \([0-9_][0-9_]*\) : nat64 }.*/\1/p' | tr -d '_')"
  require_nat "${role} claimed neuron id" "$neuron_id"
  if [ "$neuron_id" = 0 ]; then
    record_blocker "${role} claimed zero neuron ID"
    exit 2
  fi
  now="$(date +%s)"
  update_arg="(record {
    id = opt record { id = ${neuron_id} : nat64 };
    staked_maturity_e8s_equivalent = opt (0 : nat64);
    controller = opt principal \"${nns_manager}\";
    recent_ballots = vec {};
    kyc_verified = false;
    neuron_type = null;
    not_for_profit = false;
    maturity_e8s_equivalent = 0 : nat64;
    cached_neuron_stake_e8s = ${stake} : nat64;
    created_timestamp_seconds = ${now} : nat64;
    auto_stake_maturity = opt false;
    aging_since_timestamp_seconds = ${now} : nat64;
    hot_keys = vec {};
    account = blob \"$(hex_blob_literal "$subaccount")\";
    joined_community_fund_timestamp_seconds = null;
    dissolve_state = opt variant { DissolveDelaySeconds = ${approved_delay} : nat64 };
    followees = vec {};
    neuron_fees_e8s = 0 : nat64;
    visibility = null;
    transfer = null;
    known_neuron_data = null;
    spawn_at_timestamp_seconds = null;
    voting_power_refreshed_timestamp_seconds = opt (${now} : nat64);
    deciding_voting_power = null;
    potential_voting_power = null;
    eight_year_gang_bonus_base_e8s = null;
    maturity_disbursements_in_progress = opt vec {};
  })"
  update_response="$(dfx canister call --network "$network_url" --identity "$identity" \
    --candid "$governance_test_did" "$nns_governance" update_neuron "$update_arg")"
  printf '%s update=%s\n' "$role" "$update_response" >> "$log_file"
  if [ "$(printf '%s' "$update_response" | tr -d '[:space:]')" != '(null)' ]; then
    record_blocker "${role} source-shaped NNS Governance neuron update failed"
    exit 2
  fi
  info="$(dfx canister call --network "$network_url" --identity "$identity" --query \
    --candid "$governance_did" "$nns_governance" get_neuron_info "(${neuron_id} : nat64)")"
  printf '%s info=%s\n' "$role" "$info" >> "$log_file"
  info_compact="$(printf '%s' "$info" | tr -d '_')"
  printf '%s' "$info_compact" | grep -q "stakee8s = ${stake}" || {
    record_blocker "${role} public NNS neuron observation has the wrong stake"
    exit 2
  }
  printf '%s' "$info_compact" | grep -q "dissolvedelayseconds = ${approved_delay}" || {
    record_blocker "${role} public NNS neuron observation has the wrong dissolve delay"
    exit 2
  }
  printf '%s\n' "$neuron_id"
}

transfer_delta "Jupiter staging fee float" "$nns_manager" "" "$jupiter_fee_float"
transfer_delta "two-week maturity staging fee float" "$nns_manager" "$two_week_staging_hex" "$two_week_fee_float"
two_year_neuron_id="$(claim_and_shape_neuron two-year "$two_year_nonce" "$two_year_stake")"
two_week_neuron_id="$(claim_and_shape_neuron reward-backing "$two_week_nonce" "$two_week_stake")"
if [ "$two_year_neuron_id" = "$two_week_neuron_id" ]; then
  record_blocker "local protected NNS neuron roles resolved to the same ID"
  exit 2
fi

sed -i -E "s/(two_year_neuron_id = )[0-9_]+/\1${two_year_neuron_id}/" "$nns_args"
sed -i -E "s/(two_week_neuron_id = )[0-9_]+/\1${two_week_neuron_id}/" "$nns_args"
grep -Fq "two_year_neuron_id = ${two_year_neuron_id}" "$nns_args"
grep -Fq "two_week_neuron_id = ${two_week_neuron_id}" "$nns_args"

fixture="${GENERATED_DIR}/nns-readiness-fixture.toml"
cat > "$fixture" <<EOF
[staging]
canonical_icp_fee_e8s = ${expected_fee}
jupiter_fee_float_e8s = ${jupiter_fee_float}
two_week_fee_float_e8s = ${two_week_fee_float}

[two_year_neuron]
id = ${two_year_neuron_id}
controller = "${nns_manager}"
claim_controller = "${operator}"
nonce = ${two_year_nonce}
seeded_principal_e8s = ${two_year_stake}
dissolve_delay_seconds = ${approved_delay}
auto_stake_maturity = false
ordinary_maturity_e8s = 0
staked_maturity_e8s = 0

[reward_backing_neuron]
id = ${two_week_neuron_id}
controller = "${nns_manager}"
claim_controller = "${operator}"
nonce = ${two_week_nonce}
seeded_principal_e8s = ${two_week_stake}
dissolve_delay_seconds = ${approved_delay}
auto_stake_maturity = false
ordinary_maturity_e8s = 0
staked_maturity_e8s = 0
maturity_disbursement_pending = false
EOF

mark_phase_done 12-provision-local-nns-readiness \
  "two_year_neuron_id=${two_year_neuron_id} reward_backing_neuron_id=${two_week_neuron_id} controller=${nns_manager} canonical_fee=${expected_fee} jupiter_float=${jupiter_fee_float} two_week_float=${two_week_fee_float}"
