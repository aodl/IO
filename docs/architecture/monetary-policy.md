# Launch monetary policy

For redemption, `D = total IO supply - IO reserve - excluded IO`. The pre-pull quote is `floor(redeemed IO * liquid ICP / D)`. The approval fee is already burned; the transfer-from fee burns during the pull and benefits remaining holders. The request must satisfy `redeemed IO + IO fee <= D`, gross ICP must exceed the payout fee, and net ICP must meet the caller minimum.

Jupiter received ICP is split with checked arithmetic: 40% permanent NNS stake and the remainder liquid backing. Ordinary NNS maturity is handled by `StakeMaturity(40%)` followed by `DisburseMaturity(100% remaining)`; actual modulated ICP is backing. Standard SNS transfer fees burn.
