# ADR: Pooled claim-backing complexity exception

- Status: accepted
- Date: 2026-08-23
- Baseline commit: `44c37cf7222b343dda9b7f63ac128a02614bcda7`

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

## Decision

The combined ceiling is 13,050 lines and the NNS Manager ceiling is 5,160
lines. These provide respectively 235 lines (1.83%) and 99 lines (1.96%) of
headroom at this checkpoint. The Stream, pure-economics, reward-boundary,
ledger-boundary, shared-type, and per-file ceilings do not change. In
particular, every production file remains limited to 1,000 normally formatted
lines.

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
