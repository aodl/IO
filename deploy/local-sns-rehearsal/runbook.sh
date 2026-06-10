#!/usr/bin/env bash
set -euo pipefail

# local-only operator entrypoint; this does not call mainnet
# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib-local-sns.sh
source "${SCRIPT_DIR}/scripts/lib-local-sns.sh"

subcommand="${1:-print-next-steps}"
shift || true
require_local_script_guard "$subcommand" "$@"

case "$subcommand" in
  check)
    "${SCRIPT_DIR}/scripts/00-check-prereqs.sh" "$@"
    ;;
  render-sns-init)
    "${SCRIPT_DIR}/scripts/01-render-sns-init.sh" "$@"
    ;;
  record-ids)
    "${SCRIPT_DIR}/scripts/02-record-canister-ids.sh" "$@"
    ;;
  capture-evidence)
    "${SCRIPT_DIR}/scripts/03-capture-ledger-evidence.sh" "$@"
    ;;
  render-wiring)
    "${SCRIPT_DIR}/scripts/04-render-local-wiring.sh" "$@"
    ;;
  validate)
    "${SCRIPT_DIR}/scripts/05-validate-evidence.sh" "$@"
    ;;
  print-next-steps)
    cat <<'EOF'
Local-only official SNS rehearsal flow:
1. Run runbook.sh check.
2. Copy local-vars.example.toml to local-vars.toml and fill only local principals.
3. Run runbook.sh render-sns-init.
4. Deploy IO dapp canisters locally and run official local SNS tooling manually.
5. Run runbook.sh record-ids, then fill canister-ids.local.toml with local SNS and IO dapp IDs.
6. Run runbook.sh capture-evidence to print local ledger/index/governance/root calls.
7. Paste observed evidence into canister-ids.local.toml.
8. Run runbook.sh validate and cargo run -p xtask -- validate_local_sns_ledger.

No mainnet commands are part of this runbook.
EOF
    ;;
  *)
    printf 'unknown subcommand: %s\n' "$subcommand" >&2
    printf 'known: check, render-sns-init, record-ids, capture-evidence, render-wiring, validate, print-next-steps\n' >&2
    exit 2
    ;;
esac
