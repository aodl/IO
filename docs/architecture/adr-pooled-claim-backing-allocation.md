# ADR: unified pooled claim-backing allocation

## Status

Accepted.

This ADR supersedes the separate-endowment design. It defines the production
claim-backing economics and bounded orchestration model. Acceptance does not
authorize deployment, mainnet activity, or production configuration values.

## Scope and evidence boundary

The upstream mechanics are preserved separately by commit
`eceb406604d51ed7d4730bdce3581f73f4c65121` and the
[post-Mission-70 candidate evidence](../testing/post-mission70-nns-candidate.md).
This ADR does not repeat that investigation. The exact component boundary is
recorded by the active pin and its maintained candidate tests.

## Accounting model

```text
C = total_io_supply
  - protocol_reserve_io
  - explicitly_nonredeemable_governance_io

B = L + P + U + T

L = liquid claim backing
P = active pooled two-week NNS principal
U = pending two-week unwind principal
T = exact claim backing frozen in an in-transit operation
K = permanent/exact-two-year protocol principal, excluded from B
A_backing = all structurally active claim-bearing ordinary staked IO
A_reward = the subset currently eligible for reward allocation

claim_rate = B / C
```

Operational fee reserves, if such a policy were selected, are excluded from
`B`. Maturity is not an asset and enters neither `B` nor `K` until Governance
produces an actual canonical ICP Mint. These categories are accounting
identities; they do not each require an independent stable field. All products,
sums, subtractions and floors use checked `u128` integer arithmetic.

The implementation has no separately endowed reward-backing capital.
Claim backing is one pool whose physical liquidity and NNS lifecycle states are
the `L`, `P`, `U` and `T` partitions above.

`B=0, C=0, A_backing=0` is valid empty genesis. `B>0, C=0, A_backing=0` is
backing without claims and has no normal claim rate or target. `B=0, C>0` is an
invalid uncovered-claims state. `C=0, A_backing>0` is invalid, as are
`A_backing>C`, exclusions greater than supply and arithmetic overflow. These
conditions have distinct typed errors. Every allocation validates both its
pre-event and post-event canonical observation before calculating a target;
an invalid observation is never converted into a target or silently clamped.

`A_backing`, not `A_reward`, is used in the backing target. A generation can
remain structurally active for solvency while reward-ineligible because its
exit or re-entry proof is incomplete. Reward eligibility is a separate
prospective policy and never changes historical reward observations.

The reward gate is a separate fail-closed coverage check:

```text
A_reward <= A_backing <= C
P <= B
backing_target = floor(A_backing * B / C)
reward_target = floor(A_reward * B / C)
reward allocation permitted only when P >= reward_target
```

`backing_target` remains the physical reconciliation target. `A_reward` is
derived from the bounded per-neuron eligibility records; it is not a second
stored aggregate. Initial structurally active stake below the pooled-parent
minimum remains in `A_backing` but not `A_reward`. A pending re-entry remains
outside `A_reward` until adding it still leaves `P>=reward_target`. Existing
eligible stake may continue only while its own reward target is covered. A
shortfall pauses reward processing; exact or over-target coverage permits only
prospective rewards, never retroactive rewards. `P>B` is an impossible
canonical partition and fails before either reward target is accepted.

## Liquid-first claim ingress

Every new claim-backing amount enters the Stream Manager liquid claim Account
before pooled positioning. The receipt freezes the pre-inflow claim scalars,
source operation identity, net liquid credit, exact NNS fingerprint and, where
IO is delivered, one canonical recipient vector. It persists only one current
recipient transfer. Completion retains a compact replay result and no recipient
history.

The receipt uses the pre-inflow rate:

```text
maximum_backed_IO = floor(net_liquid_credit * C0 / B0)
```

True empty genesis uses one IO per net claim-backing ICP. Every successfully
delivered IO increases `C`; maturity settlement does not assume that a
recipient remains structurally active and does not supply a synthetic `dA`.
The next daily observation derives `A_backing` and `A_reward` from canonical
SNS state and staking-account ledger balances.

### Jupiter

For an authorized source amount `S`, Jupiter proves the source block, sends
`floor(S*40/100)` gross toward permanent capital, and sends the remaining
claim gross minus its exact transfer fee to Stream liquid. Stream settles the
Jupiter IO recipient at the pre-inflow `B/C` rate. The ordinary Jupiter
recipient remains claim-bearing unless it is separately configured as an
explicit nonredeemable governance Account.

