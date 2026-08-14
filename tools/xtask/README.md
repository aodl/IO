# IO xtask

`xtask` is IO's stable repository orchestration interface. It turns the
workspace's build, test, Candid, release, security, SNS, evidence, and PocketIC
checks into named commands whose composition is reviewed in
[`src/main.rs`](src/main.rs). Run commands from the repository root:

```bash
cargo run -p xtask -- <command> [arguments]
```

With no command, xtask runs `test_all`.

## Prerequisites and safety

- Use the Rust toolchain in `rust-toolchain.toml` and the checked-in
  `Cargo.lock`.
- Frontend setup requires Node/npm and runs `npm ci` from the locked
  `package-lock.json`; it may require network access.
- Live local integration uses the project `icp` CLI.
- PocketIC-required commands need, for example:

  ```bash
  POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server
  ```

  On a resource-constrained workstation, the maintained serial settings are:

  ```bash
  IO_SNS_BAZEL_JOBS=1 CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 \
    POCKET_IC_MUTE_SERVER=1 \
    POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server \
    cargo run -p xtask -- test_ci
  ```

- Run heavyweight commands serially. Frontend setup, debug-canister builds,
  PocketIC suites, `test_all`, and `verify_release` share generated paths.
- Value-moving PocketIC suites must run serially; their single-operation,
  ledger, generated-Wasm, and local-network fixtures are not concurrent test
  resources.
- “Required” means missing prerequisites are failures. Non-required real-
  canister/local-evidence commands may report an explicit skip when their
  external inputs are absent.
- xtask validation does not authorize mainnet. Do not supply a public boundary
  endpoint, production identity, or mainnet network flag to local workflows.
  Stateful rehearsal is owned by the guarded local-only scripts, not by an
  implicit xtask default.

## Recommended entry points

| Command | Purpose | Inputs/effects |
| --- | --- | --- |
| `simplicity_check` | Enforce production-size, API, authority, and simplified-execution guardrails | Fast, static, no network |
| `test_all` | Developer aggregate: preflight, unit/static checks, debug PocketIC path, local `icp` build, E2E, permissive security scan | Builds generated frontend/debug artifacts; PocketIC live tests skip when unavailable |
| `test_ci` | Strict CI aggregate, including exact-source/artifact/evidence checks, required PocketIC/SNS paths, workspace tests, wasm32 check, and clippy | Requires `POCKET_IC_BIN`; heavy and serial |
| `verify_release` | Release gate over APIs, pins, exact-source artifacts, wiring, stable storage, evidence, frontend, SNS, real-harness registration, and required security scan | Performs two exact-source release builds and frontend setup; does not deploy |
| `preflight` | Workspace check plus Candid, NNS pin, and install-argument validation | Fast compile/static path |
| `test_unit` | IO unit/static suites and test-registration guardrails | No required PocketIC server |

## Workspace, component, and PocketIC tests

| Commands | Meaning |
| --- | --- |
| `check`, `fmt_check` | `cargo check --workspace --all-targets`; workspace rustfmt check |
| `test_unit`, `test_e2e`, `test_local_integration` | Unit/static aggregate; simplified boundary E2E; artifact/API plus local `icp project show`/`icp build` path |
| `test_pocketic_integration`, `test_pocketic_required` | Build debug canisters and run manager/Historian PocketIC tests; required form fails without `POCKET_IC_BIN` |
| `stream_manager_unit`, `nns_neuron_manager_unit` | Focused manager library tests |
| `stream_manager_pocketic_integration`, `nns_neuron_manager_pocketic_integration`, `historian_pocketic_integration` | Focused live PocketIC targets; caller supplies `POCKET_IC_BIN` when execution is required |
| `build_debug_canisters` | Build all IO and mock debug Wasms after frontend setup; generates `debug-artifacts/` and may run `npm ci` |

## API, configuration, wiring, and stable-state gates

