# Anchored dynamic-backing E2E coverage matrix

| Behavior | Required gate |
|---|---|
| Paused install and strict launch state | Stream/NNS same-Wasm PocketIC upgrades plus old-state rejection |
| Jupiter 40/60 | Exact source block, fee-reduced permanent/liquid credits, pre-event rate, replay |
| Dynamic bootstrap | Memo-0 Account collision, exact 10 ICP anchor, hostile dust surplus, claimed parent policy, and no IO issuance |
| Structural scheduling | 12-hour independent structural observation, reward-event fencing, same-generation 60-second reconciliation retry, restart/upgrade reconstruction |
| Following rewards | Exact candidate follow ballot, nonzero maturity, voting-power refresh |
| Two-week paired maturity | Semantic balance capture, donation inclusion, 40/60 fees, pre-rate issuance, recipient settlement, Account isolation |
| Two-year maturity | Semantic balance capture, anchor then permanent-shortfall replenishment, nonrecursive reimbursement fees, 40/60 remainder, no IO issuance |
| Sticky cohorts | Split/start separation, cancellation, readiness, return, maturity cleanup |
| Cohorts | More than 32 historical generations, one aggregate child/generation, natural 29-live healthy bound, ready-child priority and exact return |
| Redemption | Monotone `B/C` frozen quote, exact prepared ICRC-1 push proof, durable payout obligation, delayed-liquidity recovery and exact replay |
| Refresh lag | Ledger stake authoritative despite ancillary refresh failure |
| Failure/upgrade | Submitted/proved phases, ambiguous effects, expired deduplication, exact proof |
| Boundary | Independent Governance/Ledger pins and exact candidate behavior |
| Observability | Historian/frontend distinguish rate, liquidity, pool, unwind, and permanent capital |

The checked-in historical SNS evidence remains proof of the superseded model.
Corrected-economics canonical evidence is intentionally absent until a separately
authorized complete rehearsal succeeds.

Historical compatibility inventory still names the real SNS ledger and these
labels: Installed direct-reserve redemption, Exact reward allocation, and
Historian separation.
Scanner-era tests are historical; these labels keep coverage provenance visible
without making the superseded economics normative.
