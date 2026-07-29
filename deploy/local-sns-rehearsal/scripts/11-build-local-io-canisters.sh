#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# local-only IO debug Wasm build phase for official SNS rehearsal.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

cd "${REPO_ROOT}"
log_file="$(phase_log_file 11-build-local-io-canisters)"
: > "$log_file"
run_logged "$log_file" cargo run -p xtask -- build_debug_canisters