| Command | Meaning |
| --- | --- |
| `did_surface` | Match production DIDs, exported methods, frontend declarations, and debug separation |
| `validate_install_args [local\|mainnet\|all]` | Validate constructor shapes and the non-runnable mainnet templates; default is `all` |
| `validate_nns_boundary_pin` | Validate pinned NNS source/Wasm/DID contract and related tests/docs |
| `validate_prelaunch_public_shell` | Check legacy DevMainnet status and no-value-moving boundaries |
| `validate_production_wiring` | Parse offline production-planned templates and enforce principals, fees, roles, protected targets, and reserved/authority distinctions |
| `validate_historian_freshness` | Check Historian production read-only/freshness and protected-reference guardrails |
| `validate_stable_storage` | Check schema registry, stable fixtures, constructor-only DIDs, and upgrade/storage documentation |
| `e2e_coverage_matrix_check` | Validate the checked-in E2E coverage registry |
| `live_stream_manager_pocketic_gate_check` | Validate registration of required live Stream Manager gates |

`validate_production_wiring` and `validate_install_args` require protected IO
NNS neuron `10292412127977304661` and require the NNS Manager authority to be
`oae4c-3iaaa-aaaar-qb5qq-cai`. That protected principal is accepted only for
the specific manager role and remains forbidden as an arbitrary mutation
target. These validators are static and do not inspect or call mainnet.

## Frontend

| Command | Meaning |
| --- | --- |
| `frontend_setup` | Run `npm run setup:frontend` (`npm ci`; network may be required) |
| `frontend_build` | Build/stamp the browser bundle |
| `frontend_unit` | Run browser unit tests |
| `frontend_certified_asset_tests` | Build browser assets, run Rust asset tests, and run the frontend PocketIC smoke when `POCKET_IC_BIN` is set |
| `frontend_required`, `frontend_all` | Required aggregate of setup, build, browser tests, and certified-asset tests (`frontend_all` is an alias) |

## Historian and SNS-shaped compatibility

| Command | Meaning |
| --- | --- |
| `historian_tests` | Historian DID guardrail plus library tests |
| `historian_required` | Historian tests, debug build, and live PocketIC test; fails without `POCKET_IC_BIN` |
| `sns_apy_policy_tests` | Reward-policy unit suite |
| `sns_governance_read_tests`, `sns_governance_read_required` | Canonical SNS reward-boundary and Stream reward-evidence tests; required name currently adds no external prerequisite |
| `sns_ledger_index_tests` | Ledger/index types, scheduler, and SNS-shaped mock tests |
| `sns_ledger_index_required` | Ledger/index tests plus live Stream Manager PocketIC flows; requires `POCKET_IC_BIN` |
| `sns_root_lifecycle_tests` | Root/lifecycle guardrails and unit tests |
| `sns_root_lifecycle_required` | Root/lifecycle tests plus serial live PocketIC lifecycle target; requires `POCKET_IC_BIN` |
| `sns_pocketic_smoke` | Harness check and optional PocketIC topology tests |
| `sns_pocketic_required` | Required topology and Root lifecycle PocketIC targets; requires `POCKET_IC_BIN` |

## SNS configuration, framework profiles, and local evidence

| Command | Meaning |
| --- | --- |
| `sns_harness_check` | Static guardrails for local SNS fixtures, docs, scripts, and source-built tooling notes |
| `sns_config_validate` | Validate checked-in SNS template structure without external SNS tooling |
| `sns_config_validate_official` | Optional source-built `sns init-config-file ... validate`; runs only with `IO_RUN_SOURCE_BUILT_SNS_VALIDATE=1` and an available `sns` binary |
| `sns_official_testing_check` | Validate the maintained official-SNS testing package and safety wording |
| `sns_launch_readiness_check [--strict]` | Validate `tools/sns/launch-readiness.toml`; normal mode reports incomplete count, strict mode rejects incompleteness |
| `validate_local_sns_rehearsal` | Static local-only package/guardrail validation |
| `validate_local_sns_ledger` | Validate ignored per-run ledger evidence when present; otherwise explicitly skips |
| `validate_local_sns_evidence_package <directory>` | Intrinsically validate one complete monitoring/canonical candidate package |
| `validate_local_sns_committed_evidence` | Validate every immutable package and the exact selected current package |
| `validate_local_sns_scripts` | Execute no-network script fixtures and negative safety cases |
| `local_sns_evidence_tests` | Parse supplied local evidence only with `IO_LOCAL_SNS_REHEARSAL_ACK=local-only`; path may be set with `IO_LOCAL_SNS_EVIDENCE` |

