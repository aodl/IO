# Stable State Fixtures

These are local launch-schema descriptors for deterministic tests. They are not
live canister snapshots and were not generated from mainnet state.

The current canisters use strict typed launch state. Rust tests build equivalent
typed values in memory so round-trip, semantic validation, future-version and
corruption rejection remain executable. The descriptors keep the inventory
reviewable and give `xtask validate_stable_storage` stable paths to require.

Corrupt fixtures deliberately contain non-Candid text and must reject rather than defaulting to an
empty production state. There are no fixtures for obsolete pre-launch schemas.
