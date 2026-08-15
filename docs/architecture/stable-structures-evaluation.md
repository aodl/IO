# Stable structures evaluation

The launch implementation already uses `ic-stable-structures` narrowly. Stream
Manager stores its bounded V1 control state in one `StableCell` and completed
per-caller redemption replay records in a `StableBTreeMap`. NNS Manager stores
bounded V1 control state in one `StableCell` and successful Jupiter block replay
records in a `StableBTreeMap`. Historian remains a bounded, rebuildable V1
snapshot.

This split keeps the one active operation, one pending entitlement batch, one
passive unwind child, scalar cooldowns, and bounded refresh-failure list easy to
validate as a whole. Permanent maps are reserved for identities whose deletion
would weaken exact replay: a caller entry is written only on completed
redemption, and a Jupiter block entry only on canonical successful completion.
Invalid proof probes allocate no map entries.

A broad record-by-record rewrite is not part of launch. It would add memory
region ownership, key encoding, partial-update, and schema-evolution surface
without removing a demonstrated bottleneck. The encoded maximum-state tests,
strict launch-V1 decode tests, semantic validators, and PocketIC same-Wasm
upgrade tests are the applicable launch evidence.

If measured legitimate lifetime volume approaches a stable-memory or upgrade
limit after launch, any new layout is a separately reviewed post-launch change.
It must preserve permanent monetary replay evidence and fail closed on unknown
state; no pre-launch compatibility path is retained.
