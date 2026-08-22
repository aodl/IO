# Pooled claim-backing E2E coverage matrix

| Behavior | Required gate |
|---|---|
| Paused install and strict launch state | Stream/NNS same-Wasm PocketIC upgrades plus old-state rejection |
| Jupiter 40/60 | Exact source block, fee-reduced permanent/liquid credits, pre-event rate, replay |
| Initial pooled staking | 0/50/100% structural stake, lazy parent, no IO issuance |
| Following rewards | Exact candidate follow ballot, nonzero maturity, voting-power refresh |
| Joint pooled maturity | All-liquid/all-pool/mixed, fee boundaries, reward coverage, recipient settlement |
| Permanent maturity | Stake 40%, actual Mint dynamic claim route, no IO |
| Sticky cohorts | Split/start separation, cancellation, readiness, return, maturity cleanup |
| Capacity | 32 accepted, 33rd no effect, retirement permits later generation |
| Redemption | `B/C` quote, liquid shortfall before IO pull, exact retry |
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
