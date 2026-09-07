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
is the request. Staging is not reserve, so the amount remains in `C`. The
single scheduler's bounded one-Account index scan discovers block IDs; the
canonical IO ledger exact-proves the transfer before the source Account or
amount can authorize value movement. Only after fresh coherent `B/C`, matching
fees, and `L >= gross` does Stream persist the exact payout intent. Payout
success retires the staged amount economically, and an exact staging-to-reserve
sweep replaces that temporary exclusion with physical supply/reserve
accounting. Normal illiquidity leaves the candidate queued and creates no debt.
The SNS-index scan is newest-first with an exclusive upper cursor. A captured
multi-page interval retains its prior committed watermark until traversal
reaches it; queue pressure and upgrade preserve the resume cursor. Execution is
chronological within a discovered page, without a global oldest-first claim
across pages. Sweep intent creation rechecks the exact paid operation after its
fee await and persists one immutable timestamp. Compatible late success is
monotone; stale rejection cannot overwrite newer progress.

Structural stake observation runs every 12 hours and updates the sorted registry
and latest reconciliation checkpoint without consuming or crediting a reward
event. Daily reward processing retains its canonical event deadline and
300-second safety margin. One reconstructed one-shot scheduler chooses the
earliest structural, reward, approximately 60-second redemption poll, or exact-recovery deadline. A successful
structural checkpoint drives reconciliation immediately; retryable contention
continues the same generation after 60 seconds rather than waiting for another
structural poll. Absolute nanosecond retry/poll deadlines are rounded upward
only after checked addition. Busy in-flight recovery and reward-paused retained
due work receive a bounded future rearm instead of feeding an expired deadline
back as zero-delay work. At most one cohort may be committed per structural generation.
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
is never inferred from an arbitrary active unwind. There is no target queue or
second scheduler. Reward allocation is prospective
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
no-argument redemption worker prompt, claim receipts, reward
observation/backing, lifecycle, and public status. The redemption caller
provides no monetary facts or destination. Public progress reports only
real action boundaries (`Idle`, `Pending`, `RateLimited`, `Completed`, and `Stuck`, plus the exact
receipt permit another canister must satisfy); operator status retains
diagnostic internal phase text.

SNS lifecycle proposal validation is a pure local submission-time preflight.
Execution remains authoritative because readiness conditions can change while
a proposal is voting. The reviewed SNS Governance implementation treats every
normal target reply as successful execution without decoding an
application-level `Err`, so an authenticated `set_paused` call replies normally
only when the requested durable lifecycle state is reached (or was already
reached). Unaccepted pause/readiness requests reject at the transport boundary;
unauthorized callers retain the ordinary typed error. Exact resumable monetary
state, including a proved payout awaiting its reserve sweep, keeps
its existing readiness and recovery semantics.

Stable state is a strict prelaunch launch schema with one monetary slot, bounded
registry, latest checkpoint, accumulator, pending batch, one account-history
cursor, and an ordered 64-block candidate map.
Install and upgrade reopen Paused; old states are rejected and immutable work
remains resumable.

Useful checks:

```bash
cargo test -p io-stream-manager --lib
cargo run -p xtask -- did_surface
cargo run -p xtask -- validate_stable_storage
cargo check -p io-stream-manager --target wasm32-unknown-unknown
```
