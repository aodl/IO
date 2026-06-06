# SNS Testflight Plan

SNS testflight is a future manual/mainnet rehearsal. It is not CI, not a real launch, and has no real swap.

The goal is to verify that IO can operate after a mock SNS controls the dapp canisters:

- deploy IO dapp canisters in a testflight environment;
- deploy a testflight SNS from a reviewed `sns_init.yaml`;
- keep developer recovery control while testing;
- add SNS root as co-controller;
- register IO dapp canisters with SNS root;
- test dapp upgrades through SNS governance proposals;
- verify `release-artifacts/manifest.json` hashes are referenced in proposal preparation;
- verify frontend/historian canister IDs are configured for the testflight environment;
- verify the testflight does not touch the existing canister that owns IO NNS neuron 6345890886899317159.

This plan does not finalize IO economics, final tokenomics, final swap parameters, final treasury distribution, final developer neurons, fallback controllers, production canister IDs, or SNS launch proposal payloads.

The detailed package lives under `tools/sns/testflight/`.
