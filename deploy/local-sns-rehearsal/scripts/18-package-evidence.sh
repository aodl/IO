#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# Packages one new immutable, layered anchored-dynamic evidence directory only
# after every official and controlled phase has an exact checkpoint.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

required_phases=(
  10-bootstrap-official-network
  11-build-local-io-canisters
  12-deploy-local-dapps
  12-provision-local-nns-readiness
  13-propose-and-finalize-sns
  14-discover-sns-canisters
  15-redemption-complete
  16-exercise-index-and-archives
  17-exercise-governance-and-controllers
  17-manager-upgrade-restart
  17-one-day-reward-observed
  18-exercise-account-semantic-protocol
)
for phase in "${required_phases[@]}"; do
  if ! phase_is_done "$phase"; then
    record_blocker "evidence packaging requires exact successful checkpoint ${phase}"
    exit 2
  fi
done

source_commit="$(phase_detail_value 11-build-local-io-canisters source_commit)"
artifact_commit="$(phase_detail_value 11-build-local-io-canisters artifact_commit)"
manifest_sha256="$(phase_detail_value 11-build-local-io-canisters manifest_sha256)"
official_commit="$(phase_detail_value 10-bootstrap-official-network official_ic_commit)"
checkout_clean="$(phase_detail_value 10-bootstrap-official-network clean)"
sns_sha="$(phase_detail_value 10-bootstrap-official-network sns_cli_sha256)"
sns_testing_sha="$(phase_detail_value 10-bootstrap-official-network sns_testing_sha256)"
sns_testing_init_sha="$(phase_detail_value 10-bootstrap-official-network sns_testing_init_sha256)"
if [ "$official_commit" != "$PINNED_IC_COMMIT" ] || [ "$checkout_clean" != true ]; then
  record_blocker "official SNS evidence must come from the exact clean pinned checkout"
  exit 2
fi
if [ "$(sha256sum "${REPO_ROOT}/release-artifacts/manifest.json" | awk '{print $1}')" != "$manifest_sha256" ]; then
  record_blocker "release manifest changed after the artifact identity checkpoint"
  exit 2
fi

evidence_date="${IO_LOCAL_SNS_EVIDENCE_DATE:-$(date -u +%F)}"
if ! printf '%s' "$evidence_date" | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2}$'; then
  record_blocker "IO_LOCAL_SNS_EVIDENCE_DATE must be YYYY-MM-DD"
  exit 2
fi
package_name="${evidence_date}-${source_commit:0:7}-anchored-dynamic"
package_root="${REHEARSAL_DIR}/evidence"
package_path="${package_root}/${package_name}"
if [ -e "$package_path" ]; then
  record_blocker "refusing to overwrite immutable evidence package ${package_path}"
  exit 2
fi

stage="$(mktemp -d "${GENERATED_DIR}/anchored-dynamic-package.XXXXXX")"
selector_path="${package_root}/current-canonical.toml"
selector_backup="$(mktemp "${GENERATED_DIR}/current-canonical.XXXXXX.toml")"
cp "$selector_path" "$selector_backup"
cleanup_failed_package() {
  local status=$?
  if [ "$status" -ne 0 ]; then
    cp "$selector_backup" "$selector_path"
    rm -rf "$stage" "$package_path"
    printf 'preceding selector restored and candidate removed\n' >&2
  fi
  rm -f "$selector_backup"
  exit "$status"
}
trap cleanup_failed_package EXIT

cp "$(sns_init_file)" "$stage/sns_init.local.yaml"
cp "$(local_vars_file)" "$stage/local-vars.sanitized.toml"
cp "$(runtime_file)" "$stage/runtime.sanitized.toml"
cp "$(stream_install_args_file)" "$stage/stream-install-args.did"
cp "$(nns_install_args_file)" "$stage/nns-manager-install-args.did"
cp "${GENERATED_DIR}/scenario-results.toml" "$stage/scenario-results.toml"
cp "${REPO_ROOT}/release-artifacts/manifest.json" "$stage/release-manifest.json"
cp "${GENERATED_DIR}/logs/18-exercise-account-semantic-protocol.log" "$stage/pocketic-validation.log"
cp "${GENERATED_DIR}/logs/17-observe-one-day-reward.log" "$stage/historian-dashboard.log"
render_local_account_map "$stage/account-map.toml" "$(sns_canister_id governance)"

