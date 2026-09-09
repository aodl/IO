# Stable launch state

`io_stream_manager` stores one `StableCell<StableStreamState>`.
`StableStreamState` has only `V1(StreamStateV1)` and contains the unique,
bounded, service-ordered, at-most-64 semantic-redemption candidate vector
beside its cursor. Its encoded type remains `Vec<u64>`; service-order semantics
do not change the stable schema.

`io_nns_neuron_manager` stores `StableCell<StableNnsState>`. `StableNnsState` has only `V1(NnsStateV1)`.

Install writes strict launch V1 state. Stream Manager and NNS Manager start Paused;
Historian initializes its bounded observation state independently. Upgrade
decodes and fully validates only the canister's launch V1 shape; malformed,
older development, future, or corrupt state traps. No production canister
contains a pre-launch migration or fallback decode path.

Stream V1 and NNS V1 include required scalar launch-schema markers,
both 13. Stream marker 13 replaces marker 12 with one minimal account-history
cursor, an in-state bounded candidate vector, and the two-stage immutable
payout/sweep operation. It removes the separate candidate memory, durable
poll/manual gates, last result, transient paid-unswept projection, and redundant
operation phases. Marker 12 is deliberately rejected without migration. NNS
marker 13 removes the obsolete fee-liability
and its maturity reimbursement phase while retaining the mandatory Dynamic
identity, claim-bearing parent principal, and anchor capacity. NNS marker 12 is
deliberately rejected; there is no migration or fallback decoder.

Stream marker 13 contains the fields needed for the canonical SNS genesis
baseline. A round-zero last event is valid only as a zero-credit activation
baseline, optionally with its exact `StructuralOnly` observation and a
fingerprinted reconciliation checkpoint whose event marker is zero. No new
stable field is needed. Both baseline forms round-trip and reopen Paused without
loss; malformed marker-13 states and marker 12 remain
rejected. Pending entitlement batches and credit-bearing observations still
require a nonzero event round.

The NNS maturity state stores the semantic role, compact command intent,
canonical pending-disbursement facts, frozen capture, and outgoing effect
recovery. It stores neither a staging-balance baseline nor Mint provenance.
Two-year delivery has no Stream receipt state; two-week delivery retains only
its entitlement generation, exact target replay binding, and the paired
receipt/effect state required for exactly-once settlement.

Permanent growth is narrow and proof-gated. Stream retains only its bounded
redemption cursor/candidate vector; the IO/ICP ledger blocks are the durable
external redemption and completion history. Completed Jupiter block indexes arise only after a canonical successful
deposit and remain permanent replay protection. Invalid Jupiter probes and
failed SNS neuron refreshes allocate no stable collection, cooldown, lease, or
negative-cache entry. Maximum-size tests cover the actual launch state rather
than ancillary recovery fixtures; permanent monetary replay evidence is not
deleted to reclaim space.
