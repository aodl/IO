# io_historian

Public read-model canister.

Production historian canister `tjqj3-uaaaa-aaaar-qb7xa-cai` is a fiduciary-subnet reservation with status `ReservedNotLive`. It is empty/inert and not live. The paired production frontend reservation is `torpp-zyaaa-aaaar-qb7xq-cai`.

The previous historian `yo47z-piaaa-aaaac-qg3xa-cai` and frontend `6h2pa-qiaaa-aaaao-qp4fa-cai` are `DevMainnet` only: superseded as production targets, retained only as dev/test canisters, not on the fiduciary subnet, and not production IO protocol canisters.

IO remains pre-launch. The canonical SNS IO ledger is not launched, no value-moving protocol canister is live, IO issuance is not live, and IO redemption is not live.

## Role

- Public read model.
- Dashboard/query surface.
- Reconstructs state from ledgers/indexes/governance/management-canister observations and local/test fixtures.
- Not a value-moving authority.
- May be incomplete or wrong and must be rebuildable from canonical source observations.

Historian must not depend on broad public query APIs from value-moving canisters. Historian may query observable/public sources and ledgers/indexes in future production wiring, but this milestone only wires local/test ingestion and pure reconstruction helpers.

The DevMainnet frontend consumes this historian read model. Historian remains a public read model, not protocol truth.

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

Stable storage hardening does not make IO live. Historian fixtures are local/test fixtures, not live snapshots. Missing first-install state defaults to honest prelaunch read-model state, while corrupt upgrade state fails closed. Historian is rebuildable and is not a value-moving authority or protocol truth.

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

Production data-source wiring remains future work. No deployment workflows, `dfx` requirements, SNS launch flows, or value-moving economics changes are part of this milestone. The existing IO neuron-owner canister `oae4c-3iaaa-aaaar-qb5qq-cai` and IO neuron `6345890886899317159` are not touched by the DevMainnet historian.

## Freshness Model

Historian source health is part of the public read model. The state is rebuildable, not canonical protocol truth, and not a value-moving authority. Production-shaped ingestion is observation/freshness only and does not activate production adapters.

Source health exposes fresh, stale, missing, incomplete, observed-only, prelaunch/not-applicable, error/retryable, and unknown observations. The missing/stale/incomplete states are visible, and missing/stale/incomplete fields must not be interpreted as zero protocol value.

IO protocol is not live. SNS IO ledger remains not launched. Production fiduciary canisters are reserved empty/inert placeholders, while the previous frontend/historian shell is DevMainnet only. Index canisters are the normal account-history abstraction; index canisters are the default source for account-history observations. Raw ledger/archive traversal is not the default path.
