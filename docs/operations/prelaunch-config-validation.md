# Prelaunch Config Validation

Prelaunch validation is dry-run/config validation only. No production execution is active.

IO protocol remains not live. SNS IO ledger is not launched. IO issuance is not live. IO redemption is not live. Production fiduciary canisters are reserved empty/inert placeholders with no value-moving Wasm installed, and production activation is a later audited milestone.

Run:

```bash
cargo run -p xtask -- validate_production_wiring
```

The command parses checked-in production wiring templates, validates principal roles and fee policy, confirms IO_TEST ledger is non-canonical, checks protected references, confirms the four IO-owned production canister IDs match the reserved fiduciary IDs, and confirms value-moving production DIDs remain constructor-only. It does not make network calls.

Production `io_stream_manager`, `io_nns_neuron_manager`, `io_historian`, and `frontend` IDs are concrete reserved fiduciary IDs with status `ReservedNotLive`; they are not live protocol deployments. Template SNS principal values are planned wiring placeholders only and do not prove SNS launch or readiness.

Protected references:

- `oae4c-3iaaa-aaaar-qb5qq-cai` must not be touched.
- IO neuron `6345890886899317159` must not be touched.

use `icp-cli` convention for future manual mainnet operations. required workflows do not use `dfx`.

## Production Wiring Checklist

- Review `deploy/production-wiring/template.toml`.
- Review `deploy/production-wiring/dry-run.example.toml`.
- Review `deploy/production-wiring/canister-ids.toml`.
- Confirm any future SNS values are planned inputs only until SNS launch.
- Confirm production fiduciary canisters are reserved, empty/inert, not live, and have no production activation.
- Confirm no value-moving canister IDs are marked live.
- Confirm no NNS, SNS, DevMainnet frontend/historian, or other unrelated mainnet/system canister ID is used as a production target.
