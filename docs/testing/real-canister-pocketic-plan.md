# Real-Canister PocketIC Plan

Goal: run all-real or as-real-as-possible canister stacks under the Real-framework PocketIC layer without mainnet calls, without unpinned downloads in CI, and without treating IO-owned mocks as real SNS framework proof. The ledger/index layer is executable when pinned local artifacts are supplied. Governance/root and full IO E2E remain blocked until pinned artifacts and init drivers exist.

## Current Feasibility

| Canister/framework | Can be installed today? | Reason |
| --- | --- | --- |
| real SNS ledger | Opt-in executable | `tests/e2e_real_canisters` installs a locally supplied pinned SNS ledger Wasm in PocketIC and exercises ICRC metadata, balances, transfer, BadFee, InsufficientFunds, Duplicate, same-Wasm upgrade, and constant total supply. No pinned real SNS ledger Wasm is present in the repo. |
| real SNS index | Opt-in executable | `tests/e2e_real_canisters` installs a locally supplied pinned SNS index Wasm in PocketIC, points it at the ledger, checks account history for reserve/user transfers, and verifies same-Wasm upgrade. No pinned real SNS index Wasm is present in the repo. |
| real SNS governance | Blocked | No pinned real SNS governance Wasm/init package is present. Normal SNS staking and rewards need governance init data and ledger linkage. |
| real SNS root | Blocked | No pinned real SNS root Wasm/init package is present. |
| real SNS swap | Blocked | No pinned real SNS swap Wasm/init package is present. |
| SNS-W in PocketIC | Blocked | No pinned SNS-W Wasm and no local SNS-W driver are present. |
| official local SNS tooling | Optional/manual | `dfx 0.27.0` is installed locally, but `dfx sns` is unavailable in this environment. Required CI must not depend on `dfx`. |
| attach tests to completed local SNS canister set | Evidence-only today | `validate_local_sns_ledger` can validate a completed evidence file. Direct canister calls require `IO_LOCAL_SNS_CANISTER_CALLS=local-only` and a future local-only caller. |

## Required Artifacts

Use a local directory supplied by `IO_REAL_SNS_WASM_DIR`. Use `IO_REAL_SNS_WASM_MANIFEST` to point at a local manifest, or create ignored `tests/e2e_real_canisters/wasms.local.toml`. The manifest format is documented by `tests/e2e_real_canisters/wasms.example.toml`. The first ledger/index smoke layer expects, at minimum:

- `sns_ledger.wasm`
- `sns_ledger_sha256` in the manifest
- `sns_index.wasm`
- `sns_index_sha256` in the manifest

Future governance/root/swap/SNS-W layers should add pinned Wasms and hashes for:

- `sns_governance.wasm`
- `sns_root.wasm`
- `sns_swap.wasm`
- `sns_wasm.wasm` or the current official SNS-W artifact name

The source, version, license, and SHA-256 must be recorded before any CI or reproducible local test consumes these artifacts. Do not download unpinned Wasms in CI. Do not fetch from mainnet.

## Harness Status

The crate `tests/e2e_real_canisters` defines ignored tests:

```bash
IO_REAL_SNS_WASM_DIR=/path/to/pinned/wasms \
IO_REAL_SNS_WASM_MANIFEST=tests/e2e_real_canisters/wasms.local.toml \
POCKET_IC_BIN=/home/codexdev/.local/bin/pocket-ic-server \
cargo test -p e2e-real-canisters real_sns_ledger_index_smoke -- --ignored --nocapture
```

The ledger/index tests skip only when no artifact env/manifest is configured. If artifact env is present but invalid, or if artifacts are configured without `POCKET_IC_BIN`, they fail. They do not download Wasms or call mainnet.

The executable ledger/index layer covers:

- `real_sns_ledger_index_smoke`
- `real_sns_ledger_index_same_wasm_upgrade_preserves_balances_history_and_duplicates`

The governance and full IO E2E test names are registered as ignored blockers, but they do not claim real-framework coverage:

- `real_sns_governance_staking_smoke`
- `real_canister_e2e_icp_to_io_stake_reward_redemption`

## First Runnable Layer To Build Next

1. Provide pinned local ledger/index Wasms and manifest hashes.
2. Run the ledger/index ignored tests and record evidence in `deploy/local-sns-rehearsal/real-canister-e2e-evidence.example.toml` copied to a local evidence file.
3. Add pinned governance/root Wasms and an SNS governance init driver that supports normal staking.
4. Prove normal SNS staking/top-up/list-neurons through real governance/root.
5. Wire IO stream-manager tests to real SNS ledger/index/governance snapshots without calling mock debug APIs.

## Manual Official Local SNS Layer

The official local SNS rehearsal remains optional/manual. Completed local SNS proof exists only when `deploy/local-sns-rehearsal/canister-ids.local.toml` is produced from a completed local rehearsal and:

```bash
cargo run -p xtask -- validate_local_sns_ledger
```

passes. That evidence gate does not call canisters by default. A future direct-calls layer must require both:

```bash
IO_LOCAL_SNS_REHEARSAL_ACK=local-only
IO_LOCAL_SNS_CANISTER_CALLS=local-only
```

and must reject any `--network ic` path.
