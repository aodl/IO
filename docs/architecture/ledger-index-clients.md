# Ledger and index boundaries

Value-moving canisters use canonical ledger queries for balances, fees,
standards, total supply, and exact blocks. Prepared redemption proves the
caller-supplied ICRC-1 push block directly; it uses no allowance or
`transfer_from`. Value-moving canisters never use account-history indexes to
discover monetary intent.

Exact current/archive retrieval is limited to supplied Jupiter or maturity receipts and proof of a persisted stuck transfer. Index scanning, archive traversal for global history and reconciliation belong to `io_historian`.
