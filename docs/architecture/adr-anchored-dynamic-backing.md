# ADR: Anchored dynamic backing

- Status: accepted Tranche-A replacement architecture
- Date: 2026-08-30
- Source baseline: `b0f85cfd87b7e43989346f8780bb02a7f197526d`

## Authoritative invariants

These invariants govern every implementation tranche and are executable in
the test-only `io-proposed-economics-tests` model.

1. **Claim floor.** At every reachable normal Ready state, exact same-e8
   arithmetic proves `B / C >= 1 ICP / IO`, equivalently `B >= C` for `C > 0`.
2. **Claim-rate monotonicity.** Consecutive canonical checkpoints satisfy
   `B_after / C_after >= B_before / C_before`. Exact transit remains in `T`;
   an effect that lacks fee capacity stops before its irreversible boundary.
3. **Anchor isolation.** The replenishable entitlement is exactly 10 ICP.
   Anchor and unsolicited surplus are outside `B`; neither issues IO nor
   enlarges the entitlement.
4. **Fee conservation.** A fee paid to relocate already-existing claim backing
   consumes excluded anchor capacity exactly once. A fee paid to deliver fresh
   value reduces that fresh net credit and creates no reimbursement entitlement.
   An anchor-restoration fee is paid from fresh TwoYear maturity without
   recursive debt. Redemption uses its frozen gross/net quote, and external
   fees create no protocol debt. Replay cannot charge a fee twice.
5. **Synchronization with economic fail-safe.** Healthy scheduling targets
   liquid claim backing before SNS unlock. Arbitrary distributed liveness is
   not assigned a finite bound. Delayed backing stays exactly in `B`, and an
   exact pushed redemption becomes a durable at-most-once payout obligation.
6. **Push-redemption solvency.** Preparation creates no debt. A timely exact
   push creates an unconditional durable obligation which survives expiry,
   restart, and upgrade; monotone claim rate keeps its frozen quote solvent.
7. **Dust tolerance.** Unsolicited ICP cannot block bootstrap, alter `B` or
   `C`, enlarge anchor entitlement, issue IO, or prove an expected transfer.
8. **Exact partition.** At stable boundaries, physical Dynamic principal is
   exactly claim principal plus excluded anchor plus excluded surplus. Global
   claim backing is exactly `B = L + P + U + T`; positive residual is excluded
   surplus and negative residual fails closed.

## Decision scope and supersession

This ADR is the replacement authority for the pre-launch IO backing design. It
supersedes the following decisions wherever current normative documents still
describe them as frozen:

- Policy-A erosion of claim backing by internal ICP fees;
- claim-funded lazy creation of the pooled NNS parent;
- one shared exact-14-day constant for SNS eligibility and NNS dissolve delay;
- `MAX_LIVE_UNWIND_COHORTS = 32` and normal `CapacityPending` behavior; and
- ICRC-2 allowance plus `transfer_from` redemption.

Historical ADRs, release artifacts, and evidence packages remain immutable
records for their own source. They do not define this replacement.

## Current implementation inventory

