# Simplified integration slice

The launch slice is explicit: a user performs one ordinary ICRC-1 transfer into
the fixed Stream redemption staging Account. Staging remains claim-bearing.
The bounded account-filtered scanner discovers candidates, and the
canonical ledger exact-proves each block before Stream derives the source
Account and amount. A fresh `B/C` quote is frozen only when liquid ICP covers
the gross. Exact payout retires the staged IO economically; exact
staging-to-reserve sweep makes that retirement physical. There is no allowance,
spender authority, pull, special memo, nonce, expiry, or caller-supplied block.

Jupiter and maturity use authenticated or proof-carrying commands and exact
liquid-receipt permits. The NNS Manager owns the preseeded Dynamic-neuron anchor,
fee capacity, replenishment, and generation-based unwind recovery. Structural
SNS synchronization is independent of daily reward credit.

No generic monetary scanner exists. The sole discovery exception observes one
semantic redemption Account and decodes each transaction ID plus only the
transfer destination and amount needed to suppress obviously irrelevant proof
candidates. These fields are availability hints only: every returned ID advances
scanner coverage, while the canonical IO ledger independently proves operation
type, source Account, destination, amount, and all monetary facts before any
effect. Permissionless wake calls perform no external call themselves and have
a heap-global ten-second admission bound on sustained fast-path acceleration.
Local tests
install command canisters and canonical ledgers and exercise separate effect,
ambiguity, restart, timer, and exact-proof boundaries.
