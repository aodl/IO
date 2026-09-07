# Launch monetary policy

Claim backing is `B = L + P + U + T`. At stable boundaries, `C = total IO supply - formal IO reserve - nonredeemable governance IO`. IO sent to the fixed redemption staging Account remains in `C`. The user's ordinary staging-transfer fee burns under ledger rules and is neither reconstructed nor reimbursed. After exact block proof and only when `L` covers the whole payout, Stream freezes `gross = floor(staged IO * B / C)` and `net = gross - canonical ICP fee`.

Canonical payout success reduces `B` by gross and economically retires the staged IO. Until the exact staging-to-reserve sweep succeeds, `C` additionally excludes that one paid-but-unswept amount. The sweep sends staged amount minus the canonical IO fee, debits staging by the full staged amount, burns the fee, and increases reserve by the remainder; physical supply/reserve accounting then replaces the temporary exclusion without changing `B` or `C`. No redemption fee consumes the Dynamic anchor.

Economic meaning follows the protocol-controlled Account holding fungible ICP. IO does not track that ICP's upstream provenance after custody. The fixed two-week and two-year maturity staging subaccounts are domain-separated and owned by the NNS Manager. After canonical maturity finalization, the complete positive Account balance freezes once. Exact delivery debits the whole capture, so late value left behind and donations present before the next operation are consumed by that next operation under the Account's semantics.

Jupiter and two-week maturity use the same checked paired-inflow transformation: captured ICP is split into 40% permanent gross and 60% claim gross, each exact transfer fee is deducted once, and backed IO is frozen at the pre-inflow `B/C` rate. Jupiter sends the IO to its configured recipient; two-week maturity allocates it to the frozen entitlement generation.

Two-year maturity is anchor-first and issues no IO. Its fresh semantic capture first restores any Dynamic-anchor deficit, including that restoration transfer's fee from the same fresh capture. Only the valid remainder receives the ordinary 40% permanent / 60% claim split; each leg enters its destination net of its own delivery fee. Anchor restoration creates no recursive reimbursement debt.

IO still proves ambiguous irreversible outgoing effects. Exact Ledger transfers, NNS Split, child Disburse, and cached-stake reflection retain the evidence needed to prevent duplicate execution or asset loss.
