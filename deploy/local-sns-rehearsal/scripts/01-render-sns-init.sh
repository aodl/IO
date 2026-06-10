#!/usr/bin/env bash
set -euo pipefail

# optional local-only renderer for the official local SNS init file
# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-local-sns.sh
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

vars_file="${1:-${REHEARSAL_DIR}/local-vars.toml}"
template="${REHEARSAL_DIR}/sns_init.local.template.yaml"
output="${REHEARSAL_DIR}/generated/sns_init.local.yaml"

require_file "$vars_file"
require_file "$template"
mkdir -p "$(dirname "$output")"

required_keys=(
  fallback_controller_principal
  io_stream_manager_canister
  io_nns_neuron_manager_canister
  io_historian_canister
  frontend_canister
  developer_neuron_principal
  protocol_reserve_principal
  archive_controller_principal
  logo_url
  token_logo_url
)

rendered="$(cat "$template")"
for key in "${required_keys[@]}"; do
  value="$(toml_string "$vars_file" local "$key")"
  if [ -z "$value" ]; then
    printf 'missing required local variable: [local].%s\n' "$key" >&2
    exit 2
  fi
  case "$value" in
    TODO*|*"{{"*|*"}"*) printf 'placeholder local variable: %s\n' "$key" >&2; exit 2 ;;
    "${PROTECTED_CANISTER}"|"${PROTECTED_NEURON}") printf 'protected value in %s\n' "$key" >&2; exit 2 ;;
    ryjl3-tyaaa-aaaaa-aaaba-cai|qhbym-qaaaa-aaaaa-aaafq-cai|rrkah-fqaaa-aaaaa-aaaaq-cai)
      printf 'mainnet/prior canister is not allowed in local variable %s\n' "$key" >&2
      exit 2
      ;;
  esac
  case "$key" in
    logo_url|token_logo_url) ;;
    *)
      if ! printf '%s' "$value" | grep -Eq '^[a-z0-9-]+$'; then
        printf 'local variable %s does not look like principal text\n' "$key" >&2
        exit 2
      fi
      ;;
  esac
  rendered="${rendered//\{\{${key}\}\}/$value}"
done

if printf '%s' "$rendered" | grep -Eq 'TODO_LOCAL|\{\{|--network ic|-n ic'; then
  printf 'rendered sns_init still contains placeholders or forbidden network text\n' >&2
  exit 2
fi

printf '%s\n' "$rendered" > "$output"
printf 'wrote local-only rendered SNS init: %s\n' "$output"
