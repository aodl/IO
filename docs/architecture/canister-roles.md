# IO Canister Roles

## io_nns_neuron_manager

NNS-only canister. It manages:

- IO's 2-year NNS neuron `6345890886899317159`.
- A pooled 2-week NNS staking position for active IO SNS stakers.
- Temporary split/dissolving children needed to unwind the pooled 2-week position.

It transfers ICP maturity/principal to `io_stream_manager` using source metadata that lets the stream manager classify the flow.

## io_stream_manager

Main economic canister. It owns the protocol accounting model:

- Jupiter Faucet ICP -> 40% to 2-year stake, 60% liquid, backed IO to Jupiter Faucet.
- 2-year maturity ICP -> 40% restaked, 60% liquid, no IO issued.
- 2-week maturity ICP -> 40% restaked to 2-week pool, 60% liquid, backed IO to eligible IO SNS neurons.

## io_historian

Public read model and observability surface. It owns dashboard/query APIs for protocol snapshots, bounded stream/redemption/reward history, NNS lifecycle summaries, index health, governance participation, release artifacts, canister status, and ingestion status.

Historian is not a value-moving authority. It may be incomplete or wrong and should be rebuildable from canonical sources such as ledgers, indexes, governance data, release manifests, and management-canister observations. Local/test ingestion APIs are debug-only and are absent from the production DID.

## frontend

Placeholder UI canister. It should consume historian APIs such as `get_dashboard_state`, not `io_stream_manager` or `io_nns_neuron_manager` internals. Frontend text is not protocol truth.
