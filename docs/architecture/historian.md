# Historian Architecture

`io_historian` is IO's public observability read model. It is intentionally separate from the value-moving canisters.

## Responsibilities

- expose bounded public query APIs for dashboard/frontend consumption;
- preserve durable read-model state across upgrades;
- represent missing source observations explicitly;
- reconstruct public records from observable ledger/index/governance/release data where possible;
- summarize release artifact hashes and canister/module status observations;
- publish scan/index health and governance participation summaries.

Historian does not issue IO, redeem IO, move ICP, manage neurons, classify value-moving source events for execution, or decide protocol economics.

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

Production source adapters remain future work. The frontend consumes the production historian read surface through browser Candid calls, but historian observations remain a rebuildable read model rather than protocol truth.
