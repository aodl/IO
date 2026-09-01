# ADR: Pooled claim-backing complexity exception

- Status: historical review baseline with current diagnostic measurements. The
  maturity/provenance machinery described historically is superseded by
  [`account-semantic-maturity.md`](account-semantic-maturity.md). Numerical
  ceilings in this ADR are no longer launch correctness conditions; executable
  semantic architecture checks remain mandatory. Lazy bootstrap, Policy A,
  the 32-cohort branch, shared delay, and pull redemption are superseded by
  [`adr-anchored-dynamic-backing.md`](adr-anchored-dynamic-backing.md).
- Date: 2026-08-23
- Baseline commit: `44c37cf7222b343dda9b7f63ac128a02614bcda7`
- Final-correction baseline: `221d8c7703d4ad4cf58c7c30ca07ed056663b369`
- Cross-flow correction baseline: `0e7299eb43503351d80cbee933cfdab15f3b4f6b`
- Source-local completion baseline: `4f56e48afa62dc1b775e85f420c3b71a376d2db2`
- Production-final baseline: `7100aa4`
- Current release source: `e727d688b3aec0e6dace6a499a4979bf66cad2c8`

## Context

The constitutional baseline before reward integration was 10,449 normally
formatted production Rust lines. The first source-complete pooled
claim-backing feature checkpoint reached 14,542 lines. This replacement deletes
the deleted source-specific maturity routes, the monolithic redemption snapshot,
parallel entitlement and backing registries, duplicated frozen-recipient state,
and full completed-operation histories. The simplified result is 12,815 lines,
1,727 fewer than the feature checkpoint and below the 13,000-line review target.

The exact component change from the feature checkpoint is:

| Counted component | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Stream Manager | 6,181 | 5,098 | -1,083 |
| NNS Manager | 5,365 | 5,061 | -304 |
| pure economics | 218 | 220 | +2 |
| ledger boundary | 517 | 517 | 0 |
| reward policy | 447 | 282 | -165 |
| SNS reward boundary | 457 | 457 | 0 |
| accounts | 45 | 45 | 0 |
| NNS types | 1,275 | 1,086 | -189 |
| receipt types | 37 | 49 | +12 |
| **Combined** | **14,542** | **12,815** | **-1,727** |

The remaining state is required to prove one active monetary operation at a time:

- Stream retains one active operation, one bounded 1,000-record neuron
  registry, one pending entitlement batch, compact caller replay records, and
  one latest no-effect reconciliation checkpoint.
- NNS Manager retains one active command, at most 32 passive cohorts, one
  pooled-parent identity, pending maturity evidence, and compact completed
  replay evidence.
- A claim receipt retains one frozen recipient vector, one cursor, and only the
  current transfer attempt. Completion discards recipient and transfer history.
- Transit ownership changes between Stream and NNS phases and stores only the
  unreflected residual, preventing `P + T` double counting.

Further deletion would remove exact proof, stable retry, bounded cohort
cleanup, prospective reward eligibility, or the independent canonical
observation boundaries. Those are monetary correctness requirements rather
than optional orchestration.

The final correctness tranche first deleted permanent-neuron identity and
policy fields from the monetary claim observation, arbitrary active-unwind
generation inference, exact-equality donation branches, and active root
rehearsal authority. It then added only the state needed for monotone parent
credit proof, net committed child value, exact prepared-exit membership and
ambiguous-call replay. The measured correction delta from the simplified
checkpoint is:

| Counted component | Simplified checkpoint | Final correction | Delta |
| --- | ---: | ---: | ---: |
| Stream Manager | 5,098 | 5,353 | +255 |
| NNS Manager | 5,061 | 5,337 | +276 |
| pure economics | 220 | 220 | 0 |
| ledger boundary | 517 | 517 | 0 |
| reward policy | 282 | 282 | 0 |
| SNS reward boundary | 457 | 457 | 0 |
| accounts | 45 | 45 | 0 |
| NNS types | 1,086 | 1,144 | +58 |
| receipt types | 49 | 49 | 0 |
| **Combined** | **12,815** | **13,404** | **+589** |

The unavoidable additions keep one operation slot per canister. Stream adds
one optional exact prepared-reconciliation request; NNS adds one compact last
completed-unwind replay record. Operation-variant and persistent-collection
counts do not increase. Physical child principal remains Governance evidence,
while net value and its derived liability prevent a second backing loss at
disbursement. No mutable fee-debt scalar, SNS per-neuron principal mapping,
second scheduler, migration, feature flag, packed source, or formatter
exception is introduced.

The final cross-flow correction replaces duplicate permanent-credit evidence
with one monotone proof shared by Jupiter and TwoWeek, removes
per-cohort Governance reads from claim observation, and removes Split dispatch
from unwind preparation. It adds durable permanent refresh checkpoints, one
bounded full-snapshot retry for after-await generation drift, daily parent
voting-power refresh, and compact exact-candidate cross-flow tests. No operation
variant or persistent collection is added. The measured final delta is:

