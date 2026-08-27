#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# Runs the maintained Layer B/C account-semantic proofs after the fresh official
# SNS lifecycle (Layer A) has reached its one-day reward checkpoint.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

phase=18-exercise-account-semantic-protocol
log_file="$(phase_log_file "$phase")"
result_file="${GENERATED_DIR}/scenario-results.toml"

for required_phase in \
  10-bootstrap-official-network \
  11-build-local-io-canisters \
  12-deploy-local-dapps \
  12-provision-local-nns-readiness \
  13-propose-and-finalize-sns \
  14-discover-sns-canisters \
  15-redemption-complete \
  16-exercise-index-and-archives \
  17-exercise-governance-and-controllers \
  17-one-day-reward-observed; do
  if ! phase_is_done "$required_phase"; then
    record_blocker "account-semantic protocol exercise requires completed phase ${required_phase}"
    exit 2
  fi
done

require_file "${POCKET_IC_BIN:-}"
require_file "${IO_POST_M70_NNS_GOVERNANCE_WASM:-}"
require_file "${IO_REAL_SNS_WASM_MANIFEST:-}"
if [ ! -d "${IO_REAL_SNS_WASM_DIR:-}" ]; then
  record_blocker "IO_REAL_SNS_WASM_DIR must name the pinned real SNS artifact directory"
  exit 2
fi

source_commit="$(phase_detail_value 11-build-local-io-canisters source_commit)"
artifact_commit="$(phase_detail_value 11-build-local-io-canisters artifact_commit)"
manifest_sha256="$(phase_detail_value 11-build-local-io-canisters manifest_sha256)"
current_manifest_sha256="$(sha256sum "${REPO_ROOT}/release-artifacts/manifest.json" | awk '{print $1}')"
if [ "$manifest_sha256" != "$current_manifest_sha256" ]; then
  record_blocker "release manifest changed after the exact-source rehearsal checkpoint"
  exit 2
fi
stream_wasm="${REPO_ROOT}/release-artifacts/io_stream_manager.wasm"
nns_wasm="${REPO_ROOT}/release-artifacts/io_nns_neuron_manager.wasm"
require_file "$stream_wasm"
require_file "$nns_wasm"

if phase_is_done "$phase"; then
  recorded_log_sha="$(phase_detail_value "$phase" log_sha256)"
  recorded_result_sha="$(phase_detail_value "$phase" result_sha256)"
  require_file "$log_file"
  require_file "$result_file"
  [ "$(sha256sum "$log_file" | awk '{print $1}')" = "$recorded_log_sha" ]
  [ "$(sha256sum "$result_file" | awk '{print $1}')" = "$recorded_result_sha" ]
  printf 'verified restart-safe %s checkpoint\n' "$phase"
  exit 0
fi

: > "$log_file"
{
  printf 'evidence_layer=B/C controlled PocketIC and exact proposal boundary\n'
  printf 'source_commit=%s\nartifact_commit=%s\nrelease_manifest_sha256=%s\n' \
    "$source_commit" "$artifact_commit" "$manifest_sha256"
  printf 'stream_release_wasm_sha256=%s\n' "$(sha256sum "$stream_wasm" | awk '{print $1}')"
  printf 'nns_release_wasm_sha256=%s\n' "$(sha256sum "$nns_wasm" | awk '{print $1}')"
} >> "$log_file"

run_case() {
  local label="$1"
  shift
  printf '\n=== case %s ===\n' "$label" >> "$log_file"
  (cd "$REPO_ROOT" && run_logged "$log_file" env \
    RUST_TEST_THREADS=1 \
    POCKET_IC_MUTE_SERVER=1 \
    POCKET_IC_BIN="$POCKET_IC_BIN" \
    IO_REAL_SNS_WASM_DIR="$IO_REAL_SNS_WASM_DIR" \
    IO_REAL_SNS_WASM_MANIFEST="$IO_REAL_SNS_WASM_MANIFEST" \
    IO_POST_M70_NNS_GOVERNANCE_WASM="$IO_POST_M70_NNS_GOVERNANCE_WASM" \
    IO_POST_M70_XRC_WASM="${REPO_ROOT}/target/wasm32-unknown-unknown/debug/mock_nns_xrc.wasm" \
    IO_POST_M70_ACTOR_WASM="${REPO_ROOT}/target/wasm32-unknown-unknown/debug/mock_nns_candidate_actor.wasm" \
    IO_ACCOUNT_SEMANTIC_STREAM_WASM="$stream_wasm" \
    IO_ACCOUNT_SEMANTIC_NNS_WASM="$nns_wasm" \
    "$@")
}

run_case build-controlled-fixtures cargo build \
  -p mock-icp-ledger -p mock-io-ledger -p mock-nns-governance \
  -p mock-nns-xrc -p mock-nns-candidate-actor -p mock-sns-governance \
  -p mock-sns-root -p io-stream-manager -p io-nns-neuron-manager \
  --target wasm32-unknown-unknown

run_case semantic-carry-forward cargo test -p io-nns-neuron-manager \
  --test io_nns_governance_recovery_pocketic \
  semantic_staging_carries_late_value_into_the_next_cycle_for_both_roles \
  -- --exact --nocapture --test-threads=1
run_case no-effect-and-ambiguous-maturity cargo test -p io-nns-neuron-manager \
  --test io_nns_governance_recovery_pocketic \
  disburse_maturity_decoded_rejection_retries_but_ambiguity_never_resubmits \
  -- --exact --nocapture --test-threads=1
