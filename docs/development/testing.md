# Testing

The suite follows the Jupiter Faucet convention of running everything through `xtask`.

```bash
cargo run -p xtask -- test_all
```

The repository is intentionally test-first. The PocketIC test files include deterministic model coverage plus real install/call tests that run when `POCKET_IC_BIN` points at a compatible PocketIC server binary.

## Commands

```bash
cargo run -p xtask -- test_unit
cargo run -p xtask -- test_pocketic_integration
cargo run -p xtask -- test_pocketic_required
cargo run -p xtask -- sns_harness_check
cargo run -p xtask -- sns_governance_read_tests
cargo run -p xtask -- sns_governance_read_required
cargo run -p xtask -- sns_ledger_index_tests
cargo run -p xtask -- sns_ledger_index_required
cargo run -p xtask -- sns_root_lifecycle_tests
cargo run -p xtask -- sns_root_lifecycle_required
cargo run -p xtask -- sns_pocketic_smoke
cargo run -p xtask -- sns_pocketic_required
cargo run -p xtask -- test_local_integration
cargo run -p xtask -- test_e2e
cargo run -p xtask -- test_all
cargo run -p xtask -- test_ci
cargo run -p xtask -- build_canisters
cargo run -p xtask -- verify_artifacts
cargo run -p xtask -- build_debug_canisters
cargo run -p xtask -- validate_install_args
cargo run -p xtask -- security_scan
cargo run -p xtask -- security_scan_required
cargo run -p xtask -- verify_release
```

`test_all` is the local default. It builds debug Wasm for PocketIC integration and the live install tests skip cleanly when `POCKET_IC_BIN` is unset.

`test_pocketic_required` is strict about PocketIC availability and fails if `POCKET_IC_BIN` is unset.

`sns_harness_check` validates local SNS docs, fixture skeletons, and required script guardrails without PocketIC, `dfx`, or mainnet access.

`sns_governance_read_tests` runs host/unit coverage for local/mock SNS governance reads, proposal pagination, snapshot conversion, exclusions, and TwoWeekMaturity allocation. It does not require PocketIC and does not call live SNS governance.

`sns_governance_read_required` builds debug Wasm and runs the read-only PocketIC SNS governance test. It requires `POCKET_IC_BIN`.

`sns_ledger_index_tests` runs host/unit coverage for production-shaped ledger/index DTOs and adapters, local SNS-shaped ledger/index transfer and scan boundaries, cursor errors, duplicate proof handling, mock ledger/index crates, and scheduler boundary helpers. It does not require PocketIC.

`sns_ledger_index_required` builds debug Wasm and runs PocketIC stream-manager value-flow tests through the local SNS-shaped ledger/index boundary. It requires `POCKET_IC_BIN`.

`sns_root_lifecycle_tests` runs host/unit checks for the mock SNS root, mock governance upgrade proposal flow, release artifact manifest matching, lifecycle docs, and required-script guardrails. It does not require PocketIC and does not use `dfx`.

`sns_root_lifecycle_required` builds debug Wasm and runs the mock SNS root/controller lifecycle PocketIC tests. It requires `POCKET_IC_BIN`, verifies controller handoff to the mock root, executes proposal-shaped upgrades through the local root intent path, checks stable-state preservation, and keeps production DIDs constructor-only.

`sns_pocketic_smoke` runs the SNS harness check and skips the live topology/governance/root lifecycle tests when `POCKET_IC_BIN` is unset. `sns_pocketic_required` is strict about PocketIC availability, installs IO canisters with SNS-shaped local principals, runs the read-only SNS governance read test, and runs the mock SNS root lifecycle test.

`test_ci` is strict: it runs formatting, workspace check, DID surface validation, release artifact build and manifest/SHA verification, install-args validation, required security scan, unit tests, required PocketIC integration, required SNS root lifecycle PocketIC tests, local integration, e2e tests, and clippy with `-D warnings`.

`verify_release` is release-oriented and intentionally does not call `test_ci`, avoiding command recursion. It runs DID surface, release builds, artifact verification, install-args validation, `sns_harness_check`, host SNS governance read tests, host SNS ledger/index tests, host SNS root lifecycle tests, and `security_scan_required`.

## Coverage added in this version

### Monetary-policy unit tests

- 40/60 split preserves total, including small amounts and rounding.
- Jupiter Faucet deposits issue IO only against the 60% liquid backing.
- 2-year NNS maturity issues no IO and increases the redemption rate.
- 2-week NNS maturity issues backed IO to eligible IO SNS neurons.
- Later Jupiter Faucet inflows mint fewer IO after 2-year yield increases backing.
- Redemptions preserve the redemption rate when no rounding is involved.
- Protocol reserve IO and the non-dissolvable genesis neuron are excluded from redeemable supply.
- Insufficient reserve and insufficient liquidity failures are atomic.

### Reward-policy unit tests

- Closed proposal non-participation reduces entitlement.
- No closed proposals does not penalise neurons.
- Proposal vote counts are capped at the eligible closed proposal count.
- Genesis, protocol-owned, dissolving, zero-stake, and zero-time neurons are excluded.
- Allocation uses stake-time, not a naive snapshot.
- Rounding dust is reported and not silently lost.

### NNS-neuron-manager model tests

