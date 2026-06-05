# io_nns_neuron_manager

NNS-only operational canister. Intended deployment target for the already-created canister principal `oae4c-3iaaa-aaaar-qb5qq-cai`, which controls IO's 2-year NNS neuron `6345890886899317159`.

It should manage the permanent 2-year neuron and the pooled 2-week staking neuron, including split/dissolve/merge/disbursement scheduling. It should not calculate IO issuance.
