# ADR: Pooled claim-backing complexity exception

- Status: accepted
- Date: 2026-08-23
- Baseline commit: `44c37cf7222b343dda9b7f63ac128a02614bcda7`
- Final-correction baseline: `221d8c7703d4ad4cf58c7c30ca07ed056663b369`

## Context

The constitutional baseline before reward integration was 10,449 normally
formatted production Rust lines. The first source-complete pooled
claim-backing feature checkpoint reached 14,542 lines. This replacement deletes
the direct AllPool/Mixed maturity routes, the monolithic redemption snapshot,
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

The remaining state is required to prove one monetary effect at a time:

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

## Decision

The final ceilings are 5,480 Stream Manager lines, 5,460 NNS Manager lines,
and 13,720 combined lines. Against the final measured counts these provide
127 lines (2.37%), 123 lines (2.30%), and 316 lines (2.36%) of headroom. The
pure-economics, reward-boundary, ledger-boundary, shared-type, and per-file
ceilings do not change. In particular, every production file remains limited
to 1,000 normally formatted lines.

This exception applies only after the deletion-first replacement described
above. It does not authorize source packing, `rustfmt` suppression, hidden
production paths, a second operation slot, a new scheduler, or old/new feature
flags. Future growth beyond either revised ceiling requires another accepted
exception with new measured need and deletion evidence.

## Consequences

The implementation is larger than the pre-reward constitutional baseline but
materially smaller than the rejected direct-route architecture. Its state
space remains bounded and its economic model has one claim-ingress boundary,
one global reconciliation path, and one implementation of every formula.

This ADR does not activate production or authorize deployment, funding,
controller, identity, artifact, evidence, or mainnet work.