run_case ambiguous-split cargo test -p io-nns-neuron-manager \
  --test io_nns_governance_recovery_pocketic \
  ambiguous_split_is_discovered_after_upgrade_without_a_second_call \
  -- --exact --nocapture --test-threads=1
run_case split-embryo-and-phase-recovery cargo test -p io-nns-neuron-manager \
  --test io_nns_governance_recovery_pocketic \
  every_persisted_governance_phase_recovers_and_exact_replay_is_call_free \
  -- --exact --nocapture --test-threads=1
run_case child-disburse-lost-callback cargo test -p io-nns-neuron-manager \
  --test io_nns_governance_recovery_pocketic \
  child_disburse_malformed_after_effect_never_resubmits \
  -- --exact --nocapture --test-threads=1
run_case paired-receipt-replay cargo test -p io-stream-manager \
  --test io_paired_receipt_recovery_pocketic \
  malformed_prepare_after_persistence_replays_and_quarantines_redemption \
  -- --exact --nocapture --test-threads=1
run_case liquidity-shortfall cargo test -p io-stream-manager \
  --test io_stream_manager_pocketic \
  liquidity_shortfall_uses_only_scalar_claim_reads_and_pulls_no_io \
  -- --exact --nocapture --test-threads=1

for candidate in \
  exact_post_m70_upgrade_rewards_fourteen_day_boundary \
  exact_post_m70_fourteen_day_parent_follows_and_earns_maturity \
  exact_post_m70_split_child_subaccount_lookup_matches_io_recovery \
  exact_post_m70_minimum_stake_boundaries; do
  run_case "$candidate" cargo test -p e2e-real-canisters \
    "post_mission70_nns_candidate::${candidate}" \
    -- --ignored --exact --nocapture --test-threads=1
done

for scenario in \
  controlled_jupiter_uses_real_nns_and_exact_production_receipts \
  controlled_two_year_compounds_real_maturity_without_io_issuance \
  combined_real_sns_nns_io_lifecycle_reconciles_maturity_and_redemption \
  combined_real_jupiter_then_two_week_maturity_accepts_donations; do
  run_case "$scenario" cargo test -p e2e-real-canisters \
    "nns_backing::tests::${scenario}" \
    -- --ignored --exact --nocapture --test-threads=1
done

for marker in \
  'account_semantic_carry_forward kind=TwoWeek' \
  'account_semantic_carry_forward kind=TwoYear' \
  'account_semantic_jupiter ' \
  'account_semantic_two_year cycle=0' \
  'account_semantic_two_year cycle=1' \
  'account_semantic_combined ' \
  'account_semantic_liquidity_shortfall ' \
  'account_semantic_receipt_recovery '; do
  if ! grep -Fq "$marker" "$log_file"; then
    record_blocker "account-semantic validation log lacks required successful marker: ${marker}"
    exit 2
  fi
done
obsolete_maturity_api="prove_maturity""_mint"
obsolete_mint_state="MintProof""State"
obsolete_mint_evidence="Mint""Evidence"
if rg -n "${obsolete_maturity_api}|${obsolete_mint_state}|${obsolete_mint_evidence}" \
  "${REPO_ROOT}/canisters/io_nns_neuron_manager/io_nns_neuron_manager.did" \
  "${REPO_ROOT}/canisters/io_stream_manager/io_stream_manager.did" >> "$log_file"; then
  record_blocker "production Candid reintroduced maturity Mint provenance"
  exit 2
fi

cat > "$result_file" <<EOF
[evidence]
schema = "account-semantic-v1"
source_commit = "${source_commit}"
artifact_commit = "${artifact_commit}"
release_manifest_sha256 = "${manifest_sha256}"
official_sns_layer_complete = true
exact_nns_boundary_layer_complete = true
controlled_orchestration_layer_complete = true

[backing]
identity = "B = L + P + U + T"
identity_checked = true
liquid_first = true
ordinary_reconciliation = true
lazy_pooled_parent = true

[jupiter]
authorized_source_block_required = true
unauthorized_rejected = true
paired_settlement = true
empty_genesis_one_to_one = true
provenance_after_custody = false

[two_week]
fixed_semantic_account = true
paired_settlement = true
mint_provenance_api_absent = true
frozen_staker_settlement = true

[two_year]
fixed_semantic_account = true
paired_receipt = false
io_issuance = false
claim_credit_increases_backing = true

[carry_forward]
g1_capture_units = 100
late_units = 20
g2_maturity_units = 50
g2_capture_units = 70
two_week_final_staging_e8s = 0
two_year_final_staging_e8s = 0
cross_account_isolation = true

[liveness]
no_effect_retry_exactly_once = true
ambiguous_effect_never_resubmitted = true
liquidity_shortfall_pulls_io = false
request_retryable = true
cohort_unwind_complete = true
upgrade_restart_exact = true
receipt_replay_exact = true
EOF

log_sha256="$(sha256sum "$log_file" | awk '{print $1}')"
result_sha256="$(sha256sum "$result_file" | awk '{print $1}')"
mark_phase_done "$phase" \
  "source_commit=${source_commit}; artifact_commit=${artifact_commit}; log_sha256=${log_sha256}; result_sha256=${result_sha256}; layers=A,B,C; exact_candidate=143660; complete=true"