### Pooled-parent maturity

For an actual canonical Mint `M`:

```text
permanent_gross = floor(M * 40 / 100)
claim_gross = M - permanent_gross
permanent_credit = permanent_gross - exact_permanent_fee
liquid_claim_credit = claim_gross - exact_claim_fee
```

The permanent credit is proved first. The claim credit then enters Stream
liquid through the common receipt and settles the already-frozen entitlement
batch. Delivery increases `C` but makes no immediate claim about
`A_backing`. There is no maturity-specific parent destination or second
maturity transfer.

### Permanent-neuron maturity

The controlled permanent neuron first executes and proves
`StakeMaturity(40%)`. Governance then disburses all remaining ordinary
maturity and the protocol proves the actual Mint. That entire Mint, minus one
exact claim transfer fee, enters Stream liquid. It releases no IO. The retained
staked maturity remains permanent productive capital outside `B`.

### Child return

A dissolved child proves its principal transfer to Stream liquid. Its
zero-principal ordinary and staked maturity are then cleaned through the proved
merge path, after which the bounded child record is retired. Reward re-entry
state is generation-free after cleanup and cannot retain the child slot.

### Ordinary post-ingress reconciliation

Receipt completion marks the ordinary daily observation work due but does not
wait for physical positioning. A fresh bounded observation later calculates:

```text
target = floor(A_backing * B / C)
```

Only ordinary reconciliation may transfer a material liquid delta to the
pooled parent or commit a material unwind cohort. A top-up debits liquid by its
gross leg, credits the parent by gross minus the exact fee, and reduces `B`
once under Policy A. A claim ingress can therefore incur one additional ICP
fee when the fresh global target requires a later top-up. This fee is accepted
because liquid-first ingress removes source-specific destination planners and
their durable effect graphs.

Future reward events remain fail-closed unless the canonical pooled principal
covers `floor(A_reward*B/C)`. Receipt completion itself never restores reward
eligibility.

## Purpose-specific observations and unified registry

A `ClaimSnapshot` reads only scalar IO supply/reserve/exclusions, liquid ICP,
NNS `P/U/T`, fees, epochs, operation sequence and fingerprints. Redemption
uses this fixed-size read path and never lists SNS neurons or scans staking
Accounts. Its active state freezes only the exact quote scalars and adverse
postcondition floors.

A `DailyStakeObservation` verifies SNS Governance, brackets the reward event
and scalar snapshot, lists neurons once, and reads each distinct eligible
staking Account at most once. It rejects duplicate IDs and Accounts and is
bounded at 1,000 neurons. A non-dissolving neuron with any delay other than
1,209,600 seconds remains claim-bearing in `C` but is excluded from
`A_backing` and rewards; it cannot block redemption, daily observation or
reconciliation.

One sorted bounded registry stores neuron ID, canonical staking Account,
accumulated eligible credit, structural classification, prospective reward
status and an optional unresolved cohort generation. Freezing a batch moves
credit out of these records into the receipt's single recipient vector while
later events may accumulate fresh credit in the same registry.

Daily processing first reconciles structure, then proves coverage for the
existing eligible subset, then credits the current event, and only afterward
considers re-entry for a future event. All active pending re-entries are
promoted together only when `P` covers the full `A_backing` target;
otherwise none are promoted. This prevents per-neuron ordering from allocating
coverage that does not exist.

## Transit ownership

A Stream liquid-to-parent top-up has one owner at every phase:

- before transfer success, the value remains in `L` and `T=0`;
- after exact Stream transfer success but before NNS accepts proof, Stream owns
  only the unreflected residual in `T`;
- after NNS accepts proof, NNS owns that residual and Stream contributes zero;
- as cached parent principal reflects the credit, the residual is
  `expected_before + expected_credit - observed_parent`;
- at full reflection the value is wholly in `P` and `T=0`.

The observed parent must remain within the committed before/after interval.
An ambiguous submitted transfer makes the snapshot unavailable until exact
proof. Consequently `P+T` can never count more than the committed final
credit.
## Conservation

Every completed transition must prove:

