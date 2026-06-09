# Historian Architecture

`io_historian` is IO's public observability read model. It is intentionally separate from the value-moving canisters.

Phase 1 public shell is live on mainnet with historian canister `yo47z-piaaa-aaaac-qg3xa-cai` and frontend canister `6h2pa-qiaaa-aaaao-qp4fa-cai`. IO remains pre-launch: no value-moving protocol canister is live, no canonical IO SNS ledger exists yet, IO issuance is not live, and IO redemption is not live.

## Responsibilities

- expose bounded public query APIs for dashboard/frontend consumption;
- preserve durable read-model state across upgrades;
- represent missing source observations explicitly;
- reconstruct public records from observable ledger/index/governance/release data where possible;
- summarize release artifact hashes and canister/module status observations;
- publish scan/index health and governance participation summaries.

Historian does not issue IO, redeem IO, move ICP, manage neurons, classify value-moving source events for execution, or decide protocol economics. It is a public read model, not protocol truth.

## Public Surface

The production DID exposes read-only queries including `get_dashboard_state`, `get_protocol_snapshot`, `get_redemption_rate`, `list_streams`, `list_redemptions`, `list_rewards`, `list_nns_lifecycle_events`, `get_index_health`, `get_governance_summary`, `get_release_artifacts`, and `get_canister_status_summary`.

All history lists are bounded and paginated. There is no unbounded event dump.

## Debug/Test Ingestion

Debug Wasm exposes ingestion methods through `io_historian_debug.did`. These methods feed local observations into the read model for unit and PocketIC tests. They are not production APIs and must not be used as protocol authority.

## Accounting Snapshot

The historian snapshot uses the existing redemption-rate inputs without changing IO economics:

```text
redeemable_io_supply =
  total_io_supply
  - protocol_reserve_io
  - non_redeemable_governance_io

redemption_rate =
  liquid_icp_reserve / redeemable_io_supply
```

If total supply, excluded supply, liquid reserve, or redeemable supply is unavailable, the snapshot is incomplete. The two-year NNS principal is represented as non-liquid backing and is excluded from liquid redemption NAV.

## Rebuildability

Historian state is useful for continuity and frontend responsiveness, but it is a read model. Full canonical history remains in ledger/index/governance sources and release artifacts. If historian state diverges, recovery should rebuild or correct historian observations rather than adding broad production query/control APIs to value-moving canisters.

Production source adapters remain future work. The frontend consumes the production historian read surface through browser Candid calls, but historian observations remain a rebuildable read model rather than protocol truth. In Phase 1, the frontend was built with `CANISTER_ID_IO_HISTORIAN=yo47z-piaaa-aaaac-qg3xa-cai`; this does not activate `io_stream_manager`, `io_nns_neuron_manager`, the existing IO neuron-owner canister `oae4c-3iaaa-aaaar-qb5qq-cai`, or IO neuron `6345890886899317159`.

## Freshness Sources

Historian source health is observation/freshness only. It is a public read model, rebuildable, not canonical protocol truth, and not a value-moving authority. Source health distinguishes fresh, stale, missing, incomplete, observed-only, prelaunch/not-applicable, error/retryable, and unknown observations. The missing/stale/incomplete states are visible.

IO protocol is not live. SNS IO ledger remains not launched. Missing/stale/incomplete historian fields must not be interpreted as zero protocol value. Index canisters remain the normal account-history abstraction; index canisters are the default source for account-history observations. Raw ledger/archive traversal is not the default path.
