# Mainnet Prelaunch Public Shell

This document records the Phase 1 mainnet public shell. It is not a deployment runbook and does not authorize mainnet operations.

## Phase 1 Record

- Phase: `MainnetPreLaunchPublicShell`
- Record path: `deploy/phase1-mainnet/`
- Canister ID file: `deploy/phase1-mainnet/canister-ids.toml`
- Release artifact manifest reference: `release-artifacts/manifest.json`

Live public-shell canisters:

- `frontend`: `6h2pa-qiaaa-aaaao-qp4fa-cai`
- `io_historian`: `yo47z-piaaa-aaaac-qg3xa-cai`

Frontend URLs:

- `https://6h2pa-qiaaa-aaaao-qp4fa-cai.icp0.io/`
- `https://6h2pa-qiaaa-aaaao-qp4fa-cai.raw.icp0.io/`

The frontend consumes historian reads from `yo47z-piaaa-aaaac-qg3xa-cai`. The frontend build was configured with `CANISTER_ID_IO_HISTORIAN=yo47z-piaaa-aaaac-qg3xa-cai`.

## Explicit Non-Launch Status

IO remains pre-launch. The Phase 1 public shell does not activate protocol economics.

- `io_stream_manager` is not deployed.
- `io_nns_neuron_manager` is not deployed.
- No value-moving protocol canister is live.
- The canonical SNS IO ledger is not launched.
- IO issuance is not live.
- IO redemption is not live.
- The existing IO neuron-owner canister `oae4c-3iaaa-aaaar-qb5qq-cai` is not touched.
- IO neuron `6345890886899317159` is not touched.

Historian is a public read model and dashboard source, not protocol truth. Frontend text and historian observations must not be treated as authority for IO issuance, redemption, reserves, neuron management, or SNS launch state.

## Validation

Use the local file-based guardrail:

```bash
cargo run -p xtask -- validate_prelaunch_public_shell
```

The command reads repository files only. It does not deploy, install, upgrade, reinstall, update settings, call mainnet, mint IO, transfer IO, pay ICP, or call NNS `manage_neuron`.
