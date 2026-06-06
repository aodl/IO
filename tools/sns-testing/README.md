# Optional Local SNS Testing

This directory contains optional local-only helpers for a future `dfinity/sns-testing` rehearsal. These scripts are not required CI, not used by `verify_release`, and not proof of official launch readiness until a full local SNS launch has completed.

The official SNS launch path uses `dfx sns`; this is not part of required IO workflows.

Expected manual order:

1. Prepare a local `dfinity/sns-testing` checkout and let it manage the local replica.
2. Run `./tools/sns-testing/check-prereqs.sh`.
3. Build/deploy IO dapp canisters into the local environment with `./tools/sns-testing/deploy-io-dapp-local.sh`.
4. Fill `tools/sns/sns_init.io.local.yaml` with local dapp canister IDs, fallback controller principals, and local SNS canister IDs.
5. Run `./tools/sns-testing/validate-local-sns-config.sh`.
6. Run `./tools/sns-testing/run-local-sns-testing.sh` only after the operator has reviewed the generated commands.

These scripts must not use `--network ic`, must not call mainnet, and must not start a replica inside the dapp deployment step. The existing canister that owns IO NNS neuron 6345890886899317159 is not touched.
