# Prepared-push redemption frontend contract

IO remains inert and not live. This is the required launch interaction, not an
activation claim.

1. Read the IO fee and caller replay/nonce state.
2. Ask `prepare_redemption` for the exact source Account, reserve destination,
   principal amount, IO/ICP fees, frozen gross/net quote, expiry, nonce,
   redemption ID, and deterministic memo.
3. Display the frozen quote and request affirmative wallet consent.
4. Submit one wallet `icrc1_transfer` from the prepared subaccount to reserve
   with the exact amount, fee, memo, and creation time.
5. Capture its block index and call `settle_redemption` to exact-prove it.
6. Display coarse `Pending`, `Completed`, or `Stuck`; expose permissionless
   recovery without presenting internal durable phases as a compatibility API.

Preparation performs no transfer and creates no ICP debt. A transfer made
inside the preparation window may settle later. A transfer created after expiry
or with the wrong source, destination, amount, fee, memo, or spender is not a
redemption. Once the matching block is proved, the payout obligation cannot
expire or disappear; unexpected missing liquidity becomes recoverable
`PayoutOwed`, not cancellation or another requested IO push.

The ICP payout always returns to the same caller/subaccount. The UI does not
offer an arbitrary destination, allowance, approval, spender, or pull mode.
Arbitrary unsupported transfers are not recovered by a ledger scanner.

Protocol history comes from the Historian. The UI discloses stale, missing, or
incomplete sources and never feeds Historian balances into a monetary command.
