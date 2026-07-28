# Reward Allocation

Two-week maturity may issue backed IO to eligible active IO SNS stakers. Ordinary IO reward eligibility requires exactly 14 days. Stock SNS parameters cap the maximum dissolve delay and can gate voting with a minimum delay, but they do not prevent a user from creating a shorter neuron. Longer-than-14-day reward positions are prevented by the SNS maximum; shorter neurons are technically creatable but receive no IO protocol rewards and do not contribute to the two-week NNS backing target. Rewards use frozen cohort stake multiplied only by closed-proposal participation; no duration or age multiplier exists.

```text
participation_factor =
  eligible_closed_proposals_voted_on / eligible_closed_proposals_total
```

If no eligible proposals closed during the interval, participation is treated as 100%.

Votes through following count as participation in the model. Accepted and rejected closed reward-eligible proposals count. Open proposals, proposals outside the cohort period, and excluded topics do not count.

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

Read-only SNS governance snapshotting feeds this policy by capturing a frozen cohort of exact reward-eligible 14-day, non-dissolving user neurons and then summarizing closed-proposal participation for that cohort. New stake, top-ups, and shorter technically creatable SNS neurons join no current reward cohort unless they satisfy the exact eligibility policy at a later capture. A cohort member that is no longer an exact eligible destination at payout forfeits its calculated share to protocol-reserve dust.

Local SNS ledger/index tests route TwoWeekMaturity reward transfers through the local SNS-ledger-shaped `LedgerTransferClient` path and assert recipient account balances. Partial recipient transfer failures retry only incomplete recipients, and rounding dust remains unissued. The local mock ledger exposes fees for interface correctness, but reward allocations are not silently reduced by hidden fee subtraction.
