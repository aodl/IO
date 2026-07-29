#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# guarded cleanup helper; does not kill broad user processes.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 19-cleanup-official-network)"
: > "$log_file"
printf 'No automatic cleanup performed. Stop only loopback processes started by the reviewed local SNS rehearsal run.\n' | tee -a "$log_file"
