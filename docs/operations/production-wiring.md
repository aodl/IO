# Production Wiring

This milestone defines planned adapter wiring and dry-run/config validation only. No production execution is active.

IO protocol remains not live, SNS IO ledger is not launched, no value-moving IO canister is deployed to production, and IO issuance/redemption remain inactive. production activation is a later audited milestone with separate review.

Stable storage hardening does not activate production adapters. Stable-state fixtures are local/test fixtures, not live snapshots. The historian is a rebuildable read model; corrupt value-moving upgrade state must fail closed rather than defaulting to empty production state.

The wiring templates cover ICP ledger/index, NNS governance, NNS ledger/index aliases, future SNS root/governance/ledger/index, IO ledger/index role naming, and explicit transfer fees. IO_TEST ledger is non-canonical and must not be labelled as canonical SNS IO.

`deployment_targets.io_stream_manager` and `deployment_targets.io_nns_neuron_manager` are intentionally `null` until IO canister IDs are deliberately allocated in a later audited deployment dry-run/proposal package milestone. Template SNS principal values are planned wiring placeholders only and do not prove SNS launch or readiness.

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
- Phase 1 frontend/historian live canisters and unrelated mainnet/system canisters are not value-moving targets.
- Value-moving deployment target fields are `null` until audited IO canister ID allocation.
- Protected canister and neuron IDs are not mutation targets.
