# Ledger and index boundaries

Value-moving canisters use canonical ledger queries for balances, fees,
standards, total supply, and exact blocks. Redemption has one deliberately
narrow discovery dependency: bounded account-filtered IO-index history for the
fixed semantic staging Account. Index data cannot authorize value movement;
Stream exact-proves every candidate through the canonical ledger/archive
boundary before creating a payout.

The configured SNS index contract returns newest-first account history and
treats `start` as an exclusive upper cursor. Stream captures a head, retains
the prior committed coverage watermark while walking older pages, and commits
the captured head only after the interval reaches that watermark. Queue
pressure pauses this traversal at its durable exclusive resume cursor.
Candidate discovery is deterministic and bounded, but settlement order is not
FIFO. A candidate without whole-gross liquidity may be deferred behind other
discovered candidates because no quote, reservation, or debt exists before
activation. This keeps smaller payable redemptions reachable without adding
per-user scheduling state.

Stream defines a launch-index-specific width-subtyped DTO containing each
transaction ID plus only `transfer.to` and `transfer.amount`. Destination and
amount are availability hints used to suppress obviously irrelevant proof
candidates; they do not validate a redemption. Scanner coverage advances over
every returned transaction ID. Before any monetary effect, the canonical IO
ledger independently proves operation type, source Account, destination,
amount, and all monetary facts. Stream does not depend on the generic
`io-ledger-types` index client or its direction/lag/status model.

Exact current/archive retrieval covers discovered staging blocks, supplied
Jupiter or maturity receipts, and proof of persisted outgoing effects. Generic
index scanning, archive traversal for global history and reconciliation belong
to `io_historian`.