| Area | Current owner | Replacement disposition |
| --- | --- | --- |
| Pooled-parent bootstrap and claim observation | NNS `api`, `pool_flow`, `lifecycle`, `claim_assets`; `io-nns-types::backing` | Replace optional/lazy bootstrap with a mandatory pre-Ready Dynamic-neuron bootstrap. Retain exact Governance observation and replay machinery. |
| Target and reconciliation economics | `io-core-model::reconcile`; Stream `pool_reconciliation`; NNS pool permits | Replace fee-eroding target planning and the parent-minimum claim floor with claim-principal planning plus explicit anchor capacity. |
| ICP fees | Core reconcile, Jupiter, maturity, unwind, and transfer phases | Replace implicit source-bucket erosion with the closed fee classification below. Retain immutable intents and canonical effect proof. |
| Permanent-neuron credit | NNS `permanent_credit`, Jupiter, maturity | Retain exact neuron/subaccount and cached-stake proof. Fresh permanent delivery is credited net of its fee and creates no liability. |
| TwoWeek maturity | NNS maturity state/flow and Stream paired receipt | Retain semantic staging and paired issuance. Both fresh delivery legs are credited net of their own fee; paired IO uses the net claim credit. |
| TwoYear maturity | NNS maturity state/flow | Restore only the Dynamic-anchor deficit from fresh capture, including its transfer fee, then apply ordinary gross 40/60 to the valid remainder. Retain no-IO issuance and semantic capture. |
| Split, child, and disbursement | NNS `unwind_flow`; `io-nns-types::{pool,backing}` | Retain sticky exact commands/proofs. Consume Split and committed future-disbursement fee capacity once at the sticky boundary. |
| SNS timing/eligibility | Separate core-model constants; governance/reward boundaries; Stream scheduler | NNS delay is 1,209,600 seconds and SNS user delay is 1,296,060 seconds after the 12-hour timing proof. |
| Cohort lifecycle | NNS stable state/API; `MAX_LIVE_UNWIND_COHORTS`; `CapacityPending` | Delete the product cap/variant. Retain one aggregate child per generation and prioritize ready-child service before another Split. |
| Redemption | Stream state/API/redemption; ledger boundary/types | Replace allowance, pull intent, and `transfer_from` with prepared ICRC-1 push proof and a durable payout obligation. Retain bounded caller nonce/replay. |
| Frontend redemption | `frontend/web/src/app/redemption.js` and redemption UI/tests | Replace approval/allowance UX with prepare, explicit push, block settlement, and resume. |
| Historian/status | Historian raw adapters/model/DID and frontend projection | Retain layered observation; add anchor partition, deficits, push obligation, and cohort-priority representations only where operationally necessary. |
| Stable schemas | Stream marker 10; NNS marker 13; strict launch fixtures | The replacement encoded shapes reject markers 9/12 without migration. |
| Bootstrap/rehearsal tooling | install args, production wiring, local SNS runbook/evidence validators | Replace lazy-parent fixtures and pull-redemption phases with preseeded anchor, dust, push, replenishment, timing, and >32 historical-generation evidence. |
| Normative documentation | pooled-backing, fees, maturity, scheduler, redemption, readiness docs | Mark superseded decisions explicitly. Preserve historical package descriptions. |

No second production architecture is permitted. Deletion occurs in the same
source tranche in which each replacement becomes authoritative.

## Frozen physical and economic architecture

The pooled parent is renamed the **Dynamic 14-day IO neuron**. Its NNS dissolve
delay and every child delay are exactly `1_209_600` seconds. The production
staking memo is `0`, its staking Account must not collide with the protected
permanent-neuron Account, it is non-dissolving, it auto-stakes according to the
reviewed policy, and it follows the protected permanent IO neuron.

The NNS Manager must establish and prove this neuron before Ready. The operator
seeds its deterministic Account with 10 ICP. A canonical balance below
`1_000_000_000` e8s fails bootstrap. A balance at or above that amount succeeds:
exactly 10 ICP initializes excluded anchor and every positive residual is
excluded unattributed surplus. Neither category issues IO or enters claim
backing.

At a stable canonical parent boundary:

```text
physical_dynamic_parent
    = claim_bearing_dynamic_principal
    + excluded_anchor_available
    + excluded_unattributed_surplus
```

An explicit physical in-flight adjustment is allowed only while a real
unresolved effect requires it. It must be exact and disappear at the next
canonical stable boundary. Unexpected positive residual defaults to excluded
surplus; negative residual is an invariant failure.

Claim backing remains:

```text
B = L + P + U + T
```

`P` is only accounted claim-bearing Dynamic principal. Anchor and surplus are
outside `B`; physical principal must never be used as a shortcut for `P`.
Claim-bearing `P = 0` is valid while the physical neuron retains protocol
capital.

The anchor target and maximum replenishment entitlement are both exactly 10
ICP. `anchor_available_e8s` is the only required fee-capacity scalar and is
always in `0..=1_000_000_000`. No permanent fee liability, per-fee journal,
donor provenance, per-user child accounting, generic queue, scanner, second
monetary slot, or second scheduler is introduced.

All realised maturity of the Dynamic neuron follows the ordinary pooled /
TwoWeek reward path. Principal provenance does not partition neuron maturity.

## Executable invariant details

The executable test-only model in `tests/economics` checks these rules after
every representative transition.

### A. Claim-rate floor

Every normal Ready state with claim-bearing supply `C > 0` satisfies `B >= C`
(ICP and IO both use e8 precision). Empty genesis has `B = C = 0`.

### B. Claim-rate monotonicity

For consecutive canonical checkpoints:

```text
B_after / C_after >= B_before / C_before
```

Comparison is exact and overflow-safe. Transit keeps ambiguous physical moves
inside `B`. A fee-bearing effect that lacks anchor capacity stops before its
irreversible boundary.

### C. Anchor isolation

Seed and unsolicited surplus never enter `B` or `C`, never issue IO, never
increase the anchor target, and never increase the later replenishment claim.

### D. Exact fee conservation

