# ADR: unified pooled claim-backing allocation

## Status

Proposed — implementation pending.

This proposal does not supersede the accepted separate-endowment ADR, change
the active NNS pin, or authorize deployment. Production code continues to
implement the accepted pre-replacement model until a later reviewed tranche
deletes and replaces it.

## Scope and evidence boundary

The upstream mechanics are preserved separately by commit
`eceb406604d51ed7d4730bdce3581f73f4c65121` and the
[post-Mission-70 candidate evidence](../testing/post-mission70-nns-candidate.md).
This ADR does not repeat that investigation or activate its candidate. It
defines only proposed pure economics and a bounded replacement plan.

## Proposed accounting model

```text
C = total_io_supply
  - protocol_reserve_io
  - explicitly_nonredeemable_governance_io

B = L + P + U + T

L = liquid claim backing
P = active pooled two-week NNS principal
U = pending two-week unwind principal
T = exact claim backing frozen in an in-transit operation
K = permanent/max-delay protocol principal, excluded from B
A_backing = all structurally active claim-bearing ordinary staked IO
A_reward = the subset currently eligible for reward allocation

claim_rate = B / C
```

Operational fee reserves, if such a policy were selected, are excluded from
`B`. Maturity is not an asset and enters neither `B` nor `K` until Governance
produces an actual canonical ICP Mint. These categories are accounting
identities; they do not each require an independent stable field. All products,
sums, subtractions and floors use checked `u128` integer arithmetic.

The proposed replacement has no separately endowed reward-backing capital.
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

## Unified post-event allocation

Every event supplies actual values, not source-based destination percentages:

```text
Q  = actual net claim-backing increment
dC = actual change in claim-bearing IO supply
dA = actual change in structurally active staked IO

B1 = B0 + Q
C1 = C0 + dC
A_backing1 = A_backing0 + dA

target1 = floor(A_backing1 * B1 / C1)
pool_need = max(0, target1 - P0)
to_pool = min(Q, pool_need)
to_liquid = Q - to_pool

remaining_under_target = max(0, target1 - (P0 + to_pool))
resulting_over_target = max(0, (P0 + to_pool) - target1)
```

`U` and `T` remain in `B` but are not `P`. Allocation does not move them into
the active pool. The event result contains post-event `B`, `C`, `A_backing`,
target, both destinations and both residuals. Both `A_backing0<=C0` and
`A_backing1<=C1` are mandatory.

### Jupiter inflow

Ignoring fees and floors, Jupiter contributes `Q = 60%` of actual source ICP
to claim backing and releases `dC = Q / pre_event_rate` reserve IO to a liquid
recipient; `dA = 0`. When the pool starts on target, preserving `B/C` while
`A_backing` is unchanged preserves the target exactly, so all `Q` remains
liquid. Jupiter's separate permanent contribution is not a reason to top up the
pool.

Examples from the executable model:

- Empty genesis: `Q=60, dC=60` produces `B=C=60`, target zero, all liquid.
- Rate 1: `B=C=1,000, A_backing=P=250, Q=dC=600` preserves target 250.
- Rate 2: `B=2,000, C=1,000, A_backing=250, P=500, Q=600, dC=300` preserves target
  500.
- With `B=3, C=2, A_backing=P=1, Q=1`, floor delivery is zero, the rate appreciates,
  and the one-unit target delta goes to the pool. This is rounding, not source
  provenance.

An IO reserve transfer fee is an IO supply burn, not an ICP backing fee. If 60
IO is delivered and one IO is burned, the reserve debit is 61, total supply
falls by one, and `C` increases by the actual delivered 60. Nominal reserve
debit must not be used as `dC`.

### Pooled two-week maturity

For actual canonical Mint `M`:

```text
permanent_leg = floor(M * 40 / 100)
Q = M - permanent_leg - exact_claim_reducing_fees
dC = actual_delivered_IO
dA = actual_delivered_IO
```

At `B=C=1,000, A_backing=P=500`, a Mint of 1,000 gives `K += 400` and `Q=600`.
Delivering 600 IO gives `B=C=1,600`, `A_backing=1,100`, target 1,100 and allocates all
600 to the pool without diluting the rate. With only 300 IO actually delivered,
the target-derived result is 484 to the pool and 116 liquid. With no delivery
because the reward entitlement is fully forfeited, the result is 300 pool and
300 liquid. A one-e8s claim-reducing fee makes `Q=599`; actual delivered IO,
not nominal reserve debit, remains authoritative.

Therefore “60% always goes to the pooled neuron” is not a rule. Partial
distribution, forfeiture, fees and floors can make part of the claim leg
liquid. Allocating from the post-event target avoids an unnecessary later
split/unwind and its fee.

### Permanent/max-delay-neuron maturity

Initially ignoring fee subsidy:

```text
permanent_leg = floor(M * 40 / 100)
Q = M - permanent_leg
dC = 0
dA = 0
```

