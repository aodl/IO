# IO Suite

Initial Rust workspace for the IO protocol suite, following the Jupiter Faucet suite style without copying unnecessary canister boundaries.

IO follows a minimal production API pattern for value-moving canisters.

The core value-moving canisters are:

- `io_stream_manager`
- `io_nns_neuron_manager`

These should not become public dashboard/query APIs.

The public read surface belongs in:

- `io_historian`
- `frontend`, via historian/read-model APIs

Production data flows should be ledger/index driven where possible. Debug/test builds may expose richer APIs for local development and model/PocketIC testing.

## Canisters

- `io_nns_neuron_manager` — NNS-only operational canister. It controls/manages the existing 2-year IO NNS neuron and a pooled 2-week NNS neuron strategy. It does **not** issue IO or calculate IO rewards.
- `io_stream_manager` — main monetary-policy canister. It scans/classifies ICP streams, applies the 40/60 split, issues backed IO when the stream type requires it, handles redemptions, and computes IO SNS-staker entitlement.
- `io_historian` — placeholder read-model canister.
- `frontend` — placeholder frontend canister.

Known live NNS neuron configuration:

```text
IO 2-year NNS neuron id: 6345890886899317159
Current controlling canister principal: oae4c-3iaaa-aaaar-qb5qq-cai
```

## API Surface Policy

Production core canisters:

- Minimal public DID, currently install-args-only for the value-moving canisters.
- Timer/ledger/index driven.
- No broad `get_state`, `get_events`, or dashboard endpoints.
- No caller-submitted arbitrary stream-kind processing.
- No debug time controls.

Debug/test builds:

- May expose `debug_get_state`.
- May expose `debug_process_stream_event`.
- May expose `debug_advance_model_time`.
- Used by tests only.

Historian:

- Owns public read/query APIs.
- Reconstructs state from ledgers, indexes, governance sources, management canister status, and public metadata where possible.

## Ledger-as-Interface Principle

Jupiter Faucet deposits ICP into an IO-controlled account/subaccount. The stream manager scans/verifies ledger/index data.

Users redeem by transferring IO to a redemption account/subaccount. The stream manager scans/verifies IO ledger/index data.

NNS maturity is disbursed by `io_nns_neuron_manager` to distinguishable IO stream-manager accounts/subaccounts. The stream manager classifies those ledger flows.

Production callers should not be able to simply assert "this is a Jupiter Faucet stream" or "this is two-week maturity".

## Current Status

- Model-level tests are green through the repository `xtask` suite.
- `io_stream_manager` and `io_nns_neuron_manager` have explicit install args with validation.
- Both value-moving canisters persist explicit stable snapshots with `ic_cdk::storage::stable_save` / `stable_restore`.
- Both value-moving canisters persist durable operation journals and scheduler cursors for retryable value-moving work.
- Internal scheduler modules have debug/test ticks that scan mock ledger/governance canisters and drive the model.
- Debug APIs exist only for development/testing.
- Real production ledger/NNS/SNS integrations are not yet implemented; the first runnable slice targets mock canisters.
- Production DIDs are intentionally minimal and expose no production query/control methods on value-moving canisters.
- Security and operations baselines live under `docs/security/` and `docs/operations/`.
- Release artifacts include a deterministic-gzip artifact set, SHA sidecars, and `release-artifacts/manifest.json`.
- Local SNS harness documentation and fixture skeletons live under `docs/operations/local-sns-testing.md` and `tools/sns/`. They provide topology/config smoke coverage only; full SNS governance, ledger/index wiring, and SNS root/controller lifecycle tests remain future work.

## Tests

Run the whole first-pass suite:

```bash
cargo run -p xtask -- test_all
```

Useful subsets:

```bash
cargo run -p xtask -- test_unit
cargo run -p xtask -- test_pocketic_integration
cargo run -p xtask -- test_pocketic_required
cargo run -p xtask -- sns_harness_check
cargo run -p xtask -- sns_governance_read_tests
cargo run -p xtask -- sns_governance_read_required
cargo run -p xtask -- sns_pocketic_smoke
cargo run -p xtask -- test_ci
cargo run -p xtask -- test_local_integration
cargo run -p xtask -- stream_manager_unit
cargo run -p xtask -- nns_neuron_manager_unit
cargo run -p xtask -- validate_install_args
cargo run -p xtask -- security_scan
cargo run -p xtask -- security_scan_required
cargo run -p xtask -- verify_release
```

