# Semantic-staging redemption frontend contract

The frontend is a convenience initiator, not a monetary authority or completion
historian.

1. Read the selected Account balance, canonical IO fee, configured minimum, and
   fixed staging Account. Reject malformed, non-positive, over-bound,
   below-minimum, or unfunded input before consent.
2. Show explicit wallet consent for one ordinary ICRC-1 transfer from the
   selected Account to staging.
3. Submit once with the ledger fee, one `created_at_time`, and no special memo,
   canister preparation, expiry, quote bound, allowance, or browser receipt
   database.
4. Treat `Ok` and an exact ledger `Duplicate` as staged. Call no-argument
   `process_redemptions()` once as a best-effort, coalesced wake. A wake failure
   cannot turn confirmed staging into transfer failure.
5. On a transport exception, say that the transfer outcome is unknown and may
   have succeeded. Do not retry. Direct the user to wallet or ledger history
   before authorizing another redemption.

A same-page `submitting` guard prevents accidental double clicks while consent
or the ledger call is outstanding. There is no Check button, persistent browser
receipt state, completion matching, automatic ambiguous retry, or completion
recovery across reload. Automatic bounded backend discovery is the durable path
for a staging transfer that committed.

Any pre-transfer rate is indicative. Stream computes the final
`floor(X * B / C)` gross only after canonical proof, a fresh coherent snapshot,
matching configured fees, and whole-gross liquidity. Staged IO remains
claim-bearing until settlement. Tiny and unsupported transfers create no payout
or refund and do not block later candidates.
