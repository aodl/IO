#!/usr/bin/env bash
set -euo pipefail

# optional local-only SNS candidate validation wrapper

cargo run -p xtask -- sns_config_validate

if [ "${IO_RUN_DFX_SNS_VALIDATE:-}" = "1" ]; then
  cargo run -p xtask -- sns_config_validate_official
else
  printf 'skipping optional dfx sns validation; set IO_RUN_DFX_SNS_VALIDATE=1 to opt in\n'
fi
