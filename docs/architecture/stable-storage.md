# Stable launch state

`io_stream_manager` stores `StableCell<StableStreamState>` plus `StableBTreeMap<Principal, CallerRedemptionState>`. `StableStreamState` has only `V1(StreamStateV1)`.

`io_nns_neuron_manager` stores `StableCell<StableNnsState>`. `StableNnsState` has only `V1(NnsStateV1)`.

Install always writes Paused V1 state. Upgrade reopens and fully validates the envelope and its self-bound configuration; malformed, future or corrupt state traps. No stream/NNS V0, experimental schema, cursor fixture, legacy payout conversion or prelaunch migration is supported. Historian retains its separate real migration history.
