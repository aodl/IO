# Mainnet Readiness

IO is not ready for production mainnet deployment.

Missing before production:

- real ICP ledger and index clients;
- real IO/SNS ledger and index clients;
- real NNS governance client;
- real SNS governance client;
- install args validated against final real principals;
- controller handoff plan from Jupiter Faucet governance to IO SNS governance;
- emergency governance process and proposal templates;
- stable-structures migration plan if state grows beyond compact stable snapshots;
- certified historian/frontend plan;
- external audit of accounting, retry, upgrade, and controller behavior;
- production monitoring for ledger/index lag, archive gaps, and journal retries.

The current mock-driven journals and scheduler flows are production-shaped but not audited. No current script deploys to mainnet.
