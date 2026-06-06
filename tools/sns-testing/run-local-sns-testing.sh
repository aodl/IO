#!/usr/bin/env bash
set -euo pipefail

# optional local-only dfinity/sns-testing rehearsal launcher

if [ "${IO_SNS_TESTING_ACK:-}" != "local-only" ]; then
  printf 'set IO_SNS_TESTING_ACK=local-only before running this optional local helper\n' >&2
  exit 2
fi

if [ -z "${SNS_TESTING_DIR:-}" ]; then
  printf 'set SNS_TESTING_DIR to a local dfinity/sns-testing checkout\n' >&2
  exit 2
fi

if [ ! -d "$SNS_TESTING_DIR" ]; then
  printf 'SNS_TESTING_DIR does not exist: %s\n' "$SNS_TESTING_DIR" >&2
  exit 2
fi

cargo run -p xtask -- sns_config_validate

printf 'Review tools/sns/sns_init.io.local.yaml and the sns-testing checkout before submitting any local proposal.\n'
printf 'This helper stops before running dfx sns commands; execute the sns-testing documented steps manually from %s.\n' "$SNS_TESTING_DIR"
