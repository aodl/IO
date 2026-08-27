#!/usr/bin/env bash
set -euo pipefail

# optional local-only maintained official SNS testing bootstrap phase.
# Equivalent targets are documented by `. scripts/env.sh`; this rehearsal uses
# their bazel-bin outputs directly so it never writes into the sibling checkout.
# The canonical validation surface is `sns init-config-file --init-config-file-path`.
# Requires IO_LOCAL_SNS_REHEARSAL_ACK=local-only.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-local-sns.sh
source "${SCRIPT_DIR}/lib-local-sns.sh"
require_local_script_guard "$@"

log_file="$(phase_log_file 10-bootstrap-official-network)"
: > "$log_file"

official_commit="${IO_LOCAL_SNS_OFFICIAL_IC_COMMIT:-${PINNED_IC_COMMIT}}"
case "$official_commit" in
  [0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]) ;;
  *) record_blocker "official IC source commit must be an exact 40-hex value"; exit 2 ;;
esac

network_url="${IO_LOCAL_SNS_NETWORK_URL:-http://127.0.0.1:8080}"
require_loopback_url "$network_url"

require_command_available git
require_command_available sha256sum

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

# The exact source targets are //rs/sns/testing:sns-testing-init,
# //rs/sns/testing:sns-testing, and //rs/sns/cli:sns. Their filtered-Bazel
# build is monitored separately; this phase consumes and hashes those outputs.

if [ ! -x "$checkout/bazel-bin/rs/sns/cli/sns" ] \
  || [ ! -x "$checkout/bazel-bin/rs/sns/testing/sns-testing" ] \
  || [ ! -x "$checkout/bazel-bin/rs/sns/testing/sns-testing-init" ]; then
  record_blocker "source-built SNS binaries are unavailable from the pinned filtered-Bazel output tree"
  exit 2
fi
for binary in \
  "$checkout/bazel-bin/rs/sns/cli/sns" \
  "$checkout/bazel-bin/rs/sns/testing/sns-testing" \
  "$checkout/bazel-bin/rs/sns/testing/sns-testing-init"; do
  printf 'source_binary=%s sha256=%s\n' "${binary#$checkout/}" \
    "$(sha256sum "$binary" | awk '{print $1}')" >> "$log_file"
done

rendered_sns="$(sns_init_file)"
if [ -f "$rendered_sns" ]; then
  run_logged "$log_file" "$checkout/bazel-bin/rs/sns/cli/sns" \
    --network "$network_url" init-config-file --init-config-file-path "$rendered_sns" validate || {
    record_blocker "rendered SNS init failed the pinned source-built SNS CLI parser"
    exit 2
  }
fi

mark_phase_done 10-bootstrap-official-network \
  "official_ic_commit=${actual_commit}; checkout=${checkout}; clean=true; sns_cli_sha256=$(sha256sum "$checkout/bazel-bin/rs/sns/cli/sns" | awk '{print $1}'); sns_testing_sha256=$(sha256sum "$checkout/bazel-bin/rs/sns/testing/sns-testing" | awk '{print $1}'); sns_testing_init_sha256=$(sha256sum "$checkout/bazel-bin/rs/sns/testing/sns-testing-init" | awk '{print $1}')"
printf 'official local SNS bootstrap prerequisites passed; start maintained SNS testing tooling manually or through reviewed follow-up driver\n'
