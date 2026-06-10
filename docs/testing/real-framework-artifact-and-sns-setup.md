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

## Implemented Real-Ledger Exact-Economics Layer

`tests/e2e_real_canisters::real_canister_e2e_icp_to_io_stake_reward_redemption` is now an opt-in ignored PocketIC test backed by real pinned SNS ICRC ledger/index Wasms. It is not a full SNS governance or real NNS proof yet, but it takes the first complete executable step beyond ledger smoke tests:

- installs two real ICRC ledger/index pairs on the SNS subnet using the pinned `sns_ledger` and `sns_index` artifacts;
- treats one pair as the local ICP-flow ledger and one pair as the local IO/SNS ledger for canister-level value-flow proof;
- drives a Jupiter Faucet 100 ICP deposit through a real ledger transfer;
- applies IO model accounting and verifies the exact 40/60 split and 60 IO backed issuance;
- transfers backed IO from protocol reserve to Jupiter Faucet on the real IO ledger;
- fast-forwards PocketIC time before processing 2-year maturity;
- proves holding IO compounds through redemption-rate increase without issuing IO;
- processes 2-week maturity and allocates backed IO rewards with exact expected amounts for full-participation and half-participation stakers;
- transfers staker rewards on the real IO ledger and checks real index account history;
- redeems held IO at the current exact redemption rate and pays ICP on the real local ICP-flow ledger;
- checks real ledger/index history for deposit, issuance, rewards, redemption, and payout blocks.

This layer still does **not** prove normal SNS neuron staking, SNS root/governance behavior, SNS-W deployment, real NNS maturity mechanics, or official SNS launch/swap lifecycle. It is deliberately named and gated as a real-ledger exact-economics E2E, not an all-real SNS/NNS E2E.

## Artifact Fetch Workflow

`tools/scripts/fetch-real-canister-artifacts` provides an opt-in local fetch helper for the first real-ledger layer. It reads `IO_REAL_SNS_WASM_MANIFEST` or `tests/e2e_real_canisters/wasms.local.toml`, downloads only pinned `source_url` entries for `sns_ledger` and `sns_index`, and verifies SHA-256 before moving files into `IO_REAL_SNS_WASM_DIR` or `.real-canister-wasms`.

The script refuses non-HTTPS/non-approved URL shapes and does not run in default CI. The no-network verification path remains `cargo run -p xtask -- verify_real_canister_artifacts` / `real_canister_artifact_manifest_check`, which checks local files and hashes only.

## Implemented IO Harness Additions

The IO harness now has three opt-in layers:

1. `real_sns_ledger_index_smoke` installs pinned real SNS ledger/index Wasms on the SNS subnet and verifies ICRC metadata, transfers, errors, duplicate handling, index history, and same-Wasm upgrade behaviour.
2. `real_canister_e2e_icp_to_io_stake_reward_redemption` uses pinned real ICRC ledger/index canisters for the ledger movement slice and the pure IO accounting/reward policy crates for exact expected economics: Jupiter Faucet ICP input, 40/60 split, backed IO issuance, holder compounding via rate increase, two-week staker rewards, participation-weighted higher staking returns, and redemption at the current rate.
3. `real_sns_governance_staking_smoke` now performs a strict full-framework artifact/app-subnet preflight. It intentionally still fails the required governance gate after preflight until the SNS-W deploy/finalize/list-neurons driver is implemented.

Use `tools/scripts/run-real-framework-e2e` for the local all-in-one operator path after copying this file to `tests/e2e_real_canisters/wasms.local.toml` and setting `POCKET_IC_BIN`. The script fetches pinned artifacts, verifies compressed source hashes, decompresses to installable Wasms, fills local uncompressed hashes, and runs the ignored real-framework tests. It does not use `--network ic` and must not be run against production fiduciary canisters.

### Remaining Real SNS-W Driver Work

The exact-economics E2E is a real-ledger test, not yet a complete SNS-W-launched governance test. The remaining implementation step is to port DFINITY's NNS installer and SNS-W deployment payload DTOs so the harness can:

- install NNS ledger/root/governance/lifeline/SNS-W/registry/CMC with valid init payloads;
- publish the pinned SNS root/governance/ledger/index/swap/archive Wasms to SNS-W through NNS proposals;
- deploy SNS through SNS-W rather than direct-installing ledgers;
- finalize the swap;
- prove normal SNS `list_neurons`, top-up, dissolve-delay, voting/following, and root app-control behaviour.
