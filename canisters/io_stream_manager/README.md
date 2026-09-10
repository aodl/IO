# IO Stream Manager

`io_stream_manager` owns the IO reserve, spendable liquid ICP backing, direct
redemption, purpose-specific claim and daily-stake observations, a bounded
per-neuron registry, one pending entitlement batch, and one serialized
monetary slot.

The scalar claim snapshot brackets its ledger reads with identical NNS control
epoch, operation sequence, and fingerprint observations. It derives:

```text
C = total_io_supply - protocol_reserve_io - nonredeemable_governance_io
B = L + P + U + T
claim_rate = B / C
pooled_target = floor(A_backing * B / C)
reward_target = floor(A_reward * B / C)
```

`L` is spendable liquid backing, `P` only the claim-bearing Dynamic-parent
principal, `U` live
passive-child net claim backing, and `T` exact net backing represented by a
persisted in-transit phase. Physical child principal is retained separately for
Governance commands; `U/T` deduct the exactly derived unavoidable future
disbursement fee once sticky child commitment is proved. The Dynamic anchor and
unattributed parent surplus are excluded. Permanent capital,
unminted maturity, cycles, and operational balances are excluded. The separate
bounded daily observation lists SNS neurons once. Pool policy is a separate
canonical observation used by daily reward and reconciliation work. Following
or permanent-neuron query failures cannot erase existing claim assets or block
a liquid redemption. Voting-power refresh is best-effort housekeeping and
never gates money. Governance supplies
neuron identity/state; each distinct exact IO
Ledger staking Account is read at most once and supplies `A_backing`. A delayed
ancillary SNS `ClaimOrRefresh` cannot hide a successful reward transfer.

An ordinary ICRC-1 transfer into the fixed semantic redemption staging Account
is the request. Staging is not reserve, so the amount remains in `C`. A small
dedicated coarse timer's bounded one-Account index scan discovers block IDs; the
canonical IO ledger exact-proves the transfer before the source Account or
amount can authorize value movement. Only after fresh coherent `B/C`, matching
fees, and `L >= gross` does Stream persist the exact payout intent. Payout
success immediately persists an exact staging-to-reserve sweep using the IO fee
frozen at activation. Normal illiquidity leaves the candidate queued and creates no debt.
The SNS-index scan is newest-first with an exclusive upper cursor. A captured
multi-page interval retains its prior committed watermark until traversal
reaches it; queue pressure and upgrade preserve the resume cursor. Candidate
discovery is deterministic and bounded, but settlement order is not FIFO. A
candidate without whole-gross liquidity may be deferred behind other discovered
candidates because no quote, reservation, or debt exists before activation.
This prevents one oversized or temporarily illiquid redemption from blocking
smaller payable redemptions without introducing reservations, liabilities, or
per-user scheduling state. Every returned ID advances coverage, but only
transfer hints to staging at or above the minimum enter the proof queue; the
canonical ledger still independently proves every monetary fact. Each outgoing intent is persisted before its ledger await;
compatible late success is monotone and stale rejection cannot overwrite newer
progress. While the two-ledger settlement is active, canonical claim supply and
claim rate are unavailable instead of projecting a transient paid-unswept value.

Structural stake observation runs every 12 hours and updates the sorted registry
and latest reconciliation checkpoint without consuming or crediting a reward
event. Daily reward processing retains its canonical event deadline and
300-second safety margin. Its one-shot scheduler chooses the earliest structural
or reward deadline. A successful structural checkpoint drives reconciliation immediately; retryable contention
continues the same generation after 60 seconds rather than waiting for another
structural poll. Redemption uses a separate ephemeral coarse timer; the one
monetary slot, not the number of timers, serializes value-moving work. A wake may
run later than its approximate interval under contention. At most one cohort may
be committed per structural generation.
SNS Governance initializes a canonical dummy genesis reward event at round zero
with a nonzero end timestamp, zero span, no settled proposals, and no rewards.
First readiness freezes that identity as a zero-credit activation baseline. An
observation of the identical event is `StructuralOnly`: it may establish
prospective eligibility and a valid reconciliation marker zero, but it cannot
increase reward counters or credit. Positive sequence-span metadata is required
only when the event advances; credit-bearing events and pending entitlement
batches always use nonzero rounds. Redemption remains valid before round one.
Exit membership moves through exact `ExitPrepared { generation }` and
`ExitCommitted { generation }` states resolved by the matching NNS request; it
is never inferred from an arbitrary active unwind. There is no target queue.
Reward allocation is prospective
and requires `P >= reward_target`.

Jupiter and two-week maturity use one narrow paired-backing receipt. Every
paired claim credit enters Stream liquid after its fresh delivery fee. A permit
freezes exact pre-inflow economics, that net liquid credit, and one recipient
vector; matching IO is calculated from the same net credit. Its kind only
selects the configured Jupiter Account or a frozen entitlement generation.
Two-year maturity creates no matching IO and therefore uses no receipt.
Completion marks ordinary target reconciliation due; any later liquid-to-parent
transfer is determined only from a fresh global target.
Recipient settlement deliberately handles one recipient transfer per resume;
that is a bounded per-flow work limit, not a protocol-wide effect-count rule.

Production methods expose the fixed staging Account, a permissionless
no-argument redemption wake hint, claim receipts, reward
observation/backing, lifecycle, and public status. The redemption caller
provides no monetary facts or destination, and the wake performs no external
work in the caller's invocation. Public progress reports only
real action boundaries (`Idle`, `Pending`, `Completed`, and `Stuck`, plus the exact
receipt permit another canister must satisfy); operator status retains
diagnostic internal phase text.

Public urgent wakes share a heap-only global ten-second admission cooldown.
Captured index pages and an already-discovered candidate backlog drain through
one internal near-term wake per successful page or completed/discarded
candidate without increasing the coarse head-poll rate or batching pages or
monetary operations. Index errors and illiquid candidates return to coarse
retry.

A normal coarse redemption wake reads at most one index page before servicing
at most one queued candidate, even when the queue is non-empty. Near-term wakes
read only a captured continuation page, or discover a fresh head when the queue
is empty. Page size is capped to the free capacity in the unique, service-ordered
64-block queue. A full queue deliberately backpressures newer discovery until a
candidate completes or is discarded; scanner coverage is not advanced past an
unrepresented hint, and all staged IO remains claim-bearing.

SNS lifecycle proposal validation is a pure local submission-time preflight.
Execution remains authoritative because readiness conditions can change while
a proposal is voting. The reviewed SNS Governance implementation treats every
normal target reply as successful execution without decoding an
application-level `Err`, so an authenticated `set_paused` call replies normally
only when the requested durable lifecycle state is reached (or was already
reached). Unaccepted pause/readiness requests reject at the transport boundary;
unauthorized callers retain the ordinary typed error. Any active operation
rejects readiness before canonical monetary reads. Immutable redemption work,
including a proved payout awaiting its reserve sweep, remains recoverable
by SNS Governance through `resume` or exact proof while Paused.

Stable state is a strict prelaunch marker-13 schema with one monetary slot,
bounded registry, latest checkpoint, accumulator, pending batch, one minimal
account-history cursor, and a unique bounded 64-block service queue in the same
stable state.
Install and upgrade reopen Paused; old states are rejected and immutable work
remains resumable. Active redemption recovery completes or is exactly proved
while Paused before ordinary readiness may enter Ready and reinstall timers.

Useful checks:

```bash
cargo test -p io-stream-manager --lib
cargo run -p xtask -- did_surface
cargo run -p xtask -- validate_stable_storage
cargo check -p io-stream-manager --target wasm32-unknown-unknown
```