```text
post_total_assets = pre_total_assets
                  + actual_external_mint_or_inflow
                  - explicit_fees
                  - external_payouts
```

Moving value among `L`, `P`, `U` and `T` creates nothing. Every claim-backing
e8s is in exactly one of those categories. The permanent leg enters only `K`.
Unminted maturity enters neither side. A transfer from one backing bucket to
another debits its gross amount, credits gross minus the exact fee and reduces
`B` by that fee.

An exact fee does not create an accounting imbalance. It destroys an asset and
therefore reduces backing or permanent capital. Replacing it later is a subsidy
or expense policy, not balancing the books.

## ICP fee inventory

“One fee” below means the canonical ICP ledger fee proved with the operation
(10,000 e8s in the candidate evidence), never a hard-coded estimate.

| Operation | Exact payer/source | Effect under Policy A | External charge? | Reimbursement eligibility under B/C |
| --- | --- | --- | --- | --- |
| Stream liquid to pooled staking account | Stream claim-backing liquid account | Reduces `B` by one fee | No | Yes |
| NNS parent split | Active pooled parent; child is credited gross minus fee | Reduces `B` by one fee | No | Yes |
| Child merge-back | Pending/passive child | Reduces `B` by one fee; parent receives child principal minus fee | No | Yes |
| Child disbursement | Dissolved child | Reduces `B` by one fee | No | Yes |
| Maturity staging to permanent staking | Permanent leg in staging | Reduces `K` by one fee | No | No |
| Maturity staging to Stream liquid | Claim leg in staging | Reduces the actual net `Q`/`B` by one fee | No | Yes |
| Jupiter staging to permanent staking | Jupiter permanent leg | Reduces `K` by one fee | No | No |
| Jupiter staging to Stream liquid backing | Jupiter claim leg | Reduces the actual net `Q`/`B` by one fee | No | Yes |
| Redemption payout | Gross liquid claim payout; user receives gross minus fee | `B` falls by the gross quote; fee is part of that payout loss | Yes, deducted from gross | No |
| Technical staging-account funding | The economic leg for which the staging balance is funded | Reduces `B` when claim-funded; reduces `K` when permanent-funded | No | Only the claim-funded case |

An incoming external Jupiter transfer fee is paid by the external sender; `Q`
is the actual received ICP and no unreceived amount enters assets. `ClaimOrRefresh`,
NNS configure calls and maturity observation have no ICP ledger fee.

IO ledger approval, transfer-from and reward-delivery fees are not ICP fees and
must never enter an ICP fee counter. An ICRC-2 approval fee is paid and burned
from the external user's IO account. Transfer-from and reward fees reduce IO
total supply; `dC` uses actual delivery and the canonical supply/reserve formula.

## Internal-fee policies considered

### A — claim backing bears exact internal fees

Every qualifying fee paid from `L`, `P`, `U` or `T` immediately reduces `B`.
The target and rate use that reduced amount. There is no debt, fee reserve,
reimbursement attribution or later convergence promise. New issuance at the
post-fee rate preserves that rate; later yield is ordinary shared yield.

This makes exact internal operations a transparent pro-rata operating cost to
claim holders. It needs no new persistent state or per-user accounting, cannot
double count a fee, has no reserve-depletion failure mode and introduces no
delayed cross-cohort transfer. Liveness requires only enough source balance for
the requested amount plus the exact fee.

### B — excluded operational fee reserve

The smallest coherent B design has two stable scalars:
`operational_fee_reserve_e8s` and `operational_fee_reserve_target_e8s`. The
reserve must physically reside inside the Stream's canonical liquid ICP account
so canonical balance is partitioned as `spendable L + excluded reserve` without
another transfer. Redemption must reserve the excluded amount before quoting
spendable liquidity.

When a qualifying fee destroys backing elsewhere, the same amount is
reclassified from excluded reserve to `L`; this restores `B` without a transfer.
NNS split, merge and disbursement fees can therefore be compensated even though
they occur outside Stream. Direct Stream fees use the same reclassification.
Permanent-neuron maturity first replenishes the reserve target; only the
remainder is split 40/60 and dynamically allocated.

