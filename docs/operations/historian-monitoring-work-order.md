# Historian production ingestion work order

The historian is a rebuildable, non-authoritative read model. It must never authorize issuance, redemption, reserve movement, neuron commands, lifecycle activation or launch state.

Implement the smallest adapters in this order:

1. Observe release manifest hashes and canonical canister module hashes/controllers. Record unavailable and mismatch distinctly; never coerce either to zero or healthy.
2. Observe SNS ledger total supply and protocol-reserve balance, ICP ledger liquid balance, and their configured exact Accounts. Derive redemption rate from one coherent observation batch.
3. Observe SNS index status and the reserve/account histories needed to explain ledger movements. Discover archives through Root/ledger responses and record ranges when present; do not add a monetary scanner.
4. Observe stream status: latest reward event/skip classification, processed/missed counts, live and pending policy/eligible totals, immutable batch generation, active operation and Paused/Ready state.
5. Observe NNS-manager status: ordinary/staked maturity, preparation phase, passive unwind child, active operation, target/readiness state and stuck reason. Governance remains canonical.
6. Observe SNS/NNS Governance parameter freshness and exact expected module hashes, plus SNS Root dapp membership and controllers.
7. Publish freshness/watermark state and explicit retryable errors. A missing source remains missing, never a fabricated balance or proof of absence.

Prefer deleting frozen-cohort, proposal-ratio and scanner compatibility fields from historian presentation before adding new fields. Add a production query only when no existing narrow status method or canonical ledger/governance query supplies the required observation. Mainnet adapter activation and endpoint selection are separate authorized work.

Completion evidence is: source-shaped adapter unit tests, PocketIC observations against final compatible SNS/NNS topology, same-Wasm historian upgrade, production DID/debug separation, `did_surface`, `validate_historian_freshness`, and `validate_stable_storage` passing.