The PocketIC tests include real install/call/upgrade coverage for the IO and mock canisters when `POCKET_IC_BIN` points at a compatible server. `test_all` is the local default and can run in environments where the live PocketIC tests skip. `test_pocketic_required` fails when `POCKET_IC_BIN` is missing.

Command semantics:

- `test_all`: local default; may skip live PocketIC tests when `POCKET_IC_BIN` is unset, but reports that clearly.
- `test_ci`: strict test gate; requires PocketIC and runs core checks, security scan, artifacts, DID guardrails, and integration suites.
- `sns_harness_check`: deterministic local SNS docs/fixture/script guardrail; no PocketIC, no `dfx`, and no mainnet calls.
- `sns_governance_read_tests`: host/unit coverage for mock-backed SNS governance reads, snapshot conversion, exclusions, and reward allocation; no PocketIC.
- `sns_governance_read_required`: strict PocketIC read-only SNS governance test; requires `POCKET_IC_BIN`.
- `sns_pocketic_smoke`: permissive SNS topology smoke; skips clearly when `POCKET_IC_BIN` is unset.
- `sns_pocketic_required`: strict SNS topology and read-only governance smoke; fails when `POCKET_IC_BIN` is unset.
- `verify_release`: release-readiness gate; runs DID surface, canister builds, artifact verification, install-args validation, local SNS harness checks, host SNS governance read tests, and required security scan. It does not deploy.

## Build

This repo intentionally uses `icp.yaml`/`icp-cli` style configuration, not `dfx.json`, matching the Jupiter Faucet suite convention. `icp.yaml` points at pre-built artifacts generated by `xtask build_canisters`.

Build installable release Wasm artifacts with:

```bash
cargo run -p xtask -- build_canisters
```

Expected release outputs for each release canister:

```text
release-artifacts/<canister>.wasm
release-artifacts/<canister>.wasm.gz
release-artifacts/<canister>.wasm.sha256
release-artifacts/<canister>.wasm.gz.sha256
release-artifacts/manifest.json
```

`cargo run -p xtask -- verify_artifacts` checks that raw/gzipped artifacts and SHA sidecars exist, sidecars match artifacts, the manifest matches paths/hashes/sizes, and no stale release artifacts exist for known canisters.

The frontend artifact is a placeholder Rust Wasm canister matching the current placeholder frontend crate.

## Security And Operations

- Controller and recovery: `docs/security/controller-and-recovery.md`
- Threat model: `docs/security/threat-model.md`
- Supply chain: `docs/security/dependency-and-supply-chain.md`
- Audit readiness: `docs/security/audit-readiness.md`
- Deployment/runbook/reproducible builds: `docs/operations/`

Current scripts do not deploy, install, upgrade, update settings, or call mainnet.

## Expanded test suite

Run the full suite with:

```bash
cargo run -p xtask -- test_all
```

This now runs unit, PocketIC-shaped integration, local CLI-shaped integration, and e2e model tests. The most important added coverage is:

- 2-year maturity strengthens backing and issues no IO.
- 2-week maturity issues backed IO to eligible IO SNS neurons.
- Fast-forwarded maturity accrual in `io_nns_neuron_manager`.
- Two-week unwind splits and cancel-dissolve merge-back behaviour.
- Duplicate transaction/idempotency checks.
- Journal-driven retry after failed IO issuance, partial 2-week distribution, redemption payout/return, and NNS maturity transfer failures.
- Stable-state and PocketIC upgrade/retry checks for pending journal work.
- Unknown-source rejection.
- Reward weighting by stake-time and closed-proposal participation.
- End-to-end stream flow across the NNS manager and stream manager models.

The PocketIC test files now include real install/call tests guarded by `POCKET_IC_BIN`. When a compatible PocketIC server binary is configured, `cargo run -p xtask -- test_pocketic_integration` builds debug Wasm for the IO and mock canisters, installs them, and exercises debug update/query calls. Without `POCKET_IC_BIN`, those install tests skip and the deterministic model coverage still runs.
