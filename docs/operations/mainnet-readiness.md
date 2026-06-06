# Mainnet Readiness

IO is not ready for production mainnet deployment.

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

The current mock-driven journals and scheduler flows are production-shaped but not audited. Downstream transfer paths use `LedgerTransferClient` mock adapters in debug/PocketIC runs; local scan sources can use `LedgerIndexClient` against mock index canisters. No current script deploys to mainnet.

The repo contains production-shaped ledger/index and governance Candid models, boundary tests, local/mock SNS governance read snapshotting, local SNS ledger/index value-flow tests, a bounded `io_historian` public read model, a certified frontend asset canister, and mock/PocketIC SNS root/controller lifecycle upgrade tests. We currently run SNS-shaped mock/PocketIC tests. They validate topology/config readiness, read-only mock governance reads, local redemption observation through SNS-index-shaped history, local reward/return transfers through SNS-ledger-shaped accounts, historian debug-ingested observations, certified frontend HTTP routing, and local proposal-shaped root upgrade intent; they are not official SNS launch tests.

We do not currently run the official SNS launch locally in required CI. Official `sns-testing` is optional and heavier. The official SNS launch path uses `dfx sns`; this is not part of required IO workflows. SNS testflight is a future manual/mainnet rehearsal. IO's canonical IO ledger should be the SNS ledger; any IO_TEST ledger is non-canonical. The existing canister that owns IO NNS neuron 6345890886899317159 is not touched by these tests.

Production scan/index adapters, live governance adapters, historian production ingestion adapters, archive traversal, fee policy, index lag alerting, SNS launch configuration, production SNS root/governance wiring, fallback-controller handoff, and duplicate-transfer proof checks against real blocks must be finalized before any mainnet deployment proposal.
