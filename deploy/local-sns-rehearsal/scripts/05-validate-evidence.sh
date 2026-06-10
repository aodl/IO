#!/usr/bin/env bash
set -euo pipefail

# optional local-only evidence validator; this does not call canisters
# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-local-sns.sh
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

evidence="${1:-${REHEARSAL_DIR}/canister-ids.local.toml}"
require_file "$evidence"

if [ -n "${IO_LOCAL_SNS_REHEARSAL_XTASK:-}" ]; then
  IO_LOCAL_SNS_EVIDENCE="$evidence" "${IO_LOCAL_SNS_REHEARSAL_XTASK}" local_sns_evidence_tests
else
  cd "${REPO_ROOT}"
  cargo run -p xtask -- validate_local_sns_ledger
fi
