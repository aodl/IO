# Launch monetary policy

Claim backing is `B = L + P + U + T`. For redemption, `C = total IO supply - IO reserve - excluded IO`, the immutable quote is `floor(redeemed IO * B / C)`, and spendable liquid `L` is the independent liquidity limit. The approval fee is already burned; the transfer-from fee burns during the pull and benefits remaining holders. The request must satisfy `redeemed IO + IO fee <= C`, gross ICP must exceed the payout fee, and net ICP must meet the caller minimum.

Economic meaning follows the protocol-controlled Account holding fungible ICP. IO does not track that ICP's upstream provenance after custody. The fixed two-week and two-year maturity staging subaccounts are domain-separated and owned by the NNS Manager. After canonical maturity finalization, the complete positive Account balance freezes once. Exact delivery debits the whole capture, so late value left behind and donations present before the next operation are consumed by that next operation under the Account's semantics.

Jupiter and two-week maturity use the same checked paired-inflow transformation: captured ICP is split into 40% permanent gross and 60% claim gross, each exact transfer fee is deducted once, and backed IO is frozen at the pre-inflow `B/C` rate. Jupiter sends the IO to its configured recipient; two-week maturity allocates it to the frozen entitlement generation. Two-year maturity uses the same physical 40/60 split but issues no IO, so its claim credit is ordinary yield.

IO still proves ambiguous irreversible outgoing effects. Exact Ledger transfers, NNS Split, child Disburse, and cached-stake reflection retain the evidence needed to prevent duplicate execution or asset loss.
