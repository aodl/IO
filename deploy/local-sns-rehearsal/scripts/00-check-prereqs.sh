#!/usr/bin/env bash
set -euo pipefail

# optional local-only official SNS rehearsal prerequisite check
# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-local-sns.sh
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

cd "${REPO_ROOT}"
cargo run -p xtask -- validate_local_sns_rehearsal

if command -v dfx >/dev/null 2>&1; then
  dfx --version
  if dfx sns --help >/dev/null 2>&1; then
    printf 'dfx sns is available for optional manual local-only rehearsal.\n'
  else
    printf 'dfx sns is not available; install the SNS extension before manual rehearsal.\n' >&2
    exit 2
  fi
else
  printf 'dfx is not available; optional official local SNS rehearsal cannot run yet.\n' >&2
  exit 2
fi

printf 'No dfx sns command was run. Follow deploy/local-sns-rehearsal/README.md for local-only next steps.\n'
