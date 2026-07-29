#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# guarded placeholder for local-only SNS canister discovery.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 14-discover-sns-canisters)"
: > "$log_file"
record_blocker "SNS canister discovery requires a finalized local SNS; no discovery call was executed"
exit 2
