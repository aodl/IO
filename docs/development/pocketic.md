# PocketIC Development

The repository has a cached Rust `pocket-ic` test dependency and real install/call tests in `tests/pocketic/`.

To run real PocketIC tests, set `POCKET_IC_BIN` to a compatible PocketIC server binary and run:

```bash
export POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server
cargo run -p xtask -- test_pocketic_integration
```

Use the strict command when PocketIC must be present:

```bash
cargo run -p xtask -- test_pocketic_required
```

The command first builds debug Wasm for:

- IO canisters
- mock ledgers
- mock indexes
- mock governance canisters
- mock Jupiter Faucet

When `POCKET_IC_BIN` is unset, the real install tests skip and the deterministic model tests in the same files still run.

The live tests install real Wasm canisters into PocketIC and cover:

- Jupiter Faucet deposits observed through mock ICP ledger history.
- 2-year maturity deposits that issue no IO.
- 2-week maturity deposits allocated through mock SNS governance snapshots.
- redemption transfers that pay mock ICP and return IO to reserve.
- NNS-manager maturity and unwind ticks that emit mock ICP ledger transfers.
- an NNS-manager to stream-manager topology where maturity transfers are emitted and then scanned by the stream manager.
- upgrade-before-retry flows where durable journals resume failed IO issuance, redemption IO return, and maturity transfer operations.

Time fast-forward tests use two layers:

- model tests call explicit model time advancement for deterministic maturity/unwind checks;
- real PocketIC tests call `PocketIc::advance_time` and then use debug model time controls until timer-driven production time integration is added.

These tests use mock ledgers, mock indexes, and mock governance canisters. They do not use real NNS, SNS, ICP ledger, IO ledger, or mainnet canisters.

`cargo run -p xtask -- test_ci` requires `POCKET_IC_BIN` and includes the live PocketIC integration suite. GitHub Actions should either provide a compatible PocketIC binary or run the non-PocketIC workflow steps plus document the missing strict gate.
