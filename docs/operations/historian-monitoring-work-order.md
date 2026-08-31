# Historian production ingestion work order

The historian is a rebuildable, non-authoritative read model. It must never authorize issuance, redemption, reserve movement, neuron commands, lifecycle activation or launch state.

Implemented canonical adapters, in refresh order:

1. Observe release manifest hashes and canonical canister module hashes/controllers. Record unavailable and mismatch distinctly; never coerce either to zero or healthy.
2. Observe SNS ledger total supply and protocol-reserve balance, ICP ledger liquid balance, and their configured exact Accounts. Derive redemption rate from one coherent observation batch.
3. Observe SNS index status and the reserve/account histories needed to explain ledger movements. Discover archives through Root/ledger responses and record ranges when present; do not add a monetary scanner.
4. Observe stream status: latest reward event/skip classification, processed/missed counts, live and pending policy/eligible totals, immutable batch generation, active operation and Paused/Ready state.
5. Observe NNS-manager status: baseline/generation state, active operation, exact latest two-week target/status and passive-unwind principal. Query the two configured local NNS neuron IDs through public NNS Governance only for stake, staked maturity, dissolve delay and state; ordinary maturity is not fabricated when the public query does not expose it.
6. Observe SNS Governance parameter/reward freshness, public NNS Governance build metadata, exact expected SNS/IO module hashes, and SNS Root dapp membership/controllers.
7. Publish freshness/watermark state and explicit retryable errors. A missing source remains missing, never a fabricated balance or proof of absence.

Frozen-cohort, proposal-ratio, generic ingestion and scanner presentation fields are deleted from the current model. Mainnet configuration/activation remains separate authorized work.

Current completion evidence is the selector-bound schema-v2
`deploy/local-sns-rehearsal/evidence/2026-08-31-9211341-anchored-dynamic`
package. It records the current historian configuration/module identity,
canonical Account map, fresh ledger/index/Governance/Root observations, and
convergence with `L=50,000,149,899,251`, `P=49,999,750,101,249`, `U=0`,
`T=0`, `B=99,999,900,000,500`, and `C=20,000,079,960,000`, while preserving
production DID/debug separation and passing historian, stable-storage and
committed-evidence validators. The immutable
`2026-08-12-4320fdf-monitoring` package remains secondary historical evidence
for its own prior-to-current hash-changing historian upgrade and recorded
release; it was not rebound.
