# NNS neuron manager

The manager is self-bound to its running canister principal and uses distinct Jupiter, two-year maturity, two-week maturity, unwind and operational-fee Accounts. Each sending staging Account requires explicit fee float.

Jupiter stakes checked 40% into the permanent neuron and delivers the remainder as proved liquid backing. Maturity stakes 40% of ordinary maturity and disburses all remaining maturity; actual modulated ICP is backing. A two-week-staker reward-backing command can be prepared only by the stream manager for one exact frozen entitlement-batch generation. Target capacity is only the canonical non-dissolving parent stake; a dissolving child is reported separately. Target growth reports UnderTarget and never consumes liquid backing. Material excess permits one direct unwind child, while fee-sized excess is recorded within conservative unwind tolerance.

The protected parent is the two-week-staker reward-backing NNS neuron, not a
14-day NNS position. Its approved dissolve delay is exactly 252,288,000 seconds
(eight years). The first readiness transition proves the configured parent ID,
exact seeded principal, zero canonical ordinary and staked maturity,
`auto_stake_maturity = false`, no pending maturity, exact non-dissolving delay
and no child ambiguity. That baseline is durable across upgrades. Retained
staked maturity is expected after the baseline, but auto-stake or dissolve-state
drift pauses later preparation.

`reconcile_two_week_backing_readiness` authenticates the stream, persists changed targets and advances an internal target generation independently of entitlement-batch generations. Same-target replay re-queries the parent without advancing either generation. UnderTarget requires separately authorized principal growth. OverTarget uses the one direct unwind. After canonical `StartDissolving`, the exact child becomes passive and the immediate slot is free for maturity on the reduced parent. Merge-back and direct disbursement clear child evidence only after canonical proof. No second child or queue exists.

On Ready, the stream freezes and immediately prepares the same immutable entitlement generation in one update. A post-upgrade Paused manager continues already immutable active or passive maturity work. No target queue, batch queue or second child exists.

Production authority is intended to remain at existing controller `oae4c`; `tatch` is unused. No mainnet operation is authorized.
