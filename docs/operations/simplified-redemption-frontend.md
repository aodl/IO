# Semantic-staging redemption frontend contract

The frontend is an authenticated convenience client, not a monetary authority
or activation claim.

1. Obtain and display `get_redemption_staging_account()`.
2. Read the canonical IO fee, Stream's
   `get_minimum_redemption_io_e8s()` value, and the selected Account balance.
   Reject non-integer, non-positive, over-bound, below-minimum, or unfunded
   input before wallet consent.
3. Show explicit wallet consent for one ordinary ICRC-1 transfer from the
   selected canonical Account to staging.
4. Submit the amount with the ledger fee, a client-retained immutable
   `created_at_time`, and no special redemption memo, canister preparation,
   expiry, quote bound, or allowance. An ambiguous transport retry reuses the
   exact payload; `Duplicate` identifies the original successful block.
   Persist the submitted/possibly-effective dispatch state before invoking the
   ledger so page teardown cannot restore a definitely-unsent state.
   If an earlier response was ambiguous, a later `TooOld`, fee, or temporary
   error does not prove absence: retain the original identity, stop blind
   retransmission, and require wallet/ledger-history review. A definitive
   rejection of the initial call may be cleared for a separately consented
   request.
5. Call no-argument `process_redemptions()` once as an optional fast path.
6. Display `Pending` or `RateLimited` as safely staged work and explain that the
   same worker runs automatically approximately once per minute.
7. Retain the staging block before prompting the worker. Worker transport or
   protocol failure displays `IO staged; processing pending`, never transfer
   failure. Submit, manual check, reload, and status refresh use the same exact
   source-block/effective-Account match against both worker output and Stream's
   bounded latest completion. Another user's completion remains local Pending.
   Every post-await callback rereads the Account-scoped receipt state and
   merges evidence by immutable intent; an older observation or rejection
   cannot overwrite a newer submitted, acknowledged, or completed receipt.
   A confirmed completion is cached locally before a deliberately consented
   same-amount request receives a new client identity.

Any pre-transfer rate is indicative. Stream freezes the final
`floor(X * B / C)` gross only after canonical ledger proof, fresh coherent
economics, matching configured fees, and sufficient whole-gross liquid ICP.
The final net subtracts the canonical ICP payout fee.

The user's initial IO transfer fee is an ordinary ledger burn and is not
reimbursed. Staged IO remains claim-bearing until canonical payout success, so
index lag, fee drift, or insufficient liquid ICP delays processing without
creating debt. After payout, Stream exact-sweeps the staged amount into reserve;
the sweep's IO fee is handled by ordinary supply/reserve conservation.

The endpoint accepts no block, Account, amount, destination, quote, memo, nonce,
or fee. Anyone may prompt it, but a durable global cooldown and fixed per-run
scan/queue bounds protect cycles. The canonical staged-transfer block fixes the
source payout Account.

Tiny and unsupported transfers create no payout or automatic refund and do not
block later candidates. Their IO remains in staging and in claim-bearing `C`.
