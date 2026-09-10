# Pooled claim-backing feature preservation

IO remains not live and its reserved production canisters remain inert.

| Protocol property | Owner and mechanism |
|---|---|
| Claim rate | Stream computes `B/C`, where `B=L+P+U+T` |
| Jupiter | 40% permanent, net 60% liquid claim backing; IO at the pre-event rate |
| Initial staking | Existing liquid claim backing moves to the pooled parent without reducing `B`; an eligible relocation fee consumes Dynamic-anchor capacity exactly once |
| Two-year maturity | Capture its complete semantic staging balance; restore the Dynamic-anchor deficit and its restoration fee first, then split only the valid remainder 40/60 into permanent/liquid credits with no IO issuance |
| Two-week maturity | Capture its complete distinct semantic staging balance; use the shared Jupiter paired-inflow split and allocate backed IO to the frozen batch |
| Reconciliation | Independent 12-hour structural generations, one immediate NNS command, and one natural aggregate child per committed generation; ready children have priority and there is no product cohort cap |
| Sticky cancellation | Precommit cancels without a fee; postcommit child lifecycle continues independently |
| Rewards | Exact canonical SNS reward-event entitlements accumulate independently of structural synchronization; one frozen batch proceeds through the single monetary slot when its ordinary backing preconditions hold |
| Redemption | An ordinary transfer to one semantic staging Account remains in `C`; bounded index discovery and canonical block proof precede a fresh `B/C` quote, whole-gross liquid-`L` check, exact payout, and staging-to-reserve sweep |
| Observability | Historian and frontend expose projections without monetary authority |

The replacement keeps no source-event history, user-to-child principal map,
fee-loss counter, reimbursement debt, fee-reserve subsidy state, generic target
queue, generic monetary scanner, or old launch-state migration. Exact fees paid
from an existing claim-backing bucket consume the Dynamic anchor once; fresh
delivery fees reduce the fresh credit, and permanent-leg fees reduce permanent
capital. Redemption fees do not consume the Dynamic anchor.
