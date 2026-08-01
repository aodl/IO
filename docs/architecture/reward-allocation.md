# Reward Allocation

## Preserved launch semantics

The simplified executor does not change allocation results. The earning window
is exactly 1,209,600 seconds; stake is frozen; direct and followed participation
count; late stake and top-ups are excluded; forfeiture remains dust without
redistribution; and the full backing target is preserved. Delayed maturity
payout does not change eligibility. Settlement is sequential under the single
active monetary operation.

Two-week maturity may issue backed IO to eligible active IO SNS stakers. Ordinary IO reward eligibility requires exactly 14 days. Stock SNS parameters cap the maximum dissolve delay and can gate voting with a minimum delay, but they do not prevent a user from creating a shorter neuron. Longer-than-14-day reward positions are prevented by the SNS maximum; shorter neurons are technically creatable but receive no IO protocol rewards and do not contribute to the two-week NNS backing target. Rewards use frozen cohort stake multiplied only by closed-proposal participation; no duration or age multiplier exists.

```text
participation_factor =
  eligible_closed_proposals_voted_on / eligible_closed_proposals_total
```

If no eligible proposals closed during the interval, participation is treated as 100%.

Direct and followed votes are both represented by their resulting canonical SNS ballot. Only `Yes = 1` and `No = 2` count; `Unspecified = 0` and unsupported values do not. A reward-eligible proposal counts only when `captured_at < decided_at <= closes_at`. A proposal open before capture is carried by bounded ID and counts if it closes in that interval; a proposal already closed at capture does not.

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

Read-only SNS governance evidence feeds this policy by capturing a frozen cohort of exact reward-eligible 14-day, non-dissolving user neurons and a bounded proposal-window anchor. Protocol-owned and Jupiter-governance staking Accounts are excluded by exact effective Account. New stake and top-ups do not alter the frozen cohort. Every destination is rechecked immediately before payout; a member that has become ineligible forfeits its calculated share to reserve dust.

The launch settlement persists exact allocations, recipient progress, rounding dust, forfeiture, and total dust in the active two-week receipt. One `resume` performs at most one recipient transition. The immutable transfer precedes a separately persisted refresh submission, and a later canonical observation must show the expected stake increase before progress advances. Each actual recipient consumes one explicit IO fee; every dust component remains in reserve and is never redistributed.