Every unavoidable ICP fee has exactly one economic treatment. A fee paid to
relocate already-existing claim backing consumes excluded anchor capacity
exactly once. A fee paid to deliver fresh value reduces that fresh net credit
and creates no reimbursement entitlement. An anchor-restoration fee is paid
from fresh TwoYear maturity and creates no recursive debt. A redemption fee
belongs to the frozen gross/net quote. External fees create no protocol debt.
Canonical replay is non-effecting.

### E. Synchronization target with economic fail-safe

The selected scheduler makes normal SNS unlock later than structural
detection, reconciliation, Split, accepted StartDissolving, the exact NNS
delay, prioritized principal return, and the deterministic stress allowance.
No finite bound is claimed for arbitrary distributed failure. In that degraded
case all backing remains represented in `B`; a proved push creates a durable
payout obligation which completes at most once when liquidity is canonical.

### F. Push-redemption solvency

Preparation creates neither a reservation nor an obligation. Exact proof of a
timely push simultaneously excludes the redeemed claim from `C`, removes its
frozen gross payout from `B`, and creates an equal excluded durable payout
obligation. The ledger makes aggregate pushed principal no greater than the
then-outstanding claim supply. Monotone rates make every frozen earlier quote
no greater than its later fair value. A block and intent can settle once.

### G. Dust tolerance

Unsolicited value cannot block bootstrap/readiness, change `B`/`C`, enlarge
anchor entitlement, issue IO, impersonate an expected transfer, or create a
reward entitlement. Semantic maturity Accounts still capture their complete
balance where that authority is deliberate; exact transfer proofs remain
distinct from complete Account capture.

### H. Exact partition

Every e8 belongs to one physical/economic category. Parent claim, anchor, and
surplus are pairwise disjoint; parent, child, and transit claim backing are
pairwise disjoint; and the exact global claim partition is `B = L + P + U + T`.

## Fee inventory

| Event | Treatment |
| --- | --- |
| Operator seed transfer | External; ignored. |
| Unsolicited external transfer | Excluded or surplus under the existing Account rules; no reimbursement entitlement. |
| Stream liquid to Dynamic top-up | Existing claim backing; the fee consumes anchor exactly once after canonical proof. |
| NNS Split | Existing claim backing; the fee consumes anchor exactly once at sticky child commitment. |
| Future child Disburse fee | Existing claim backing; reserve and consume anchor once at sticky child commitment. |
| Actual child Disburse | The fee was already economically accounted; no second charge. |
| Jupiter 60% claim delivery | Fresh ingress; the fee reduces fresh claim credit. |
| Jupiter 40% permanent delivery | Fresh ingress; the fee reduces fresh permanent credit. |
| TwoWeek 60% claim delivery | Fresh maturity; the fee reduces fresh claim credit. |
| TwoWeek 40% permanent delivery | Fresh maturity; the fee reduces fresh permanent credit. |
| TwoYear anchor restoration | The fee is paid from fresh TwoYear capture and creates no recursive debt. |
| TwoYear ordinary 60% claim delivery | Fresh maturity; the fee reduces the fresh claim addition. |
| TwoYear ordinary 40% permanent delivery | Fresh maturity; the fee reduces the fresh permanent addition. |
| Redemption ICP payout | The fee belongs to the redeemer's frozen gross/net quote. |
| IO-ledger fees | Existing IO-ledger supply semantics; not an ICP anchor liability. |

An ambiguous effect creates no second fee charge. Canonical proof of a
fee-bearing existing-backing effect consumes anchor at most once. Canonical
proof of a fresh-value delivery only advances its immutable transfer state.

## Why only existing-backing movement fees consume the anchor

A ledger fee can have two economically different meanings. If IO already has
`x` ICP of claim backing and merely moves it from one backing bucket to another,
a transfer fee would destroy part of an already-existing claim. Repeated user
staking and unstaking can cause those relocations repeatedly. The excluded
Dynamic anchor therefore replaces such unavoidable losses so internal backing
movement does not reduce `B/C`.

If ICP has only just entered IO through Jupiter or has only just been realised
as maturity, the transfer fee occurs before that value becomes new claim
backing or permanent capital. The correct new credit is the post-fee amount. No
existing claim has lost value. For paired fresh claim inflows, IO release is
calculated against that same post-fee claim credit, so no dilution is
introduced.

Permanent capital is outside `B`, so a fee on a fresh permanent-capital
delivery cannot reduce the IO claim rate. A persistent fee-shortfall balance
for permanent capital and a later make-whole path therefore protect no required
solvency invariant. They add stable state, transfer phases, proof branches,
Candid surface, and audit burden. This architecture deliberately omits them.

## TwoYear restore-then-split

