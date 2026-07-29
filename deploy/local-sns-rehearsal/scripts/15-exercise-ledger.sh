#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# guarded placeholder for local-only fee-burn ledger exercise.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 15-exercise-ledger)"
: > "$log_file"
record_blocker "ledger exercise requires a completed local SNS ledger ID and funded local accounts; no ledger mutation was executed"
exit 2
