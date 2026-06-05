# First Integration Slice

This slice makes the IO canisters runnable as installable Wasm and adds mock-driven scheduler flows.

## Real Canister Execution

`cargo run -p xtask -- build_canisters` builds release `wasm32-unknown-unknown` `cdylib` outputs and writes raw Wasm, deterministic gzipped Wasm, SHA sidecars, and `manifest.json` under `release-artifacts/`.

For each canister, the expected outputs are:

```text
release-artifacts/<canister>.wasm
release-artifacts/<canister>.wasm.gz
release-artifacts/<canister>.wasm.sha256
release-artifacts/<canister>.wasm.gz.sha256
release-artifacts/manifest.json
```

`cargo run -p xtask -- verify_artifacts` checks existence, SHA consistency, manifest hashes/sizes, and stale release files.

Debug Wasm builds expose local-only methods such as `debug_get_state`, `debug_tick`, and model time controls. Production DIDs for the value-moving canisters remain constructor-only.

## Mock Integrations

Mock canisters live under `tests/mocks/`:

- `mock_icp_ledger`
- `mock_io_ledger`
- `mock_icp_index`
- `mock_io_index`
- `mock_nns_governance`
- `mock_sns_governance`
- `mock_jupiter_faucet`

The mock ledgers keep balances and transaction history with source, destination, memo, block index, amount, and timestamp. The mock governance canisters expose debug APIs for maturity, unwind, SNS neurons, proposals, and votes.

The mock index canisters are thin wrappers around mock ledger history. Live scheduler tests configure the stream manager with the mock index canisters for scans. Downstream value-moving transfers route through `LedgerTransferClient` mock adapters, which still call the mock ledgers underneath.

## Scheduler Flows

`io_stream_manager::debug_tick`:

- scans mock ICP history for deposits to `stream_manager_deposit`;
- classifies Jupiter Faucet, 2-year maturity, and 2-week maturity by source/memo;
- journals each relevant source block and processes completed operations once;
- transfers issued IO from `protocol_reserve` to Jupiter Faucet or eligible SNS-neuron reward accounts through the transfer boundary;
- scans IO transfers to `redemption`;
- pays ICP and returns redeemed IO to `protocol_reserve` through the transfer boundary.
- resumes retryable operations from the durable journal before scanning new blocks.

`io_nns_neuron_manager::debug_tick`:

- disburses 2-year and 2-week maturity from the model or mock governance;
- emits ICP transfer requests to `stream_manager_deposit` through the transfer boundary;
- drives mock governance split/start-dissolving, stop/merge, and ready unwind principal disbursement paths;
- handles two-week rebalance planning and ready unwind disbursement.
- journals maturity/unwind ICP transfers and finalizes local model state only after the downstream boundary transfer succeeds.

## Durable State

The stream manager persists operation journal entries and ICP/IO scan cursors. Two-week distributions store per-recipient transfer status. Redemptions store separate ICP payout and IO return status so an upgrade or repeated tick can continue without double-paying.

The NNS manager persists operation journal entries and maturity/unwind scheduler checkpoints. The current implementation covers the mock maturity/unwind ICP transfer flows and includes placeholder operation kinds for pool split, merge-back, and restake work.

## Limits

This is not production ledger or governance wiring. Downstream transfer paths now use the `LedgerTransferClient` boundary, but the debug/PocketIC scan sources still use mock `debug_get_transactions` APIs. Production scan/index adapters and archive traversal are future work. The debug scheduler tick is absent from production DIDs. The `io-ledger-types` crate provides production-shaped ledger/index types, transfer error mapping, fee representation, and index cursor/lag/archive modelling so future real adapters can be introduced without rewriting journal semantics. The operation journals are production-shaped but not audited. See `docs/architecture/ledger-index-clients.md`, `docs/security/threat-model.md`, and `docs/operations/mainnet-readiness.md` before real-client work.
