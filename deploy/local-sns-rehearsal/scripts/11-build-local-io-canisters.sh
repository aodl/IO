#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# exact release-artifact verification phase for official SNS rehearsal.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

cd "${REPO_ROOT}"
log_file="$(phase_log_file 11-build-local-io-canisters)"
: > "$log_file"
artifact_commit="$(git -C "$REPO_ROOT" rev-parse HEAD)"
if ! git -C "$REPO_ROOT" diff --quiet -- \
  || ! git -C "$REPO_ROOT" diff --cached --quiet --; then
  record_blocker "stateful rehearsal requires a clean tracked worktree at the artifact-recording commit"
  exit 2
fi
run_logged "$log_file" cargo run -p xtask -- verify_recorded_source
source_commit="$(jq -er '.git_commit' release-artifacts/manifest.json)"
git cat-file -e "${source_commit}^{commit}"
if ! git diff --quiet "$source_commit" HEAD -- . ':(exclude)release-artifacts'; then
  record_blocker "release artifacts do not describe the exact current source tree"
  exit 2
fi
for canister in io_stream_manager io_nns_neuron_manager io_historian frontend; do
  raw_path="$(manifest_artifact_value "$canister" raw_wasm_path)"
  gz_path="$(manifest_artifact_value "$canister" gz_wasm_path)"
  raw_hash="$(manifest_artifact_value "$canister" raw_wasm_sha256)"
  gz_hash="$(manifest_artifact_value "$canister" gz_wasm_sha256)"
  printf '%s raw=%s gzip=%s source=%s\n' "$canister" "$raw_hash" "$gz_hash" "$source_commit" >> "$log_file"
  [ "$(sha256sum "$raw_path" | awk '{print $1}')" = "$raw_hash" ]
  [ "$(sha256sum "$gz_path" | awk '{print $1}')" = "$gz_hash" ]
done
if ! git -C "$REPO_ROOT" diff --quiet -- \
  || ! git -C "$REPO_ROOT" diff --cached --quiet --; then
  record_blocker "exact-source verification changed the tracked artifact-recording tree"
  exit 2
fi
if ! git -C "$REPO_ROOT" show \
  "${artifact_commit}:release-artifacts/manifest.json" | cmp - release-artifacts/manifest.json; then
  record_blocker "artifact-recording commit does not contain the exact current release manifest"
  exit 2
fi
manifest_sha256="$(sha256sum release-artifacts/manifest.json | awk '{print $1}')"
mark_phase_done 11-build-local-io-canisters \
  "source_commit=${source_commit}; artifact_commit=${artifact_commit}; manifest_sha256=${manifest_sha256}; tracked_clean=true"
