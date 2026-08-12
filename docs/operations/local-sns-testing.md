# Local SNS Testing

Required CI uses SNS-shaped mock/PocketIC tests. The maintained official SNS launch rehearsal is an optional local operator layer. The immutable 2026-08-11 and `2026-08-12-4320fdf-monitoring` packages remain historical mechanics/connectivity evidence, but their redemption-rate evidence is superseded because they queried the Governance default Account instead of the canonical treasury distribution subaccount. `2026-08-12-4320fdf-canonical-economics` is the accepted corrected package; none of these packages is official release adoption or mainnet evidence.

IO uses local SNS compatibility testing as an additional safety layer. It does not replace typed-operation, retry, artifact, DID, stable-state or release guardrails.

Pure model tests remain the main accounting guardrail.

Mock and PocketIC tests exercise bounded failures, retry and upgrade behavior without becoming monetary truth.

## Four-Layer Compatibility Model

Layer 1: IO mock/PocketIC SNS-shaped harness.

This is the fast internal safety layer. It uses mock governance/root/ledger/index canisters and PocketIC tests to exercise IO-specific lifecycle assumptions. These tests are not official SNS launch tests, not SNS-W, not decentralization swap, not mainnet testflight, and not proof of official launch readiness.

Layer 2: PocketIC NNS/SNS/application subnet topology.

This creates NNS, SNS, and application subnets where supported by the pinned PocketIC dependency. It is useful for canister ID ranges, constructor principal acceptance, controller topology, and value-moving DID guardrails. It still does not run official SNS launch unless real SNS canisters are installed.

Layer 3: Official SNS Local Launch Rehearsal.

Official SNS testing is optional and heavier. Follow the current official ICP/DFINITY SNS testing documentation as the source of truth. The historical standalone `dfinity/sns-testing` repository is deprecated; if the official docs reference successor tooling or a new repository/location, use that current official location.

The maintained official local SNS flow uses the source-built `sns` CLI; any `dfx`-based SNS testing for IO is optional, local-only, and not part of `test_ci` or `verify_release`. Required repository workflows must not depend on `dfx`.

The official local SNS rehearsal package lives under `deploy/local-sns-rehearsal/`. It provides a local `sns_init` candidate, local evidence template, no-network validators, and manual runbook for creating a real SNS-created local ledger/index/governance/root stack and recording local evidence in `canister-ids.local.toml`. The no-network package validator is:

```bash
cargo run -p xtask -- validate_local_sns_rehearsal
```

The completed-ledger evidence validator is:

```bash
cargo run -p xtask -- validate_local_sns_ledger
```

It validates the checked-in completed inventory; absence is a failure, not a
completion skip. New runs belong in new immutable evidence packages.

Layer 4: SNS testflight.

SNS testflight is a future manual/mainnet rehearsal. It is not a real launch, has no real swap, and must not be confused with the NNS proposal/SNS-W production launch path.

## IO-Owned PocketIC SNS Harness

The IO-owned harness uses PocketIC where practical and stays inside the repository's normal Rust and xtask workflow. Required checks do not require `dfx` and do not call mainnet.

The harness includes:

- pure model tests as the main accounting guardrail;
- mock and PocketIC tests for typed-operation retry and upgrade guardrails;
- local SNS-like topology checks with NNS/SNS/application subnets where available;
- mock SNS governance observation and command-boundary tests;
- standalone mock SNS ledger/index interface tests that are not launch monetary authority;
- mock SNS root/controller lifecycle tests through proposal-shaped governance/root canisters;
- production DID checks that keep `io_stream_manager` and `io_nns_neuron_manager` on the reviewed simplified command surfaces.

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
