# Mainnet readiness

IO is not live. ProductionActive is unavailable and no production timer exists. Reserved production canisters are not deployment targets in this work.

The NNS manager design belongs at existing neuron controller `oae4c-3iaaa-aaaar-qb5qq-cai`; reserved `tatch-ciaaa-aaaar-qb7wq-cai` remains inert and unused. A later explicitly approved audit must inspect module/controllers/stable state and authorize any clean install or upgrade.

Launch readiness requires all simplified execution, installed real-ledger, NNS authority, upgrade/failure, official SNS evidence and GitHub CI gates to pass. Unsupported transfers remain unsupported by design.

Controlled PocketIC evidence reaches the protected two-week path through a zero-maturity baseline, independent target reconciliation, one direct unwind, StakeMaturity, DisburseMaturity, delayed exact Mint, proof-bound stream receipt and recipient settlement using pinned official NNS Governance and ICP ledger Wasms. This is local evidence only and grants no mainnet authority. Production installation remains Paused and requires a separate reviewed launch decision.

Local rehearsal evidence concerns a real SNS-created stack installed through SNS-W with the test symbol IO_TEST. It is non-canonical evidence for a local protocol reserve; IO is not launched on mainnet.

Reserved mapping record: `io_stream_manager` `thset-pqaaa-aaaar-qb7wa-cai`; `io_nns_neuron_manager` `tatch-ciaaa-aaaar-qb7wq-cai`; `io_historian` `tjqj3-uaaaa-aaaar-qb7xa-cai`; `frontend` `torpp-zyaaa-aaaar-qb7xq-cai`. This inventory does not grant authority to `tatch`.

Historian remains a public read model: rebuildable, not canonical protocol truth, and not a value-moving authority. IO protocol is not live; SNS IO ledger remains not launched. `missing/stale/incomplete` observations remain explicit, and index canisters are historian inputs only.
