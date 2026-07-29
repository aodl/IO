#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# guarded placeholder for local-only governance/controller proof.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 17-exercise-governance-and-controllers)"
: > "$log_file"
record_blocker "governance/controller proof requires finalized local SNS root/governance and a harmless local upgrade probe; no proposal was submitted"
exit 2
