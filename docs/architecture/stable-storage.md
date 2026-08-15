# Stable launch state

`io_stream_manager` stores `StableCell<StableStreamState>` plus `StableBTreeMap<Principal, CallerRedemptionState>`. `StableStreamState` has only `V1(StreamStateV1)`.

`io_nns_neuron_manager` stores `StableCell<StableNnsState>`. `StableNnsState` has only `V1(NnsStateV1)`.

Install writes strict launch V1 state. Stream Manager and NNS Manager start Paused;
Historian initializes its bounded observation state independently. Upgrade
decodes and fully validates only the canister's launch V1 shape; malformed,
older development, future, or corrupt state traps. No production canister
contains a pre-launch migration or fallback decode path.

Stream V1 includes one required scalar launch-schema marker so the simplified
field set cannot accidentally decode the immediately preceding development V1
shape after availability-only fields were deleted. This is schema identity,
not a migration or operational state machine.

Permanent growth is narrow and proof-gated. Stream caller records arise from
authenticated redemption requests and retain one replay/result slot per actual
caller. Completed Jupiter block indexes arise only after a canonical successful
deposit and remain permanent replay protection. Invalid Jupiter probes and
failed SNS neuron refreshes allocate no stable collection, cooldown, lease, or
negative-cache entry. Maximum-size tests cover the actual launch state rather
than ancillary recovery fixtures; permanent monetary replay evidence is not
deleted to reclaim space.