The rate appreciates. If the pool was on target, the pool receives the exact
target delta, approximately `floor(Q * A_backing / C)`, and the rest remains liquid.
For `B=C=1,000` and `Q=600`, the tested `A_backing/C` cases are:

| A_backing/C | Pre-event P | To pool | To liquid |
| ---: | ---: | ---: | ---: |
| 0% | 0 | 0 | 600 |
| 25% | 250 | 150 | 450 |
| 50% | 500 | 300 | 300 |
| 100% | 1,000 | 600 | 0 |

If `P=100` while the pre-event target is 500, all 600 goes to the pool and 100
remains under target. If `P=900`, the post-event target is 800, all 600 remains
liquid, and ordinary reconciliation later unwinds the 100 excess. Pending
unwind and transit remain in total backing when calculating the target but do
not become active pooled principal. Bounded integer loops test rounding across
small values.

Permanent-neuron yield needs no source-provenance attribution. The same target
rule determines its destination.

### Fee-aware physical claim-leg route

Dynamic allocation is executed through the smallest deterministic physical
route, not by pretending the destination split is fee-free. The planner takes
the canonical parent-existence observation and exact minimum parent-creation
credit together with the exact observed transfer fees:

1. The permanent gross leg is debited from staging once and its destination is
   credited by `permanent_gross - permanent_transfer_fee`.
2. Evaluate the claim leg with its unavoidable first staging-transfer fee. Use
   one staging-to-liquid transfer when the target needs no pooled credit, when
   the two-fee candidate's net pooled credit is no larger than the second exact
   fee, when the optional second fee cannot be paid, or when the parent is
   absent and that net credit is below the exact creation minimum. Record the
   remaining under-target delta for batching. Failure to afford the optional
   second fee is an `AllLiquid` decision, not a global planning error.
3. Use one staging-to-pool transfer only when the post-one-fee target consumes
   the entire claim credit, or when the two-fee candidate has zero liquid
   credit, and either the parent exists or the exact one-fee credit satisfies
   the minimum valid parent-creation amount. In the latter case, credit the
   entire one-fee claim amount, report the one-fee target and bounded
   over-target residual, and do not spend the optional second fee.
4. Use staging-to-liquid followed by liquid-to-pool only when the one-fee
   target is mixed, both recalculated destination credits are positive, the
   pooled credit after the second exact fee is strictly greater than that fee,
   and the credit is executable for the existing parent or the lazy-creation
   minimum. Otherwise select the applicable direct one-fee route.

The executable planner evaluates at most the one-fee and two-fee candidates.
Its result records the parent-aware decision, permanent source debit and credit,
claim staging debit,
first claim credit, optional liquid-to-pool source debit, both destination
credits, exact fees, and post-fee `B`, `K` and target. The selected route's
reported target is always recalculated using that route's actual fee count.
Candidate fixtures cover boundaries where changing from one fee to two changes
the target floor; the planner terminates after at most two evaluations.

The executable sub-fee counterexample uses `B=C=1,000,000,000`,
`A_backing=500,000,000`, `P=529,989,000`, actual Mint `100,000,000`, and an
exact claim-transfer fee of `10,000`, with no `dC` or `dA`. The two-fee
candidate would credit only `1,000` to the pool while spending that second
`10,000` fee. The selected one-fee route instead credits `59,990,000` to
liquid, spends no second fee, and records the `6,000` under-target residual for
later batching.

The all-pool boundary fixture uses `B=C=100,000,000,000`,
`A_backing=50,000,000,000`, `P=49,970,010,000`, actual Mint `100,000,000`, an
exact claim-transfer fee of `10,000`, and no `dC` or `dA`. The two-fee candidate
has zero liquid credit, so the selected route is direct one-fee `AllPool`: it
credits `59,990,000`, reports the one-fee target `50,029,995,000` and bounded
`5,000` over-target residual, performs no liquid-to-pool transfer, and retains
`10,000` more total claim backing than the two-fee alternative.

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
| Maturity staging to pooled staking | Claim leg in staging | Reduces `B` by one fee | No | Yes |
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

## Bounded future state and complexity

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

The exact stable-state, line-count and deletion/net delta is unknown until a
reviewed production replacement diff exists. Public method count may remain
stable, but cohort/restake proof and status Candid types will change. This
proposed ADR authorizes no increase or recalibration of the production
simplicity budget.

The implementation must be a replacement with no feature flag and no prelaunch
migration:

- delete the liquid-only target;
- delete the separate two-week endowment and external `UnderTarget` funding;
- delete the liquid two-week receipt representation;
- delete obsolete source-specific 40/60 destination choices;
- add one unified checked allocator and post-fee planner;
- reuse existing exact transfer intents, one active operation and fixed NNS
  mechanics.

Active-pin selection, stable-state edits, public interfaces and monetary
orchestration remain future work. The controlled evidence above resolves the
child-maturity accounting and realization route. No production implementation
should begin until the pure-model treatment has been independently reviewed
and a maximum detection/reconciliation interval, minimum spacing between
committed cohort generations, every other maximum lifetime margin, and a
derived production cohort capacity have been established.
