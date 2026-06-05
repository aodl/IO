# Mainnet Readiness

IO is not ready for production mainnet deployment.

Missing before production:

- audited real ICP ledger and index clients built on the `io-ledger-types` boundary;
- audited real IO/SNS ledger and index clients built on the `io-ledger-types` boundary;
- audited real NNS governance client implementing the `io-governance-types` boundary;
- audited real SNS governance client implementing the `io-governance-types` boundary;
- install args validated against final real principals;
- controller handoff plan from Jupiter Faucet governance to IO SNS governance;
- emergency governance process and proposal templates;
- stable-structures migration plan if state grows beyond compact stable snapshots;
- certified historian/frontend plan;
- external audit of accounting, retry, upgrade, and controller behavior;
- production monitoring for ledger/index lag, archive gaps, and journal retries.

The current mock-driven journals and scheduler flows are production-shaped but not audited. Downstream transfer paths use `LedgerTransferClient` mock adapters in debug/PocketIC runs; scan sources still use mock `debug_get_transactions`. No current script deploys to mainnet.

The repo contains production-shaped ledger/index and governance Candid models and boundary tests, but no mainnet wiring has been performed. Production scan/index adapters, governance adapters, archive traversal, fee policy, index lag alerting, proposal pagination, SNS launch configuration, and duplicate-transfer proof checks against real blocks must be finalized before any mainnet deployment proposal.
