# Fee and dust accounting

Every ICP fee has exactly one economic classification.

| Fee | Classification | Accounting |
| --- | --- | --- |
| Stream liquid → Dynamic top-up | Claim-backing internal | Consume exact anchor capacity when the effect is canonically established. |
| NNS Split | Claim-backing internal | Consume anchor at sticky child commitment. |
| Committed future child Disburse fee | Claim-backing internal | Reserve/consume anchor once when unavoidable; do not consume again at Disburse. |
| Jupiter/TwoWeek/ordinary TwoYear claim leg | Fresh value delivery | Deduct from that leg's gross allocation; credit only the net amount. |
| Jupiter/TwoWeek/ordinary TwoYear permanent leg | Fresh value delivery | Deduct from that leg's gross allocation; credit only the net permanent addition. |
| Anchor restoration transfer | Fresh TwoYear maturity cost | Deduct from newly realised capture; create no recursive debt. |
| Redemption ICP payout | User quote cost | Deduct from gross payout; no anchor entitlement. |
| IO Ledger transfer fee | IO Ledger burn | Reduces ledger supply under ledger semantics; never ICP fee debt. |
| External Jupiter or anchor-seed sender fee | External | Outside IO internal accounting. |

For an existing-backing movement fee `f`, economic reclassification decreases
`anchor_available` by exactly `f`; the corresponding excluded protocol capital
replaces the physically destroyed claim backing, so `B` and `B/C` do not fall.
Insufficient capacity stops before the next irreversible fee-bearing effect.

Fresh-value delivery fees occur before that value becomes claim backing or
permanent capital, so the post-fee amount is the new economic credit. Realised
TwoYear maturity restores the anchor up to the fixed 10-ICP target; that
restoration transfer's fee consumes the same fresh capture and creates no debt.
Permanent capital is outside `B`, so its fresh delivery fee cannot lower the
claim rate.

SNS IO transfer fees use the ledger's standard burn policy. Reward recipient
rounding remains IO in reserve. Unsolicited Dynamic-parent ICP is excluded
surplus and neither claim backing nor fee entitlement. Intentional fee changes
require pause, no unsafe unresolved effect, reviewed configuration, canonical
verification, and readiness restoration.
