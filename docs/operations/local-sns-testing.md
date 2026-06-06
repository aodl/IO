# Local SNS Testing

IO uses local SNS testing as an additional compatibility layer. It does not replace the existing accounting, journal, retry, artifact, or DID guardrails.

## Strategy

1. Pure model tests remain the main accounting guardrail.
2. Mock and PocketIC tests remain the main journal, retry, and upgrade guardrail.
3. Local SNS harness tests provide SNS topology, config, and controller compatibility.
4. Local SNS governance read tests exercise mock-backed SNS neuron and proposal pages through the `SnsGovernanceClient` boundary.
5. Local SNS ledger/index value-flow tests exercise mock-backed ICRC ledger and index pages through `LedgerTransferClient` and `LedgerIndexClient`.
6. SNS root/controller lifecycle tests exercise mock governance proposals, mock root controller intent, artifact hash checks, and PocketIC upgrades.

The local SNS harness is not production launch configuration. It must not call mainnet, must not use `--network ic`, and must not deploy, install, upgrade, reinstall, or update settings on mainnet.

## Official SNS Testing Flow

The Internet Computer ecosystem has official SNS local testing flows based on SNS launch configuration files, commonly represented as `sns_init.yaml`-style inputs. Those flows are useful reference material for future launch validation and local compatibility work.

Some official SNS steps require the `dfx sns` extension. There is no `icp-cli` equivalent for every operation, including `add-nns-root` and some SNS init validation paths. IO therefore does not add `dfx` to required workflows.

Any `dfx`-based SNS testing for IO is optional, local-only, and not part of `test_ci` or `verify_release`. It must remain outside required CI and release commands unless the repository intentionally adopts a reviewed replacement workflow.

## IO-Owned PocketIC SNS Harness

The IO-owned harness uses PocketIC where practical and stays inside the repository's normal Rust and xtask workflow. Required checks do not require `dfx` and do not call mainnet.

The harness includes topology, config, and read-only governance smoke tests:

- load and validate the local SNS fixture skeleton;
- install IO canisters with SNS-shaped constructor principals in PocketIC;
- preserve production value-moving DIDs as constructor-only;
- keep debug APIs confined to debug DIDs and debug Wasm tests;
- prove required scripts do not invoke `dfx` or `--network ic`.
- seed mock SNS governance neurons/proposals in PocketIC;
- read paginated governance records through a `SnsGovernanceClient` implementation;
- keep production-shaped SNS governance canister adapters fixture-tested only and unwired from the local/default execution path;
- convert governance snapshots into TwoWeekMaturity allocation inputs;
- observe local IO redemption transfers through SNS-index-shaped account history;
- send redemption IO returns and TwoWeekMaturity rewards through the local SNS-ledger-shaped transfer boundary;
- test duplicate transfer, index lag, archive-required, pagination, retry, and idempotency behavior without live SNS calls.
- test SNS root/controller lifecycle behavior through mock governance proposals and a mock root canister;
- verify upgrade proposals against `release-artifacts/manifest.json`;
- set the mock SNS root as a PocketIC controller for IO value-moving canisters;
- preserve pending stream-manager and NNS-manager journal work across mock SNS-root-style upgrades.

These SNS harness tests use mock/local/PocketIC canisters only. The SNS root/controller lifecycle is mock/PocketIC only: mock governance/root records an approved intent, the test harness executes the PocketIC upgrade as the mock root controller, and the root records the outcome. They are not live SNS adapters, do not run official SNS launch or swap flows, and do not call mainnet.

Production-shaped SNS governance DTOs and the Wasm-gated `SnsGovernanceCanisterClient` are covered by host Candid fixtures in `io-governance-types`. The local SNS harness does not call live SNS governance and does not run official SNS launch, swap, or testflight flows.

Run deterministic local lifecycle checks with:

```bash
cargo run -p xtask -- sns_root_lifecycle_tests
```

Run strict live PocketIC lifecycle checks with:

```bash
POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server cargo run -p xtask -- sns_root_lifecycle_required
```
