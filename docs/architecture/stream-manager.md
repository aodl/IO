# Stream Manager

The Stream Manager owns the IO reserve, spendable liquid ICP backing, prepared
ICRC-1 push redemption, canonical SNS structural/reward observation, one pending
entitlement batch, and one serialized monetary operation.

Its canonical snapshot brackets IO/ICP ledger and SNS reads with two identical
NNS observations. It derives claim-bearing supply `C`, liquid backing `L`,
claim-bearing Dynamic-parent principal `P`, live-child principal `U`, exact in-transit backing
`T`, total backing `B=L+P+U+T`, structural active stake `A_backing`, and the
prospectively eligible subset `A_reward`. Governance supplies neuron identity
and structural state; the IO ledger staking Account supplies stake value.

Each successful 12-hour structural observation refreshes the bounded, sorted
neuron registry and stores one latest reconciliation checkpoint without
crediting a reward event. Daily reward observation remains separately fenced by
the canonical event deadline and 300-second margin. One ephemeral earliest-
deadline timer wakes structural, reward, or 60-second retry work. There is no
target queue or second monetary scheduler. Reward allocation is allowed only
when Dynamic claim principal covers `floor(A_reward*B/C)`.

Redemption prepares `floor(user_io*B/C)` without reserving ICP. The user pushes
the exact IO principal to reserve through ICRC-1; Stream exact-proves source,
subaccount, destination, amount, fee, memo, creation window, no spender, and
non-replay. That proof creates a durable payout obligation. Missing liquid ICP
after proof pauses as `PayoutOwed`, survives upgrade/restart, and later pays at
most once. Claim-rate monotonicity keeps prepared quotes supportable across
intervening protocol work; caller replay and active-operation clearing remain
one atomic no-`await` completion.

Jupiter and two-week maturity enter through one paired-backing receipt. The
receipt is identified by the authenticated NNS Manager's operation sequence,
exact claim credit, and recipient policy: Jupiter or one frozen entitlement
generation. It freezes pre-inflow economics and the bounded recipient vector
before the credit becomes redeemable. Two-year maturity is ordinary unpaired
yield and enters liquid backing without a receipt. IO-ledger staking balances
remain authoritative when an ancillary SNS `ClaimOrRefresh` is delayed.
Public progress is coarse and action-oriented. Internal phase names remain
operator diagnostics, and multi-recipient settlement stays bounded to one
recipient transfer per resume.

Install and post-upgrade state are Paused. Reviewed unpause reconstructs the
Stream scheduler from semantic checkpoints; the NNS Manager independently
reconstructs exact operation/ready-child recovery deadlines. IO remains inert
and prelaunch.
