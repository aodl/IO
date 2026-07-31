# ADR: NNS authority location

- Status: accepted for simplified launch design
- Date: 2026-07-31

NNS neuron `6345890886899317159` has immutable controller authority at `oae4c-3iaaa-aaaar-qb5qq-cai`. Hotkeys do not supply stake and disbursement authority. The simplified production NNS manager is therefore intended eventually to run at that existing controller canister.

The reserved canister `tatch-ciaaa-aaaar-qb7wq-cai` remains `ReservedNotLive`, inert and unused. The design rejects an `oae4c` to `tatch` authority adapter unless a later complexity-exception ADR, supported by an explicitly authorized mainnet audit, demonstrates that the full manager cannot run at `oae4c`.

The existing module, controllers and stable state at `oae4c` require a separately approved mainnet audit. This ADR authorizes no inspection, deployment, upgrade, funding or other mainnet action.