{
  printf '[mode]\nnetwork = "local"\nsource = "official-local-sns-rehearsal"\nio_protocol_live = true\n\n'
  printf '[provenance]\nio_release_source_commit = "%s"\nio_artifact_recording_commit = "%s"\n\n' "$source_commit" "$artifact_commit"
  printf '[sns_canisters]\n'
  for role in root governance ledger index swap; do
    printf '%s = "%s"\n' "$role" "$(sns_canister_id "$role")"
  done
  printf '\n[io_dapp_canisters]\n'
  printf 'io_stream_manager = "%s"\n' "$(toml_string "$(local_vars_file)" local io_stream_manager_canister)"
  printf 'io_nns_neuron_manager = "%s"\n' "$(toml_string "$(local_vars_file)" local io_nns_neuron_manager_canister)"
  printf 'io_historian = "%s"\n' "$(toml_string "$(local_vars_file)" local io_historian_canister)"
  printf 'frontend = "%s"\n' "$(toml_string "$(local_vars_file)" local frontend_canister)"
} > "$stage/canister-ids.local.toml"

{
  for phase in "${required_phases[@]}"; do
    printf '===== %s =====\n' "$phase"
    cat "$(phase_done_file "$phase")"
    phase_log="$(phase_log_file "$phase")"
    if [ -f "$phase_log" ]; then
      cat "$phase_log"
    fi
  done
} | sed "s#${HOME}#<LOCAL_HOME>#g" > "$stage/official-sns.log"

cat > "$stage/manifest.toml" <<EOF
[provenance]
evidence_schema = "anchored-dynamic-v1"
official_ic_repository = "dfinity/ic"
official_ic_source_commit = "${official_commit}"
sns_testing_source_path = "rs/sns/testing"
io_release_source_commit = "${source_commit}"
io_artifact_recording_commit = "${artifact_commit}"
release_manifest_sha256 = "${manifest_sha256}"
complete = true
monitoring = true
canonical_redemption_economics = true
account_semantic_economics = true
network = "local"
EOF

cat > "$stage/toolchain-provenance.toml" <<EOF
[official_sns]
source_commit = "${official_commit}"
clean_checkout = true

[release]
source_commit = "${source_commit}"
artifact_recording_commit = "${artifact_commit}"
EOF

cat > "$stage/source-built-tools.toml" <<EOF
[source]
repository = "dfinity/ic"
commit = "${official_commit}"
clean = true

[tools]
sns_sha256 = "${sns_sha}"
sns_testing_sha256 = "${sns_testing_sha}"
sns_testing_init_sha256 = "${sns_testing_init_sha}"
EOF

cat > "$stage/evidence-layers.toml" <<EOF
[official_sns]
source = "source-built-official-sns"
complete = true
proves = "SNS launch, wiring, proposals, controllers, ledger, index, root, swap and reward observation"

[exact_nns]
source = "proposal-143660-pocketic"
complete = true
proves = "14-day threshold, following, maturity, minimum stake, split, dissolve and disburse mechanics"

[orchestration]
source = "controlled-current-io-pocketic"
complete = true
proves = "account semantics, paired issuance, no-issuance yield, carry-forward, liveness and recovery"
EOF

cat > "$stage/nns-boundary.toml" <<EOF
[governance]
proposal = 143660
source_commit = "c748b8e76b90ceef329c055e6f7b38a00aae8745"
compressed_wasm_sha256 = "e4e9e99730dbee3a6fb9a95b40b10b512ad4831c9d2f6efb51d3f0a5d243b503"
raw_wasm_sha256 = "573af1cde5bf55a5e4dbf2d47f8dd340f7a73a107eebbc645fe1202b97f61e85"
did_sha256 = "6e9a397f4bf0adc913980ef6c176e765534617d0ce59d52e7bcc66add2b0cd71"
checked_through_proposal = 143685
checked_on = "2026-08-26"
exact_candidate_passed = true

