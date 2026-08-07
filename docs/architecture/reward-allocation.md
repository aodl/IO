# Reward Allocation

## Preserved launch semantics

The earning window is exactly 1,209,600 seconds; eligible stake is frozen; late
stake and top-ups are excluded; forfeiture remains dust without redistribution;
and the full backing target is preserved. Delayed maturity payout does not
change eligibility. Settlement is sequential under the single stream operation.

Two-week maturity may issue backed IO to eligible active IO SNS stakers.
Eligibility requires an exact 14-day non-dissolving neuron. Proposal-bearing
events use canonical SNS Governance reward shares as the complete weight,
including the SNS's approved native voting-power policy. IO does not reconstruct
ballots, age bonus, dissolve-delay bonus, or voting-power multiplier arithmetic.

```text
recipient_weight = canonical_latest_reward_event_reward_shares
```

If no proposal settled in the event, the fallback weight is exact eligible stake
frozen at capture. If proposals settled but eligible canonical shares total
zero, IO issues no reward and does not fall back to full participation.

Rounding is conservative. Dust is reported and remains unissued. Excluded Jupiter governance and protocol-owned neurons cannot receive allocations.

The economics remain unchanged:

```text
redeemable_io_supply =
  total_io_supply
  - protocol_reserve_io
  - non_redeemable_governance_io

redemption_rate =
  liquid_icp_reserve / redeemable_io_supply
```

Only liquid ICP counts as redemption NAV.

Read-only SNS governance evidence captures a frozen cohort of exact eligible
14-day, non-dissolving user neurons and the latest reward-event checkpoint.
Protocol-owned and Jupiter-governance staking Accounts are excluded by exact
effective Account. Close accepts only the exact next single-round event. A
missed or multi-round event allocates nothing and leaves the backed pool in
reserve. Every destination is rechecked immediately before payout; a member
that has become ineligible forfeits its calculated share to reserve dust.

Readiness binds the reviewed Governance module to an exact native reward-event
duration of 1,209,600 seconds and requires both native reward rates to be zero.
The Governance event field is latest-event-only: IO does not reconstruct a
missed event. Proposal-bearing events with zero eligible canonical shares issue
no reward; no-proposal events use exact eligible stake frozen at capture.

The launch settlement persists exact allocations, recipient progress, rounding dust, forfeiture, and total dust in the active two-week receipt. One `resume` performs at most one recipient transition. The immutable transfer precedes a separately persisted refresh submission, and a later canonical observation must show the expected stake increase before progress advances. Each actual recipient consumes one explicit IO fee; every dust component remains in reserve and is never redistributed.
