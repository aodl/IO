# Stream manager

The stream manager owns the IO reserve, liquid ICP Account, direct ICRC-2 redemption, proof-bound liquid receipts, exact reward cohorts and one typed active monetary operation.

Redemption pulls IO from the authenticated caller Account directly to reserve. A separate `resume` pays ICP with `icrc1_transfer`; another read-only progression verifies postconditions and commits. It never accepts a payout destination, scans an index, maintains replicated balances, or refunds unsupported direct transfers.

The production service is limited to `redeem`, receipt preparation/completion, `resume`, exact proof, governance pause/readiness and local `get_status`.
