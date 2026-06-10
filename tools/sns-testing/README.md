# Optional Local SNS Testing

This directory contains optional local-only helpers for a future official local SNS rehearsal. These scripts are not required CI, not used by `verify_release`, and not proof of official launch readiness until a full local SNS launch has completed.

Follow the current official ICP/DFINITY SNS testing documentation as the source of truth. The historical standalone `dfinity/sns-testing` repository is deprecated; if the official docs reference successor tooling or a new repository/location, use that current official location.

The official SNS launch path may require `dfx sns`; this is optional/manual and local-only for this layer, not part of required IO workflows.

Expected manual order:

1. Prepare the local SNS testing environment according to the current official ICP/DFINITY SNS testing documentation.
2. Run `./tools/sns-testing/check-prereqs.sh`.
3. Build/deploy IO dapp canisters into the local environment with `./tools/sns-testing/deploy-io-dapp-local.sh`.
4. Fill `tools/sns/sns_init.io.local.yaml` with local dapp canister IDs, fallback controller principals, and local SNS canister IDs.
5. Run `./tools/sns-testing/validate-local-sns-config.sh`.
6. Run `./tools/sns-testing/run-local-sns-testing.sh` only after the operator has reviewed the generated commands.

These scripts must not use `--network ic`, must not call mainnet, and must not start a replica inside the dapp deployment step. The existing canister that owns IO NNS neuron 6345890886899317159 is not touched.
