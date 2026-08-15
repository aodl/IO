# Stable launch state

`io_stream_manager` stores `StableCell<StableStreamState>` plus `StableBTreeMap<Principal, CallerRedemptionState>`. `StableStreamState` has only `V1(StreamStateV1)`.

`io_nns_neuron_manager` stores `StableCell<StableNnsState>`. `StableNnsState` has only `V1(NnsStateV1)`.

Install writes strict launch V1 state. Stream Manager and NNS Manager start Paused;
Historian initializes its bounded observation state independently. Upgrade
decodes and fully validates only the canister's launch V1 shape; malformed,
older development, future, or corrupt state traps. No production canister
contains a pre-launch migration or fallback decode path.

Permanent growth is narrow and proof-gated. Stream caller records arise from
authenticated redemption requests and retain one replay/result slot per actual
caller. Completed Jupiter block indexes arise only after a canonical successful
deposit and remain permanent replay protection. Invalid Jupiter probes use one
persisted canister-wide timestamp and allocate no per-block entry. Failed SNS
neuron refreshes are capped at 1,000 entries with 32-byte neuron IDs and
256-byte diagnostics; one persisted timestamp bounds retry calls. These bounds
are checked by state validation and maximum-size tests rather than by deleting
monetary replay evidence.
