#!/usr/bin/env bash
set -euo pipefail

# optional local-only evidence initializer; this does not call canisters
# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-local-sns.sh
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

evidence="${1:-${REHEARSAL_DIR}/canister-ids.local.toml}"
example="${REHEARSAL_DIR}/canister-ids.local.example.toml"
require_file "$example"

if [ -f "$evidence" ]; then
  printf 'refusing to overwrite existing evidence file: %s\n' "$evidence" >&2
  exit 2
fi

cp "$example" "$evidence"
printf 'created local-only evidence draft: %s\n' "$evidence"
printf 'Fill local SNS root/governance/ledger/index/swap and local IO dapp IDs from the official local SNS run.\n'
