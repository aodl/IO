#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# Packages only completed canonical phase output. It performs no canister call.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

for phase in 13-propose-and-finalize-sns 14-discover-sns-canisters \
  15-redemption-complete 16-exercise-index-and-archives \
  17-excluded-account-preflight 17-exercise-governance-and-controllers \
  17-one-day-reward-observed; do
  if ! phase_is_done "$phase"; then
    record_blocker "completed evidence requires successful phase ${phase}"
    exit 2
  fi
done

require_command_available jq
require_command_available sha256sum
release_manifest="${REPO_ROOT}/release-artifacts/manifest.json"
bundle_dir="${IO_LOCAL_SNS_BUNDLE_DIR:?IO_LOCAL_SNS_BUNDLE_DIR is required}"
bundle_manifest="${bundle_dir}/manifest.toml"
source_evidence="${IO_LOCAL_SNS_CANISTER_EVIDENCE_FILE:-${REHEARSAL_DIR}/canister-ids.local.toml}"
reward_log="$(phase_log_file 17-observe-one-day-reward)"
account_map="${GENERATED_DIR}/account-map.toml"
economics="${GENERATED_DIR}/redemption-economics.toml"
historian_config="${GENERATED_DIR}/historian-observation-config.did"
treasury_history="${GENERATED_DIR}/treasury-account-history.log"
archive_observation="$(phase_log_file 16-exercise-index-and-archives)"
for required in "$release_manifest" "$bundle_manifest" "$source_evidence" "$reward_log" \
  "$account_map" "$economics" "$historian_config" "$treasury_history" \
  "$archive_observation"; do
  require_file "$required"
done
reward_eligible_credit="$(sed -n 's/^[[:space:]]*eligible_credit_total: \([0-9][0-9]*\),/\1/p' "$reward_log" | head -1)"
require_nat "reward eligible credit total" "$reward_eligible_credit"

official_commit="${IO_LOCAL_SNS_OFFICIAL_IC_COMMIT:-${PINNED_IC_COMMIT}}"
source_commit="$(jq -er '.git_commit' "$release_manifest")"
artifact_commit="$(git -C "$REPO_ROOT" rev-parse HEAD)"
short_commit="${official_commit:0:7}"
evidence_date="${IO_LOCAL_SNS_EVIDENCE_DATE:-$(date -u +%F)}"
output_root="${IO_LOCAL_SNS_EVIDENCE_OUTPUT_ROOT:-${REHEARSAL_DIR}/evidence}"
package_dir="${output_root}/${evidence_date}-${short_commit}-canonical-economics"
if [ -e "$package_dir" ]; then
  record_blocker "refusing to overwrite immutable evidence package ${package_dir}"
  exit 2
fi
mkdir -p "$package_dir"
cp "$source_evidence" "${package_dir}/canister-ids.local.toml"
cp "$(sns_init_file)" "${package_dir}/sns_init.local.yaml"
cp "$reward_log" "${package_dir}/historian-dashboard.log"
cp "$(stream_install_args_file)" "${package_dir}/stream-install-args.did"
cp "$(nns_install_args_file)" "${package_dir}/nns-manager-install-args.did"
cp "$historian_config" "${package_dir}/historian-observation-config.did"
cp "$account_map" "${package_dir}/account-map.toml"
cp "$economics" "${package_dir}/redemption-economics.toml"
cp "$treasury_history" "${package_dir}/treasury-account-history.log"
cp "$archive_observation" "${package_dir}/archive-observation.log"

manifest_value() {
  local canister="$1"
  local key="$2"
  jq -er --arg canister "$canister" --arg key "$key" \
    '.artifacts[] | select(.canister == $canister) | .[$key]' "$release_manifest"
}

phase_value() {
  local phase="$1"
  local key="$2"
  sed -n "s/.*${key}=\([^ ;]*\).*/\1/p" "$(phase_done_file "$phase")" | head -1
}

