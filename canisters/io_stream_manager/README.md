# io_stream_manager

Main IO monetary-policy canister.

## Role

- Owns IO monetary policy.
- Classifies incoming ICP/IO ledger streams.
- Applies the canonical 40/60 split.
- Issues backed IO from protocol reserve when applicable.
- Handles redemption logic by ledger/index-observed IO deposits.

This canister is value-moving. Its production API must remain minimal.

Do not add public dashboard/query endpoints here. Do not add public methods that trust caller-supplied stream kinds in production. Use the debug DID for local/model tests.

## Production API and Init Args

The production DID is install-args-only:

```did
service : (InitArgs) -> {}
```

`InitArgs` defines the initial IO supply, protocol reserve, non-redeemable governance supply, two-week backing bps, and optional placeholder principals for future Jupiter Faucet, NNS manager, ICP ledger/index, and IO ledger/index integrations.

Validation rejects:

- total supply lower than reserve plus non-redeemable governance supply
- `two_week_pool_backing_bps > 10_000`
- present-but-empty or malformed optional principal text

## Stable State

Upgrade persistence uses an explicit stable snapshot saved with `ic_cdk::storage::stable_save` and restored with `stable_restore`. The snapshot preserves config, protocol accounting, processed transaction IDs, active staked IO, and two-week pool backing bps. Host tests exercise export/import round trips without exposing stable-state methods in the production DID.

## Scheduler Skeleton

`src/scheduler/` contains the internal scheduler boundary for ledger/index-observed work.
On non-Wasm hosts, `scheduler_tick_plan_only()` remains a planning helper and `scheduler_tick_once()` does not perform external calls.
In debug/test Wasm, `debug_tick` can scan configured local/mock ICP and IO ledger/index canisters, classify observed flows, execute downstream mock-ledger transfers through `LedgerTransferClient`, and update durable operation journals and scan progress.
The scan path uses index canisters as the account-history abstraction. ICP-style descending/newest-first pages use separate latest/head and oldest/backfill cursors, and page contents are applied in chronological order after validation. Ascending local/mock pages keep forward cursor semantics and allow global ledger block gaps.
Cursor advancement is conservative and journal-gated; unreadable, lagged, duplicate, or non-progressing index pages do not advance scan progress as if history were complete.

Production-shaped ICP/ICRC ledger and index adapters live behind `io-ledger-types` traits, but they are not wired into default production execution in this milestone.
The production DID remains constructor-only and does not expose scheduler control or query methods.
Archive-required and index-lag states are modelled as retryable boundary errors; raw ledger/archive traversal is not implemented in scheduler execution.
Public historian/frontend read surfaces for scan status remain future work.

## Stream Semantics

`JupiterFaucet`:

- 40% 2-year stake accounting.
- 60% liquid reserve.
- IO to Jupiter Faucet.

`TwoYearMaturity`:

- 40% restake.
- 60% liquid reserve.
- No IO issuance.

`TwoWeekMaturity`:

- 40% restake into pooled 2-week position.
- 60% liquid backing.
- Backed IO to eligible IO SNS neurons.
