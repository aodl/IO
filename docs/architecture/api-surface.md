# API Surface Architecture

IO follows a minimal production API pattern for value-moving canisters. The core canisters should behave like operational protocol components rather than public dashboards.

## Minimal Production API Principle

The value-moving canisters are:

- `io_stream_manager`
- `io_nns_neuron_manager`

Their production DIDs should remain minimal. Production behavior should be driven by install args, internal state, timers, ledger/index scans, governance observations, and management-canister observations where possible. The current acceptable production shape for value-moving canisters is install-args-only:

```did
service : (InitArgs) -> {}
```

Do not add broad production methods such as `get_state`, `get_events`, dashboard views, caller-submitted stream-kind processors, redemption controls that trust arbitrary caller assertions, or debug time controls.

Install-args-only evolution is not considered a broad API expansion. Public query/control methods on `io_stream_manager` or `io_nns_neuron_manager` still require an explicit architecture exception.

## Debug DID vs Production DID

Production DIDs are the deployed protocol boundary. They should expose only the narrow methods required for protocol liveness.

Debug DIDs are for local development, model tests, and PocketIC-shaped tests. They may expose richer methods such as:

- `debug_get_state`
- `debug_get_redemption_rate`
- `debug_process_stream_event`
- `debug_redeem`
- `debug_get_config`
- `debug_plan_rebalance`
- `debug_advance_model_time`
- `debug_tick`

Debug methods must not become production read/control APIs.

`xtask did_surface` rejects broad state/config/redemption/event methods, any `debug_` method, and `debug_tick` in production DIDs. It also checks release value-moving Wasm artifacts for exact debug/control method strings when artifacts are present. The Wasm scan is intentionally narrower than the DID scan to avoid false positives from normal Rust runtime strings and internal field names.

The production DIDs for `io_stream_manager` and `io_nns_neuron_manager` remain constructor-only services.

Stable-state export/import helpers are host-test/debug-only implementation aids and must not be added to production DIDs.

## Ledger-as-Interface

Production flows should prefer ledger/index-observed interfaces.

Jupiter Faucet ICP stream:

- Jupiter Faucet transfers ICP to an `io_stream_manager` account/subaccount.
- `io_stream_manager` scans and validates ledger/index data.
- IO issuance is triggered internally.

User redemption:

- A user transfers IO to a redemption account/subaccount.
- `io_stream_manager` scans and validates IO ledger/index data.
- ICP payout is triggered internally.

2-year and 2-week maturity:

- `io_nns_neuron_manager` disburses ICP to `io_stream_manager` account/subaccount targets with distinguishable memo/subaccount data.
- `io_stream_manager` scans and validates ledger/index data.
- The stream manager classifies those observed ledger flows.

Production callers should not be able to assert arbitrary stream kinds such as "Jupiter Faucet", "two-year maturity", or "two-week maturity" through public methods.

## Historian as Public Read Model

`io_historian` owns the future public read/query surface. The frontend should consume historian/read-model APIs rather than value-moving canister internals.

The historian should reconstruct state from observable sources where possible:

- ICP ledger/index.
- IO/SNS ledger/index.
- NNS governance where possible.
- SNS governance where possible.
- Management canister status where possible.
- Canister metadata, install args, and governance proposal records.

The historian should not depend on broad public query APIs from `io_stream_manager` or `io_nns_neuron_manager`.

## Why Core Canisters Avoid Broad Queries

Broad production query APIs on value-moving canisters create pressure to expose operational internals as stable public contracts. That makes upgrades harder, encourages dashboard clients to couple directly to monetary-policy state, and risks leaking implementation details that should remain internal.

The safer pattern is:

- Core value-moving canisters keep narrow production boundaries.
- Debug/test builds expose rich local tooling.
- Historian/read-model canisters provide public query ergonomics.
- Ledger/index and governance observations form the durable interface between components where practical.

## Exceptions

A production method may be added to a value-moving canister only if:

1. It cannot be represented safely as a ledger/index-observed flow.
2. It is required for protocol liveness.
3. It has narrow input semantics.
4. It does not expose sensitive operational state.
5. It is documented in this file.

Documented exceptions:

- None.
