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

`src/scheduler/` contains a no-op `scheduler_tick_once()` for future timer-driven work. It currently records planned responsibilities only: scanning ICP ledger/index data for Jupiter Faucet and NNS maturity deposits, scanning IO ledger/index data for redemption transfers, classifying observed flows, and processing authorized streams internally. It performs no external calls.

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
