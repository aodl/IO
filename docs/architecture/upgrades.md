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
- operation journal entries, including phases, retry counts, last errors, downstream transfer blocks, two-week recipient transfer status, and redemption payout/return status
- scheduler cursors for ICP and IO index scans

`io_nns_neuron_manager` preserves:

- config derived from init args
- simulated NNS model state
- two-week pool state
- scheduler config principals from init args
- operation journal entries for maturity/unwind ICP transfers and pool operation placeholders
- scheduler maturity/unwind checkpoints

## Current Limits

No `ic-stable-structures` layout has been introduced yet. That is intentional while the state is still compact and model-oriented. Future real ledger, NNS, SNS, or large-index state should revisit the storage layout before mainnet scale.

Stable-state export/import helpers are available only for host tests/debug builds and are not production API methods.

Host tests exercise stable export/import preservation for journals and cursors. PocketIC tests include upgrade-before-retry coverage for pending stream-manager and NNS-manager operations using debug Wasm.

The stable layout is production-shaped for the integration slice, but it has not been audited for mainnet operation scale.

Upgrade proposals should use artifacts that pass:

```bash
cargo run -p xtask -- verify_release
```

The command verifies DID boundaries, rebuilds release Wasm, checks artifact hashes/manifest, validates install args without deployment, and runs the strict security scan.
