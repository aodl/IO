# Production Wiring

This milestone defines planned adapter wiring and dry-run/config validation only. No production execution is active.

IO protocol remains not live, SNS IO ledger is not launched, no value-moving IO Wasm is installed on the reserved production canisters, and IO issuance/redemption remain inactive. production activation is a later audited milestone with separate review.

Stable storage hardening does not activate production adapters. Stable-state fixtures are local/test fixtures, not live snapshots. The historian is a rebuildable read model; corrupt value-moving upgrade state must fail closed rather than defaulting to empty production state.

The wiring templates cover ICP ledger/index, NNS governance, NNS ledger/index aliases, future SNS root/governance/ledger/index, IO ledger/index role naming, and explicit transfer fees. IO_TEST ledger is non-canonical and must not be labelled as canonical SNS IO.

The production IO-owned canister IDs are fiduciary-subnet reservations:

| Canister | Production ID | Status |
| --- | --- | --- |
| `io_stream_manager` | `thset-pqaaa-aaaar-qb7wa-cai` | `ReservedNotLive` |
| `io_nns_neuron_manager` | `tatch-ciaaa-aaaar-qb7wq-cai` | `ReservedNotLive` |
| `io_historian` | `tjqj3-uaaaa-aaaar-qb7xa-cai` | `ReservedNotLive` |
| `frontend` | `torpp-zyaaa-aaaar-qb7xq-cai` | `ReservedNotLive` |

These fiduciary canisters are reserved, empty/inert placeholders. They are not live, no value-moving Wasm is installed, no production activation has happened, and no IO issuance/redemption is enabled. `deployment_targets.io_stream_manager` and `deployment_targets.io_nns_neuron_manager` must match the exact reserved fiduciary IDs. Template SNS principal values are planned wiring placeholders only and do not prove SNS launch or readiness.

The previous frontend/historian canisters are recorded under `deploy/mainnet-dev/legacy-phase1/` as `DevMainnet`. They are superseded as production targets, retained only as dev/test canisters, not on the fiduciary subnet, and not production IO protocol canisters. Codex may only consider them deployable dev targets when explicitly instructed; Codex must not deploy to production fiduciary IDs without explicit future production activation instructions.

Protected references must remain references only:

- `oae4c-3iaaa-aaaar-qb5qq-cai`
- `6345890886899317159`

use `icp-cli` convention for future manual mainnet operations. required workflows do not use `dfx`.

## Production Wiring Checklist

- `cargo run -p xtask -- validate_production_wiring` passes.
- `cargo run -p xtask -- did_surface` passes.
- Production DIDs for value-moving canisters remain `service : (InitArgs) -> {}`.
- `ProductionActive` is not accepted by config validation.
- ICP ledger/index and NNS governance match known mainnet principals.
- SNS root/governance/ledger/index are present as a group in production-planned config.
- Ledger/index pairs are complete.
- ICP and IO ledger fees are explicit and non-zero.
- DevMainnet frontend/historian canisters and unrelated mainnet/system canisters are not production targets.
- Value-moving deployment target fields match the reserved fiduciary IDs while status remains `ReservedNotLive`.
- Protected canister and neuron IDs are not mutation targets.
