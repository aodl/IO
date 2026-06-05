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
cargo run -p xtask -- test_local_integration
cargo run -p xtask -- test_e2e
cargo run -p xtask -- test_all
cargo run -p xtask -- test_ci
cargo run -p xtask -- build_canisters
cargo run -p xtask -- verify_artifacts
cargo run -p xtask -- build_debug_canisters
```

`test_all` is the local default. It builds debug Wasm for PocketIC integration and the live install tests skip cleanly when `POCKET_IC_BIN` is unset.

`test_pocketic_required` is strict about PocketIC availability and fails if `POCKET_IC_BIN` is unset.

`test_ci` is strict: it runs formatting, workspace check, DID surface validation, release artifact build and SHA verification, unit tests, required PocketIC integration, local integration, e2e tests, and clippy with `-D warnings`.

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

`xtask test_local_integration` builds release Wasm artifacts, validates the DID guardrail, validates `icp.yaml` with `icp-cli`, runs `icp build`, and then runs the CLI-shaped Rust integration tests. It does not start a local replica or deploy canisters.


## Additional preflight commands

The default `test_all` command now runs a workspace `cargo check --workspace --all-targets` before the test suites. Formatting is intentionally available as a separate command rather than forced in `test_all` so early scaffolding failures remain focused on compile/test issues.

```bash
cargo run -p xtask -- check
cargo run -p xtask -- fmt_check
cargo run -p xtask -- preflight
cargo run -p xtask -- did_surface
cargo run -p xtask -- verify_artifacts
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
