# ADR: NNS authority location

- Status: accepted for simplified launch design
- Date: 2026-07-31

Two-year protected NNS neuron `10292412127977304661` has controller authority at
`oae4c-3iaaa-aaaar-qb5qq-cai`. Hotkeys do not supply stake and disbursement
authority. The simplified production NNS Manager therefore executes at that
existing controller canister.

The architecture has one execution identity and no authority adapter or
forwarding canister. Any later proposal to introduce one requires a separate
complexity-exception ADR and an explicitly authorized mainnet audit.

The existing module, controllers and stable state at `oae4c` require a separately approved mainnet audit. This ADR authorizes no inspection, deployment, upgrade, funding or other mainnet action.
