# Fee and dust accounting

Every ICP fee has exactly one economic classification.

| Fee | Classification | Accounting |
| --- | --- | --- |
| Stream liquid → Dynamic top-up | Claim-backing internal | Consume exact anchor capacity when the effect is canonically established. |
| NNS Split | Claim-backing internal | Consume anchor at sticky child commitment. |
| Committed future child Disburse fee | Claim-backing internal | Reserve/consume anchor once when unavoidable; do not consume again at Disburse. |
| Jupiter/TwoWeek/ordinary TwoYear claim leg | Claim-backing internal | Consume anchor exactly once. |
| Jupiter/TwoWeek/ordinary TwoYear permanent leg | Permanent-capital internal | Increment permanent fee shortfall exactly once. |
| Anchor/permanent reimbursement transfers | Fresh maturity cost | Deduct from newly realised TwoYear maturity; create no recursive debt. |
| Redemption ICP payout | User quote cost | Deduct from gross payout; no anchor or permanent entitlement. |
| IO Ledger transfer fee | IO Ledger burn | Reduces ledger supply under ledger semantics; never ICP fee debt. |
| External Jupiter or anchor-seed sender fee | External | Outside IO internal accounting. |

For a qualifying claim fee `f`, economic reclassification decreases
`anchor_available` by exactly `f`; the corresponding excluded protocol capital
replaces the physically destroyed claim backing, so `B` and `B/C` do not fall.
Insufficient capacity stops before the next irreversible fee-bearing effect.

Permanent fees do not affect `B`; they increase only
`permanent_fee_shortfall_e8s`. Realised TwoYear maturity restores the anchor up
to the fixed 10-ICP target, then restores permanent shortfall. Transfer fees for
those restorations consume fresh maturity and never become a new shortfall.

SNS IO transfer fees use the ledger's standard burn policy. Reward recipient
rounding remains IO in reserve. Unsolicited Dynamic-parent ICP is excluded
surplus and neither claim backing nor fee entitlement. Intentional fee changes
require pause, no unsafe unresolved effect, reviewed configuration, canonical
verification, and readiness restoration.