For a complete semantic capture `M`:

1. If `M` can fund an anchor reimbursement transfer, deliver
   `min(anchor_target - anchor_available, M - transfer_fee)` and deduct both
   principal and transfer fee from fresh maturity. Otherwise retain `M` in the
   staging Account.
2. Apply the ordinary gross 40/60 allocation only to the valid remainder.
   Each ordinary leg pays its delivery fee from its own fresh gross allocation,
   so its economic addition is the net credit.
3. A remainder unable to fund both ordinary transfers remains in the fixed
   TwoYear staging Account for a later complete capture.

No TwoYear path issues IO.

Captured but unplanned TwoYear value remains outside `B` and contributes zero
to `T`. The exact ordinary claim credit cannot be known until the serialized
plan freezes the then-current anchor deficit. After that boundary, only the
frozen `ordinary.claim_credit` contributes to `T`; the transition therefore
never removes a speculative full-capture claim estimate from backing.

## Prepared push pricing proof

For a preparation at rate `r0 = B0/C0`, its gross quote is
`q = floor(x * B0 / C0)`. Invariant B gives every later pre-push rate
`r1 >= r0`, so `q <= floor(x * r1)`: the frozen quote cannot overpay relative
to the later fair rate.

For independently prepared pushes, ledger conservation limits the sum of
accepted pushed principal to the claim-bearing IO held by those sources. At
each exact push proof, the frozen gross is removed from claim backing and moved
to an excluded payout obligation. Sequential application of the monotone quote
inequality keeps total obligations within the backing surrendered by the
corresponding claims. The same ledger block and deterministic intent cannot be
accepted twice.

Expiry governs the transfer's canonical creation time, not settlement-call
time. A timely transfer remains settleable after expiry. An actually late or
otherwise unmatched transfer is unsupported and does not create an obligation.

## Current scheduler timing graph

The source audit found one Stream reward timer and no NNS automatic recovery
timer:

```text
latest SNS reward event
  -> event end + 86,400s + 300s reward margin
  -> timer sets reward_work_due and stake_observation_due together
  -> Root/module check
  -> claim snapshot before
  -> reward event before
  -> paged SNS neuron observation + one IO balance query per active neuron
  -> reward event after
  -> claim snapshot after
  -> combined reward/structural checkpoint
  -> ensure_latest once
  -> ordinary next reward timer even when reconciliation stayed Pending

NNS Pool/Split/child lookup/StartDissolving
  -> exact active-operation recovery only when resume is called
  -> passive child ready_at_seconds
  -> Disburse/proof only when resume is called
```

Observation errors use a 60-second Stream retry. A successful observation
followed by retryable reconciliation does not: it logs the failure and returns
to the daily timer. Permissionless keepers exist, but no canister timer wakes
active NNS work or a child at `ready_at_seconds`. This coupling is superseded.

## Candidate structural cadences and cost

The reviewed neuron population ceiling is 1,000 and SNS `list_neurons` pages
100 records. With one excluded IO Account, one structural poll makes:

```text
SNS Governance pages       = floor(neurons / 100) + 1
IO balance queries         = neurons + 2 * (reserve + excluded Accounts)
approximate total calls    = neurons + Governance pages + 17
```

The constant 17 covers the Root summary, four before/after NNS claim-asset
observations, and the remaining two-snapshot ledger facts. It is a source-level
capacity estimate, not a measured cycles quotation. The per-neuron balance
queries dominate cycles cost.

| Cadence | Worst detection | Generations/day | Natural live bound | Governance queries/day at 1 / 100 / 1,000 neurons | IO balance queries/day at 1 / 100 / 1,000 neurons | Approx. total calls/day at 1,000 | Healthy slack |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 12h | 43,200s | 2 | 29 | 2 / 4 / 22 | 10 / 208 / 2,008 | 2,056 | 42,660s (11h51m) |
| 6h | 21,600s | 4 | 57 | 4 / 8 / 44 | 20 / 416 / 4,016 | 4,112 | 64,260s (17h51m) |
| 4h | 14,400s | 6 | 85 | 6 / 12 / 66 | 30 / 624 / 6,024 | 6,168 | 71,460s (19h51m) |
| 1h | 3,600s | 24 | 337 | 24 / 48 / 264 | 120 / 2,496 / 24,096 | 24,672 | 82,260s (22h51m) |

The healthy slack uses the full worst phase-placement lag plus a 600-second
deterministic stress budget: two structural retries, two Pool-contention
retries, two command-reflection retries, one ready-return retry, and 180
seconds for successful call chains. This is deliberately not an IC/network
SLA. All candidates fit, and 12 hours is the slowest candidate with a large
multi-hour margin, the lowest polling cost, and the smallest cohort footprint.
The selected structural cadence is therefore **12 hours**.