The minimum target must cover the maximum reviewed sum of qualifying fees that
can be frozen across the serialized Stream operation and fixed NNS operation
slots. Depletion must use an all-or-nothing rule: if the reserve cannot cover a
whole exact fee, that entire fee falls back to Policy A rather than partially
subsidizing it or pausing redemption. This preserves liveness but makes policy
behavior state-dependent. Stable reserve accounting, payout reservation,
replenishment ordering and canonical balance partitioning add audit surface.

### C — outstanding fee-loss counter

One stable checked scalar records only exact qualifying fees that actually
reduced `B`:

```text
F = unreimbursed_internal_claim_fee_e8s

reimbursement = min(M, F)
remaining = M - reimbursement
permanent_leg = floor(remaining * 40 / 100)
new_claim_backing = reimbursement + remaining - permanent_leg
F1 = F - reimbursement
```

The new claim backing then uses the unified allocator. Permanent fees,
externally charged redemption fees and IO ledger fees never increment `F`.
The counter supports multiple fee events, checked overflow and partial
reimbursement when maturity is insufficient. It restores aggregate assets only
when future permanent-neuron maturity arrives; it does not restore historical
holder ownership.

The required counterexample is executable-tested:

```text
initial             B=100 C=100
fee                 B= 99 C=100 F=1
new issuance        B=198 C=200
reimbursement       B=199 C=200 F=0
final rate          199/200 = 0.995
```

Later holders share the reimbursement. This timing-dependent hidden value
transfer is not acceptable as the default IO fee policy. Calling it exact
historical-holder restoration would be false.

## Recommendation

Adopt **Policy A — fees are claim-holder operating costs** in the future
replacement.

It is economically correct without pretending that destroyed assets still
exist, adds no persistent state, requires no per-user attribution, cannot
double count, has no reimbursement timing transfer, preserves liveness without
a reserve and is directly auditable from exact ledger/governance proofs. Its
explicit consequence is that internal claim-backing fees permanently reduce
the claim rate by their exact pro-rata amount. That is preferable to Policy B's
state-dependent subsidy or Policy C's cross-cohort transfer.

## Deterministic anti-oscillation rule

Before freezing a direction-changing operation under Policy A:

```text
current_raw_target = floor(A_backing * B / C)
Bfee = B - exact_total_fee_of_next_operation
post_fee_raw_target = floor(A_backing * Bfee / C)
```

- With no parent, `A_backing=0` never creates one. If `current_raw_target` is below the
  canonical minimum parent principal, return typed
  `Hold(BelowMinimumStake)` and keep all backing liquid. If the pre-fee raw
  target meets the minimum, lazily create the parent from liquid. Its
  planned post-fee target is at least the minimum, and the frozen source debit
  is exact credited principal plus the transfer fee.
- With an existing parent, use
  `planned_target=max(post_fee_raw_target, minimum_parent_principal)`. A later
  target below minimum therefore retains the minimum parent rather than
  creating and destroying it across observations.
- Under target: if `planned_target - P <= next_operation_fee`, return typed
  `Hold(FeeTolerance)`. Otherwise freeze a top-up whose credited amount is that
  delta and whose source debit is credited amount plus fee.
- Over target: hold unless `P - planned_target` is at least the exact minimum
  child gross (`minimum_stake + split_fee`). Otherwise freeze that gross
  unwind, expecting gross minus fee in `U`.
- A required minimum parent may leave bounded over-target principal. An
  unexecutable child delta leaves at most `minimum_child_gross - 1` tolerance.
- After canonical proof, recompute from actual credited amounts. Do not reverse
  direction for a residual no larger than the next direction's total fee.

Because the frozen target already includes the operation's fee loss, an exact
completion lands on that post-fee target rather than causing `top up -> fee ->
unwind`. Minimum stake and the existing-parent residual are the only additional
bounded tolerances. No generic hysteresis, cooldown, lease or queue is needed.

## Bounded passive unwind cohorts and delayed reward re-entry

SNS dissolve cancellation changes reward eligibility and later allocation; it
does not reverse a committed NNS lifecycle. Before a split intent is submitted,
the existing canonical reconciliation cadence nets start/cancel observations
into one latest target delta. A cancellation can therefore remove an
`ExitObserved` state without an NNS call or fee. No new scheduler is required.

Submission of the exact split transfer intent is the split commit point. It
freezes the aggregate gross amount from `P` into `T` without assigning any
amount to a member neuron. The single active NNS slot then progresses through:

