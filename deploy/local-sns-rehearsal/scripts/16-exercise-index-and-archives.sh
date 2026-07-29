#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# guarded placeholder for local-only index/archive exercise.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 16-exercise-index-and-archives)"
: > "$log_file"
record_blocker "index/archive exercise requires completed local SNS ledger/index/archive canisters; no archive stress run was executed"
exit 2
