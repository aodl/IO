# IO Monetary Policy

The canonical redemption rate is:

```text
liquid ICP reserve / redeemable IO supply
```

Redeemable IO supply excludes protocol reserve IO and the non-dissolvable Jupiter Faucet genesis governance neuron. Normal user-staked IO remains redeemable supply because it can eventually dissolve and redeem.

Any IO transferred from protocol reserve into user circulation must be backed at the pre-event redemption rate.