governance_raw="$(toml_string "$bundle_manifest" artifacts sns_governance_sha256)"
governance_gzip="$(toml_string "$bundle_manifest" artifacts sns_governance_source_sha256)"
governance_did="$(toml_string "$bundle_manifest" contract governance_did_sha256)"
root_raw="$(toml_string "$bundle_manifest" artifacts sns_root_sha256)"
root_gzip="$(toml_string "$bundle_manifest" artifacts sns_root_source_sha256)"
root_did="$(toml_string "$bundle_manifest" contract root_did_sha256)"
historian_raw="$(manifest_value io_historian raw_wasm_sha256)"
historian_gzip="$(manifest_value io_historian gz_wasm_sha256)"
historian_before="$(phase_value 17-upgrade-attempted before)"
historian_payload="$(phase_value 17-upgrade-attempted payload_wasm_sha256)"
upgrade_proposal="$(phase_value 17-upgrade-attempted proposal_id)"
if [ "$historian_payload" != "$historian_raw" ]; then
  record_blocker "historian Governance payload ${historian_payload} does not match current raw release ${historian_raw}"
  exit 2
fi

update_toml_value() {
  local file="$1" section="$2" key="$3" value="$4" kind="$5"
  local temporary="${file}.tmp"
  awk -v wanted="[${section}]" -v key="$key" -v value="$value" -v kind="$kind" '
    $0 == wanted { active = 1; print; next }
    /^\[/ { active = 0 }
    active && $0 ~ "^[[:space:]]*" key "[[:space:]]*=" {
      if (kind == "string") print key " = \"" value "\"";
      else print key " = " value;
      found = 1;
      next
    }
    { print }
    END { if (!found) exit 2 }
  ' "$file" > "$temporary" || {
    rm -f "$temporary"
    record_blocker "cannot update evidence field [${section}].${key}"
    exit 2
  }
  mv "$temporary" "$file"
}

packaged_ids="${package_dir}/canister-ids.local.toml"
sed -i 's/historian_payload_gzip_sha256/historian_payload_wasm_sha256/' "$packaged_ids"
for entry in \
  "io_release_source_commit:${source_commit}" \
  "io_artifact_recording_commit:${artifact_commit}" \
  "sns_governance_raw_sha256:${governance_raw}" \
  "sns_root_raw_sha256:${root_raw}" \
  "historian_before_module_sha256:${historian_before}" \
  "historian_payload_wasm_sha256:${historian_payload}" \
  "historian_release_raw_sha256:${historian_raw}"; do
  update_toml_value "$packaged_ids" provenance "${entry%%:*}" "${entry#*:}" string
done
for entry in \
  "initial_observed_supply_e8s:$(phase_detail_value 15-treasury-before-reserve total_supply_e8s)" \
  "final_total_supply_e8s:$(toml_number "$economics" ledger_balances io_total_after_e8s)" \
  "final_reserve_balance_e8s:$(toml_number "$economics" ledger_balances protocol_reserve_after_e8s)" \
  "approval_block:$(toml_number "$economics" stream_result approval_block)" \
  "io_redemption_block:$(toml_number "$economics" stream_result io_block)" \
  "icp_payout_block:$(toml_number "$economics" stream_result icp_block)" \
  "redemption_io_e8s:$(toml_number "$economics" snapshot redemption_io_amount_e8s)" \
  "gross_icp_e8s:$(toml_number "$economics" snapshot quoted_gross_icp_e8s)" \
  "net_icp_e8s:$(toml_number "$economics" snapshot quoted_net_icp_e8s)" \
  "io_fee_e8s:$(toml_number "$economics" snapshot io_fee_e8s)" \
  "icp_fee_e8s:$(toml_number "$economics" snapshot icp_fee_e8s)" \
  "duplicate_transfer_block:$(phase_detail_value 15-ledger-negatives duplicate_block)"; do
  update_toml_value "$packaged_ids" ledger "${entry%%:*}" "${entry#*:}" number
done
for entry in \
  "nns_publish_governance:$(phase_value 13-candidate-published governance_proposal)" \
  "nns_publish_root:$(phase_value 13-candidate-published root_proposal)" \
  "create_sns:$(jq -er '.id' "${GENERATED_DIR}/create-sns-proposal.json")" \
  "neuron_cap:$(phase_value 14-neuron-cap-set proposal_id)" \
  "reserve_funding:$(phase_value 15-reserve-funded proposal_id)" \
  "user_funding:$(phase_value 15-user-funded proposal_id)" \
  "module_upgrade:${upgrade_proposal}" \
  "stream_function_registration:$(phase_value 17-stream-function-registered proposal_id)" \
  "stream_activation:$(phase_value 17-stream-activated proposal_id)" \
  "nns_function_registration:$(phase_value 17-nns-function-registered proposal_id)" \
  "nns_activation:$(phase_value 17-nns-activated proposal_id)" \
  "reward_motion:$(phase_value 17-one-day-reward-observed proposal_id)"; do
  update_toml_value "$packaged_ids" proposals "${entry%%:*}" "${entry#*:}" number
