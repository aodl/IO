#!/usr/bin/env bash
set -euo pipefail

# optional local-only guided evidence capture; can print local dfx calls and optionally record manual observations
# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-local-sns.sh
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

evidence="${1:-${REHEARSAL_DIR}/canister-ids.local.toml}"
require_file "$evidence"

sns_ledger="$(toml_string "$evidence" sns_canisters ledger)"
sns_index="$(toml_string "$evidence" sns_canisters index)"
sns_governance="$(toml_string "$evidence" sns_canisters governance)"
sns_root="$(toml_string "$evidence" sns_canisters root)"
reserve_owner="$(toml_string "$evidence" ledger_evidence protocol_reserve_account_owner)"
reserve_subaccount="$(toml_string "$evidence" ledger_evidence protocol_reserve_subaccount_hex)"

for value in "$sns_ledger" "$sns_index" "$sns_governance" "$sns_root" "$reserve_owner"; do
  case "$value" in
    ""|TODO*) printf 'evidence file still has missing/TODO canister or reserve owner fields\n' >&2; exit 2 ;;
    "${PROTECTED_CANISTER}"|"${PROTECTED_NEURON}") printf 'protected target in evidence file\n' >&2; exit 2 ;;
  esac
done

printf '# Local-only SNS ledger evidence commands\n'
printf 'SNS_LEDGER=%q\nSNS_INDEX=%q\nSNS_GOVERNANCE=%q\nSNS_ROOT=%q\n' "$sns_ledger" "$sns_index" "$sns_governance" "$sns_root"
printf 'RESERVE_OWNER=%q\nRESERVE_SUBACCOUNT=%q\n' "$reserve_owner" "$reserve_subaccount"
printf '\n# Read-only ledger observations\n'
printf 'dfx canister call --network local "$SNS_LEDGER" icrc1_symbol "()"\n'
printf 'dfx canister call --network local "$SNS_LEDGER" icrc1_fee "()"\n'
printf 'dfx canister call --network local "$SNS_LEDGER" icrc1_total_supply "()"\n'
printf 'dfx canister call --network local "$SNS_LEDGER" icrc1_balance_of "(record { owner = principal \\"$RESERVE_OWNER\\"; subaccount = null })"\n'
printf '\n# Mutating local-only transfer/error observations are documented in commands.local.example.md.\n'
printf '# Paste observed values into %s and run scripts/05-validate-evidence.sh.\n' "$evidence"
