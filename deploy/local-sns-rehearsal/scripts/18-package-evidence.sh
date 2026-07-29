#!/usr/bin/env bash
set -euo pipefail

# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.
# packages sanitized local-only evidence or a blocker report; no network calls.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

official_commit="${IO_LOCAL_SNS_OFFICIAL_IC_COMMIT:-2d7f90fb23672cc3b81c216a33d04c75672dd308}"
short_commit="${official_commit:0:7}"
package_dir="${REHEARSAL_DIR}/evidence/2026-07-29-${short_commit}"
mkdir -p "$package_dir"

blocker="$(cat "${REHEARSAL_DIR}/generated/blockers/latest-blocker.txt" 2>/dev/null || printf 'official local SNS rehearsal not completed: prerequisite phase did not complete')"
cat > "${package_dir}/manifest.toml" <<EOF
[provenance]
official_ic_repository = "dfinity/ic"
official_ic_source_commit = "${official_commit}"
sns_testing_source_path = "rs/sns/testing"
complete = false
blocker_report = "blocker-report.md"
EOF

cat > "${package_dir}/blocker-report.md" <<EOF
# Official Local SNS Rehearsal Blocker

The official local SNS rehearsal not completed in this environment.

Exact blocker:

${blocker}

Required maintained source path: dfinity/ic rs/sns/testing at ${official_commit}.

No mainnet call, deployment, install, upgrade, funding, controller mutation, or production canister operation was executed.
EOF

(cd "$package_dir" && sha256sum manifest.toml blocker-report.md > SHA256SUMS)
printf 'wrote sanitized blocker evidence package: %s\n' "$package_dir"
