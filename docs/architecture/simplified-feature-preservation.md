# Pooled claim-backing feature preservation

IO remains not live and its reserved production canisters remain inert.

| Protocol property | Owner and mechanism |
|---|---|
| Claim rate | Stream computes `B/C`, where `B=L+P+U+T` |
| Jupiter | 40% permanent, net 60% liquid claim backing; IO at the pre-event rate |
| Initial staking | Existing liquid claim backing moves to the pooled parent; only fees reduce `B` |
| Two-year maturity | Capture its semantic staging balance delta; split 40/60 into permanent/liquid credits with no IO issuance |
| Two-week maturity | Capture its distinct semantic staging balance delta; use the shared Jupiter paired-inflow split and allocate backed IO to the frozen batch |
| Reconciliation | One daily generation, one immediate NNS command, up to 32 passive cohorts |
| Sticky cancellation | Precommit cancels without a fee; postcommit child lifecycle continues independently |
| Rewards | Exact daily entitlements with global pooled reward-coverage gating |
| Redemption | `B/C` quote and independent spendable-`L` availability check |
| Observability | Historian and frontend expose projections without monetary authority |

The replacement keeps no source-event history, user-to-child principal map,
fee-loss counter, reimbursement debt, fee-reserve subsidy state, target queue,
or old launch-state migration. Exact fees paid from a claim-backing bucket
reduce `B` once; fees paid from permanent capital reduce permanent capital.
