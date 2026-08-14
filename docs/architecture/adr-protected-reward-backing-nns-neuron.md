# ADR: protected reward-backing NNS neuron

## Status

Accepted.

## Decision

The protected NNS parent that backs rewards for eligible two-week SNS stakers
is an NNS-voting neuron. Its launch-approved dissolve delay is exactly
252,460,800 seconds (eight 365.25-day years), matching the controlled pinned-NNS
Governance observation.
The parent must be non-dissolving and have effective
`auto_stake_maturity = false`. Pinned NNS Governance encodes disabled
auto-stake as either `null` or `opt false`; the narrow observation normalizes
both to false and preserves `opt true` as drift.

The exact 1,209,600-second rule applies to ordinary SNS IO reward-neuron
eligibility, the user withdrawal delay, and the beneficiary class. It does not
describe the protected NNS parent's dissolve delay. Normative prose calls the
parent the **two-week-staker reward-backing NNS neuron** so the beneficiary
rule cannot be mistaken for an NNS configuration rule.

First readiness must prove the exact configured neuron ID, exact seeded cached
principal, zero ordinary maturity, zero staked maturity, disabled auto-stake,
no pending maturity disbursement, and the exact non-dissolving approved delay.
Only then may the durable launch baseline be recorded. Later retained staked
maturity is expected from IO's deliberate 40% stake operation, but auto-stake
must remain disabled and the parent must remain non-dissolving at the approved
delay before every new maturity start.

The eight-year delay must not block reward liveness. An over-target split uses
one immediate operation only through canonical `StartDissolving` proof. The
exact child then occupies one passive unwind slot while immediate maturity work
may continue on the reduced parent. There is an absolute maximum of one child:
no target queue, child queue, ladder, or second unwind exists.

Unexpected parent configuration pauses new NNS preparation while preserving
immutable active and passive evidence. It does not fabricate, discard, or use
SNS maturity as entitlement evidence.

IO remains Paused, inert, prelaunch and not live. This ADR authorizes no
deployment, identity operation, mainnet call, or change to the daily SNS
entitlement policy.
