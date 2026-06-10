#!/usr/bin/env bash
set -euo pipefail

# optional local-only official SNS rehearsal prerequisite check

case "${IO_LOCAL_SNS_REHEARSAL_ACK:-}" in
  local-only) ;;
  *)
    printf 'set IO_LOCAL_SNS_REHEARSAL_ACK=local-only before using this optional local helper\n' >&2
    exit 2
    ;;
esac

cargo run -p xtask -- validate_local_sns_rehearsal

if command -v dfx >/dev/null 2>&1; then
  dfx --version
  if dfx sns --help >/dev/null 2>&1; then
    printf 'dfx sns is available for optional local-only rehearsal\n'
  else
    printf 'dfx sns is not available; install the sns extension before manual rehearsal\n' >&2
    exit 2
  fi
else
  printf 'dfx is not available; optional official local SNS rehearsal cannot run yet\n' >&2
  exit 2
fi

printf 'Follow deploy/local-sns-rehearsal/README.md and the official sns-testing instructions manually.\n'
