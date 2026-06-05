# Upgrade Architecture

The value-moving IO canisters use explicit stable snapshots for the current implementation phase.

## Mechanism

`io_stream_manager` and `io_nns_neuron_manager` use:

```rust
ic_cdk::storage::stable_save
ic_cdk::storage::stable_restore
```

The stable payloads are dedicated structs, not ad hoc debug API responses. This keeps upgrade state under canister ownership while production DIDs remain install-args-only.

## Preserved State

`io_stream_manager` preserves:

- config derived from init args
- protocol accounting state
- processed transaction IDs
- active staked IO total
- two-week pool backing bps
- scheduler config principals from init args

`io_nns_neuron_manager` preserves:

- config derived from init args
- simulated NNS model state
- two-week pool state
- scheduler config principals from init args

## Current Limits

No `ic-stable-structures` layout has been introduced yet. That is intentional while the state is still compact and model-oriented. Future real ledger, NNS, SNS, or large-index state should revisit the storage layout before mainnet scale.

Stable-state export/import helpers are available only for host tests/debug builds and are not production API methods.

Future production scheduler cursors should be added as explicit stable fields before mainnet use. Do not infer cursor state from volatile timers or transient in-memory client state.