[ledger]
pin_scope = "independent"
EOF

cat > "$stage/phase-inventory.toml" <<EOF
[phases]
official_bootstrap = true
release_identity = true
sns_finalized = true
canisters_discovered = true
ledger_redemption = true
index_archive = true
governance_controllers = true
manager_upgrade_restart = true
daily_reward = true
account_semantic_protocol = true
EOF

{
  printf '[release]\nsource_commit = "%s"\nartifact_recording_commit = "%s"\nmanifest_sha256 = "%s"\n\n' "$source_commit" "$artifact_commit" "$manifest_sha256"
  while IFS=$'\t' read -r canister raw gzip; do
    section="$canister"
    [ "$canister" != frontend ] || section=io_frontend
    printf '[%s]\nraw_wasm_sha256 = "%s"\ngzip_wasm_sha256 = "%s"\n\n' "$section" "$raw" "$gzip"
  done < <(jq -r '.artifacts[] | [.canister, .raw_wasm_sha256, .gz_wasm_sha256] | @tsv' "${REPO_ROOT}/release-artifacts/manifest.json")
} > "$stage/release-evidence.toml"

cat > "$stage/README.md" <<EOF
# Anchored-dynamic local release evidence

This immutable package binds IO source ${source_commit} to artifact commit
${artifact_commit} and one fresh local SNS topology. The evidence is layered:
the source-built official SNS environment proves launch and wiring; the exact
proposal-143660 PocketIC suite proves the active NNS Governance boundary; and
the controlled current-IO fixture proves anchored-dynamic orchestration.

Fungible ICP provenance is not tracked after custody. The fixed TwoWeek and
TwoYear staging Accounts determine treatment. Ambiguous irreversible outgoing
effects remain exact-effect proved to prevent duplicate transfers or commands.
EOF

for forbidden in 'identity.pem' '.pem' 'seed phrase' 'private key' '--network ic' '-n ic'; do
  if rg -n -i --fixed-strings -- "$forbidden" "$stage" >/dev/null; then
    record_blocker "sanitized evidence package contains forbidden material marker: ${forbidden}"
    exit 2
  fi
done

(cd "$stage" && find . -maxdepth 1 -type f ! -name SHA256SUMS -printf '%P\n' | sort | xargs sha256sum > SHA256SUMS && sha256sum -c SHA256SUMS)
mkdir -p "$package_root"
mv "$stage" "$package_path"

(cd "$REPO_ROOT" && cargo run -p xtask -- validate_local_sns_evidence_package "deploy/local-sns-rehearsal/evidence/${package_name}")

selector_temporary="$(mktemp "${GENERATED_DIR}/current-canonical.candidate.XXXXXX.toml")"
cat > "$selector_temporary" <<EOF
[schema]
version = 2

[current]
package = "${package_name}"
io_release_source_commit = "${source_commit}"
io_artifact_recording_commit = "${artifact_commit}"
release_manifest_sha256 = "${manifest_sha256}"
package_manifest_sha256 = "$(sha256sum "$package_path/manifest.toml" | awk '{print $1}')"
package_sha256s_sha256 = "$(sha256sum "$package_path/SHA256SUMS" | awk '{print $1}')"
EOF
mv "$selector_temporary" "$selector_path"
(cd "$REPO_ROOT" && cargo run -p xtask -- validate_local_sns_committed_evidence)

mark_phase_done 18-package-evidence \
  "package=${package_name}; source_commit=${source_commit}; artifact_commit=${artifact_commit}; package_manifest_sha256=$(sha256sum "$package_path/manifest.toml" | awk '{print $1}'); package_sha256s_sha256=$(sha256sum "$package_path/SHA256SUMS" | awk '{print $1}'); complete=true"
trap - EXIT
rm -f "$selector_backup"
printf '%s\n' "$package_path"