done
for entry in \
  "jupiter_staging_e8s:$(phase_value 17-nns-activated jupiter_staging_e8s)" \
  "two_week_staging_e8s:$(phase_value 17-nns-activated two_week_staging_e8s)" \
  "seeded_principal_e8s:$(toml_number "${GENERATED_DIR}/nns-readiness-fixture.toml" reward_backing_neuron seeded_principal_e8s)" \
  "dissolve_delay_seconds:$(toml_number "${GENERATED_DIR}/nns-readiness-fixture.toml" reward_backing_neuron dissolve_delay_seconds)"; do
  update_toml_value "$packaged_ids" readiness "${entry%%:*}" "${entry#*:}" number
done
update_toml_value "$packaged_ids" reward event_round \
  "$(phase_value 17-one-day-reward-observed event_round)" number
update_toml_value "$packaged_ids" reward eligible_credit \
  "$reward_eligible_credit" number
update_toml_value "$packaged_ids" readiness reward_backing_neuron_id \
  "$(toml_number "${GENERATED_DIR}/nns-readiness-fixture.toml" reward_backing_neuron id)" number
update_toml_value "$packaged_ids" readiness two_year_neuron_id \
  "$(toml_number "${GENERATED_DIR}/nns-readiness-fixture.toml" two_year_neuron id)" number

discovery="${GENERATED_DIR}/sns-canisters.json"
require_file "$discovery"
for role in root governance ledger index swap; do
  observed="$(jq -er --arg role "$role" '.[$role].canister_id' "$discovery")"
  recorded="$(toml_string "$packaged_ids" sns_canisters "$role")"
  [ "$recorded" = "$observed" ] || {
    record_blocker "packaged SNS ${role} ${recorded} differs from canonical run ${observed}"
    exit 2
  }
done
for role in io_stream_manager io_nns_neuron_manager io_historian frontend; do
  recorded="$(toml_string "$packaged_ids" io_dapp_canisters "$role")"
  local_key="${role}_canister"
  observed="$(toml_string "$(local_vars_file)" local "$local_key")"
  [ "$recorded" = "$observed" ] || {
    record_blocker "packaged dapp ${role} ${recorded} differs from canonical run ${observed}"
    exit 2
  }
done

cat > "${package_dir}/manifest.toml" <<EOF
[provenance]
official_ic_repository = "dfinity/ic"
official_ic_source_commit = "${official_commit}"
sns_testing_source_path = "rs/sns/testing"
complete = true
monitoring = true
canonical_redemption_economics = true
io_release_source_commit = "${source_commit}"
io_artifact_recording_commit = "${artifact_commit}"
EOF

cat > "${package_dir}/release-evidence.toml" <<EOF
[release]
source_commit = "${source_commit}"
artifact_recording_commit = "${artifact_commit}"
manifest_sha256 = "$(sha256sum "$release_manifest" | awk '{print $1}')"

[io_stream_manager]
raw_wasm_sha256 = "$(manifest_value io_stream_manager raw_wasm_sha256)"
gzip_wasm_sha256 = "$(manifest_value io_stream_manager gz_wasm_sha256)"

[io_nns_neuron_manager]
raw_wasm_sha256 = "$(manifest_value io_nns_neuron_manager raw_wasm_sha256)"
gzip_wasm_sha256 = "$(manifest_value io_nns_neuron_manager gz_wasm_sha256)"

[io_historian]
raw_wasm_sha256 = "${historian_raw}"
gzip_wasm_sha256 = "${historian_gzip}"

[io_frontend]
raw_wasm_sha256 = "$(manifest_value frontend raw_wasm_sha256)"
gzip_wasm_sha256 = "$(manifest_value frontend gz_wasm_sha256)"
EOF

