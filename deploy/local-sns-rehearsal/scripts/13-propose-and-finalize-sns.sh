#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# guarded placeholder for local-only SNS proposal/finalization.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 13-propose-and-finalize-sns)"
: > "$log_file"
record_blocker "SNS proposal/finalization requires completed maintained official bootstrap and local dapp IDs; no proposal was submitted"
exit 2