| Counted component | Prior correction | Cross-flow final | Delta |
| --- | ---: | ---: | ---: |
| Stream Manager | 5,353 | 5,377 | +24 |
| NNS Manager | 5,337 | 5,460 | +123 |
| pure economics | 220 | 220 | 0 |
| ledger boundary | 517 | 528 | +11 |
| reward policy | 282 | 282 | 0 |
| SNS reward boundary | 457 | 457 | 0 |
| accounts | 45 | 45 | 0 |
| NNS types | 1,144 | 1,206 | +62 |
| receipt types | 49 | 49 | 0 |
| **Combined** | **13,404** | **13,624** | **+220** |

The source-local completion tranche quarantines supply-paired credit until the
matching Stream permit, makes every persisted idempotent Governance phase
observe/reissue recoverable, resolves reconciliation replay before policy
work, closes transfer-failure ownership, and retains the committed unwind fee
basis. The implementation first reused the shared monotone permanent-credit
proof, deleted obsolete Jupiter pause variants and replay-time policy branches,
and consolidated claim-asset ownership and command-result classification. It
adds no operation phase, collection, scheduler, feature flag, migration, or
fee-debt scalar. The final measured delta is:

| Counted component | Starting checkpoint | Review final | Delta |
| --- | ---: | ---: | ---: |
| Stream Manager | 5,459 | 5,452 | -7 |
| NNS Manager | 5,726 | 6,109 | +383 |
| pure economics | 220 | 220 | 0 |
| ledger boundary | 528 | 528 | 0 |
| reward policy | 282 | 282 | 0 |
| SNS reward boundary | 457 | 457 | 0 |
| accounts | 45 | 45 | 0 |
| NNS types | 1,211 | 1,278 | +67 |
| receipt types | 49 | 49 | 0 |
| **Combined** | **13,977** | **14,420** | **+443** |

The account-semantic source closure removes maturity balance baselines and
pre-effect neuron observations from durable intent, flattens passive maturity
evidence, and makes exact TwoWeek replay advance the matching work. Passive
selection examines both maturity slots and chooses captured delivery before a
finalization-ready capture, then the earliest finalization and stable role
order. It adds no operation variant or persistent collection. The final normally
formatted counts are:

| Counted component | Source-closure start | Source-closure final | Delta |
| --- | ---: | ---: | ---: |
| Stream Manager | 5,387 | 5,416 | +29 |
| NNS Manager | 5,926 | 5,977 | +51 |
| pure economics | 220 | 220 | 0 |
| ledger boundary | 528 | 528 | 0 |
| reward policy | 230 | 230 | 0 |
| SNS reward boundary | 457 | 457 | 0 |
| accounts | 67 | 67 | 0 |
| NNS types | 1,206 | 1,224 | +18 |
| receipt types | 43 | 43 | 0 |
| **Combined** | **14,064** | **14,162** | **+98** |

The source closure added 69 lines for the explicit two-slot action selector and
exact replay wake-up behavior; it replaces state rather than retaining the
deleted baseline/provenance nesting. The maintained rehearsal then exposed one
narrow liveness defect: an NNS reconciliation could complete through a
permissionless keeper before Stream observed its frozen request. The exact
completed-NNS reconciliation replay recovery adds 29 Stream lines and no NNS or
shared-component lines. The final account-semantic release is therefore 98
lines above the source-closure start and remains 136 lines smaller than the
preceding 14,298-line production-final source.

The maximum encoded NNS state with all 32 live cohorts is 5,434 bytes against
its 1,000,000-byte stable-cell bound. The maximum exercised Stream state with
the full 1,000-record neuron registry is 111,209 bytes against its
2,000,000-byte stable-cell bound.

## Decision

The account-semantic release used 5,520 Stream Manager lines, 6,125 NNS Manager
lines, and 14,485 combined lines as review ceilings. Its recorded measurements
were 5,416, 5,977 and 14,162, leaving 104 lines (~1.88%), 148 lines (~2.42%),
and 323 lines (~2.23%) of historical review headroom.

The recorded anchored Dynamic-parent replacement and execution simplification
measure 5,215 Stream Manager lines, 7,073 NNS Manager lines, and 15,196
combined lines. Relative to the hardened pre-refactor worktree
(5,416 / 6,002 / 14,187), the complete replacement changes those counts by
-201 / +1,071 / +1,009. It also removes two durable redemption phases, one
stable field, and 21 public progress variants without adding a timer, stable
scheduler, or active-operation slot.

`xtask simplicity_check` continues to print these and the pure-economics,
reward-boundary, ledger-boundary, shared-type, and per-file measurements as
review diagnostics. Raw line-count thresholds are not correctness failures.
Hard failures remain for semantic regressions such as scanners, generic
journals, obsolete intake/refund paths, monetary execution driven by timers,
forbidden dependencies, runnable unresolved production templates, and stale
normative economic or execution promises.

The historical review ceilings do not authorize source packing, `rustfmt`
suppression, hidden production paths, a second operation slot, a new scheduler,
or old/new feature flags. Material growth still requires explicit review with
measured need and deletion evidence, but crossing a raw count does not by
itself establish a protocol correctness failure.

## Consequences

The implementation is larger than the pre-reward constitutional baseline but
materially smaller than the rejected direct-route architecture. Its state
space remains bounded and its economic model has one claim-ingress boundary,
one global reconciliation path, and one implementation of every formula.

This ADR does not activate production or authorize deployment, funding,
controller, identity, artifact, evidence, or mainnet work.
