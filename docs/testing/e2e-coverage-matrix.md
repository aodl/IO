# Simplified E2E coverage matrix

| Behavior | Current gate | Remaining |
|---|---|---|
| Paused install and narrow APIs | Stream/NNS PocketIC smoke | Readiness negative matrix |
| Real SNS ICRC-2 primitives | Pinned real SNS ledger test | Official source rehearsal evidence |
| Installed direct-reserve redemption | Pinned real IO ledger + canonical local ICP ledger | Upgrade, stale callback and exact-proof cases |
| Exact reward allocation | All 18 `io_reward_policy` tests | Installed serialized fan-out |
| Jupiter 40/60 | Checked pure arithmetic | Full protected-neuron operation |
| Direct maturity | Typed policy tests | Real NNS governance and delayed Mint proof |
| Target/unwind | V1 typed state | Real split/merge/disburse lifecycle |
| Historian separation | DID and source guardrails | Simplified status ingestion |

Scanner-era tests are historical and are not launch coverage.
