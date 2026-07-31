# Stable Storage

## Clean launch schema

Value-moving canisters support only launch schema V1. Prelaunch stream schemas
0-6, NNS schemas 0-3, scanner cursors, reward reservation mirrors, lossy payout
migrations, and experimental physical-backing states are not runtime
compatibility obligations. Future post-launch migration history begins at V1.
A later local/mainnet first-install or reinstall requires the explicit reset gate
in the simplification plan; this tranche performs no mainnet operation.

Stable storage hardening does not make IO live. No value-moving IO canister is deployed to production, production adapters are not active, the IO protocol remains not live, and the SNS IO ledger remains not launched.

The stable-state fixtures are local/test fixtures, not live snapshots. Files under
`tests/fixtures/stable-state/` were not generated from mainnet state.

## Schema Registry

The central registry lives in `crates/io_stable_schema`. It records each canister name, current
stable schema version, supported previous versions, fixture paths, bounds summary, and compaction
policy summary. `cargo run -p xtask -- validate_stable_storage` checks the registry and fixture
inventory without network calls.

Registered canisters:

- `io_stream_manager`: current schema version 3, supports legacy unversioned v0 stable roots,
  v1 stream-manager states with scalar reward reservations, and v2 full-backing snapshots.
- `io_nns_neuron_manager`: current schema version 2, supports legacy unversioned v0 stable roots
  and v1 fixed two-week dissolve configuration snapshots.
- `io_historian`: current schema version 2, supports v0 read-model fixtures with source health
  recomputed from state and legacy v1 duration-weighted governance summaries.

## io_stream_manager

Root type: `VersionedStableState { schema_version, state: StableState }`.

The stable state contains config, protocol accounting, processed transaction IDs, active exact
two-week staked IO, reward cohort evidence, operation journal, and scheduler/index cursors.
Production wiring is an optional config field and defaults absent in local fixtures. Pending
redemption records preserve gross ICP payout, fee, net user payout, IO return fee, retry status,
transfer blocks, user account, and last error.

Schema v2 stores reward reservation accounting as explicit unspent and
externally-spent-but-model-uncommitted buckets. Restore validates the split against recipient
transfer/proof evidence, validated preflight totals, and processed transaction evidence. Completed
reward operations must have zero reservation and processed-transaction evidence. Terminal, manual,
or uncertain value-moving reward states retain unavailable debit or fail closed; corrupt
value-moving state must fail closed instead of silently initializing or releasing reserve.

Schema v3 stores the frozen two-week reward cohort. Restore validates cohort ordering, exact member
IDs, positive frozen stake, checked totals, and consumed-operation references. Legacy v2 snapshots
must have 100% two-week backing in both config and state; any other backing value fails closed.
No historical cohort is fabricated during migration.

Pre-upgrade saves the versioned root. Post-upgrade first decodes the versioned root, then falls
back to the prior unversioned `StableState` root as schema version 0. Stable state that is missing,
corrupt, or uses an unsupported future version fails closed. Missing first-install state is handled
by normal `init`, not by treating corrupt upgrade state as empty.

Retry-critical journals, processed transaction IDs, duplicate proofs, and account-history cursors
are not silently evicted. Current growth risk is documented in `journal-compaction.md`.

## io_nns_neuron_manager

Root type: `VersionedStableState { schema_version, state: StableState }`.

The stable state contains config, simulated NNS model state, two-week pool state, pending lifecycle
operation journal, and scheduler cursors. Defaults preserve the protected reference values
`oae4c-3iaaa-aaaar-qb5qq-cai` and `6345890886899317159`; they remain protected references only,
not mutation targets.

Schema v2 uses the shared exact 14-day dissolve-delay constant for pooled and unwind neurons.
Legacy v1 snapshots must contain that same two-week duration; any other value fails closed.

Pre-upgrade saves the versioned root. Post-upgrade first decodes the versioned root, then falls
back to the prior unversioned `StableState` root as schema version 0. Stable state that is missing,
corrupt, or uses an unsupported future version fails closed. Missing first-install state is handled
by normal `init`.

Pending NNS lifecycle retry state is canonical for future execution and must not be casually
discarded or compacted before audited activation rules exist.

## io_historian

Root type: `StableState { schema_version, ... }`.

The stable state contains protocol/reserve/supply read-model snapshots, bounded stream,
redemption, reward, and NNS lifecycle histories, index health, governance participation, release
artifact observations, canister status observations, and last ingestion timestamp. Source health is
recomputed from stable fields and policies so missing source-health fields default to honest
prelaunch/missing semantics rather than zero protocol values.

Schema v2 migrates legacy v1 duration-weighted governance by resetting the rebuildable governance
summary. Historian is not protocol truth, so source observations can rebuild that read model without
preserving stale duration-weighted summary data.

Pre-upgrade saves the stable root. Post-upgrade fails closed if stable state is missing, corrupt, or
uses an unsupported future version. First install uses default prelaunch read-model state.

Historian is a rebuildable read model. It is not protocol truth and is not a value-moving authority.
It may evict old read-model entries because canonical data remains in source observations.

## Bounded Collections

- Historian stream history: 256 records, newest deterministic records retained by `record_id`;
  eviction loses rebuildable read-model convenience data.
- Historian redemption history: 256 records, same policy.
- Historian reward history: 256 records, same policy.
- Historian NNS lifecycle history: 256 records, same policy.
- Historian index health: 32 summaries, deduplicated by `record_id`.
- Historian release artifact status: 32 summaries, deduplicated by canister name.
- Historian canister status: 32 summaries, deduplicated by canister name.
- Historian governance neuron summaries: 512 records, bounded read model.
- Historian page limit: 100 records.
- Stream-manager processed transaction set: no silent eviction; requires audited compaction.
- Stream-manager operation journal: no silent eviction of retry-critical entries.
- NNS-manager operation journal: no silent eviction of retry-critical entries.
- Account-history cursors: scalar cursor state, preserved across migrations.

## Corruption And Missing State

Missing stable state on first install is accepted only through normal init/default paths; missing first-install state is not an upgrade corruption recovery path. Corrupt stable state on upgrade fails closed and visibly; corrupt value-moving state must fail closed. Unsupported future versions fail closed, and unsupported old versions fail unless a migration exists. Partial snapshots cannot be silently interpreted as empty production state, especially for value-moving canisters.

Value-moving production DIDs remain constructor-only:

```did
service : (InitArgs) -> {}
```

Debug DIDs remain separate. Frontend must not call value-moving canisters or import historian debug
declarations.
