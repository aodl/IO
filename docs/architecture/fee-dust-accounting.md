# Fee and dust accounting

SNS IO fees use the standard burn policy. The approval fee burns before redemption; the transfer-from fee burns during the direct reserve pull and benefits remaining holders. The ICP payout fee is deducted from gross ICP.

Redemption uses canonical pre-pull balances and checked arithmetic. Reward transfer fees burn one per recipient; forfeiture and rounding remain dust in reserve without redistribution. Staging transfer fees are paid from explicit prefunded fee float and never deducted from backing amounts.

Intentional fee changes require governance pause, no unresolved operation, config upgrade, canonical fee verification and readiness unpause.
