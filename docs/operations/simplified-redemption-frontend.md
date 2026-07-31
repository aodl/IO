# Simplified redemption frontend contract

IO remains inert and not live. This is the required launch interaction, not an
activation claim.

1. Read the IO fee and existing allowance.
2. If an incompatible allowance exists, clear it using
   `expected_allowance`.
3. Approve the exact stream-manager spender account for
   `io_amount + transfer_from fee`, with a short expiry and
   `expected_allowance`.
4. Display that approval burns one IO fee and the later transfer-from burns a
   second IO fee.
5. Call `redeem` with the user's exact subaccount, sequential nonce, expiry,
   minimum ICP output, and IO/ICP fee maxima.
6. Display `Busy`, `Paused`, and `Stuck` without suggesting funds are absent or
   an operator can mark work complete.

The ICP payout always returns to the same caller/subaccount. The UI must not
offer an arbitrary payout destination. Unsupported direct transfers create no
claim and are not automatically refunded.

Protocol history comes from the historian. The UI must disclose stale, missing,
or incomplete historian sources and must never feed historian balances into a
monetary command.
