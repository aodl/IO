# Prelaunch Config Validation

Prelaunch validation is dry-run/config validation only. No production execution is active.

IO protocol remains not live. SNS IO ledger is not launched. IO issuance is not live. IO redemption is not live. No value-moving IO canister is deployed to production, and production activation is a later audited milestone.

Run:

```bash
cargo run -p xtask -- validate_production_wiring
```

The command parses checked-in production wiring templates, validates principal roles and fee policy, confirms IO_TEST ledger is non-canonical, checks protected references, and confirms value-moving production DIDs remain constructor-only. It does not make network calls.

Value-moving deployment target fields are intentionally `null` until IO canister IDs are deliberately allocated in a later audited deployment dry-run/proposal package milestone. Template SNS principal values are planned wiring placeholders only and do not prove SNS launch or readiness.

Protected references:

- `oae4c-3iaaa-aaaar-qb5qq-cai` must not be touched.
- IO neuron `6345890886899317159` must not be touched.

use `icp-cli` convention for future manual mainnet operations. required workflows do not use `dfx`.

## Production Wiring Checklist

- Review `deploy/production-wiring/template.toml`.
- Review `deploy/production-wiring/dry-run.example.toml`.
- Confirm any future SNS values are planned inputs only until SNS launch.
- Confirm `io_stream_manager` is not deployed in Phase 1.
- Confirm `io_nns_neuron_manager` is not deployed in Phase 1.
- Confirm no value-moving canister IDs are marked live.
- Confirm no NNS, SNS, Phase 1 frontend/historian, or other unrelated mainnet/system canister ID is used as a value-moving deployment target placeholder.