```text
SplitCommitted { generation, gross }
SplitProved { generation, child_neuron_id, principal }
StartDissolvingCommitted { generation, child_neuron_id, principal }
```

Canonical split proof moves the credited child principal from `T` into `U` and
charges the exact split fee once, but it does **not** create a passive cohort or
release the active slot. The child is initially non-dissolving with the
inherited exact 14-day delay; the dissolve clock has not begun.
`StartDissolving` is a separate command. Only a canonical Governance
observation of `WhenDissolvedTimestampSeconds(ready_at)` creates:

```text
PassiveCohort {
  generation,
  child_neuron_id,
  principal,
  ready_at,
  lifecycle/proof_state
}
```

The effective start is derived as `ready_at - 1,209,600`; it is never supplied
by a caller. A rejected `StartDissolving` leaves `SplitProved` available for a
retry. Callback loss leaves `StartDissolvingCommitted` until canonical
Governance state proves the exact child is dissolving. The active slot is
released only by that proof.

The child ID belongs only to the active aggregate operation and live aggregate
cohort. Per-neuron state carries only a generation marker; there is no
user-to-child map or per-user principal attribution. One aggregate generation
may include several `ExitObserved` SNS neurons. One NNS command remains active
at a time, while proved cohorts are passive and multiple generations may
dissolve concurrently. Each child completes relative to its own canonical
StartDissolving time and returns principal to global `L`, even if any member
cancels dissolve. Cancellation does not stop, merge, or duplicate the child.

The minimum conceptual states and transitions are:

| State | Canonical transition | Reward eligibility / NNS effect |
| --- | --- | --- |
| `ActiveBacked` | First dissolving observation -> `ExitObserved` | Ineligible immediately; no retroactive reward |
| `ExitObserved` | Active before split commit -> `ActiveBacked` | No NNS effect or fee; eligible from the next canonical reward observation |
| `ExitObserved` | Split intent submitted -> `ExitCommitted(generation)` | Gross `P -> T`; unwind becomes sticky |
| `ExitCommitted` | Active observation -> `ReentryPending` | Child continues; no merge; remains ineligible |
| `ReentryPending` | Dissolving observation -> `ExitCommitted` | Latest state changes only; no second child or transfer |
| `ExitCommitted` or `ReentryPending` | Child disbursement proof -> `LiquidReturned` | Net principal `U -> L`; exact disbursement fee once |
| `LiquidReturned` | Latest state active and executable target delta -> `RestakePlanned` | No external transfer submitted; plan may be discarded |
| `RestakePlanned` | Exact transfer intent submitted -> `RestakeCommitted` | Exact liquid debit is frozen in `T`; cannot be discarded |
| `RestakeCommitted` | Exact cached-principal credit proved -> `RestakeProved` | `T -> P`, fee counted once even after callback loss |
| `RestakeProved` | Latest state active -> `ActiveBacked` | Eligible prospectively from the next canonical reward observation |
| `RestakeProved` | Latest state dissolving -> `ExitObserved` | Restake remains counted; later net reconciliation may commit a new unwind generation |
| `RestakeProved` | Latest state liquid or dissolved -> inactive liquid-exit status | Restake remains counted; no reward eligibility |
| Completed exit reference | Canonical `LiquidOrDissolved` | Remains ineligible; clear the completed generation marker, and treat later staking as fresh structural activation |

If the latest state is dissolving or `LiquidOrDissolved` at or before
returned-liquidity planning, the value remains in `L`. Any valid active member
leaving the plan's active subset before submission invalidates the global
target snapshot, discards `RestakePlanned`, and returns every still-active
planned member to returned-liquidity planning. Clearing the last valid active
member therefore cannot leave a plan behind. A fresh plan may be calculated
for the remaining active subset. `commit_restake` requires the exact still-current
planned generation, its live returned cohort, and at least one current active
generation member. Production should preferably calculate this no-effect plan
and persist the exact `RestakeCommitted` transfer intent in the same update,
after validating the canonical snapshot. It should not create durable plan
history. The test-only `RestakePlanned` state remains useful for transition
tests, but it is neither an economic asset nor an irreversible operation.

