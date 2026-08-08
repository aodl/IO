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
eligible IO neuron receives the canonical `reward_shares` tagged with that
event's exact end timestamp. An absent or stale field contributes zero. A
current-event participation field without its `Uint128` value fails closed.
The shares already sum canonical ballot voting power across the proposals
settled in the event; IO does not reconstruct ballots or multiply them by a
second participation ratio. A proposal-bearing event with zero eligible shares
adds zero entitlement and does not use a stake fallback.

For an event whose `settled_proposals` list is empty, Governance has no proposal
ballots from which to populate current-event participation fields. IO therefore
ignores every participation field and grants full participation credit to each
currently eligible neuron, weighted by its canonical eligible stake. The
fallback trigger is only `settled_proposals.is_empty()`, never the sum of
observed reward shares.

Event weights accumulate directly and non-overlappingly:

```text
accumulated_weight_i += event_weight_i
```

Each settled proposal therefore contributes its canonical ballot voting power,
while a no-proposal day acts as one virtual unanimous proposal weighted by
eligible stake. Checked `u128` arithmetic is required. IO retains the current
accumulator, one immutable pending batch, the latest event/skip observation and
cumulative counters; it does not retain ballots, per-proposal state, an event
archive or a moving accounting window.

Governance entitlement time and ICP-backing receipt time are intentionally
asynchronous. Daily observations continue while one frozen entitlement batch
awaits the protected NNS position's actual modulated ICP receipt and sequential
IO payout. Only actually received ICP determines the backed IO pool. A frozen
zero-weight batch completes without recipient transfers and leaves the full
backed amount in protocol reserve; later events cannot claim it.

The exact 1,209,600-second duration remains authoritative for ordinary IO
reward-neuron eligibility, user withdrawal delay and the protected two-week NNS
position. It is a staking-product rule, not the SNS reward-event duration or an
independent accounting cohort.

## Availability and skips

The upstream neuron field contains only latest-event participation. A normal
next event has a round delta of one and
`rounds_since_last_distribution == 1`. The same event is pending and causes no
mutation. A missed or catch-up span is recorded as a bounded typed skip, adds no
entitlement, advances the observation checkpoint, preserves undistributed
backing and leaves redemption available. Missing events are availability
failures, not zero-participation evidence or monetary completion assertions.
IO does not reconstruct proposals, native maturity or fabricated no-proposal
days.

## Observation

Historian and frontend views may derive rolling seven-day or thirty-day
participation, APY or other display averages from available observations. Those
views are explicitly non-authoritative and never affect allocation. Missing
observations remain missing rather than becoming zero.

## Replaced policy

This decision supersedes the participation-specific two-week reward cohort,
its capture/close operations and timer, frozen proposal-period stake snapshot,
exact-next-14-day-event alignment and one-event-to-one-maturity timing rule.
There is no ballot fallback, maturity-entitlement fallback, legacy cohort
reward path or monetary rolling average.

IO remains Paused, inert, prelaunch and not live. This decision authorizes no
deployment or mainnet operation.
