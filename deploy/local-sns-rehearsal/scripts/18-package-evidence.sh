#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# Packaging is deliberately disabled until every corrected pooled-claim-backing
# lifecycle phase has completed in one fresh maintained SNS rehearsal.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

record_blocker \
  "corrected pooled claim-backing canonical evidence is missing; complete and review the fresh Jupiter, lazy-parent, following, joint-maturity, cohort, liquidity, refresh-lag, failure, and upgrade rehearsal before enabling packaging"
exit 2