After `RestakeCommitted`, an observed or possibly effective transfer must be
finished and proved exactly. Neither dissolving nor `LiquidOrDissolved` can
erase `T`, the committed generation, exact operation relationship, transfer
intent, or fee. The observation updates only the latest SNS state and keeps
reward eligibility disabled. Exact proof moves the credit to `P` once while
retaining that relationship through `RestakeProved`; `finish_restake` then
applies the latest state. Active becomes `ActiveBacked` prospectively,
dissolving becomes `ExitObserved`, and liquid or dissolved becomes an inactive
liquid-exit status. The credited pooled principal remains counted in every
case, and later ordinary net reconciliation may unwind it when the latest state
is not active. Callback loss therefore cannot double-submit or double-count the
restake. Repeated start/cancel observations for the same committed generation
change only latest state and bounded eligibility status; they do not create a
second child for that generation.

Aggregate returned-liquidity planning processes the active and dissolving
subsets independently. An active member may complete re-entry while another
member of the same generation remains dissolving. A dissolving member remains
ineligible and does not block the active subset. A re-dissolve or liquidation
before `RestakeCommitted` cancels the plan for the whole stale snapshot; after
commit the exact transfer must be proved and counted once, and the new canonical
state is handled by the next reconciliation.

Reward eligibility uses the smallest bounded representation available in the
future state design: a status plus an `eligible_from` canonical observation or
cohort/generation marker in the existing per-neuron record. It must not store
an event history. A pre-commit cancellation can set eligibility to the next
canonical reward observation. A post-commit cancellation remains ineligible
until all of the following are canonically proved:

1. the child principal returned to liquid;
2. the latest SNS state is still non-dissolving;
3. aggregate reconciliation has `P>=target`; and
4. any required exact pooled-principal increase was proved.

When no increase is required because canonical aggregate principal equals or
exceeds the target, that canonical observation satisfies the fourth condition.
An over-target pool is sufficient backing and must not permanently block
re-entry. When `P<target`, the affected generation remains ineligible until the
exact pooled credit is proved. Fee and minimum tolerances permit operational
batching but do not restore reward eligibility. Eligibility resumes only for
future reward observations; there are no retroactive rewards.

### Returned liquidity is globally reallocated

Child disbursement never implies restaking the child's original amount. The
planner recomputes `B`, `C`, `A_backing` and the target from the latest `L`, `P`, `U`
and `T`. Pending unwind and transit remain in `B`, but neither is active pooled
principal. If the latest SNS state is active, only the post-fee target delta is
eligible for restaking; if it is dissolving, all returned value stays liquid.

The executable example starts with `L=500, P=500, B=C=1,000`. A gross split of
110 with fee 10 produces `P=390, U=100, B=990`. Disbursement with fee 10
produces `L=590, P=390, B=980`. With latest `A_backing=500`, a prospective restake fee
of 10 gives `Bfee=970` and target 485. The exact restake is therefore credited
95 from a 105 liquid debit, producing `L=P=485, B=970`. The original child
principal of 100 is not automatically restaked. Eligibility resumes only after
the 95 credit is proved.

This remains aggregate when cohorts differ. The executable overlap has cohort A
committed on day 0 and cohort B on day 1 while A is still dissolving. Both
children coexist, become ready from their separately proved canonical clocks,
and return to shared `L`. Cancellation in A does not reverse A.
Cancel/re-dissolve in B does not create another B child. Disbursing A neither
erases nor delays B.

The aggregate membership fixture commits three SNS neurons in one generation
and one child. A stays dissolving, B cancels, and C cancels then dissolves
again. All three store the same generation and no principal amount. On return,
B completes re-entry independently; A and C stay ineligible without blocking
B. C's re-dissolve cancels an unsubmitted restake plan without another split.
`LiquidOrDissolved` clears A and C's completed generation references while
keeping them ineligible; later staking is a fresh structural activation. No
ICP child, e8s range, or transfer is assigned to a user or SNS neuron.

### Cohort retirement

The bounded collection counts live unresolved child lifecycles, not history.
Cohort identity is required only while that NNS child lifecycle remains
unresolved. Once principal return is canonically proved, child maturity has
been moved to the parent or proved zero, and child cleanup is complete, the
child record must be independently retireable. Members still awaiting reward
re-entry move to a bounded generation-free status. Pending re-entry must not
keep a resolved child slot occupied indefinitely, and later eligibility
restoration is based on the global reward-coverage invariant rather than
retention of a historical child generation.

