# Simplified E2E coverage matrix

| Behavior | Current gate | Remaining |
|---|---|---|
| Paused install and narrow APIs | Stream/NNS PocketIC smoke | Readiness negative matrix |
| Real SNS ICRC-2 primitives | Pinned real SNS ledger test | Official source rehearsal evidence |
| Installed direct-reserve redemption | Pinned real IO ledger + canonical local ICP ledger | Upgrade, stale callback and exact-proof cases |
| Exact reward allocation | All 18 `io_reward_policy` tests | Installed serialized fan-out |
| Jupiter 40/60 | Release IO Wasms + pinned real NNS Governance/ICP ledger: exact deposit, stake/refresh, liquid receipt, fixed IO settlement, fee and replay | Real transport-ambiguity injection |
| Direct maturity | Release manager + pinned real NNS Governance: two compounded StakeMaturity/DisburseMaturity cycles, delayed Mint, unchanged IO supply | Real SNS trigger and adverse modulation fixture |
| Target/unwind | Pinned real split/passive dissolve/maturity/direct disburse proof with upgrades | Separate real merge-back interruption fixture |
| Historian separation | DID and source guardrails | Simplified status ingestion |

Scanner-era tests are historical and are not launch coverage.
