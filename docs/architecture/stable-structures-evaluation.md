# Stable structures evaluation

The launch implementation uses `ic-stable-structures` narrowly. Stream Manager
stores its bounded V1 control state, minimal redemption cursor, and at-most-64
candidate vector together in one `StableCell`. NNS Manager stores
bounded V1 control state in one `StableCell` and successful Jupiter block replay
records in a `StableBTreeMap`. Historian remains a bounded, rebuildable V1
snapshot.

This split keeps the one active operation, one pending entitlement batch, and
one passive unwind child easy to validate as a whole. Ancillary refresh
failures and invalid proof probes add no stable collections or scheduler state.
The only permanent value-moving map is NNS Manager's canonical successful
Jupiter block replay set. Redemption completion history remains in canonical
ledgers; Stream stores no result database.

A broad record-by-record rewrite is not part of launch. It would add memory
region ownership, key encoding, partial-update, and schema-evolution surface
without removing a demonstrated bottleneck. The encoded maximum-state tests,
strict launch-V1 decode tests, semantic validators, and PocketIC same-Wasm
upgrade tests are the applicable launch evidence.

If measured legitimate lifetime volume approaches a stable-memory or upgrade
limit after launch, any new layout is a separately reviewed post-launch change.
It must preserve permanent monetary replay evidence and fail closed on unknown
state; no pre-launch compatibility path is retained.
