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

phase_log_dir() {
  local dir="${REHEARSAL_DIR}/generated/logs"
  mkdir -p "$dir"
  printf '%s\n' "$dir"
}

phase_log_file() {
  local name="$1"
  printf '%s/%s.log\n' "$(phase_log_dir)" "$name"
}

log_command_status() {
  local log_file="$1"
  local status="$2"
  shift 2
  {
    printf 'command:'
    printf ' %q' "$@"
    printf '\nexit_status=%s\n' "$status"
  } >> "$log_file"
}

run_logged() {
  local log_file="$1"
  shift
  {
    printf '+'
    printf ' %q' "$@"
    printf '\n'
  } >> "$log_file"
  local status
  if "$@" >> "$log_file" 2>&1; then
    status=0
  else
    status=$?
  fi
  log_command_status "$log_file" "$status" "$@"
  return "$status"
}

record_blocker() {
  local reason="$1"
  local blocker_dir="${REHEARSAL_DIR}/generated/blockers"
  mkdir -p "$blocker_dir"
  printf '%s\n' "$reason" > "${blocker_dir}/latest-blocker.txt"
  printf '%s\n' "$reason" >&2
}

require_loopback_url() {
  local url="$1"
  if [ "$url" != "${url#"${url%%[![:space:]]*}"}" ] || [ "$url" != "${url%"${url##*[![:space:]]}"}" ]; then
    printf 'refusing URL with leading or trailing whitespace: %s\n' "$url" >&2
    exit 2
  fi
  case "$url" in
    http://*) ;;
    *) printf 'refusing non-http loopback URL: %s\n' "$url" >&2; exit 2 ;;
  esac
  case "$url" in
    *'#'*|*'@'*|*'%'*|*icp-api.io*|*icp.net*|*icp0.io*|*ic0.app*|*boundary*)
      printf 'refusing unsafe local rehearsal URL: %s\n' "$url" >&2
      exit 2
      ;;
  esac
  local rest="${url#http://}"
  local authority="${rest%%[/?]*}"
  local host port
  case "$authority" in
    "[::1]":*) host="::1"; port="${authority#"[::1]:"}" ;;
    *:*:*) printf 'refusing malformed URL authority: %s\n' "$url" >&2; exit 2 ;;
    *:*) host="${authority%%:*}"; port="${authority#*:}" ;;
    *) printf 'refusing URL without explicit port: %s\n' "$url" >&2; exit 2 ;;
  esac
  case "$host" in
    localhost|127.0.0.1|::1) ;;
    *) printf 'refusing non-loopback host: %s\n' "$url" >&2; exit 2 ;;
  esac
  if ! printf '%s' "$port" | grep -Eq '^[0-9]+$' || [ "$port" -lt 1 ] || [ "$port" -gt 65535 ]; then
    printf 'refusing URL without valid explicit port: %s\n' "$url" >&2
    exit 2
  fi
}

require_command_available() {
  local command_name="$1"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    record_blocker "required command is unavailable: ${command_name}"
    exit 2
  fi
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
