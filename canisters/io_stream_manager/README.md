# io_stream_manager

Main IO monetary-policy canister.

## Role

- Owns IO monetary policy.
- Classifies incoming ICP/IO ledger streams.
- Applies the canonical 40/60 split.
- Issues backed IO from protocol reserve when applicable.
- Handles redemption logic by ledger/index-observed IO deposits.

This canister is value-moving. Its production API must remain minimal.

Do not add public dashboard/query endpoints here. Do not add public methods that trust caller-supplied stream kinds in production. Use the debug DID for local/model tests.

## Stream Semantics

`JupiterFaucet`:

- 40% 2-year stake accounting.
- 60% liquid reserve.
- IO to Jupiter Faucet.

`TwoYearMaturity`:

- 40% restake.
- 60% liquid reserve.
- No IO issuance.

`TwoWeekMaturity`:

- 40% restake into pooled 2-week position.
- 60% liquid backing.
- Backed IO to eligible IO SNS neurons.
