# Phase 1 Mainnet Public Shell

This record documents the reviewed Phase 1 mainnet public-shell deployment. It is a record of an already completed deployment, not an instruction to deploy, install, reinstall, upgrade, or update canister settings.

## Status

- Deployment phase: `MainnetPreLaunchPublicShell`
- Record date: 2026-06-06
- IO protocol live: no
- SNS IO ledger launched: no
- IO issuance active: no
- IO redemption active: no

The public shell consists only of `io_historian` and `frontend`.

## Canisters

| Canister | Mainnet ID | Phase 1 status |
| --- | --- | --- |
| `frontend` | `6h2pa-qiaaa-aaaao-qp4fa-cai` | deployed public shell |
| `io_historian` | `yo47z-piaaa-aaaac-qg3xa-cai` | deployed public read model |
| `io_stream_manager` | none | not deployed |
| `io_nns_neuron_manager` | none | not deployed |

The existing IO neuron-owner canister `oae4c-3iaaa-aaaar-qb5qq-cai` was not touched. IO neuron `6345890886899317159` was not touched.

## Frontend URLs

- Gateway: `https://6h2pa-qiaaa-aaaao-qp4fa-cai.icp0.io/`
- Raw: `https://6h2pa-qiaaa-aaaao-qp4fa-cai.raw.icp0.io/`

The frontend was built with `CANISTER_ID_IO_HISTORIAN=yo47z-piaaa-aaaac-qg3xa-cai`, so browser reads point at the Phase 1 historian canister.

## Boundaries

No value-moving protocol canister was deployed in Phase 1. The historian is a public read model, not protocol truth or a value-moving authority. The frontend consumes historian query APIs and does not activate IO issuance, redemption, NNS neuron management, SNS ledger launch, or IO economics.

The canonical SNS IO ledger is not launched. IO issuance and redemption are not live.

## Release Artifact Reference

The Phase 1 public shell should be reviewed against `release-artifacts/manifest.json` and the corresponding release artifact SHA sidecars for the built `io_historian` and `frontend` Wasm artifacts.