- Live 2-year neuron ID and controller principal constants are checked.
- Fast-forward maturity accrual/disbursement is modelled deterministically.
- 2-week pool split/unwind becomes disbursable only after two weeks.
- Cancel-dissolve before disbursement merges the unwind split back into the pool.
- Pending unwind/restake is incorporated into rebalance planning.

### Stream-manager integration-shaped tests

- Source/memo classification distinguishes Jupiter Faucet, 2-year maturity, and 2-week maturity streams.
- Unknown sources cannot issue IO.
- Duplicate ledger events are idempotently rejected.
- Failed IO issuance does not mark a transaction as processed.
- Durable stream operation journals preserve failed issuance, partial 2-week distribution, and redemption payout/return progress.
- Ledger/index cursors avoid rescanning completed mock ledger blocks while keeping duplicate source-block protection.
- Active SNS-neuron snapshots drive the target 2-week NNS staking pool.
- `io-ledger-types` unit tests cover production-shaped account/subaccount Candid fixtures, ICP account identifier vectors, ICRC subaccount validation, ICP/ICRC transfer DTOs, transfer success/error mapping, Nat and u64 overflow handling, duplicate transfer proof helpers, explicit fee fields, ICP/ICRC index page mapping, archive traversal DTOs, and index cursor/archive/lag behavior.
- `io-governance-types` unit tests cover production-shaped NNS/SNS Candid fixtures, NNS lifecycle command request/result mapping, governance error classification, governance pagination guardrails, malformed ID handling, numeric overflow handling, SNS eligibility snapshots, and SNS proposal participation summaries.
- Reward-policy and stream-manager snapshot tests cover SNS eligibility and participation feeding TwoWeekMaturity allocation, including proposal pagination, excluded governance/protocol neurons, invalid SNS neuron ID exclusion, and rounding dust.

### E2E model tests

- Jupiter Faucet -> IO issuance -> 2-week pool target -> fast-forward maturity -> 2-year no-IO stream -> 2-week staker-IO stream -> unwind -> redemption.
- Cancel-dissolve before the two-week unwind completes restores pool principal without a liquid unwind.

## Additional coverage added in this revision

The expanded suite now includes tests for:

- stream classification by source and memo;
- duplicate ledger transaction replay across stream kinds;
- atomic failure when IO reserve is exhausted;
- zero-value and tiny-e8s stream handling;
- 2-year maturity increasing backing without issuing IO;
- 2-week maturity issuing backed IO without changing the redemption rate;
- pre-event exchange-rate usage for later Jupiter Faucet entrants;
- redemption retryability after insufficient-liquidity failures;
- reward allocation dust handling and stable replay order;
- new-neuron proposal-participation windows;
- exclusion of genesis/protocol/dissolving neurons;
- NNS manager fast-forward maturity accrual;
- multiple unwind child neurons;
- cancel-dissolve before and after liquidity return;
- malicious and duplicate end-to-end input attempts.

The real PocketIC tests use debug Wasm from `target/wasm32-unknown-unknown/debug`. `xtask test_pocketic_integration` builds those artifacts first. The tests skip cleanly when `POCKET_IC_BIN` is unset so non-PocketIC development environments can still run the workspace suite.

The live PocketIC tests include upgrade-before-retry cases for stream-manager IO issuance/redemption return failures and NNS-manager maturity transfer failures. Host-level stable export/import tests preserve the journal entries and cursors directly.

The production-shaped ledger/index and governance adapter tests do not call live ICP, NNS, SNS, IO ledger, or index canisters. They are fixture and boundary tests only. The real adapter structs are not wired into default production execution, and archive traversal/governance lifecycle reconciliation are modelled but not scheduler-integrated.

Local SNS harness tests are an additional compatibility layer. They do not replace model or mock/PocketIC tests, do not run official `dfx sns` flows, and do not wire IO value-moving flows to live SNS ledger/index canisters. Read-only local SNS governance reads, local SNS ledger/index value flows, and mock SNS root/controller lifecycle upgrades are covered through mock/PocketIC tests. Production SNS root/governance wiring remains future work.

`xtask test_local_integration` builds release Wasm artifacts, validates the DID guardrail, validates `icp.yaml` with `icp-cli`, runs `icp build`, and then runs the CLI-shaped Rust integration tests. It does not start a local replica or deploy canisters.


## Additional preflight commands

The default `test_all` command now runs a workspace `cargo check --workspace --all-targets` before the test suites. Formatting is intentionally available as a separate command rather than forced in `test_all` so early scaffolding failures remain focused on compile/test issues.

```bash
cargo run -p xtask -- check
cargo run -p xtask -- fmt_check
cargo run -p xtask -- preflight
cargo run -p xtask -- did_surface
cargo run -p xtask -- verify_artifacts
cargo run -p xtask -- validate_install_args
cargo run -p xtask -- security_scan
```

## Extra coverage added before first local run

The suite also covers:

- invalid two-week backing fractions above 100%
- blank transaction ids being rejected before state mutation
- strict source/memo classification
- stream arithmetic overflow atomicity
- maturity disbursement idempotency
- split-entire-pool and split-too-much NNS manager behaviour
- cancel-after-disbursement requiring the restake path rather than merge-back
- zero-time fast-forward not accruing maturity
- reward-policy saturation and exclusion safety cases
