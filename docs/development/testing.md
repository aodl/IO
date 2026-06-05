# Testing

The suite follows the Jupiter Faucet convention of running everything through `xtask`.

```bash
cargo run -p xtask -- test_all
```

The current repository is intentionally test-first. The PocketIC and local-CLI tests are deterministic model harnesses today, with command names and test boundaries prepared for replacement by live PocketIC / `icp-cli` deployments as the canister implementations are fleshed out.

## Commands

```bash
cargo run -p xtask -- test_unit
cargo run -p xtask -- test_pocketic_integration
cargo run -p xtask -- test_local_integration
cargo run -p xtask -- test_e2e
cargo run -p xtask -- test_all
```

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

The current PocketIC tests remain deterministic model harnesses. They are intentionally shaped around the operations that will map to real PocketIC later: canister install/call boundaries, fast-forward time, NNS maturity disbursement, child-neuron unwind, and e2e protocol composition.


## Additional preflight commands

The default `test_all` command now runs a workspace `cargo check --workspace --all-targets` before the test suites. Formatting is intentionally available as a separate command rather than forced in `test_all` so early scaffolding failures remain focused on compile/test issues.

```bash
cargo run -p xtask -- check
cargo run -p xtask -- fmt_check
cargo run -p xtask -- preflight
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
