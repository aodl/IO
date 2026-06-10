# Local SNS Testing

We currently run SNS-shaped mock/PocketIC tests. We do not currently run the official SNS launch locally in required CI.

IO uses local SNS compatibility testing as an additional safety layer. It does not replace accounting, journal, retry, artifact, DID, or release guardrails.

Pure model tests remain the main accounting guardrail.

Mock and PocketIC tests remain the main journal, retry, and upgrade guardrail.

## Four-Layer Compatibility Model

Layer 1: IO mock/PocketIC SNS-shaped harness.

This is the fast internal safety layer. It uses mock governance/root/ledger/index canisters and PocketIC tests to exercise IO-specific lifecycle assumptions. These tests are not official SNS launch tests, not SNS-W, not decentralization swap, not mainnet testflight, and not proof of official launch readiness.

Layer 2: PocketIC NNS/SNS/application subnet topology.

This creates NNS, SNS, and application subnets where supported by the pinned PocketIC dependency. It is useful for canister ID ranges, constructor principal acceptance, controller topology, and value-moving DID guardrails. It still does not run official SNS launch unless real SNS canisters are installed.

Layer 3: Official SNS Local Launch Rehearsal.

Official SNS testing is optional and heavier. Follow the current official ICP/DFINITY SNS testing documentation as the source of truth. The historical standalone `dfinity/sns-testing` repository is deprecated; if the official docs reference successor tooling or a new repository/location, use that current official location.

The official SNS launch path may require `dfx sns`; any `dfx`-based SNS testing for IO is optional, local-only for this layer, and not part of `test_ci` or `verify_release`. Required repository workflows must not depend on `dfx`.

The official local SNS rehearsal package lives under `deploy/local-sns-rehearsal/`. It provides a local `sns_init` candidate, local evidence template, no-network validators, and manual runbook for creating a real SNS-created local ledger/index/governance/root stack and recording local evidence in `canister-ids.local.toml`. The no-network package validator is:

```bash
cargo run -p xtask -- validate_local_sns_rehearsal
```

The optional completed-ledger evidence validator is:

```bash
cargo run -p xtask -- validate_local_sns_ledger
```

It skips clearly until a local rehearsal operator creates `deploy/local-sns-rehearsal/canister-ids.local.toml`.

Until that evidence file is produced from a completed local rehearsal, no local SNS canister IDs are recorded and no real SNS ledger/index/governance/root behavior has been observed.

Layer 4: SNS testflight.

SNS testflight is a future manual/mainnet rehearsal. It is not a real launch, has no real swap, and must not be confused with the NNS proposal/SNS-W production launch path.

## IO-Owned PocketIC SNS Harness

The IO-owned harness uses PocketIC where practical and stays inside the repository's normal Rust and xtask workflow. Required checks do not require `dfx` and do not call mainnet.

The harness includes:

- pure model tests as the main accounting guardrail;
- mock and PocketIC tests as the main journal, retry, and upgrade guardrail;
- local SNS-like topology checks with NNS/SNS/application subnets where available;
- mock SNS governance read tests through `SnsGovernanceClient`;
- mock SNS ledger/index value-flow tests through `LedgerTransferClient` and `LedgerIndexClient`;
- mock SNS root/controller lifecycle tests through proposal-shaped governance/root canisters;
- production DID checks that keep `io_stream_manager` and `io_nns_neuron_manager` constructor-only.

The local SNS harness is not production launch configuration. It must not call mainnet, must not use `--network ic`, and must not deploy, install, upgrade, reinstall, or update settings on mainnet.

The SNS root/controller lifecycle path is mock/PocketIC only: mock governance/root records an approved intent, the test harness executes the PocketIC upgrade as the mock root controller, and the root records the outcome. It is not live SNS root/governance wiring.

IO's canonical IO ledger should be the SNS ledger; any IO_TEST ledger is non-canonical and only useful for local/mock compatibility.

The existing canister that owns IO NNS neuron 6345890886899317159 is not touched by these tests.

## Commands

Run deterministic local lifecycle checks with:

```bash
cargo run -p xtask -- sns_root_lifecycle_tests
```

Run strict live PocketIC lifecycle checks with:

```bash
POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server cargo run -p xtask -- sns_root_lifecycle_required
```

Run official-readiness package checks without `dfx`:

```bash
cargo run -p xtask -- sns_config_validate
cargo run -p xtask -- sns_official_testing_check
cargo run -p xtask -- sns_launch_readiness_check
cargo run -p xtask -- validate_local_sns_rehearsal
```
