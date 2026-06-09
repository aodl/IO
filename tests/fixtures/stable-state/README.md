# Stable State Fixtures

These fixtures are local migration descriptors for deterministic tests. They are not live canister
snapshots and were not generated from mainnet state.

The current canisters use Candid whole-state snapshots. The Rust tests build the equivalent typed
fixtures in memory so field defaults and migration behavior are checked against the current types.
The descriptor files keep the fixture inventory reviewable and give `xtask validate_stable_storage`
stable paths to require.

Corrupt fixtures deliberately contain non-Candid text and must reject rather than defaulting to an
empty production state. Empty/default fixtures model first install only, not a corrupt upgrade.