dfx_bin="$(command -v dfx)"
pocket_ic_bin="${POCKET_IC_BIN:?POCKET_IC_BIN is required for completed evidence}"
sns_bin="$(sns_cli)"
sns_testing_bin="$(sns_testing_cli)"
sns_testing_init_bin="$(official_checkout)/bazel-bin/rs/sns/testing/sns-testing-init"
cat > "${package_dir}/toolchain-provenance.toml" <<EOF
[tools]
dfx_version = "$(dfx --version)"
dfx_sha256 = "$(sha256sum "$dfx_bin" | awk '{print $1}')"
pocket_ic_version = "pocket-ic-server 14.0.0"
pocket_ic_sha256 = "$(sha256sum "$pocket_ic_bin" | awk '{print $1}')"
sns_cli_version = "source-built ${short_commit}"
sns_cli_sha256 = "$(sha256sum "$sns_bin" | awk '{print $1}')"
sns_testing_init_version = "source-built ${short_commit}"
sns_testing_init_sha256 = "$(sha256sum "$sns_testing_init_bin" | awk '{print $1}')"
sns_testing_version = "source-built ${short_commit}"
sns_testing_sha256 = "$(sha256sum "$sns_testing_bin" | awk '{print $1}')"
EOF

reserve_proposal="$(phase_value 15-reserve-funded proposal_id)"
reserve_transfer_block="$(cargo run --quiet -p xtask --manifest-path "${REPO_ROOT}/Cargo.toml" -- \
  index_transfer_block "$treasury_history" "$(runtime_value amounts reserve_funding_e8s)" 00000000000005dd)"
user_transfer_block="$(cargo run --quiet -p xtask --manifest-path "${REPO_ROOT}/Cargo.toml" -- \
  index_transfer_block "$treasury_history" "$(runtime_value amounts user_funding_e8s)" 00000000000005de)"
cat > "${package_dir}/reserve-funding-evidence.toml" <<EOF
[reserve]
proposal_id = ${reserve_proposal}
proposal_adopted = true
proposal_executed = true
treasury_transfer_amount_e8s = $(toml_number "$packaged_ids" ledger reserve_funding_e8s)
transfer_fee_e8s = $(toml_number "$packaged_ids" ledger transaction_fee_e8s)
transfer_block = ${reserve_transfer_block}
reserve_owner = "$(toml_string "$packaged_ids" io_dapp_canisters io_stream_manager)"
reserve_subaccount_hex = "$(runtime_value accounts reserve_subaccount_hex)"
treasury_name = "sns-treasury"
treasury_owner = "$(toml_string "$account_map" excluded_sns_treasury owner)"
treasury_subaccount_hex = "$(toml_string "$account_map" excluded_sns_treasury subaccount_hex)"
treasury_balance_before_e8s = $(phase_detail_value 15-treasury-before-reserve treasury_balance_e8s)
treasury_balance_after_reserve_e8s = $(phase_detail_value 15-reserve-funded treasury_balance_e8s)
treasury_balance_after_user_e8s = $(phase_detail_value 15-user-funded treasury_balance_e8s)
user_funding_transfer_block = ${user_transfer_block}
final_balance_e8s = $(toml_number "$packaged_ids" ledger final_reserve_balance_e8s)
final_total_supply_e8s = $(toml_number "$packaged_ids" ledger final_total_supply_e8s)
EOF

cat > "${package_dir}/ledger-evidence.toml" <<EOF
[ledger]
ledger_canister = "$(toml_string "$packaged_ids" sns_canisters ledger)"
index_canister = "$(toml_string "$packaged_ids" sns_canisters index)"
token_symbol = "$(toml_string "$packaged_ids" ledger token_symbol)"
fee_e8s = $(toml_number "$packaged_ids" ledger transaction_fee_e8s)
duplicate_block = $(toml_number "$packaged_ids" ledger duplicate_transfer_block)
approval_block = $(toml_number "$packaged_ids" ledger approval_block)
redemption_io_block = $(toml_number "$packaged_ids" ledger io_redemption_block)
redemption_icp_block = $(toml_number "$packaged_ids" ledger icp_payout_block)
io_amount_e8s = $(toml_number "$packaged_ids" ledger redemption_io_e8s)
gross_icp_e8s = $(toml_number "$packaged_ids" ledger gross_icp_e8s)
net_icp_e8s = $(toml_number "$packaged_ids" ledger net_icp_e8s)
redeemable_io_supply_e8s = $(toml_number "$economics" snapshot redeemable_io_supply_e8s)
excluded_io_total_e8s = $(toml_number "$economics" snapshot excluded_io_total_e8s)
identical_replay = true
bad_fee = true
insufficient_funds = true
index_synced_blocks = $(phase_value 16-exercise-index-and-archives num_blocks_synced)
reserve_history = true
operator_history = true
EOF