`sns_framework` is a separately parsed xtask subcommand that delegates to the
single maintained SNS framework runner. Its source/profile arguments are
documented in [SNS framework sources](../../docs/testing/sns-framework-sources.md):

```bash
cargo run -p xtask -- sns_framework --source official --profile contract
```

Use `--source official` by default. It resolves the checked-in, reviewed
official baseline rather than a moving upstream branch. Use `--source local`
only for unpublished candidate or sibling-IC work; the sibling checkout is
read/build input and must not be switched, pulled, reset, cleaned, or modified.
Every profile reports source mode, scope/profile, IC revision and dirty state,
tracked-diff hash, official baseline, component overrides, resolved bundle
manifest, Governance DID hash, and artifact hashes. Candidate success is
compatibility evidence, not an official release or production configuration.

## Real-canister artifacts and opt-in suites

These commands need externally supplied, checksum-pinned real NNS/SNS Wasms or
an artifact manifest. The repository does not commit those Wasms.

| Command | Meaning |
| --- | --- |
| `real_canister_harness_check` | Static harness/docs/test-registration checks |
| `real_canister_artifact_manifest_check [--required]` | Validate configured artifact manifest; optional form skips if absent |
| `verify_real_canister_artifacts` | Verify configured artifacts; skips if absent |
| `fetch_real_canister_artifacts` | Run the pinned artifact fetcher; this is explicitly network-fetching |
| `real_sns_ledger_index_tests`, `real_sns_ledger_index_required` | Unit registration plus optional/required ignored real ledger/index tests |
| `real_sns_governance_tests`, `real_sns_governance_required` | Registration-only optional path or required ignored real Governance tests |
| `real_io_e2e_tests`, `real_io_e2e_required` | Optional/required ignored real IO-stack scenarios |
| `e2e_real_coverage_check` | Coverage/static checks plus optional real ledger/index layer |

The `*_required` variants fail when either artifacts or `POCKET_IC_BIN` are
missing. Run all live/value-moving PocketIC profiles serially.

## Release artifacts and security

| Command | Meaning |
| --- | --- |
| `verify_artifacts` | Verify checked-in raw/gzip Wasm, sidecars, sizes, hashes, manifest source, and release lineage; does not generate |
| `verify_recorded_source` | Preserve checked-in artifacts and compare them with two detached builds of the exact manifest source |
| `build_recorded_source` | Build the exact manifest source through the supported detached-worktree script |
| `build_canisters` | Intentionally regenerate frontend and all release artifacts/manifest; requires the exact source context and mutates `release-artifacts/` |
| `compare_release_artifact_dirs <first> <second>` | Byte-, size-, manifest-, and file-set comparison for two artifact directories |
| `security_scan` | Permissive developer security scan |
| `security_scan_required` | Required dependency/license/security gate; missing tooling or new findings fail |

Artifact generation is not an ordinary test. Follow
[reproducible builds](../../docs/operations/reproducible-builds.md) and the
[release checklist](../../docs/operations/release-checklist.md) before using a
build command.

## Pure helpers

These commands calculate or inspect local text and do not call a network:

| Command | Arguments/output |
| --- | --- |
| `nns_neuron_staking_subaccount` | `<controller-principal> <nonce>`; prints the canonical NNS staking subaccount |
| `sns_distribution_subaccount` | `<governance-principal> <nonce>`; prints the canonical SNS distribution subaccount |
| `calculate_redemption_economics` | `<total> <reserve> <excluded> <liquid> <redeemed> <icp-fee>`; prints checked excluded/redeemable/gross/net values |
| `index_transfer_block` | `<history-file> <amount-e8s> <memo-hex>`; finds the exact matching transfer block in supplied history text |

## Which command should I run?

- Editing one crate: run its focused tests, then `fmt_check`, `check`, and
  `simplicity_check` as applicable.
- Editing DIDs, install arguments, wiring, stable state, Historian, or frontend:
  run the named focused validator above before an aggregate.
- Preparing CI: set `POCKET_IC_BIN` and run `test_ci` serially.
- Reviewing a release without generating artifacts: run `verify_release`.
- Generating a release: stop and follow the two-commit release procedure; do not
  use a build command as a repair for an unexplained artifact mismatch.
