# Canister roles

`io_stream_manager` owns claim-rate snapshots, spendable liquid backing,
reserve issuance, direct redemption, the bounded backing/reward registry, one
pending entitlement batch, and one serialized monetary operation.

`io_nns_neuron_manager` owns permanent and pooled NNS governance effects, lazy
pooled-parent creation, one immediate command, and at most 32 passive unwind
children. Its production execution identity is the existing protected-neuron
controller `oae4c-3iaaa-aaaar-qb5qq-cai`.

`io_historian` owns rebuildable observation and alerts. `frontend` presents
that advisory model. Neither can authorize or calculate monetary effects.

Reserved mapping record: Stream `thset-pqaaa-aaaar-qb7wa-cai`; Historian
`tjqj3-uaaaa-aaaar-qb7xa-cai`; frontend `torpp-zyaaa-aaaar-qb7xq-cai`. The NNS
Manager uses its separately protected execution identity. Production remains
not live.