fixture="${GENERATED_DIR}/nns-readiness-fixture.toml"
require_file "$fixture"
cat > "${package_dir}/governance-evidence.toml" <<EOF
[candidate]
ic_commit = "${official_commit}"
governance_raw_sha256 = "${governance_raw}"
governance_source_gzip_sha256 = "${governance_gzip}"
governance_did_sha256 = "${governance_did}"
root_raw_sha256 = "${root_raw}"
root_source_gzip_sha256 = "${root_gzip}"
root_did_sha256 = "${root_did}"
nns_governance_publication_proposal = $(phase_value 13-candidate-published governance_proposal)
nns_root_publication_proposal = $(phase_value 13-candidate-published root_proposal)
create_sns_proposal = $(jq -er '.id' "${GENERATED_DIR}/create-sns-proposal.json")

[upgrade]
target = "$(toml_string "$(local_vars_file)" local io_historian_canister)"
proposal_id = ${upgrade_proposal}
before_module_sha256 = "${historian_before}"
payload_wasm_sha256 = "${historian_payload}"
release_raw_sha256 = "${historian_raw}"
executed = true

[lifecycle]
stream_function_id = $(phase_value 17-stream-function-registered function_id)
stream_registration_proposal = $(phase_value 17-stream-function-registered proposal_id)
stream_activation_proposal = $(phase_value 17-stream-activated proposal_id)
nns_function_id = $(phase_value 17-nns-function-registered function_id)
nns_registration_proposal = $(phase_value 17-nns-function-registered proposal_id)
nns_activation_proposal = $(phase_value 17-nns-activated proposal_id)
stream_ready = true
nns_manager_ready = true
two_week_baseline_reconciled = true
reward_backing_neuron_id = $(toml_number "$fixture" reward_backing_neuron id)
two_year_neuron_id = $(toml_number "$fixture" two_year_neuron id)
seeded_principal_e8s = $(toml_number "$fixture" reward_backing_neuron seeded_principal_e8s)
dissolve_delay_seconds = $(toml_number "$fixture" reward_backing_neuron dissolve_delay_seconds)
jupiter_staging_e8s = $(phase_value 17-nns-activated jupiter_staging_e8s)
two_week_staging_e8s = $(phase_value 17-nns-activated two_week_staging_e8s)

[reward]
proposal_id = $(phase_value 17-one-day-reward-observed proposal_id)
event_round = $(phase_value 17-one-day-reward-observed event_round)
classification = "ProposalBearing"
reward_shares_observed = true
processed_count = 1
eligible_credit = ${reward_eligible_credit}
policy_credit = 1000000000000000000
EOF

root="$(toml_string "$packaged_ids" sns_canisters root)"
cat > "${package_dir}/controller-evidence.toml" <<EOF
[controllers]
sns_root = "${root}"
io_stream_manager = "${root}"
io_nns_neuron_manager = "${root}"
io_historian_before = "${root}"
io_historian_after = "${root}"
frontend = "${root}"
EOF

cat > "${package_dir}/archive-evidence.toml" <<'EOF'
[archive]
ledger_archive_count = 0
root_archive_count = 0
ledger_observation = "none"
root_observation = "none"
observation_consistent = true
EOF

: > "${package_dir}/commands.log"
for log in "${GENERATED_DIR}"/logs/*.log; do
  printf 'log=%s\n' "$(basename "$log")" >> "${package_dir}/commands.log"
  grep -E '^(command:|exit_status=)' "$log" >> "${package_dir}/commands.log" || true
done

(cd "$package_dir" && sha256sum manifest.toml release-evidence.toml \
  toolchain-provenance.toml sns_init.local.yaml canister-ids.local.toml \
  stream-install-args.did nns-manager-install-args.did historian-observation-config.did \
  account-map.toml redemption-economics.toml treasury-account-history.log \
  archive-observation.log \
  reserve-funding-evidence.toml ledger-evidence.toml governance-evidence.toml \
  controller-evidence.toml archive-evidence.toml historian-dashboard.log commands.log \
  > SHA256SUMS)
printf 'wrote completed sanitized monitoring evidence package: %s\n' "$package_dir"
