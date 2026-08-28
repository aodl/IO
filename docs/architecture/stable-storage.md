# Stable launch state

`io_stream_manager` stores `StableCell<StableStreamState>` plus `StableBTreeMap<Principal, CallerRedemptionState>`. `StableStreamState` has only `V1(StreamStateV1)`.

`io_nns_neuron_manager` stores `StableCell<StableNnsState>`. `StableNnsState` has only `V1(NnsStateV1)`.

Install writes strict launch V1 state. Stream Manager and NNS Manager start Paused;
Historian initializes its bounded observation state independently. Upgrade
decodes and fully validates only the canister's launch V1 shape; malformed,
older development, future, or corrupt state traps. No production canister
contains a pre-launch migration or fallback decode path.

Stream V1 and NNS V1 include required scalar launch-schema markers, respectively
8 and 11, so the atomic local redemption-completion,
account-semantic receipt/quarantine, maturity-capture,
committed-fee, and Governance-recovery shapes cannot decode preceding
development states. This is strict pre-launch schema identity, not a migration or
operational state machine. Tests reject the preceding marker/state shape and
round-trip the final shape.

Stream marker 8 removes the development-only `CompletionPrepared` and
`CallerResultApplied` phases and the stored completion result. After the final
asynchronous payout postcondition, all validation and both replacement states
are computed before the first stable write. The caller replay result and active
operation clear then occur with no `await` or fallible return between them, so a
trap rolls back the whole message. Marker 7 is deliberately rejected; no
migration or fallback decoder is retained.

The NNS maturity state stores the semantic role, compact command intent,
canonical pending-disbursement facts, frozen capture, and outgoing effect
recovery. It stores neither a staging-balance baseline nor Mint provenance.
Two-year delivery has no Stream receipt state; two-week delivery retains only
its entitlement generation, exact target replay binding, and the paired
receipt/effect state required for exactly-once settlement.

Permanent growth is narrow and proof-gated. Stream caller records arise from
authenticated redemption requests and retain one replay/result slot per actual
caller. Completed Jupiter block indexes arise only after a canonical successful
deposit and remain permanent replay protection. Invalid Jupiter probes and
failed SNS neuron refreshes allocate no stable collection, cooldown, lease, or
negative-cache entry. Maximum-size tests cover the actual launch state rather
than ancillary recovery fixtures; permanent monetary replay evidence is not
deleted to reclaim space.
