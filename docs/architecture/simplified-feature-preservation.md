# Simplified execution feature preservation

IO remains not live and its reserved production canisters remain inert.

| Protocol property | Simplified owner and mechanism |
|---|---|
| Jupiter 40/60 economics | NNS manager applies checked exact split to the proved deposit |
| Permanent two-year NNS capital | NNS manager owns and verifies the protected neuron |
| Two-year maturity | Stake 40%; disburse all remaining maturity directly to liquid ICP |
| Pooled two-week staking | NNS manager owns one pooled position and latest desired target |
| Rebalance and unwind | One immediate operation and one pending unwind child |
| Exact two-week eligibility | `io-sns-reward-boundary`; evaluated at each daily event |
| Daily entitlement accounting | Canonical event shares or empty-event stake fallback in one bounded accumulator |
| Backed reserve issuance | Stream manager verifies canonical pre/post balances |
| User redemption | Exact-account ICRC-2 pull to reserve followed by exact-account ICP payout |
| SNS fee policy | Standard fee burn with explicit current fees |
| SNS governance control | Exact governance principal controls pause/configuration by upgrade |
| Observability | Historian scans ledgers/indexes and frontend reports freshness |

## Removed mechanisms

Feature preservation does not preserve experimental machinery. Value-moving
canisters do not scan indexes, maintain account-history cursors, classify global
liabilities, infer source events, refund unsupported transfers, or prove global
absence. Redemption pulls IO directly into reserve and has no intermediate custody or compensating transfer path.
Only launch stable schema V1 is supported.

## Reward invariants

Eligibility requires positive stake, non-dissolving state and exactly 1,209,600
seconds of dissolve delay. Daily proposal-bearing events use exact current-event
canonical shares. An event with no settled proposals uses current eligible
stake; a proposal event with zero eligible shares adds zero. Raw weights
accumulate once with no moving average or ballot reconstruction.

The immutable pending entitlement batch is the exact maturity-preparation replay
intent. Below-threshold, paused, pending, or ambiguous NNS work leaves it visible
while later daily events accumulate separately. Observation and backing waits
leave redemption available; exact reserve-transfer fan-out is serialized with
redemption, and best-effort refresh cannot prolong that serialization.