This decoupling is a requirement for the later production representation, not
an instruction to enlarge the transition-test simulator in this proposal.
Retirement changes neither `B`, `P`, `U`, `T` nor reward eligibility. The freed
live slot may be reused by a later monotonically increasing generation. Live
child IDs and unresolved generations must be unique. The final production
capacity remains a reviewed bound, not a unit-fixture constant.

### Liquidity-lag bound

A future guaranteed bound must use non-overlapping reviewed terms:

```text
maximum_unresolved_cohort_lifetime
  = maximum_detection_reconciliation_interval
  + maximum_split_and_start_command_margin
  + 14_day_NNS_delay
  + maximum_readiness_to_disbursement_margin
  + maximum_maturity_cleanup_and_reference_clear_margin

max_live_cohorts
  >= ceil(maximum_unresolved_cohort_lifetime
          / minimum_spacing_between_committed_cohort_generations)
     + reviewed_operational_margin
```

The maximum detection/reconciliation interval determines the first liquidity
lag term. It is distinct from the minimum spacing between committed cohort
generations, equivalently the maximum cohort-creation rate, which is the
capacity denominator. The eventual production design must enforce at most one
committed aggregate cohort per canonical reconciliation generation. Neither
quantity selects the final capacity.

The existing Stream reward mechanism is the smallest reusable cadence source:
one durable `reward_work_due` flag and one daily one-shot timer, whose scheduled
deadline is the last event end plus 86,400 seconds plus the 300-second
observation margin. Retryable pending/ledger/busy work schedules a 60-second
retry. Cohort reconciliation can share that durable due checkpoint; frontend
and permissionless calls remain additional hints, not cadence authority. This
adds no general scheduler.

That mechanism does not yet prove a maximum detection/reconciliation interval:
reviewed pause, unavailable dependencies, repeated retryable failures, and
out-of-cycles execution can defer completion without a protocol bound. The
executable 18-day calculation and two-cohort overlap remain fixtures only.
Until that maximum interval, the minimum committed-generation spacing, and
every lifetime margin above are bounded, neither an 18-day liquidity guarantee
nor a production cohort capacity is established.

### Sticky fees and anti-churn bound

Under selected Policy A, split proof, child disbursement and a later restake
each reduce `B` by their exact fee once. The default sticky path never pays a
merge fee. Under Policy B the same qualifying losses would consume excluded
reserve once, and under Policy C they would increment the loss counter once;
neither alternative changes the lifecycle rule. A fee is never both applied to
backing and re-applied merely because eligibility changes.

The same post-fee planner and tolerance apply after disbursement. A required
credit no larger than the next operation's total fee, or smaller than the
applicable minimum stake for the operation shape, remains batched. Canonical
proof then recomputes from actual credited amounts. This prevents both fee
oscillation and cancellation-driven direction reversal.

For a normalized lifecycle with three cancel/start pairs and 10-unit fees,
immediate mirroring incurs four splits, three merges and one disbursement: 80.
Each sticky generation incurs one split, one disbursement and at most one later
restake: 30. For any number of flips during that committed generation, its
bound remains those three fee-bearing operations; flips cannot add another
child to the generation. A later independently committed target delta may use
the next bounded generation.

### Deferred shard optimization

A possible later optimization could stop a wholly canceled child, restore its
exact 14-day dissolve delay and retain it as active pooled principal without a
merge. This is not the default launch design. It requires a bounded
multi-active-shard model, shard-level target accounting and reviewed reward
eligibility rules, which add state and proof surface without being necessary
for launch correctness.

### Controlled child maturity and cleanup result

Unminted child maturity is never backing. The maintained exact-candidate
PocketIC test now proves the missing lifecycle against the locked Governance
Wasm. Before three splits the parent held ordinary maturity
`3,847,044,094,926,134` and staked maturity `2,564,696,063,284,088` e8s; parent
plus children conserved each total exactly. Selected child
`17,951,363,335,400,986,306` inherited `76,944,727` ordinary and `51,296,484`
staked-maturity e8s.

