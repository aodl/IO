# ADR: daily SNS entitlement events

## Status

Accepted for implementation.

## Decision

IO uses the SNS Governance native reward event as its canonical governance-
entitlement observation. Native SNS reward events run once per day with:

```text
initial_reward_rate_basis_points = 0
final_reward_rate_basis_points = 0
round_duration_seconds = 86,400
```

Native maturity is disabled by the zero rates. IO observes canonical voting
participation without using native SNS maturity as monetary authority.

For an event whose `settled_proposals` list is nonempty, each currently
eligible ordinary IO SNS neuron receives a fraction of one fixed daily policy
opportunity.
IO first sums the canonical `reward_shares` tagged with that event's exact end
timestamp across every neuron, including excluded or currently ineligible
neurons. An absent or stale field contributes zero. A current-event
participation field without its `Uint128` value fails closed. The eligible
credit is then:

```text
credit_i = floor(DAILY_EVENT_CREDIT * neuron_reward_shares_i
                 / total_canonical_reward_shares)
```

where `DAILY_EVENT_CREDIT = 1,000,000,000,000,000,000`. The shares already
encode canonical ballot voting power across the proposals settled in the
event; IO does not reconstruct ballots or multiply them by a second
participation ratio. A proposal-bearing event with zero canonical shares
assigns no eligible credit, forfeits its complete daily opportunity and does
not use a stake fallback.

For an event whose `settled_proposals` list is empty, Governance has no proposal
ballots from which to populate current-event participation fields. IO therefore
ignores every participation field and grants full participation credit to each
currently eligible neuron, normalized by current eligible cached IO stake:

```text
credit_i = floor(DAILY_EVENT_CREDIT * eligible_stake_i
                 / total_eligible_stake)
```

The fallback trigger is only `settled_proposals.is_empty()`, never the sum of
observed reward shares. If there are no eligible neurons, the complete daily
opportunity is forfeited.

Fixed-point credits accumulate directly and non-overlappingly:

```text
accumulated_eligible_credit_i += credit_i
accumulated_policy_credit += DAILY_EVENT_CREDIT
```

Every successfully observed, non-skipped event contributes one equal economic
unit to the policy denominator, independent of proposal count. Canonical SNS
proportions remain authoritative within a proposal-bearing day, while a
no-proposal day acts as one virtual unanimous proposal weighted by eligible
stake. Entry credits may sum to less than policy credit: excluded, ineligible,
zero-share and fixed-point remainder fractions are forfeited rather than
redistributed. Checked `u128` arithmetic is required. IO retains the current
accumulator, one immutable pending batch, the latest event/skip observation and
cumulative counters; it does not retain ballots, per-proposal state, an event
archive or a moving accounting window.

Governance entitlement time and ICP-backing realization are intentionally
asynchronous. Daily observations continue while one frozen entitlement batch
awaits the pooled parent's canonical `DisburseMaturity(100%)` record, semantic
two-week staging balance capture, common liquid-first claim receipt, and sequential IO
payout. The entitlement generation is immutable, while the maturity observed
before command submission is only a readiness observation. The canonical
pending disbursement fixes the entitlement boundary; reward maturity accrued
afterward belongs to the next batch. After finalization, the complete positive
semantic staging Account balance and its fee-reduced claim credit determine the
backed IO pool. At payout,
the backed pool is first reduced by the frozen batch's forfeited policy
fraction; the eligible pool is then allocated over eligible credits. A frozen
zero-eligible-credit batch completes without recipient transfers and leaves
the full backed amount in protocol reserve; later events cannot claim it.

The first successful readiness transition seeds the latest canonical event as
`last_processed_event` without adding eligible or policy credit. This prevents
pre-activation events from becoming retroactive IO entitlement. An existing
checkpoint survives pause/unpause and same-Wasm upgrade and is never reseeded.

Before freezing, the Stream Manager revalidates reviewed SNS Governance and
takes a bracketed canonical claim-backing snapshot. The daily checkpoint is the
single reconciliation generation. Under-target work moves existing liquid
claim backing through a typed NNS permit; over-target work may commit one
aggregate child for that generation. Capacity, liquidity, reward-coverage,
parent-minimum, Busy, or Paused outcomes leave live credit in place. A pooled
maturity operation sends its proved claim leg through the same liquid-first
receipt as other claim ingress. Receipt completion then makes ordinary global
pool reconciliation due; there is no maturity-specific pooled destination or
source route. Every replay preserves the immutable entitlement generation and
command identity; later observations accumulate in the fresh live
accumulator.

Governance readiness and every daily boundary require
`max_number_of_neurons <= 1,000`. This is a bound on the complete SNS Governance
population, not the eligible reward subset. An unexpected module hash or
parameter pauses reward processing and prevents a new freeze, but does not
invalidate immutable pending transfer intents or pause redemption.

The next observation is scheduled for the prior event end plus 86,400 seconds
and a 300-second safety margin. A pending or retryable read keeps work due and
installs one replacement one-shot timer after 60 seconds. Invalid reviewed
configuration pauses reward processing without automatic retry; no interval
timer or backoff scheduler exists.

An exact ICRC transfer to the canonical neuron staking account is recipient
monetary completion. Before awaiting Governance, IO persists one
`refresh_attempted` bit for that recipient and makes one best-effort
`ClaimOrRefresh` call through the shared narrow wire boundary. The returned
neuron ID must match the expected recipient. Governance rejection, transport
failure, malformed response, or wrong/missing ID is reported through bounded
non-authoritative canister-log telemetry; it cannot change monetary completion
or block later recipients. IO maintains no durable refresh-retry queue at
launch. A later reward delivery to the same neuron naturally makes another
best-effort attempt. Observation and backing waits do not block redemption.

The exact 1,209,600-second duration is authoritative for ordinary IO reward
eligibility, user withdrawal, and the pooled NNS parent. The parent additionally
requires a fixed reviewed following policy and periodic voting-power refresh.
This duration is not the daily SNS reward-event duration or an independent
accounting cohort.

## Availability and skips

The upstream neuron field contains only latest-event participation. A normal
next event has a round delta of one and
`rounds_since_last_distribution == 1`. The same event is pending and causes no
mutation. A missed or catch-up span is recorded as a bounded typed skip, adds no
eligible or policy credit, advances the observation checkpoint, preserves
undistributed backing and leaves redemption available. Missing events are
availability failures, not zero-participation evidence or monetary completion
assertions. IO does not reconstruct proposals, native maturity or fabricated
no-proposal days.

## Observation

Historian and frontend views may derive rolling seven-day or thirty-day
participation, APY or other display averages from available observations. Those
views are explicitly non-authoritative and never affect allocation. Missing
observations remain missing rather than becoming zero.

Historian projection distinguishes daily policy credit, eligible credit,
policy-forfeited credit, live and pending totals, distributed IO, forfeited IO,
rounding dust, event classification, skips, Governance freshness, total claim
backing, pooled target, reward coverage, and liquid availability.

## Replaced policy

This decision supersedes the participation-specific two-week reward cohort,
its capture/close operations and timer, frozen proposal-period stake snapshot,
exact-next-14-day-event alignment and one-event-to-one-maturity timing rule.
There is no ballot fallback, maturity-entitlement fallback, legacy cohort
reward path or monetary rolling average.

IO remains Paused, inert, prelaunch and not live. This decision authorizes no
deployment or mainnet operation.
