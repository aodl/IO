# Stream manager

The stream manager owns the IO reserve, liquid ICP Account, direct ICRC-2 redemption, proof-bound liquid receipts, exact reward cohorts and one typed active monetary operation. Before returning a receipt permit it reserves `ReceiptPreparation`, reads canonical supply/reserve/exclusions/liquid/fee facts, and freezes an immutable pre-receipt backing snapshot. Later donations cannot change Jupiter or reward issuance.

Reward coordination uses one active cohort and one pending closed cohort concurrently. Closing moves exact participation evidence to pending, binds the generation to NNS maturity, and permits immediate capture of the next interval. One one-shot deadline timer invokes the same permissionless close transition; no interval timer or monetary scheduler exists.

Redemption pulls IO from the authenticated caller Account directly to reserve. A separate `resume` pays ICP with `icrc1_transfer`; another read-only progression verifies postconditions and commits. It never accepts a payout destination, scans an index, maintains replicated balances, or refunds unsupported direct transfers.

The production service is limited to `redeem`, receipt preparation/completion, `resume`, exact proof, governance pause/readiness and local `get_status`.
