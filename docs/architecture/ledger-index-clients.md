# Ledger and index boundaries

Value-moving canisters use canonical ledger queries for balances, fees, standards, allowance and exact blocks. They never use account-history indexes to discover monetary intent.

Exact current/archive retrieval is limited to supplied Jupiter or maturity receipts and proof of a persisted stuck transfer. Index scanning, archive traversal for global history and reconciliation belong to `io_historian`.
