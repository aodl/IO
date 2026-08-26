#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# Produces one proposal-bearing daily reward observation through the production
# resume_reward_work keeper API.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

if ! phase_is_done 17-stream-activated; then
  record_blocker "stream lifecycle must be Ready before the daily reward observation"
  exit 2
fi
server_url="${IO_LOCAL_POCKET_IC_SERVER_URL:-}"
instance_id="${IO_LOCAL_POCKET_IC_INSTANCE_ID:-}"
require_loopback_url "$server_url"
require_nat "PocketIC instance ID" "$instance_id"
governance="$(sns_canister_id governance)"
stream="$(toml_string "$(local_vars_file)" local io_stream_manager_canister)"
historian="$(toml_string "$(local_vars_file)" local io_historian_canister)"
proposer="$(runtime_value accounts operator_principal)"
neuron_hex="$(runtime_value governance sns_neuron_subaccount_hex)"
require_hex_32_bytes "SNS reward proposer neuron subaccount" "$neuron_hex"
proposal_log="$(phase_log_file 17-reward-event-setup)"
observation_log="$(phase_log_file 17-observe-one-day-reward)"
touch "$proposal_log"

if ! phase_is_done 17-reward-neuron-eligible; then
  neuron_hex="$(runtime_value governance sns_neuron_subaccount_hex)"
  require_hex_32_bytes "SNS reward proposer neuron subaccount" "$neuron_hex"
  governance_did="$(official_checkout)/rs/sns/governance/canister/governance.did"
  eligibility_args="$(mktemp "${GENERATED_DIR}/reward-neuron-eligibility.XXXXXX.did")"
  printf '(record { subaccount = blob "%s"; command = opt variant { Configure = record { operation = opt variant { IncreaseDissolveDelay = record { additional_dissolve_delay_seconds = 1 : nat32 } } } } })\n' \
    "$(hex_blob_literal "$neuron_hex")" > "$eligibility_args"
  run_logged "$proposal_log" dfx canister call --network "$(local_network_url)" \
    --identity "$(local_identity_name)" --candid "$governance_did" "$governance" \
    manage_neuron --argument-file "$eligibility_args"
  tail -20 "$proposal_log" | grep -q 'Configure' || {
    record_blocker 'SNS reward proposer dissolve-delay adjustment did not return Configure success'
    exit 2
  }
  proposer="$(dfx canister call --network "$(local_network_url)" \
    --identity "$(local_identity_name)" --query --candid "$governance_did" \
    "$governance" get_neuron \
    "(record { neuron_id = opt record { id = blob \"$(hex_blob_literal "$neuron_hex")\" } })")"
  printf '%s\n' "$proposer" >> "$proposal_log"
  printf '%s' "$proposer" | tr -d '_' | grep -q \
    'DissolveDelaySeconds = 1209600 : nat64' || {
    record_blocker 'SNS reward proposer is not at the exact frozen two-week eligibility duration'
    exit 2
  }
  mark_phase_done 17-reward-neuron-eligible \
    "neuron=${neuron_hex} dissolve_delay_seconds=1209600 additional_seconds=1"
fi

if ! phase_is_done 17-reward-event-setup; then
  action='variant { Motion = record { motion_text = "Observe one exact IO daily reward event." } }'
  proposal_id="$(submit_sns_proposal "$proposal_log" \
    'One-day IO reward observation' \
    'Local-only proposal-bearing event for exact daily entitlement observation.' \
    "$action")"
  wait_sns_proposal "$proposal_log" "$proposal_id"
  mark_phase_done 17-reward-event-setup \
    "proposal_id=${proposal_id} advance_seconds=86400"
fi
proposal_id="$(sed -n 's/.*proposal_id=\([0-9][0-9]*\).*/\1/p' \
  "$(phase_done_file 17-reward-event-setup)")"
require_nat "reward proposal ID" "$proposal_id"

: > "$observation_log"
(cd "$REPO_ROOT" && run_logged "$observation_log" env \
  IO_POCKET_IC_SERVER_URL="$server_url" \
  IO_POCKET_IC_INSTANCE_ID="$instance_id" \
  IO_LOCAL_SNS_GOVERNANCE_ID="$governance" \
  IO_LOCAL_STREAM_MANAGER_ID="$stream" \
  IO_LOCAL_HISTORIAN_ID="$historian" \
  IO_LOCAL_HISTORIAN_SETTLE_SECONDS=60 \
  IO_LOCAL_REWARD_ADVANCE_SECONDS=86400 \
  IO_LOCAL_REWARD_RESUME=1 \
  IO_LOCAL_REWARD_CANONICAL_TWO_EVENT=1 \
  IO_LOCAL_REWARD_PROPOSER_PRINCIPAL="$proposer" \
  IO_LOCAL_REWARD_NEURON_SUBACCOUNT_HEX="$neuron_hex" \
  cargo run -p e2e-real-canisters --bin observe_existing_reward) || exit $?

for expected in \
  'advanced_pocketic_seconds=86400' \
  "id: ${proposal_id}," \
  'reward_shares: Some' \
  'ZeroEligibleParticipation' \
  'canonical_reconciliation_idle_after_attempt=' \
  'canonical_structural_refresh=Ok' \
  'canonical_reward_proposal_id=' \
  'canonical_reward_advanced_pocketic_seconds=86400' \
  'ProposalBearing' \
  'processed_reward_event_count: 2' \
  'accumulated_policy_credit: 2000000000000000000'; do
  if ! grep -Fq "$expected" "$observation_log"; then
    record_blocker "one-day reward observation lacks canonical evidence: ${expected}"
    exit 2
  fi
done
for expected in \
  'historian_settle_seconds=60' \
  'freshness: Fresh' \
  'module_match: Matching' \
  'two_week_maturity_baseline_reconciled: true' \
  'latest_two_week_target: None' \
  'nns_governance: Some'; do
  if ! grep -Fq "$expected" "$observation_log"; then
    record_blocker "one-day historian convergence lacks canonical evidence: ${expected}"
    exit 2
  fi
done
event_round="$(sed -n 's/^[[:space:]]*round: \([0-9][0-9]*\),/\1/p' "$observation_log" | head -1)"
require_nat "reward event round" "$event_round"
mark_phase_done 17-one-day-reward-observed \
  "warmup_proposal_id=${proposal_id} event_round=${event_round} classification=ProposalBearing processed_count=2 accumulated_policy_credit=2000000000000000000 prospective_reentry=true lazy_parent_reconciled=true historian_fresh=true monitoring_settle_seconds=60; see ${observation_log}"
