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

for required in bazel git; do
  require_command_available "$required"
done

if command -v dfx >/dev/null 2>&1; then
  dfx --version
else
  printf 'dfx is unavailable; maintained SNS tooling can still be built, but identity/local call phases requiring dfx will block later.\n' >&2
fi

printf 'No dfx SNS extension command was required or run. Follow deploy/local-sns-rehearsal/README.md for local-only next steps.\n'
