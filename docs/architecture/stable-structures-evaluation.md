# Stable Structures Evaluation

The current approach uses serialized whole-state snapshots with explicit schema versions and
migration tests.

## Current Serialized Whole-State Snapshots

Benefits:

- small implementation surface;
- easy host export/import testing;
- simple fail-closed corruption behavior;
- compatible with the current pre-production value-moving canisters;
- no stable memory layout migration before activation.

Risks:

- whole-state decode/encode cost grows with journals;
- large processed transaction sets can make upgrade time and stable memory use expensive;
- field-level access is unavailable;
- compaction needs explicit audited checkpoints.

## ic-stable-structures Option

`ic-stable-structures` could store journals, processed transaction IDs, cursors, and read-model
indexes in separately versioned stable memory regions.

Benefits:

- bounded per-record access;
- less whole-state upgrade pressure;
- clearer memory-region ownership;
- natural maps for duplicate proofs and processed transaction IDs.

Risks and complexity:

- requires a stable memory layout/versioning plan;
- requires migration from serialized snapshots into stable maps;
- adds key encoding and map compatibility requirements;
- increases test burden for partial migrations and memory-manager layout changes;
- harder to review safely in the same milestone as policy/schema hardening.

## Recommendation

We should defer a broad `ic-stable-structures` rewrite to a dedicated migration milestone if
value-moving journals or processed transaction sets approach size limits, or before production
activation if audit requires record-level stable storage.

The test requirements include old/current/future/corrupt fixture tests, PocketIC upgrade tests, memory
layout version tests, schema evolution tests, and rollback/fail-closed tests.

Canister-by-canister:

- `io_stream_manager`: strongest candidate because processed transactions and retry journals can
  grow. Defer until duplicate-proof checkpoint policy is specified.
- `io_nns_neuron_manager`: possible candidate for lifecycle journals, but current state is smaller.
  Defer until lifecycle compaction policy is audited.
- `io_historian`: lower risk because histories are bounded and rebuildable. Serialized snapshots
  remain acceptable unless read-model breadth grows materially.

Any future migration should keep those tests at the canister boundary that owns the stable memory
layout.
