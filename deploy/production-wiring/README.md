# Production Wiring Dry Run

This directory is dry-run/config validation only. No production execution is active.

IO protocol remains not live. SNS IO ledger is not launched. IO issuance and redemption are not live. No value-moving IO canister is deployed to production, and production activation is a later audited milestone.

The canonical production IO-owned canister IDs are fiduciary-subnet reservations recorded in `canister-ids.toml`:

| Canister | Production ID | Status |
| --- | --- | --- |
| `io_stream_manager` | `thset-pqaaa-aaaar-qb7wa-cai` | `ReservedNotLive` |
| `io_nns_neuron_manager` | `tatch-ciaaa-aaaar-qb7wq-cai` | `ReservedNotLive` |
| `io_historian` | `tjqj3-uaaaa-aaaar-qb7xa-cai` | `ReservedNotLive` |
| `frontend` | `torpp-zyaaa-aaaar-qb7xq-cai` | `ReservedNotLive` |

These production fiduciary canisters are reserved, empty/inert placeholders. They are not live, no value-moving Wasm is installed, no production activation has happened, and no IO issuance/redemption is enabled.

`deployment_targets.io_stream_manager` and `deployment_targets.io_nns_neuron_manager` must match the reserved fiduciary IDs above. Do not use NNS, SNS, DevMainnet frontend/historian, or other unrelated mainnet/system canister IDs as deployment targets.

The previous frontend/historian IDs live only in `deploy/mainnet-dev/legacy-phase1/`. They are superseded as production targets, retained only as dev/test canisters, not on the fiduciary subnet, and not production IO protocol canisters.

Protected references:

- `oae4c-3iaaa-aaaar-qb5qq-cai` is the existing inert neuron-owner canister and must not be touched.
- `6345890886899317159` is the IO NNS neuron and must not be touched.

use `icp-cli` convention for future manual mainnet operations. required workflows do not use `dfx`. IO_TEST ledger is non-canonical.

## Production Wiring Checklist

- Validate templates with `cargo run -p xtask -- validate_production_wiring`.
- Keep `io_stream_manager` and `io_nns_neuron_manager` production DIDs constructor-only.
- Keep value-moving canister targets on the exact reserved fiduciary IDs while status remains `ReservedNotLive`.
- Keep value-moving canister targets out of DevMainnet canister IDs and unrelated mainnet/system canister IDs.
- Keep protected canister and neuron IDs listed only as protected references.
- Treat SNS principal values as planned wiring placeholders only; they do not prove SNS launch or readiness.
