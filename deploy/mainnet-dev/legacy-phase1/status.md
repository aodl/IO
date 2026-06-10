# Legacy Phase 1 DevMainnet Status

Phase 1 is recorded as `LegacyPhase1DevPublicShell` in the `DevMainnet` environment. These IDs are superseded as production targets, retained only as dev/test canisters, not on the fiduciary subnet, and not production IO protocol canisters.

Dev/test public-shell canisters:

- `frontend`: `6h2pa-qiaaa-aaaao-qp4fa-cai`
- `io_historian`: `yo47z-piaaa-aaaac-qg3xa-cai`

Public URLs:

- `https://6h2pa-qiaaa-aaaao-qp4fa-cai.icp0.io/`
- `https://6h2pa-qiaaa-aaaao-qp4fa-cai.raw.icp0.io/`

Not deployed in this dev/test shell:

- `io_stream_manager`
- `io_nns_neuron_manager`

Not touched in this dev/test shell:

- Existing IO neuron-owner canister: `oae4c-3iaaa-aaaar-qb5qq-cai`
- IO neuron: `6345890886899317159`

Pre-launch status:

- IO protocol is not live.
- The canonical SNS IO ledger is not launched.
- IO issuance is not live.
- IO redemption is not live.
- Historian is a public read model, not protocol truth.

Frontend build note:

- `CANISTER_ID_IO_HISTORIAN=yo47z-piaaa-aaaac-qg3xa-cai`

Release review reference:

- `release-artifacts/manifest.json`
