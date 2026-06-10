# IO Canister Roles

## io_nns_neuron_manager

NNS-only canister. It manages:

- IO's 2-year NNS neuron `6345890886899317159`.
- A pooled 2-week NNS staking position for active IO SNS stakers.
- Temporary split/dissolving children needed to unwind the pooled 2-week position.

It transfers ICP maturity/principal to `io_stream_manager` using source metadata that lets the stream manager classify the flow.

Production fiduciary status: reserved as `tatch-ciaaa-aaaar-qb7wq-cai`, `ReservedNotLive`, empty/inert, no value-moving Wasm installed. The existing IO neuron-owner canister `oae4c-3iaaa-aaaar-qb5qq-cai`, which owns IO neuron `6345890886899317159`, remains not touched.

## io_stream_manager

Main economic canister. It owns the protocol accounting model:

- Jupiter Faucet ICP -> 40% to 2-year stake, 60% liquid, backed IO to Jupiter Faucet.
- 2-year maturity ICP -> 40% restaked, 60% liquid, no IO issued.
- 2-week maturity ICP -> 40% restaked to 2-week pool, 60% liquid, backed IO to eligible IO SNS neurons.

Production fiduciary status: reserved as `thset-pqaaa-aaaar-qb7wa-cai`, `ReservedNotLive`, empty/inert, no value-moving Wasm installed. No value-moving protocol canister is live, IO issuance is not live, IO redemption is not live, and the canonical SNS IO ledger is not launched.

## io_historian

Public read model and observability surface. It owns dashboard/query APIs for protocol snapshots, bounded stream/redemption/reward history, NNS lifecycle summaries, index health, governance participation, release artifacts, canister status, and ingestion status.

Historian is not a value-moving authority. It may be incomplete or wrong and should be rebuildable from canonical sources such as ledgers, indexes, governance data, release manifests, and management-canister observations. Local/test ingestion APIs are debug-only and are absent from the production DID.

Production fiduciary status: reserved as `tjqj3-uaaaa-aaaar-qb7xa-cai`, `ReservedNotLive`, empty/inert, and not live. The previous `yo47z-piaaa-aaaac-qg3xa-cai` historian is `DevMainnet` only: superseded as a production target, retained only as a dev/test canister, not on the fiduciary subnet, and not a production IO protocol canister.

## frontend

Certified Rust asset canister for the IO browser dashboard. It serves static assets with certified HTTP responses, strict cache/security headers, and a content-hashed browser bundle. Browser data comes from `io_historian` production read APIs such as `get_dashboard_state` and `get_public_status`, not from `io_stream_manager` or `io_nns_neuron_manager` internals. Frontend text is not protocol truth.

Production fiduciary status: reserved as `torpp-zyaaa-aaaar-qb7xq-cai`, `ReservedNotLive`, empty/inert, and not live. The previous `6h2pa-qiaaa-aaaao-qp4fa-cai` frontend is `DevMainnet` only: superseded as a production target, retained only as a dev/test canister, not on the fiduciary subnet, and not a production IO protocol canister. The dev/test frontend URLs are `https://6h2pa-qiaaa-aaaao-qp4fa-cai.icp0.io/` and `https://6h2pa-qiaaa-aaaao-qp4fa-cai.raw.icp0.io/`; it consumes dev/test historian canister `yo47z-piaaa-aaaac-qg3xa-cai`.
