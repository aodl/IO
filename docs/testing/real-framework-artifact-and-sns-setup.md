# Real Framework Artifact and SNS Setup

This note captures the DFINITY SNS/NNS PocketIC pattern to port into IO. It does not authorize mainnet operations. IO real-framework tests must not use `--network ic`, must not call mainnet, and must not touch production fiduciary placeholder canisters.

## DFINITY References Inspected

- `dfinity/ic:rs/nervous_system/integration_tests/tests/sns_lifecycle.rs`
- `dfinity/ic:rs/nervous_system/integration_tests/src/pocket_ic_helpers.rs`
- `dfinity/ic:rs/nervous_system/integration_tests/src/create_service_nervous_system_builder.rs`
- `dfinity/ic:rs/nervous_system/integration_tests/tests/upgrade_sns_controlled_canister_with_large_wasm.rs`
- `dfinity/ic:rs/nervous_system/integration_tests/BUILD.bazel`
- `dfinity/ic:MODULE.bazel`
- `dfinity/ic:bazel/mainnet-canisters.bzl`
- `dfinity/ic:mainnet-canister-revisions.json`
- `dfinity/ic:rs/sns/testing/README.md`
- `dfinity/ic:rs/sns/testing/src/bin/init.rs`
- `dfinity/ic:rs/sns/testing/src/bootstrap.rs`
- `dfinity/icp-cli-network-launcher:README.md`
- `dfinity/icp-cli-network-launcher:SPEC.md`
- `dfinity/icp-cli-network-launcher:src/main.rs`

## Recommended IO Path

Keep the primary proof path as Rust PocketIC tests in `tests/e2e_real_canisters`. Port DFINITY's minimal pattern in phases:

1. Create PocketIC with `with_nns_subnet()`, `with_sns_subnet()`, and `with_application_subnet()`.
2. Put SNS framework canisters on the SNS subnet.
3. Put IO application canisters on an application subnet.
4. Install NNS canisters explicitly before any SNS-W proposal path.
5. Populate SNS-W with pinned SNS Wasms, then deploy SNS through the NNS proposal helper pattern.
6. Exercise the normal swap/staking path through ledger transfer to swap subaccounts, `refresh_buyer_tokens`, finalize, and `list_neurons`.

Do not assume PocketIC subnet builders install initialized NNS/SNS framework canisters. They create topology. DFINITY either installs NNS through helper code (`NnsInstaller`) and publishes SNS Wasms to SNS-W, or uses `PocketIcBuilder::with_icp_features(...)` through a launcher/bootstrap path.

## What To Port

- `NnsInstaller` shape: build NNS init payloads, include SNS dedicated subnet IDs, install NNS ledger/root/governance/lifeline/SNS-W/registry at their well-known IDs, and optionally CMC/cycles ledger/index.
- `SnsWasmCanistersInstaller` shape: load root/governance/swap/index/ledger/archive Wasms, gzip if needed, hash them, and add each Wasm to SNS-W through NNS proposals.
- `CreateServiceNervousSystemBuilder` shape: deterministic local SNS init payload with immediate swap start, small participant counts, explicit dapp canisters, and test-friendly governance parameters.
- App placement from `upgrade_sns_controlled_canister_with_large_wasm.rs`: get `pocket_ic.topology().get_app_subnets()[0]` and create dapp canisters there.
- Lifecycle participation from `sns_lifecycle.rs`: fund participant ICP accounts, transfer ICP to the swap subaccount, optionally create sale tickets, call `refresh_buyer_tokens`, await committed/open/finalized lifecycle, and assert direct-participation SNS neurons via governance `list_neurons`.

## What To Avoid

- Do not vendor large DFINITY helper modules wholesale.
- Do not use unpinned downloads in CI.
- Do not build the DFINITY monorepo from IO tests.
- Do not use DFINITY's test-governance Wasm as proof of production governance behavior.
- Do not treat direct ledger/index installation as proof of SNS-W deployment or normal SNS staking.
- Do not replace Rust PocketIC tests with process-level `icp-cli-network-launcher` rehearsals.

## Artifact Pinning

DFINITY's Bazel pattern uses `mainnet-canister-revisions.json` plus `bazel/mainnet-canisters.bzl`.

For canisters built from the IC repository, the source URL is:

```text
https://download.dfinity.systems/ic/<rev>/canisters/<filename>
```

For canisters published from a separate GitHub repository, the source URL is:

```text
https://github.com/<repository>/releases/download/<tag>/<filename>
```

Every artifact entry carries SHA-256. IO should mirror this in `tests/e2e_real_canisters/wasms.local.toml` or an explicitly supplied `IO_REAL_SNS_WASM_MANIFEST`, including source kind, upstream revision or tag, filename, and SHA-256. Fetching can be an opt-in xtask command only after the manifest has pinned URL inputs and hashes. Verification can stay in default local checks because it performs no network calls and skips when no artifact directory is configured.

## Version Compatibility

Pin these as a tested set:

- `pocket-ic` crate version in `Cargo.lock`
- `POCKET_IC_BIN` server version
- NNS/SNS Wasm revision or release tag
- DTO/init payload code used by IO tests

Do not mix a new PocketIC server with old Wasm DTOs casually. Current DFINITY examples use repository-local Rust types with repository-local or mainnet-pinned Wasms, so IO must either pin matching published artifacts and DTO shapes or keep the test blocked with an explicit error.

## `icp-cli-network-launcher`

`icp-cli-network-launcher --nns` is useful as a separate local rehearsal layer. Its source shows:

- NNS subnet is always created.
- `--nns` adds an SNS subnet and II subnet.
- `--nns` enables `IcpFeatures` for NNS governance, NNS UI, SNS, and canister migration.
- The launcher package is tied to a matching PocketIC binary.

This is valuable for manual or script-level local rehearsals because it can install a functional local NNS/SNS network. It is not superior to Rust PocketIC tests for IO CI because it is process-oriented, versioned through a separate binary/package, and less convenient for asserting in-memory test state. Use it under `deploy/local-sns-rehearsal/` or an opt-in xtask rehearsal, not as a replacement for `tests/e2e_real_canisters`.

## Next Implementation Steps

1. Keep the topology correction in `tests/e2e_real_canisters`: NNS + SNS + application subnets, SNS artifacts on SNS subnet, app canisters on application subnet.
2. Extend the artifact manifest schema with DFINITY-style source metadata for each canister.
3. Add `cargo run -p xtask -- verify_real_canister_artifacts` as a no-network alias that verifies every configured artifact/hash pair.
4. Add `cargo run -p xtask -- fetch_real_canister_artifacts` only after the manifest contains pinned URLs and SHA-256 values for a complete NNS/SNS set.
5. Port a narrow NNS installer for local tests: NNS ledger, root, governance, lifeline, SNS-W, registry, and CMC only if needed.
6. Port an SNS-W population helper for root, governance, ledger, index, swap, and archive.
7. Build the SNS init payload via a small IO-owned builder derived from DFINITY's `CreateServiceNervousSystemBuilder` pattern.
8. Add one governance/root smoke test: deploy SNS through NNS proposal, finalize swap, list SNS neurons.
9. Add app-control proof: create an IO app canister on the application subnet with NNS root as co-controller, finalize SNS, assert SNS root control.
10. Add normal staking/top-up proof after the governance/root smoke is stable.
