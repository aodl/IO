# Stream Manager

The Stream Manager owns the IO reserve, spendable liquid ICP backing, direct
ICRC-2 redemption, canonical SNS backing/reward observation, one pending
entitlement batch, and one serialized monetary operation.

Its canonical snapshot brackets IO/ICP ledger and SNS reads with two identical
NNS observations. It derives claim-bearing supply `C`, liquid backing `L`,
pooled parent principal `P`, live-child principal `U`, exact in-transit backing
`T`, total backing `B=L+P+U+T`, structural active stake `A_backing`, and the
prospectively eligible subset `A_reward`. Governance supplies neuron identity
and structural state; the IO ledger staking Account supplies stake value.

Each successful daily observation refreshes the bounded, sorted neuron
registry and stores one latest no-effect reconciliation checkpoint. The same
durable one-shot reward timer wakes reward and backing work. There is no target
queue or additional scheduler. Reward allocation is allowed only when pooled
principal covers `floor(A_reward*B/C)`.

Redemption quotes `floor(user_io*B/C)` and separately requires spendable `L`.
Insufficient liquidity returns a typed shortfall before IO is pulled or a nonce
is consumed. A valid operation retains exact allowance, transfer-intent,
deduplication, replay, and postcondition proofs.

Jupiter and two-week maturity enter through one paired-backing receipt. The
receipt is identified by the authenticated NNS Manager's operation sequence,
exact claim credit, and recipient policy: Jupiter or one frozen entitlement
generation. It freezes pre-inflow economics and the bounded recipient vector
before the credit becomes redeemable. Two-year maturity is ordinary unpaired
yield and enters liquid backing without a receipt. IO-ledger staking balances
remain authoritative when an ancillary SNS `ClaimOrRefresh` is delayed.

Install and post-upgrade state are Paused. Reviewed unpause is required before
the existing one-shot timer is armed. IO remains inert and prelaunch.
