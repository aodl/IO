# Simplified integration slice

The launch slice is explicit: a user prepares an exact frozen quote, performs
one memo-bound ICRC-1 push into the Stream reserve, and supplies the resulting
block to `settle_redemption`. Canonical proof creates the durable ICP payout
obligation; permissionless recovery pays it exactly once. No allowance,
spender authority, `transfer_from`, or ledger scanner participates.

Jupiter and maturity use authenticated or proof-carrying commands and exact
liquid-receipt permits. The NNS Manager owns the preseeded Dynamic-neuron anchor,
fee capacity, replenishment, and generation-based unwind recovery. Structural
SNS synchronization is independent of daily reward credit.

No value-moving canister discovers intent from index history. Local tests
install command canisters and canonical ledgers and exercise separate effect,
ambiguity, restart, timer, and exact-proof boundaries.
