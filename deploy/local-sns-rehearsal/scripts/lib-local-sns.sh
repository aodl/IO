#!/usr/bin/env bash
set -euo pipefail

REHEARSAL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(cd "${REHEARSAL_DIR}/../.." && pwd)"
PROTECTED_CANISTER="oae4c-3iaaa-aaaar-qb5qq-cai"
PROTECTED_NEURON="6345890886899317159"

require_local_ack() {
  case "${IO_LOCAL_SNS_REHEARSAL_ACK:-}" in
    local-only) ;;
    *)
      printf 'set IO_LOCAL_SNS_REHEARSAL_ACK=local-only before using this optional local-only SNS rehearsal helper\n' >&2
      exit 2
      ;;
  esac
}

reject_mainnet_args() {
  for arg in "$@"; do
    case "$arg" in
      --network=ic|--network|ic|-n|-nic|-n=ic|mainnet|--network=mainnet)
        printf 'refusing mainnet-like argument: %s\n' "$arg" >&2
        exit 2
        ;;
      *"${PROTECTED_CANISTER}"*|*"${PROTECTED_NEURON}"*)
        printf 'refusing protected IO target in local rehearsal arguments: %s\n' "$arg" >&2
        exit 2
        ;;
    esac
  done
  while IFS='=' read -r name value; do
    case "$value" in
      ic|mainnet|*"--network ic"*|*"-n ic"*|*"${PROTECTED_CANISTER}"*|*"${PROTECTED_NEURON}"*)
        case "$name" in
          IO_LOCAL_SNS_PROTECTED_REMINDER|IO_LOCAL_SNS_REHEARSAL_ACK) ;;
          *)
            printf 'refusing unsafe environment value in %s\n' "$name" >&2
            exit 2
            ;;
        esac
        ;;
    esac
  done < <(env)
}

require_local_script_guard() {
  require_local_ack
  reject_mainnet_args "$@"
}

toml_string() {
  local file="$1"
  local section="$2"
  local key="$3"
  awk -F '=' -v section="[$section]" -v key="$key" '
    $0 == section { in_section = 1; next }
    /^\[/ { in_section = 0 }
    in_section && $1 ~ "^[[:space:]]*" key "[[:space:]]*$" {
      value = $2
      sub(/^[[:space:]]*"/, "", value)
      sub(/"[[:space:]]*$/, "", value)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", value)
      print value
      exit
    }
  ' "$file"
}

toml_number() {
  local file="$1"
  local section="$2"
  local key="$3"
  awk -F '=' -v section="[$section]" -v key="$key" '
    $0 == section { in_section = 1; next }
    /^\[/ { in_section = 0 }
    in_section && $1 ~ "^[[:space:]]*" key "[[:space:]]*$" {
      value = $2
      gsub(/[ _]/, "", value)
      print value
      exit
    }
  ' "$file"
}

require_file() {
  if [ ! -f "$1" ]; then
    printf 'missing required file: %s\n' "$1" >&2
    exit 2
  fi
}

