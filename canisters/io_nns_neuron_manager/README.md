# io_nns_neuron_manager

NNS-only operational canister. Intended deployment target for the already-created canister principal `oae4c-3iaaa-aaaar-qb5qq-cai`, which controls IO's 2-year NNS neuron `6345890886899317159`.

## Role

- NNS neuron mechanics only.
- Manages the 2-year protocol neuron.
- Manages the pooled 2-week NNS neuron.
- Manages temporary unwind child neurons.
- Disburses maturity/principal to IO stream-manager accounts/subaccounts with distinguishable memos/subaccounts.

This canister should not calculate IO issuance. This canister should not inspect IO SNS proposal participation. This canister should not expose broad production state APIs.

## Production API and Init Args

The production DID is install-args-only:

```did
service : (InitArgs) -> {}
```

`InitArgs` defines the controller canister principal, 2-year NNS neuron id, two-week dissolve delay, initial model principals, model annual bps, and optional placeholder stream-manager target config/memos.

Defaults preserve the known live constants below. Validation rejects empty or malformed controller principal text, a zero 2-year neuron id, a zero two-week dissolve delay, malformed optional stream-manager principal text, and model annual bps above the test/model ceiling.

## Stable State

Upgrade persistence uses an explicit versioned stable snapshot saved with `ic_cdk::storage::stable_save` and restored with `stable_restore`. The snapshot preserves config, simulated NNS model state, unwind children, two-week pool state, lifecycle operation journal, retry status, and scheduler cursors. Host tests exercise export/import and migration round trips without exposing stable-state methods in the production DID.

Stable storage hardening does not make IO live. This value-moving canister is not deployed to production, production adapters are not active, and the SNS IO ledger does not exist yet. Corrupt value-moving state must fail closed on upgrade. Missing first-install state is handled by init/default state and is not the same as a corrupt upgrade snapshot. Local stable-state fixtures are test fixtures, not live snapshots.

The existing neuron-owner canister `oae4c-3iaaa-aaaar-qb5qq-cai` and IO neuron `6345890886899317159` remain protected references only. Retry-critical lifecycle journal entries are not silently compacted or evicted.

## Scheduler Skeleton

`src/scheduler/` contains a no-op `scheduler_tick_once()` for future timer-driven work. It currently records planned responsibilities only: checking/disbursing 2-year maturity, checking/disbursing 2-week maturity, rebalancing the pooled 2-week neuron, and disbursing ready unwind child neurons. It performs no NNS calls.

`io-governance-types` contains production-shaped NNS governance DTOs and a Wasm-gated `NnsGovernanceCanisterClient` for future lifecycle reconciliation. Those adapters are fixture-tested only, not audited, and not wired into this canister's default execution path.

## Known Constants

```text
2-year NNS neuron id:
6345890886899317159

Controller canister principal:
oae4c-3iaaa-aaaar-qb5qq-cai
```
