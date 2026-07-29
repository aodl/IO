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
  bootstrap-official-network)
    "${SCRIPT_DIR}/scripts/10-bootstrap-official-network.sh" "$@"
    ;;
  build-local-io-canisters)
    "${SCRIPT_DIR}/scripts/11-build-local-io-canisters.sh" "$@"
    ;;
  deploy-local-dapps)
    "${SCRIPT_DIR}/scripts/12-deploy-local-dapps.sh" "$@"
    ;;
  propose-and-finalize-sns)
    "${SCRIPT_DIR}/scripts/13-propose-and-finalize-sns.sh" "$@"
    ;;
  discover-sns-canisters)
    "${SCRIPT_DIR}/scripts/14-discover-sns-canisters.sh" "$@"
    ;;
  exercise-ledger)
    "${SCRIPT_DIR}/scripts/15-exercise-ledger.sh" "$@"
    ;;
  exercise-index-and-archives)
    "${SCRIPT_DIR}/scripts/16-exercise-index-and-archives.sh" "$@"
    ;;
  exercise-governance-and-controllers)
    "${SCRIPT_DIR}/scripts/17-exercise-governance-and-controllers.sh" "$@"
    ;;
  package-evidence)
    "${SCRIPT_DIR}/scripts/18-package-evidence.sh" "$@"
    ;;
  cleanup-official-network)
    "${SCRIPT_DIR}/scripts/19-cleanup-official-network.sh" "$@"
    ;;
  print-next-steps)
    cat <<'EOF'
Local-only official SNS rehearsal flow:
1. Run runbook.sh check.
2. Copy local-vars.example.toml to local-vars.toml and fill only local principals.
3. Run runbook.sh render-sns-init.
4. Run guarded phases 10-19 only against a loopback maintained official SNS testing environment.
5. Run runbook.sh record-ids, then fill canister-ids.local.toml with local SNS and IO dapp IDs.
6. Run runbook.sh capture-evidence to print local ledger/index/governance/root calls.
7. Paste observed evidence into canister-ids.local.toml.
8. Run runbook.sh validate and cargo run -p xtask -- validate_local_sns_ledger.
9. Run runbook.sh package-evidence to create a sanitized committed evidence package or blocker report.

No mainnet commands are part of this runbook.
EOF
    ;;
  *)
    printf 'unknown subcommand: %s\n' "$subcommand" >&2
    printf 'known: check, render-sns-init, record-ids, capture-evidence, render-wiring, validate, bootstrap-official-network, build-local-io-canisters, deploy-local-dapps, propose-and-finalize-sns, discover-sns-canisters, exercise-ledger, exercise-index-and-archives, exercise-governance-and-controllers, package-evidence, cleanup-official-network, print-next-steps\n' >&2
    exit 2
    ;;
esac
