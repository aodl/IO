# Reward allocation

SNS Governance runs one canonical reward event every 86,400 seconds with both
native reward rates set to zero. Each successfully observed, non-skipped event
adds one fixed `10^18` policy-credit opportunity to a bounded, non-overlapping
per-neuron accumulator.

For a proposal-bearing event, IO sums canonical current-event shares across all
neurons, including excluded and ineligible neurons. An eligible neuron receives
only the normalized fraction represented by its canonical
`latest_reward_event_participation.reward_shares` whose event timestamp exactly
matches the current `RewardEvent`. Absent, stale or zero shares add no credit;
a malformed current-event value fails closed. A day with zero canonical shares
forfeits its whole policy opportunity; IO never substitutes a participation or
maturity fallback.

For a no-proposal event, and only when `settled_proposals` is empty, every
currently eligible neuron receives its canonical eligible stake as event
weight within the fixed daily opportunity. Old or absent participation fields
are ignored. This is one virtual unanimous proposal, not a rolling average. If
there are no eligible neurons, the whole opportunity is forfeited.

Eligibility remains an exact staking-product rule: positive stake,
non-dissolving, and exactly 1,209,600 seconds of dissolve delay. Protocol and
Jupiter neurons are excluded. The two-week duration is not an accounting epoch.

When the pooled parent has sufficient ordinary maturity and no batch is
pending, IO freezes the live accumulator into one immutable entitlement batch
and persists a `DisburseMaturity(100%)` intent. The canonical NNS pending
disbursement establishes the exact nominal amount captured by that command;
later reward accrual belongs to a future batch. Daily observations continue in
a fresh live accumulator while IO waits for the actual modulated ICP Mint.
Only the proved Mint determines the backed IO pool. Its permanent credit is
proved first and its claim leg then enters the common liquid-first receipt.
Before recipient allocation, the batch's eligible-credit fraction determines
the eligible pool; excluded, ineligible and unassigned fractions remain in
reserve. A zero-eligible-credit batch completes without recipient transfers
and forfeits the whole backed pool.

Allocations use checked integer arithmetic. Deterministic rounding dust remains
in reserve, and recipient transfers progress sequentially with upgrade-safe
postcondition checks. Observation and backing waits do not block redemption;
the exact monetary fan-out is serialized with redemption.

The historian may derive trailing participation or APY displays, but those
windows are observations and never monetary inputs. Missing events are recorded
as bounded availability failures and never interpreted as zero or fabricated
participation.

The economics remain:

```text
claim_bearing_io_supply =
  total_io_supply
  - protocol_reserve_io
  - non_redeemable_governance_io

claim_backing =
  L + P + U + T

claim_rate =
  claim_backing / claim_bearing_io_supply
```

`L` is available redemption liquidity. A redemption quote uses the global
claim rate but separately requires enough `L` to settle the requested ICP;
permanent productive capital remains outside claim backing.
