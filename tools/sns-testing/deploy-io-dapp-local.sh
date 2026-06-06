#!/usr/bin/env bash
set -euo pipefail

# optional local-only IO dapp deployment helper for a replica managed elsewhere
# This script intentionally does not start or manage any local replica process.

if [ "${IO_SNS_TESTING_ACK:-}" != "local-only" ]; then
  printf 'set IO_SNS_TESTING_ACK=local-only before running this optional local helper\n' >&2
  exit 2
fi

cargo run -p xtask -- build_canisters

printf 'Build complete. Deploy IO dapp canisters with your local environment tool, then record IDs in tools/sns/sns_init.io.local.yaml.\n'
printf 'Required dapp IDs: io_stream_manager, io_nns_neuron_manager, io_historian, frontend.\n'
printf 'Required local constructor IDs: ICP ledger/index, IO ledger/index, SNS ledger/index, SNS governance, NNS governance.\n'