Split returned that exact child ID at timestamp `1,631,608,226`. Immediately
afterward it was non-dissolving with `DissolveDelaySeconds(1,209,600)`, proving
the clock had not begun. The separate StartDissolving command was deliberately
delayed to `1,631,608,263`; Governance reported canonical readiness
`1,632,817,863`, exactly `1,209,600` seconds later. A split-derived readiness
would have been 37 seconds early.

The child voted before its separately delayed StartDissolving command. A later
reward event settled while it was dissolving and increased ordinary maturity
to `82,914,450` e8s. At readiness, Governance converted all `51,296,484`
staked-maturity e8s to ordinary. Principal disbursement left the retained child
with zero cached stake, `134,210,934` ordinary maturity, zero staked maturity,
and its canonical dissolved timestamp state. The principal ledger transfer
charged the ordinary 10,000-e8s disbursement fee.

Direct zero-principal merge was rejected because both neurons must be
non-dissolving with positive delay. `StopDissolving` was not sufficient for the
already dissolved child; increasing its delay by one second made it eligible.
Merge then moved all `134,210,934` ordinary maturity to the pooled parent,
left the retained source child empty/finalizable, created no ICP ledger block,
and incurred zero ICP fee. This merge cleanup is selected over maturity
disbursement because it uses no Mint, staging route, or monetary transfer.
Production needs only bounded cohort proof states for principal return,
maturity handled, and cleanup complete; it does not need bespoke child-reward
policy or user attribution.

## Redemption implication

Redemption first applies the same strict economic-state validation as claim-rate
and allocation. `B=0,C>0` returns typed `UncoveredClaims` for both zero and
nonzero payout fees; it never returns a zero-valued ready quote. `B>0,C=0`
reports backing without claims, and `B=C=0` reports empty genesis. For a valid
solvent state, the quote is `floor(user_io * B / C)`. Immediate availability
uses only spendable `L`; `P`, `U` and `T` support solvency but cannot fund an
immediate payout. An excluded operational reserve, if Policy B were selected,
would be neither `B` nor spendable `L`. The user's IO must not be pulled until
spendable `L` covers the gross quote. Illiquidity never creates a lower exchange
rate and this proposal introduces no queue.

## Bounded state and complexity

These are functional requirements, not simplicity-budget changes. All fee
policies share replacement of the old path with one allocator, reuse of the
single active NNS command slot, and one bounded passive cohort collection.

| Policy | New fee-policy scalars | Passive collections | Active operation | Public/API estimate | Production Rust delta |
| --- | ---: | ---: | --- | --- | --- |
| A | 0 | 1 bounded cohort collection | Existing single slot, extended for exact commit/proof values | Method count may remain stable; proof/status Candid types will change | Unknown until the replacement diff exists |
| B | 2 reserve scalars | Same one collection | Same plus reserve-consumption proof | Method count may remain stable; install/config and proof/status types will change | Unknown until the replacement diff exists |
| C | 1 fee-loss scalar | Same one collection | Same plus reimbursement proof | Method count may remain stable; proof/status types will change | Unknown until the replacement diff exists |

Policy A remains selected because it adds no fee-policy scalar. That does not
remove the functionally required passive cohort collection. Reuse the existing
canonical reward-observation checkpoint as the deterministic generation source;
do not add a scheduler counter. An existing per-neuron status may carry one
bounded generation marker. Conceptual `T` remains the exact amount in the
single active operation, while proved dissolving/ready cohorts live in the
bounded passive collection. There is no user-to-child map or event log.

The production replacement uses strict launch states, bounded collections, and
narrow proof-oriented Candid methods. This ADR authorizes no increase or
recalibration of the production simplicity budget.

The implementation must be a replacement with no feature flag and no prelaunch
migration:

- delete the liquid-only target;
- delete the separate two-week endowment and external `UnderTarget` funding;
- delete the liquid two-week receipt representation;
- delete obsolete source-specific 40/60 destination choices;
- add one unified checked allocator and post-fee planner;
- reuse existing exact transfer intents, one active operation and fixed NNS
  mechanics.

The controlled evidence above resolves the child-maturity accounting and
realization route. Production uses the canonical daily reward checkpoint as
the reconciliation generation, permits at most one committed cohort per
generation, and bounds live cohorts at 32. Release artifacts, corrected local
SNS evidence, final production memo/followee values, and deployment remain
separate reviewed work.
