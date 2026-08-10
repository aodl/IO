#!/usr/bin/env bash
set -euo pipefail

REHEARSAL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(cd "${REHEARSAL_DIR}/../.." && pwd)"
PROTECTED_CANISTER="oae4c-3iaaa-aaaar-qb5qq-cai"
PROTECTED_NEURON="6345890886899317159"
PINNED_IC_COMMIT="4320fdf2e613844eabae1927b1a23b98da3a7bc6"

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

phase_state_dir() {
  local dir="${REHEARSAL_DIR}/generated/state"
  mkdir -p "$dir"
  printf '%s\n' "$dir"
}

phase_done_file() {
  printf '%s/%s.done\n' "$(phase_state_dir)" "$1"
}

phase_is_done() {
  [ -f "$(phase_done_file "$1")" ]
}

mark_phase_done() {
  local phase="$1"
  local detail="$2"
  printf '%s\n' "$detail" > "$(phase_done_file "$phase")"
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

local_network_url() {
  local url="${IO_LOCAL_SNS_NETWORK_URL:-http://127.0.0.1:8080}"
  require_loopback_url "$url"
  printf '%s\n' "$url"
}

local_identity_name() {
  printf '%s\n' "${IO_LOCAL_SNS_IDENTITY:-codex_local}"
}

official_checkout() {
  local checkout="${IO_LOCAL_SNS_IC_CHECKOUT:-${REPO_ROOT}/../ic}"
  if [ ! -d "$checkout/.git" ]; then
    record_blocker "missing pinned IC checkout: ${checkout}"
    exit 2
  fi
  local actual_commit
  actual_commit="$(git -C "$checkout" rev-parse HEAD)"
  if [ "$actual_commit" != "$PINNED_IC_COMMIT" ]; then
    record_blocker "IC checkout HEAD ${actual_commit} does not match pinned ${PINNED_IC_COMMIT}"
    exit 2
  fi
  printf '%s\n' "$checkout"
}

sns_cli() {
  local checkout
  checkout="$(official_checkout)"
  local command_path="${IO_LOCAL_SNS_CLI:-${checkout}/bazel-bin/rs/sns/cli/sns}"
  if [ ! -x "$command_path" ]; then
    record_blocker "source-built sns CLI is unavailable: ${command_path}"
    exit 2
  fi
  printf '%s\n' "$command_path"
}

sns_testing_cli() {
  local checkout
  checkout="$(official_checkout)"
  local command_path="${IO_LOCAL_SNS_TESTING_CLI:-${checkout}/bazel-bin/rs/sns/testing/sns-testing}"
  if [ ! -x "$command_path" ]; then
    record_blocker "source-built sns-testing CLI is unavailable: ${command_path}"
    exit 2
  fi
  printf '%s\n' "$command_path"
}

runtime_file() {
  local file="${IO_LOCAL_SNS_RUNTIME_FILE:-${REHEARSAL_DIR}/runtime.local.toml}"
  require_file "$file"
  printf '%s\n' "$file"
}

runtime_value() {
  local section="$1"
  local key="$2"
  local file
  file="$(runtime_file)"
  local value
  value="$(toml_string "$file" "$section" "$key")"
  if [ -z "$value" ] || [[ "$value" == TODO* ]]; then
    record_blocker "missing runtime value [${section}].${key} in ${file}"
    exit 2
  fi
  printf '%s\n' "$value"
}

require_lower_sha256() {
  if ! printf '%s' "$2" | grep -Eq '^[0-9a-f]{64}$'; then
    record_blocker "$1 must be an exact lowercase SHA-256"
    exit 2
  fi
}

manifest_artifact_value() {
  local canister="$1"
  local key="$2"
  jq -er --arg canister "$canister" --arg key "$key" \
    '.artifacts[] | select(.canister == $canister) | .[$key]' \
    "${REPO_ROOT}/release-artifacts/manifest.json"
}

require_hex_32_bytes() {
  if ! printf '%s' "$2" | grep -Eq '^[0-9a-f]{64}$'; then
    record_blocker "$1 must be exactly 32 lowercase hex bytes"
    exit 2
  fi
}

require_nat() {
  if ! printf '%s' "$2" | grep -Eq '^[0-9]+$'; then
    record_blocker "$1 must be an unsigned decimal integer"
    exit 2
  fi
}

hex_blob_literal() {
  printf '%s' "$1" | sed 's/../\\&/g'
}

sns_canister_id() {
  local role="$1"
  local discovery="${REHEARSAL_DIR}/generated/sns-canisters.json"
  require_file "$discovery"
  jq -er --arg role "$role" '.[$role].canister_id' "$discovery"
}

submit_sns_proposal() {
  local log_file="$1"
  local title="$2"
  local summary="$3"
  local action="$4"
  local governance neuron_hex network_url identity checkout governance_did args_file response proposal_id
  governance="$(sns_canister_id governance)"
  neuron_hex="$(runtime_value governance sns_neuron_subaccount_hex)"
  require_hex_32_bytes "SNS neuron subaccount" "$neuron_hex"
  network_url="$(local_network_url)"
  identity="$(local_identity_name)"
  checkout="$(official_checkout)"
  governance_did="${checkout}/rs/sns/governance/canister/governance.did"
  require_file "$governance_did"
  args_file="$(mktemp "${REHEARSAL_DIR}/generated/manage-neuron.XXXXXX.did")"
  printf '(record { subaccount = blob "%s"; command = opt variant { MakeProposal = record { url = "https://example.invalid/io-local-rehearsal"; title = "%s"; summary = "%s"; action = opt %s } } })\n' \
    "$(hex_blob_literal "$neuron_hex")" "$title" "$summary" "$action" > "$args_file"
  response="$(dfx canister call --network "$network_url" --identity "$identity" \
    --candid "$governance_did" --argument-file "$args_file" \
    "$governance" manage_neuron 2>&1)" || {
      printf '%s\n' "$response" >> "$log_file"
      record_blocker "SNS Governance rejected proposal: ${title}"
      return 2
    }
  printf '%s\n' "$response" >> "$log_file"
  proposal_id="$(printf '%s' "$response" | tr '\n' ' ' | sed -n 's/.*MakeProposal = record { proposal_id = opt record { id = \([0-9][0-9]*\) : nat64 }.*/\1/p')"
  if [ -z "$proposal_id" ]; then
    record_blocker "could not extract proposal ID for: ${title}"
    return 2
  fi
  printf '%s\n' "$proposal_id"
}

wait_sns_proposal() {
  local log_file="$1"
  local proposal_id="$2"
  local network_url sns_testing
  network_url="$(local_network_url)"
  sns_testing="$(sns_testing_cli)"
  if ! run_logged "$log_file" "$sns_testing" --network "$network_url" sns-proposal-upvote \
    --sns-name 'IO Local Rehearsal' --proposal-id "$proposal_id" --wait true; then
    if ! tail -30 "$log_file" | grep -q 'proposal was already decided'; then
      record_blocker "SNS proposal ${proposal_id} did not execute"
      return 2
    fi
  fi
}
