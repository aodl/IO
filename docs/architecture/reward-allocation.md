# Reward Allocation

Two-week maturity may issue backed IO to eligible active IO SNS stakers. The reward policy uses stake-time multiplied by proposal participation.

```text
participation_factor =
  eligible_closed_proposals_voted_on / eligible_closed_proposals_total
```

If no eligible proposals closed during the interval, participation is treated as 100%.

Votes through following count as participation in the model. Accepted and rejected closed reward-eligible proposals count. Open proposals, proposals outside the epoch, proposals before a neuron became eligible, and excluded topics do not count.

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

Read-only SNS governance snapshotting feeds this policy by converting eligible participation summaries into `NeuronSnapshot` values. The conversion is fallible: invalid SNS neuron IDs are excluded and reported, while valid eight-byte local/mock IDs continue through allocation.

Local SNS ledger/index tests route TwoWeekMaturity reward transfers through the local SNS-ledger-shaped `LedgerTransferClient` path and assert recipient account balances. Partial recipient transfer failures retry only incomplete recipients, and rounding dust remains unissued. The local mock ledger exposes fees for interface correctness, but reward allocations are not silently reduced by hidden fee subtraction.
