#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# guarded placeholder for local-only dapp canister creation/install.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 12-deploy-local-dapps)"
: > "$log_file"
record_blocker "local dapp deployment phase requires a running maintained official loopback SNS/PocketIC environment; no deploy was executed"
exit 2
