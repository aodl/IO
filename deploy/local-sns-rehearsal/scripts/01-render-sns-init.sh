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
output="${REHEARSAL_DIR}/sns_init.local.yaml"

require_file "$vars_file"
require_file "$template"
require_command_available sha256sum
mkdir -p "$(dirname "$output")"

required_keys=(
  fallback_controller_principal
  io_stream_manager_canister
  io_nns_neuron_manager_canister
  io_historian_canister
  frontend_canister
  developer_neuron_principal
  logo_path
  token_logo_path
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
    logo_path|token_logo_path)
      case "$value" in
        /*|*..*|*://*|*\\*|"") printf 'local image path %s must be relative and local\n' "$key" >&2; exit 2 ;;
      esac
      ;;
    *)
      if ! printf '%s' "$value" | grep -Eq '^[a-z0-9-]+$'; then
        printf 'local variable %s does not look like principal text\n' "$key" >&2
        exit 2
      fi
      ;;
  esac
  rendered="${rendered//\{\{${key}\}\}/$value}"
done

for logo_key in logo token_logo; do
  path_key="${logo_key}_path"
  hash_key="${logo_key}_sha256"
  relative_path="$(toml_string "$vars_file" local "$path_key")"
  expected_hash="$(toml_string "$vars_file" local "$hash_key")"
  if ! printf '%s' "$expected_hash" | grep -Eq '^[0-9a-f]{64}$'; then
    printf '[local].%s must be an exact lowercase SHA-256\n' "$hash_key" >&2
    exit 2
  fi
  logo_file="${REHEARSAL_DIR}/${relative_path}"
  if [ -L "$logo_file" ] || [ ! -f "$logo_file" ]; then
    printf 'local logo must exist as a regular non-symlink file before validation: %s\n' "$relative_path" >&2
    exit 2
  fi
  actual_hash="$(sha256sum "$logo_file" | awk '{print $1}')"
  if [ "$actual_hash" != "$expected_hash" ]; then
    printf 'local logo SHA-256 mismatch for %s\n' "$relative_path" >&2
    exit 2
  fi
done

if printf '%s' "$rendered" | grep -Eq 'TODO_LOCAL|\{\{|--network ic|-n ic'; then
  printf 'rendered sns_init still contains placeholders or forbidden network text\n' >&2
  exit 2
fi

printf '%s\n' "$rendered" > "$output"
printf 'wrote local-only rendered SNS init: %s\n' "$output"