## Frozen scheduler design

- SNS user-neuron dissolve delay: `1_296_060` seconds (15 days + 1 minute).
- NNS Dynamic parent/child dissolve delay: `1_209_600` seconds (14 days).
- Structural synchronization cadence: 43,200 seconds (12 hours).
- Reward event cadence: 86,400 seconds (daily).
- Reward safety margin: 300 seconds, applied only to reward processing.
- Stream retry-safe reconciliation interval: 60 seconds.
- NNS active recovery retry interval: 60 seconds.

One ephemeral Stream one-shot timer selects the earliest derived deadline:
latest structural checkpoint plus 12 hours, canonical reward event end plus the
daily duration and 300-second margin, or a 60-second retry for already-due
work. It performs structural synchronization without consuming or crediting a
reward event. A successful structural checkpoint calls `ensure_latest`
immediately. Retryable incomplete reconciliation retains the same generation
and schedules 60 seconds rather than waiting 12 hours.

One ephemeral NNS one-shot timer invokes the existing state-aware recovery
machine. It selects the earliest active recovery retry or live child
`ready_at_seconds`. It re-reads canonical state, services the oldest ready
child first, and never blindly repeats an ambiguous irreversible command.
Deadlines derive from semantic checkpoints, active operation, and cohorts, so
no stable timer timestamp or additional monetary slot is introduced. Paused
does not authorize unrelated new work; already-accepted exact recovery follows
the existing reviewed Paused rules.

## Reward and structural event fencing

The registry remains one sorted neuron registry, but its backing status and
reward eligibility facet are independent. Structural polling reads the latest
canonical reward event identity only as a fence; it does not classify, consume,
or credit that event.

- A newly observed active neuron at marker `N` is eligible from `N + 1`.
- A neuron canonically observed active before event `N` remains eligible for
  `N` regardless of which keeper runs first.
- Activation first observed after event `N` completed cannot receive `N`
  retroactively.
- An exit fenced after completed event `N` starts backing unwind immediately
  but retains reward eligibility only through `N`; a fence before `N` ends at
  the preceding marker.
- Retry and upgrade preserve the same structural generation and event fence.

Only the daily reward path classifies proposal-bearing/fallback/skipped events,
increments event counters, or changes policy/eligible credit. The existing
canonical event identity and checkpoint provide replay protection. This uses a
minimal eligibility fence in the existing record, not a duplicate registry or
wall-clock eligibility rule.

## Cohort bound and ready-child priority

There is at most one aggregate child per 12-hour structural generation. Before
new Split, every ready/overdue child is serviced first. Active, ambiguous, or
proof-pending return blocks later child creation. The healthy reachable live
population is:

```text
ceil(1,209,600 / 43,200) + 1 = 29
```

The endpoint allowance covers one generation at the readiness boundary. This
is a stable-memory sizing estimate, not a product cap. More than 32 historical
generations are valid, and no `CapacityPending` variant remains. A much higher
decode sanity limit, if retained, is an invariant failure rather than flow
control.

## Healthy timing and degraded liveness

The preferred SNS delay leaves `1,296,060 - 1,209,600 = 86,460` seconds beyond
the exact child delay. Worst 12-hour phase placement plus the 600-second stress
budget consumes 43,800 seconds and leaves 42,660 seconds (11 hours 51 minutes)
before SNS unlock. Exact ready-boundary wakeup is included in that budget; the
daily reward margin is not, because reward processing is independent.

No finite scheduler proves arbitrary IC/network completion. If recovery runs
beyond SNS unlock, the child/transit value remains exactly in `U`/`T`, claim
rate does not fall, and a timely exact IO push creates an immutable owed payout.
The obligation waits for canonical Stream liquidity and completes at most once
after child recovery. That is an exceptional safety state, not ordinary
pre-redemption liquidity gating.

## Implementation consequences

Source work proceeds as one replacement:

- NNS state retains mandatory Dynamic identity, anchor availability, and
  accounted claim principal while deleting the obsolete fee liability; Stream
  retains exact prepared-push and payout-obligation state.
- strict pre-launch schema markers are bumped and prior markers rejected;
- ready-child priority replaces capacity behavior;
- normal pre-push liquidity gating is deleted; post-push missing liquidity is a
  durable invariant-breach obligation;
- one active monetary operation slot per manager, one earliest-deadline timer
  per manager, exact semantic maturity Accounts, and canonical replay remain.
