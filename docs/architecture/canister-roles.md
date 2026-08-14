# Canister roles

`io_stream_manager` owns direct redemption, liquid receipts, reserve issuance,
daily entitlement accumulation, one pending backed batch and serialized
settlement.

`io_nns_neuron_manager` owns protected NNS governance effects, fixed staging
Accounts, direct maturity and one unwind child. Its production execution
identity is the existing protected-neuron controller
`oae4c-3iaaa-aaaar-qb5qq-cai`.

`io_historian` exclusively owns index scans, archives, reconciliation, histories and alerts. It cannot authorize or calculate monetary effects.

`frontend` is advisory. Production remains not live.

Reserved mapping record: `io_stream_manager`
`thset-pqaaa-aaaar-qb7wa-cai`; `io_historian`
`tjqj3-uaaaa-aaaar-qb7xa-cai`; `frontend`
`torpp-zyaaa-aaaar-qb7xq-cai`. The NNS Manager uses its separately protected
execution identity above rather than a reserved placeholder.
