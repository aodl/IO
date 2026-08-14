# Canister roles

`io_stream_manager` owns direct redemption, liquid receipts, reserve issuance,
daily entitlement accumulation, one pending backed batch and serialized
settlement.

`io_nns_neuron_manager` owns protected NNS governance effects, fixed staging Accounts, direct maturity and one unwind child. Its production design location is the existing immutable neuron controller `oae4c-3iaaa-aaaar-qb5qq-cai`; reserved `tatch-ciaaa-aaaar-qb7wq-cai` is unused.

`io_historian` exclusively owns index scans, archives, reconciliation, histories and alerts. It cannot authorize or calculate monetary effects.

`frontend` is advisory. Production remains not live.

Reserved mapping record: `io_stream_manager` `thset-pqaaa-aaaar-qb7wa-cai`; `io_nns_neuron_manager` `tatch-ciaaa-aaaar-qb7wq-cai`; `io_historian` `tjqj3-uaaaa-aaaar-qb7xa-cai`; `frontend` `torpp-zyaaa-aaaar-qb7xq-cai`. The NNS reserved ID remains unused despite this inventory label.
