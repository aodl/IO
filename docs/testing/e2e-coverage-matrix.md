# Simplified E2E coverage matrix

| Behavior | Current gate | Remaining |
|---|---|---|
| Paused install and narrow APIs | Stream/NNS PocketIC smoke, query-only lifecycle validator rendering, invalid-payload rejection, same-Wasm Paused upgrade preservation, authenticated execution rejection; real SNS-W function `1000` and stream activation proposal `6` | NNS-manager source-shaped readiness fixture |
| Real SNS ICRC-2 primitives | Pinned real SNS ledger test plus SNS-W-created ledger blocks `9`–`11` | Committed sanitized package |
| Installed direct-reserve redemption | Pinned real IO ledger + canonical local ICP ledger; SNS-W production redemption nonce `0`, IO/ICP blocks `11`, identical retry | Upgrade, stale callback and exact-proof cases remain in focused harnesses |
| Exact one-day reward observation | Candidate Governance round `1`, one-round span, settled proposal `9`, canonical shares, fixed normalized credit, zero native maturity and explicit ineligible-neuron forfeiture | Committed sanitized package |
| Exact reward allocation | All 18 `io_reward_policy` tests | Installed serialized fan-out |
| Jupiter 40/60 | Release IO Wasms + pinned real NNS Governance/ICP ledger: exact deposit, stake/refresh, liquid receipt, fixed IO settlement, fee and replay | Real transport-ambiguity injection |
| Direct maturity | Release manager + pinned real NNS Governance: two compounded StakeMaturity/DisburseMaturity cycles, delayed exact Mint, wrong/malformed/replay proof handling, unchanged IO supply, and real auto-stake/dissolve drift rejection; pinned real SNS generic-function proposal trigger with replay rejection; stable validation accepts positive adverse modulation below nominal | An artifact or fixture that makes pinned Governance apply adverse modulation |
| Target/unwind | Pinned real split/passive dissolve/maturity/direct disburse proof with upgrades; real target-rise stop/merge survives an upgrade at `MergePrepared` and proves the fee-adjusted parent | Injected real Governance transport ambiguity for split, merge, and direct disbursement |
| Combined reward lifecycle | Candidate real SNS Governance/Root/ledger + pinned real NNS Governance/ICP ledger + release IO Wasms: skipped-event fail-closed checkpoint, exact no-proposal credit, protected maturity/Mint, three recipients, fees, supply, redemption and upgrades | No monetary-truth mock remains in this composition path |
| Historian separation | DID and source guardrails | Simplified status ingestion |

Scanner-era tests are historical and are not launch coverage.
