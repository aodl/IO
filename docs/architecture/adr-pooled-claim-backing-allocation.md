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
route, not by pretending the destination split is fee-free:

1. The permanent gross leg is debited from staging once and its destination is
   credited by `permanent_gross - permanent_transfer_fee`.
2. Evaluate the claim leg with its unavoidable first staging-transfer fee.
   If the resulting allocation is all liquid, transfer staging to liquid once.
   If it is all pool, transfer staging to the pooled staking account once.
3. Only a genuinely mixed result first transfers staging to liquid, then uses
   ordinary liquid-to-pool reconciliation. Recalculate `B` and the target with
   the second exact fee before fixing that pool credit.
4. The second-fee floor can erase the pool remainder or consume the liquid
   remainder. In those two boundary cases choose the direct one-fee route and
   expose the at-most-floor residual rather than oscillating between fee-count
   assumptions.

The executable planner evaluates at most the one-fee and two-fee candidates.
Its result records the permanent source debit and credit, claim staging debit,
first claim credit, optional liquid-to-pool source debit, both destination
credits, exact fees, and post-fee `B`, `K` and target. The selected route's
reported target is always recalculated using that route's actual fee count.
Candidate fixtures cover boundaries where changing from one fee to two changes
the target floor; the planner terminates after at most two evaluations.

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

Submission of the exact split transfer intent is the commit point. It freezes
the gross amount from `P` into `T` without charging a fee. Canonical split proof
moves the credited child principal from `T` into `U`, charges the exact split
fee once, and creates one bounded passive aggregate cohort. A cohort contains
only its deterministic generation, aggregate principal, lifecycle/proof state,
and ready-at time. There is no user-to-child ownership map, child ID in a
per-neuron record, or per-user principal attribution. A per-neuron sticky status
may reference only the committed generation.

One NNS command remains active at a time, but already-proved cohorts are passive
and multiple generations may dissolve concurrently. From each commit point its
child completes the 14-day dissolve relative to that cohort's own start time and
disburses to global `L`, even if the referenced SNS neuron cancels dissolve.
Cancellation does not stop or merge a child.

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

If the latest state is dissolving at or before returned-liquidity planning, the
value remains in `L`. `RestakePlanned` may be discarded if a new dissolving
observation arrives before submission. After `RestakeCommitted`, an observed or
possibly effective transfer must be finished and proved exactly; dissolving
cannot erase `T`, the transfer intent, or its fee. After proof, ordinary net
reconciliation may create a new unwind generation if the latest state still
requires one. Callback loss therefore cannot double-submit or double-count the
restake. Repeated start/cancel observations for the same committed generation
change only latest state and bounded eligibility status; they do not create a
second child for that generation.

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
children coexist, become ready relative to their own start times, and return to
shared `L`. Cancellation in A does not reverse A. Cancel/re-dissolve in B does
not create another B child. Disbursing A neither erases nor delays B. A mixed
eligibility scenario retains one dissolving generation while a canceled,
returned generation sees partial target restoration and remains ineligible;
only later `P>=target` restores that generation prospectively. No ICP child,
e8s range or transfer is assigned to a user or SNS neuron.

### Liquidity-lag bound

A future guaranteed bound must be derived, not selected arbitrarily:

```text
liquidity_lag_bound
  = guaranteed_reconciliation_cadence
  + 14_day_NNS_delay
  + maximum_reviewed_detection_margin
  + maximum_reviewed_command_margin
  + maximum_reviewed_disbursement_margin
```

Using candidate fixtures of one day for each cadence/margin term produces an
18-day candidate bound: `1 + 14 + 1 + 1 + 1`. These are test values, not
hard-coded proposed production economics. The executable overlap needs capacity
for two cohorts; it accepts that capacity as a fixture rather than asserting a
production collection limit. The active design does not yet prove a guaranteed
reconciliation cadence, so both the production collection capacity and the
liquidity-lag guarantee remain unresolved. Production review must establish
that cadence and each maximum margin before adopting either bound.

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

### Child maturity is a production blocker

Unminted child maturity is not backing. Production NNS implementation of this
replacement cannot begin until controlled evidence determines:

- whether a dissolving 14-day child receives later reward maturity;
- what maturity remains after principal disbursement; and
- how that maturity is ultimately realised without loss or double counting.

This ADR intentionally does not design production child-maturity state. The
future proof must identify the canonical observation and realisation path before
the passive cohort record can be finalized.

## Redemption implication

The redemption quote is always `floor(user_io * B / C)`. Immediate availability
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
orchestration remain future work. No production implementation should begin
until the pure-model treatment has been independently reviewed, a guaranteed
reconciliation cadence and maximum lag margins have been established, and the
controlled child-maturity evidence above resolves its accounting and
realisation path.
