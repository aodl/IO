# io_historian

Public read-model canister.

Phase 1 public shell is live on mainnet as historian canister `yo47z-piaaa-aaaac-qg3xa-cai`. The paired frontend canister is `6h2pa-qiaaa-aaaao-qp4fa-cai`.

IO remains pre-launch. The canonical SNS IO ledger is not launched, no value-moving protocol canister is live, IO issuance is not live, and IO redemption is not live.

## Role

- Public read model.
- Dashboard/query surface.
- Reconstructs state from ledgers/indexes/governance/management-canister observations and local/test fixtures.
- Not a value-moving authority.
- May be incomplete or wrong and must be rebuildable from canonical source observations.

Historian must not depend on broad public query APIs from value-moving canisters. Historian may query observable/public sources and ledgers/indexes in future production wiring, but this milestone only wires local/test ingestion and pure reconstruction helpers.

The Phase 1 frontend consumes this historian read model. Historian remains a public read model, not protocol truth.

## Public API

The production DID exposes bounded public queries:

- `version`
- `get_public_status`
- `get_protocol_snapshot`
- `get_reserve_snapshot`
- `get_redemption_rate`
- `list_streams`
- `list_redemptions`
- `list_rewards`
- `list_nns_lifecycle_events`
- `get_index_health`
- `get_governance_summary`
- `list_governance_participation`
- `get_release_artifacts`
- `get_canister_status_summary`
- `get_dashboard_state`

History queries are paginated with explicit request/response DTOs. There is no unbounded `get_all_events` style method.

## State And Retention

Historian stable state includes schema version, protocol/accounting snapshots, bounded stream/redemption/reward/NNS lifecycle history, index health, governance participation, release artifact status, canister status, and ingestion status.

Retention is explicit:

- stream history: 256 records
- redemption history: 256 records
- reward history: 256 records
- NNS lifecycle history: 256 records
- index health summaries: 32 records
- canister/artifact summaries: 32 records each
- governance per-neuron participation summaries: 512 records
- public page limit: 100 records

Records are ordered deterministically by stable record id and deduplicated by record id or canister name. Full canonical history remains in ledger/index/governance sources.

## Debug Ingestion

`io_historian_debug.did` contains local/test ingestion methods such as `debug_ingest_stream_record`, `debug_ingest_redemption_record`, `debug_ingest_reward_record`, `debug_ingest_index_health`, `debug_ingest_governance_snapshot`, `debug_ingest_canister_artifact_status`, and `debug_clear`.

These methods are not in the production DID. They exist so unit and PocketIC tests can feed observations without making value-moving canisters into dashboard APIs.

## Source Boundaries

The historian read model can represent:

- protocol summary and redemption rate snapshots with explicit missing-data flags;
- liquid ICP reserve, redeemable IO supply, protocol reserve IO, and governance IO snapshots;
- 2-year NNS principal as non-liquid backing, excluded from liquid redemption NAV;
- bounded stream, redemption, reward, and NNS lifecycle history;
- account-history/index health summaries;
- governance eligibility and participation summaries using `io-reward-policy`;
- release artifact and canister status summaries from manifest-shaped data.

Production data-source wiring remains future work. No deployment workflows, `dfx` requirements, SNS launch flows, or value-moving economics changes are part of this milestone. The existing IO neuron-owner canister `oae4c-3iaaa-aaaar-qb5qq-cai` and IO neuron `6345890886899317159` are not touched by the Phase 1 historian.
