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
pressure pauses this traversal at its durable exclusive resume cursor. A page
is considered chronologically internally, but candidates from a newer bounded
page may execute before an older page is discovered; no global oldest-first
execution guarantee is made.

Exact current/archive retrieval covers discovered staging blocks, supplied
Jupiter or maturity receipts, and proof of persisted outgoing effects. Generic
index scanning, archive traversal for global history and reconciliation belong
to `io_historian`.
