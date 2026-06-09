# Production Wiring Dry Run

This directory is dry-run/config validation only. No production execution is active.

IO protocol remains not live. SNS IO ledger is not launched. IO issuance and redemption are not live. No value-moving IO canister is deployed to production, and production activation is a later audited milestone.

`deployment_targets.io_stream_manager` and `deployment_targets.io_nns_neuron_manager` are intentionally `null` until IO canister IDs are deliberately allocated in a later audited deployment dry-run/proposal package milestone. Do not use NNS, SNS, Phase 1 frontend/historian, or other unrelated mainnet/system canister IDs as deployment target placeholders.

Protected references:

- `oae4c-3iaaa-aaaar-qb5qq-cai` is the existing inert neuron-owner canister and must not be touched.
- `6345890886899317159` is the IO NNS neuron and must not be touched.

use `icp-cli` convention for future manual mainnet operations. required workflows do not use `dfx`. IO_TEST ledger is non-canonical.

## Production Wiring Checklist

- Validate templates with `cargo run -p xtask -- validate_production_wiring`.
- Keep `io_stream_manager` and `io_nns_neuron_manager` production DIDs constructor-only.
- Keep value-moving canister targets `null` until audited IO canister ID allocation.
- Keep value-moving canister targets out of Phase 1 live canister IDs and unrelated mainnet/system canister IDs.
- Keep protected canister and neuron IDs listed only as protected references.
- Treat SNS principal values as planned wiring placeholders only; they do not prove SNS launch or readiness.
