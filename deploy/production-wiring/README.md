# Production Wiring Dry Run

This directory is dry-run/config validation only. No production execution is active.

IO protocol remains not live. SNS IO ledger is not launched. IO issuance and redemption are not live. No value-moving IO canister is deployed to production, and production activation is a later audited milestone.

The fiduciary-subnet reservation inventory is recorded in `canister-ids.toml`:

| Canister | Production ID | Status |
| --- | --- | --- |
| `io_stream_manager` | `thset-pqaaa-aaaar-qb7wa-cai` | `ReservedNotLive` |
| `io_historian` | `tjqj3-uaaaa-aaaar-qb7xa-cai` | `ReservedNotLive` |
| `frontend` | `torpp-zyaaa-aaaar-qb7xq-cai` | `ReservedNotLive` |

These fiduciary canisters are reserved, empty/inert placeholders. They are not
live, no value-moving Wasm is installed, no production activation has happened,
and no IO issuance/redemption is enabled. The NNS Manager uses the separate
protected execution identity below.

`deployment_targets.io_stream_manager` must match the reserved Stream Manager
ID. `deployment_targets.io_nns_neuron_manager` must match existing protected-
neuron controller `oae4c-3iaaa-aaaar-qb5qq-cai`, because the implementation
calls NNS Governance directly and requires its staging Accounts to be owned by
the executing canister. The accepted
[authority-location ADR](../../docs/architecture/adr-nns-authority-location.md)
defines one execution identity and no adapter. Do not use SNS, DevMainnet
frontend/historian, or unrelated mainnet/system canister IDs as deployment
targets.

The previous frontend/historian IDs live only in `deploy/mainnet-dev/legacy-phase1/`. They are superseded as production targets, retained only as dev/test canisters, not on the fiduciary subnet, and not production IO protocol canisters.

Protected references and the one planned authority exception:

- `oae4c-3iaaa-aaaar-qb5qq-cai` is the existing neuron-owner canister. Static validation permits it only as `deployment_targets.io_nns_neuron_manager`; every other deployment or mutation-target use is rejected. Any inspection or mutation still requires separate explicit authorization.
- `10292412127977304661` is the protected IO NNS neuron and must not be
  touched.

use `icp-cli` convention for future manual mainnet operations. required workflows do not use `dfx`. IO_TEST ledger is non-canonical.

## Production Wiring Checklist

- Validate templates with `cargo run -p xtask -- validate_production_wiring`.
- Keep the reviewed narrow production command surfaces for `io_stream_manager` and `io_nns_neuron_manager`; never expose debug completion methods.
- Keep the Stream Manager target on its exact reserved fiduciary ID and the NNS Manager target on the existing controller required by the accepted authority model.
- Keep value-moving canister targets out of DevMainnet canister IDs and unrelated mainnet/system canister IDs.
- Keep protected canister and neuron IDs listed only as protected references.
- Treat SNS principal values as planned wiring placeholders only; they do not prove SNS launch or readiness.
