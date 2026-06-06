# Official SNS Testing

We currently run SNS-shaped mock/PocketIC tests.

We do not currently run the official SNS launch locally in required CI.

Official `sns-testing` is optional and heavier. The official SNS launch path uses `dfx sns`; this is not part of required IO workflows. SNS testflight is a future manual/mainnet rehearsal.

IO's canonical IO ledger should be the SNS ledger; any IO_TEST ledger is non-canonical.

The existing canister that owns IO NNS neuron 6345890886899317159 is not touched by these tests.

## Layer 1: IO Mock/PocketIC SNS-Shaped Harness

This layer uses repo-owned mocks for SNS governance, SNS root, ledger, and index canisters. It tests IO-specific accounting, journal retry, governance-read mapping, root/controller upgrade intent, stable-state behavior, and constructor-only production DIDs.

It does not run official SNS launch, SNS-W, decentralization swap, or mainnet testflight.

## Layer 2: PocketIC NNS/SNS/Application Subnet Topology

This layer uses PocketIC topology support to create NNS, SNS, and application subnets, then installs IO dapp canisters and mocks on appropriate subnets where practical.

It is useful for canister placement, principal ranges, constructor wiring, and controller behavior. It is still not official launch unless real SNS canisters are installed.

## Layer 3: Official Local SNS Launch Rehearsal

This optional layer uses `dfinity/sns-testing` and `dfx sns` to rehearse official local launch mechanics. It can validate whether a candidate `sns_init.yaml` can move through the local SNS launch process.

This layer is not required CI, not part of `verify_release`, not run by `test_ci`, and not a substitute for security review or tokenomics decisions.

## Layer 4: Mainnet SNS Testflight

This future manual layer uses a mock SNS on mainnet to test day-to-day governance operations before real launch. It can test upgrade proposal operations, root control, controller handoff, frontend/historian configuration, and proposal tooling.

It does not perform the real SNS launch, does not run a real swap, and must not be confused with the final NNS launch proposal.

## Local References

- `tools/sns/README.md`
- `tools/sns/sns_init.io.template.yaml`
- `tools/sns/sns_init.io.local.yaml`
- `tools/sns-testing/README.md`
- `tools/sns/testflight/README.md`
- `tools/sns/launch-readiness.toml`

Official reference points used for this package include Internet Computer SNS docs for `dfx sns`, SNS testing, local `sns-testing`, testflight, and PocketIC NNS/SNS subnet integration.
