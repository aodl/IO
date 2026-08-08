# Reward allocation

SNS Governance runs one canonical reward event every 86,400 seconds with both
native reward rates set to zero. IO observes each event once and adds raw,
non-overlapping entitlement weight to a bounded per-neuron accumulator.

For a proposal-bearing event, a neuron receives only the canonical
`latest_reward_event_participation.reward_shares` whose event timestamp exactly
matches the current `RewardEvent`. Absent, stale, zero, or malformed shares add
no weight. Settled proposals with zero eligible shares remain a zero-weight
event; IO never substitutes a participation or maturity fallback.

For a no-proposal event, and only when `settled_proposals` is empty, every
currently eligible neuron receives its canonical eligible stake as event
weight. Old or absent participation fields are ignored. This is one virtual
unanimous proposal, not a rolling average.

Eligibility remains an exact staking-product rule: positive stake,
non-dissolving, and exactly 1,209,600 seconds of dissolve delay. Protocol and
Jupiter neurons are excluded. The two-week duration is not an accounting epoch.

When the NNS liquid maturity leg is ready and no batch is pending, IO freezes
the live accumulator into one immutable entitlement batch. Daily observations
continue in a fresh live accumulator while IO waits for actual modulated ICP.
Only received ICP determines the backed IO pool. A zero-weight batch completes
without recipient transfers and leaves the whole backed pool in reserve.

Allocations use checked integer arithmetic. Deterministic rounding dust remains
in reserve, and recipient transfers progress sequentially with upgrade-safe
postcondition checks. Redemption remains independent and available during
observation, backing waits, and payout delays.

The historian may derive trailing participation or APY displays, but those
windows are observations and never monetary inputs. Missing events are recorded
as bounded availability failures and never interpreted as zero or fabricated
participation.

The economics remain:

```text
redeemable_io_supply =
  total_io_supply
  - protocol_reserve_io
  - non_redeemable_governance_io

redemption_rate =
  liquid_icp_reserve / redeemable_io_supply
```

Only liquid ICP counts as redemption NAV.
