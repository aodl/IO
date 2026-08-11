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
proposal_log="$(phase_log_file 17-reward-event-setup)"
observation_log="$(phase_log_file 17-observe-one-day-reward)"
touch "$proposal_log"

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
  cargo run -p e2e-real-canisters --bin observe_existing_reward) || exit $?

for expected in \
  'advanced_pocketic_seconds=86400' \
  "id: ${proposal_id}," \
  'reward_shares: Some' \
  'ProposalBearing' \
  'processed_reward_event_count: 1' \
  'accumulated_policy_credit: 1000000000000000000'; do
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
  'latest_two_week_target: Some' \
  'nns_governance: Some'; do
  if ! grep -Fq "$expected" "$observation_log"; then
    record_blocker "one-day historian convergence lacks canonical evidence: ${expected}"
    exit 2
  fi
done
event_round="$(sed -n 's/^[[:space:]]*round: \([0-9][0-9]*\),/\1/p' "$observation_log" | head -1)"
require_nat "reward event round" "$event_round"
mark_phase_done 17-one-day-reward-observed \
  "proposal_id=${proposal_id} event_round=${event_round} classification=ProposalBearing processed_count=1 accumulated_policy_credit=1000000000000000000 historian_fresh=true monitoring_settle_seconds=60; see ${observation_log}"
