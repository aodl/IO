# Mainnet Readiness

IO is not ready for production protocol mainnet deployment.

Phase 1 public shell is live on mainnet as `MainnetPreLaunchPublicShell`. Only these public-shell canisters are live:

- `frontend`: `6h2pa-qiaaa-aaaao-qp4fa-cai`
- `io_historian`: `yo47z-piaaa-aaaac-qg3xa-cai`

Frontend URLs:

- `https://6h2pa-qiaaa-aaaao-qp4fa-cai.icp0.io/`
- `https://6h2pa-qiaaa-aaaao-qp4fa-cai.raw.icp0.io/`

This public shell does not mean the IO protocol is live. No value-moving protocol canister is live, no canonical IO SNS ledger exists yet, no IO issuance is live, and no IO redemption is live. `io_stream_manager` and `io_nns_neuron_manager` are not deployed in this phase. The existing IO neuron-owner canister `oae4c-3iaaa-aaaar-qb5qq-cai` and IO neuron `6345890886899317159` remain not touched.

The frontend consumes the Phase 1 historian canister. The historian is a public read model and observability surface, not protocol truth.

Historian freshness monitoring is also a public read model. It is rebuildable, not canonical protocol truth, and not a value-moving authority. Production-shaped ingestion is observation/freshness only and does not activate production adapters. The missing/stale/incomplete states are visible, and missing/stale/incomplete fields must not be interpreted as zero protocol value.

IO protocol is not live. SNS IO ledger remains not launched. Index canisters remain the normal account-history abstraction; raw ledger/archive traversal is not the default path.

Stable storage hardening is local/test hardening only. No value-moving IO canister is deployed to production, and production adapters are not active. Stable-state fixtures are local/test fixtures, not live snapshots. Missing first-install state is different from corrupt upgrade state: first install may default to prelaunch state, while corrupt value-moving state must fail closed. Historian is a rebuildable read model, not protocol truth. Value-moving retry/accounting state is not casually discardable. The protected canister/neuron remain untouched.

Missing before production:

- audited real ICP ledger and index clients built on the `io-ledger-types` boundary;
- audited real IO/SNS ledger and index clients built on the `io-ledger-types` boundary;
- audited real NNS governance client implementing the `io-governance-types` boundary;
- audited real SNS governance client implementing the `io-governance-types` boundary;
- production SNS root/controller lifecycle wiring and official proposal templates;
- final validated official `sns_init.yaml`;
- completed optional local `dfinity/sns-testing` rehearsal;
- completed manual mainnet SNS testflight rehearsal;
- install args validated against final real principals;
- controller handoff plan from Jupiter Faucet governance to IO SNS governance;
- emergency governance process and proposal templates;
- stable-structures migration plan if state grows beyond compact stable snapshots;
- production historian ingestion and freshness monitoring behind the certified frontend;
- external audit of accounting, retry, upgrade, and controller behavior;
- production monitoring for ledger/index lag, archive gaps, journal retries, and historian ingestion freshness.

The current mock-driven journals and scheduler flows are production-shaped but not audited. Downstream transfer paths use `LedgerTransferClient` mock adapters in debug/PocketIC runs; local scan sources can use `LedgerIndexClient` against mock index canisters. No current script deploys to mainnet. The Phase 1 public shell record is stored in `deploy/phase1-mainnet/` and references `release-artifacts/manifest.json`.

The repo contains production-shaped ledger/index and governance Candid models, boundary tests, local/mock SNS governance read snapshotting, local SNS ledger/index value-flow tests, a bounded `io_historian` public read model, a certified frontend asset canister, and mock/PocketIC SNS root/controller lifecycle upgrade tests. We currently run SNS-shaped mock/PocketIC tests. They validate topology/config readiness, read-only mock governance reads, local redemption observation through SNS-index-shaped history, local reward/return transfers through SNS-ledger-shaped accounts, historian debug-ingested observations, certified frontend HTTP routing, and local proposal-shaped root upgrade intent; they are not official SNS launch tests.

We do not currently run the official SNS launch locally in required CI. Official `sns-testing` is optional and heavier. The official SNS launch path uses `dfx sns`; this is not part of required IO workflows. SNS testflight is a future manual/mainnet rehearsal. IO's canonical IO ledger should be the SNS ledger; any IO_TEST ledger is non-canonical. The existing canister that owns IO NNS neuron 6345890886899317159 is not touched by these tests.

Production scan/index adapters, live governance adapters, historian production ingestion adapters, archive traversal, fee policy, index lag alerting, SNS launch configuration, production SNS root/governance wiring, fallback-controller handoff, and duplicate-transfer proof checks against real blocks must be finalized before any mainnet deployment proposal.
