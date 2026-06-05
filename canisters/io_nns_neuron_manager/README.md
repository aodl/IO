# io_nns_neuron_manager

NNS-only operational canister. Intended deployment target for the already-created canister principal `oae4c-3iaaa-aaaar-qb5qq-cai`, which controls IO's 2-year NNS neuron `6345890886899317159`.

## Role

- NNS neuron mechanics only.
- Manages the 2-year protocol neuron.
- Manages the pooled 2-week NNS neuron.
- Manages temporary unwind child neurons.
- Disburses maturity/principal to IO stream-manager accounts/subaccounts with distinguishable memos/subaccounts.

This canister should not calculate IO issuance. This canister should not inspect IO SNS proposal participation. This canister should not expose broad production state APIs.

## Known Constants

```text
2-year NNS neuron id:
6345890886899317159

Controller canister principal:
oae4c-3iaaa-aaaar-qb5qq-cai
```
