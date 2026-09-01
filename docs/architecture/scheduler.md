# Permissionless progression and reconstructed deadlines

IO has one immediate monetary-operation slot in each value-moving canister. A
timer never contains monetary logic: it wakes the same persisted, state-aware
recovery path exposed to permissionless keepers. Immutable intent precedes an
effect; ambiguity requires canonical proof; a timer never blindly repeats an
irreversible command.

## Stream scheduler

The Stream Manager reconstructs one ephemeral one-shot timer at the earliest of:

- the next 12-hour structural SNS observation;
- the next daily reward-event deadline plus the reward-specific 300-second
  margin; and
- a 60-second retry for already-due reconciliation or observation work.

Structural observation and reward processing are distinct semantic facets.
Structural work observes SNS neuron membership/dissolve state and canonical
stake balances, commits one structural generation, and immediately calls the
ordinary pooled reconciliation path. It never consumes a reward event, changes
processed/missed counters, or grants policy/eligible credit. A retryable NNS
contention keeps `structural_reconciliation_due`, retains the same generation,
and schedules a 60-second wake rather than waiting for the next 12-hour poll.

Reward processing alone classifies canonical proposal/fallback/skipped events
and grants credit. The round-zero genesis identity is a zero-credit activation
baseline; its exact replay is structural. Positive span metadata remains
required for advancement. Reward eligibility is event-ID fenced, not inferred
from poll wall time, so structural/reward call order cannot retroactively credit
an event or credit it twice.

## NNS recovery scheduler

The NNS Manager reconstructs one ephemeral one-shot timer from durable facts:

1. a 60-second retry boundary for an active recoverable operation; or
2. the earliest passive child's exact `ready_at_seconds`.

At the ready boundary the manager re-reads canonical Governance state and
services the oldest ready child through the ordinary exact Disburse/proof path.
It does not infer success from elapsed time. A safe retry schedules another
60-second wake. An ambiguous Split, StartDissolving, or Disburse remains in its
exact phase and is resolved from canonical identity/block evidence before any
resubmission.

Ready-child service has priority over creating another unwind child. One child
may be committed per structural generation, same-generation exits aggregate,
and unresolved ready return supplies natural backpressure. At the selected
12-hour cadence, a 14-day child lifetime yields a healthy endpoint-inclusive
bound of 29 live generations. This is a sizing result, not a public capacity
branch; historical generations may exceed 32 without `CapacityPending`.

Install and upgrade reopen Paused. Automatic scheduling does not initiate new
work while Paused. Already accepted recovery work follows the same reviewed
Paused recovery rules as permissionless `resume`. Reviewed Ready reconstructs
the next deadline without storing a timer ID or duplicate timer timestamp in
stable state.

## Timing model

The IO SNS delay is 1,296,060 seconds (15 days + 1 minute). The NNS Dynamic
child delay is exactly 1,209,600 seconds, leaving 86,460 seconds. The executable
healthy-path model budgets the worst 43,200-second detection placement plus 600
seconds of deterministic operation/retry service, leaving 42,660 seconds before
SNS unlock. This is a healthy scheduling objective, not an arbitrary network
SLA.

If distributed progress exceeds that margin, claim backing remains exactly in
`B=L+P+U+T`. A canonically proved IO push creates a durable payout obligation;
missing liquid ICP pauses that obligation until exact unwind recovery supplies
liquidity, after which payout completes at most once.
