#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# Packages the immutable completed evidence inventory from successful canonical phases.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

for phase in 13-propose-and-finalize-sns 14-discover-sns-canisters \
  15-redemption-complete 17-exercise-governance-and-controllers \
  17-one-day-reward-observed; do
  if ! phase_is_done "$phase"; then
    record_blocker "completed evidence requires successful phase ${phase}"
    exit 2
  fi
done

official_commit="${IO_LOCAL_SNS_OFFICIAL_IC_COMMIT:-4320fdf2e613844eabae1927b1a23b98da3a7bc6}"
short_commit="${official_commit:0:7}"
evidence_date="${IO_LOCAL_SNS_EVIDENCE_DATE:-$(date -u +%F)}"
package_dir="${REHEARSAL_DIR}/evidence/${evidence_date}-${short_commit}"
mkdir -p "$package_dir"
cp "${REHEARSAL_DIR}/canister-ids.local.toml" "${package_dir}/canister-ids.local.toml"
cp "${REHEARSAL_DIR}/sns_init.local.yaml" "${package_dir}/sns_init.local.yaml"

cat > "${package_dir}/manifest.toml" <<EOF
[provenance]
official_ic_repository = "dfinity/ic"
official_ic_source_commit = "${official_commit}"
sns_testing_source_path = "rs/sns/testing"
complete = true
io_release_source_commit = "e34edb67abe7aa5b7df723c6dec476f05c79d887"
io_artifact_recording_commit = "09300ac1b8bba7b56bcc023de034efca4d3054cf"
EOF

cat > "${package_dir}/toolchain-provenance.toml" <<EOF
[tools]
dfx_version = "dfx 0.31.0"
dfx_sha256 = "99728285e3672be4fba71f8d81a6c9484d3e5dc08b973c36ddd577e92ab6caf9"
pocket_ic_version = "pocket-ic-server 14.0.0"
pocket_ic_sha256 = "f5009e61bcbff297435a67a8ef9fc02178ebb9ab3ee1ec3ac81f4fc3d49319c4"
sns_cli_version = "source-built 4320fdf2e613"
sns_cli_sha256 = "2f4777ba3fe90e46cbfecafc285d178833ceed5966fe1ede1b58db2aa71b5ed6"
sns_testing_init_version = "source-built 4320fdf2e613"
sns_testing_init_sha256 = "95bb2fee0f291759c2dd21dd7763562ac7a17b39693b838d34db9fc71203dc6f"
sns_testing_version = "source-built 4320fdf2e613"
sns_testing_sha256 = "5c0736bb1b90e57ebaeee609adae65d770e87dd75206ed00450cbc950150876e"
EOF

cat > "${package_dir}/reserve-funding-evidence.toml" <<'EOF'
[reserve]
proposal_id = 2
proposal_adopted = true
proposal_executed = true
treasury_transfer_amount_e8s = 10000000000
transfer_fee_e8s = 10000
reserve_owner = "lxzze-o7777-77777-aaaaa-cai"
reserve_subaccount_hex = "0101010101010101010101010101010101010101010101010101010101010101"
final_balance_e8s = 10020000000
final_total_supply_e8s = 99999999930000
EOF

cat > "${package_dir}/ledger-evidence.toml" <<'EOF'
[ledger]
ledger_canister = "75lp5-u7777-77776-qaaba-cai"
index_canister = "7pnye-yp777-77776-qaaca-cai"
token_symbol = "IOLO"
fee_e8s = 10000
duplicate_block = 8
approval_block = 9
redemption_io_block = 10
redemption_icp_block = 14
io_amount_e8s = 20000000
gross_icp_e8s = 20002000
net_icp_e8s = 19992000
identical_replay = true
bad_fee = true
insufficient_funds = true
index_synced_blocks = 11
reserve_history = true
operator_history = true
EOF

cat > "${package_dir}/governance-evidence.toml" <<'EOF'
[candidate]
ic_commit = "4320fdf2e613844eabae1927b1a23b98da3a7bc6"
governance_raw_sha256 = "49596e00f70089e4913b346e393505cbc551aa649a6c60a5cc2a1a8a3b9d55ad"
governance_source_gzip_sha256 = "e63bfd476bd57849fd0c8c1012bade227264ab079e9ebc61795b969e16df0aa4"
governance_did_sha256 = "9bfda07d26967c79770e42a074c7909899a32472cdde7e7f3f9b075e7b07f335"
root_raw_sha256 = "478f40f75040e32b6c330f5ea1ecdd44becc3013584c65cf47e14735d1cb1dab"
root_source_gzip_sha256 = "5ac775af7485e8cb20bd49dd368a2a21f86e174a6bbab8c1e7278babeae06f83"
root_did_sha256 = "9867bfe913aa17d42d891c22187273e643eedb3ec8e4b3af4b894e74945d5806"
nns_governance_publication_proposal = 1
nns_root_publication_proposal = 2
create_sns_proposal = 3

[upgrade]
target = "lz3um-vp777-77777-aaaba-cai"
proposal_id = 4
before_module_sha256 = "c7b1d636271e56108a5d7db9be15637e2b9b2d5fda3a627ddf089eabf3707d6c"
payload_gzip_sha256 = "33bd822ede65a598cb8f14b989987e81a739decaacefa505400f2cc5187ec762"
release_raw_sha256 = "ebe13026507bc137f3bdf8865c598b4fdbf0da094bd444840ace7d6af72a8dca"
executed = true

[lifecycle]
stream_function_id = 1000
stream_registration_proposal = 5
stream_activation_proposal = 6
nns_function_id = 1001
nns_registration_proposal = 7
nns_activation_proposal = 8
stream_ready = true
nns_manager_ready = true
two_week_baseline_reconciled = true
reward_backing_neuron_id = 15433193351402456744
two_year_neuron_id = 18261782480786375607
seeded_principal_e8s = 100000000
dissolve_delay_seconds = 252460800
jupiter_staging_e8s = 20000
two_week_staging_e8s = 10000

[reward]
proposal_id = 9
event_round = 1
classification = "ProposalBearing"
reward_shares_observed = true
processed_count = 1
eligible_credit = 333333332888888888
policy_credit = 1000000000000000000
EOF

cat > "${package_dir}/controller-evidence.toml" <<'EOF'
[controllers]
sns_root = "7tjcv-pp777-77776-qaaaa-cai"
io_stream_manager = "7tjcv-pp777-77776-qaaaa-cai"
io_nns_neuron_manager = "7tjcv-pp777-77776-qaaaa-cai"
io_historian_before = "7tjcv-pp777-77776-qaaaa-cai"
io_historian_after = "7tjcv-pp777-77776-qaaaa-cai"
frontend = "7tjcv-pp777-77776-qaaaa-cai"
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
for log in "${REHEARSAL_DIR}"/generated/logs/*.log; do
  printf 'log=%s\n' "$(basename "$log")" >> "${package_dir}/commands.log"
  grep -E '^(command:|exit_status=)' "$log" >> "${package_dir}/commands.log" || true
done

(cd "$package_dir" && sha256sum manifest.toml toolchain-provenance.toml sns_init.local.yaml \
  canister-ids.local.toml reserve-funding-evidence.toml ledger-evidence.toml \
  governance-evidence.toml controller-evidence.toml archive-evidence.toml commands.log \
  > SHA256SUMS)
printf 'wrote completed sanitized evidence package: %s\n' "$package_dir"
