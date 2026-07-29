#!/usr/bin/env bash
set -euo pipefail

# optional local-only maintained official SNS testing bootstrap phase
# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-local-sns.sh
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 10-bootstrap-official-network)"
: > "$log_file"

official_commit="${IO_LOCAL_SNS_OFFICIAL_IC_COMMIT:-2d7f90fb23672cc3b81c216a33d04c75672dd308}"
case "$official_commit" in
  [0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]) ;;
  *) record_blocker "official IC source commit must be an exact 40-hex value"; exit 2 ;;
esac

network_url="${IO_LOCAL_SNS_NETWORK_URL:-http://127.0.0.1:8080}"
require_loopback_url "$network_url"

require_command_available bazel
require_command_available git

checkout="${IO_LOCAL_SNS_IC_CHECKOUT:-}"
if [ -z "$checkout" ] || [ ! -d "$checkout/.git" ]; then
  record_blocker "set IO_LOCAL_SNS_IC_CHECKOUT to an isolated dfinity/ic checkout pinned to ${official_commit}"
  exit 2
fi

actual_commit="$(git -C "$checkout" rev-parse HEAD)"
if [ "$actual_commit" != "$official_commit" ]; then
  record_blocker "official checkout HEAD ${actual_commit} does not match pinned ${official_commit}"
  exit 2
fi
if [ -n "$(git -C "$checkout" status --porcelain)" ]; then
  record_blocker "official checkout at ${checkout} is not clean"
  exit 2
fi
if [ ! -d "$checkout/rs/sns/testing" ]; then
  record_blocker "official checkout missing rs/sns/testing at ${official_commit}"
  exit 2
fi
for required_path in \
  "$checkout/rs/sns/testing/README.md" \
  "$checkout/rs/sns/testing/scripts/env.sh" \
  "$checkout/rs/sns/testing/BUILD.bazel" \
  "$checkout/rs/sns/cli/BUILD.bazel" \
  "$checkout/rs/sns/cli/test_sns_init_v2.yaml" \
  "$checkout/rs/sns/cli/src/init_config_file/friendly.rs"; do
  if [ ! -e "$required_path" ]; then
    record_blocker "official checkout missing required pinned SNS file: ${required_path#$checkout/}"
    exit 2
  fi
done

for target in \
  "//rs/sns/testing:sns-testing-init" \
  "//rs/sns/testing:sns-testing" \
  "//rs/sns/cli:sns"; do
  if ! run_logged "$log_file" bazel query "$target"; then
    record_blocker "pinned official checkout does not expose required Bazel target ${target}"
    exit 2
  fi
done

if [ ! -x "$checkout/rs/sns/testing/bin/sns" ] \
  || [ ! -x "$checkout/rs/sns/testing/bin/sns-testing" ] \
  || [ ! -x "$checkout/rs/sns/testing/bin/sns-testing-init" ]; then
  record_blocker "source-built SNS binaries are unavailable; run '. scripts/env.sh' from rs/sns/testing in the pinned checkout"
  exit 2
fi

rendered_sns="${REHEARSAL_DIR}/sns_init.local.yaml"
if [ -f "$rendered_sns" ]; then
  (cd "$checkout/rs/sns/testing" && run_logged "$log_file" ./bin/sns init-config-file --init-config-file-path "$rendered_sns" validate) || {
    record_blocker "rendered SNS init failed the pinned source-built SNS CLI parser"
    exit 2
  }
fi

printf 'official local SNS bootstrap prerequisites passed; start maintained SNS testing tooling manually or through reviewed follow-up driver\n'
