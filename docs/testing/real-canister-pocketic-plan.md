# Real-Canister PocketIC Plan

Goal: run all-real or as-real-as-possible canister stacks under the Real-framework PocketIC layer without mainnet calls, without unpinned downloads in CI, and without treating IO-owned mocks as real SNS framework proof. The ledger/index layer is executable when pinned local artifacts are supplied. The full NNS/SNS artifact set is pinned, including NNS Lifeline. SNS-W publication now uses the DFINITY compressed-source payload shape: `sns_governance.wasm.gz` is 1,695,732 bytes, the decompressed installable Wasm is 6,723,691 bytes, and the compressed full NNS proposal Candid payload is 1,696,127 bytes versus 6,724,088 bytes for the decompressed path. SNS-W deployment and swap-open are executable; swap participation/finalization, finalized governance staking, and full IO E2E remain open.

## Current Feasibility

| Canister/framework | Can be installed today? | Reason |
| --- | --- | --- |
| real SNS ledger | Opt-in executable | `tests/e2e_real_canisters` installs a locally supplied pinned SNS ledger Wasm in PocketIC and exercises ICRC metadata, balances, transfer, BadFee, InsufficientFunds, Duplicate, same-Wasm upgrade, and constant total supply. No pinned real SNS ledger Wasm is present in the repo. |
| real SNS index | Opt-in executable | `tests/e2e_real_canisters` installs a locally supplied pinned SNS index Wasm in PocketIC, points it at the ledger, checks account history for reserve/user transfers, and verifies same-Wasm upgrade. No pinned real SNS index Wasm is present in the repo. |
| real SNS governance | Narrow opt-in executable | The real governance Wasm is pinned and verified locally. A direct-governance PocketIC smoke installs empty governance state and queries `list_neurons`, `list_proposals`, and nervous-system parameters. Normal SNS staking and rewards still need ledger linkage through finalized SNS-W lifecycle. SNS-W publication uses the compressed `.wasm.gz` source artifact through NNS proposals; the decompressed Wasm is used only for PocketIC install paths. |
| real SNS root | Narrow opt-in executable | The real root Wasm is pinned and verified locally. A direct-root PocketIC smoke installs root and verifies `list_sns_canisters` against a local application-subnet dapp. Finalized-SNS root control still needs SNS-W deployment/finalization. |
| real SNS swap | Narrow opt-in executable | The real swap Wasm is pinned and verified locally. `real_sns_swap_opens_with_expected_parameters` deploys SNS through SNS-W and observes the swap open with local sale parameters. Commit/finalize and direct-participation neuron creation still need participant funding and polling helpers. |
| SNS-W in PocketIC | Narrow opt-in executable | The SNS-W Wasm is pinned and verified locally. Ignored tests install real SNS-W on the NNS subnet, query readiness methods, directly publish compressed source Wasms, and publish all six SNS Wasm slots through real NNS proposals. `real_sns_w_governance_wasm_publication_payload_sizes_are_understood` proves the prior 6,724,190-byte blocker was the decompressed governance payload plus update-call overhead. `real_sns_lifecycle_deploys_sns_via_sns_w` calls real `deploy_new_sns` after NNS proposal publication. |
| NNS Lifeline | Artifact pinned, required | DFINITY's current NNS installer uses Lifeline as part of the NNS root/control setup before SNS-W. The artifact filename is `lifeline_canister.wasm.gz`. |
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

The current full-framework manifest also includes NNS ledger/governance/root/Lifeline/registry/CMC plus ICP ledger/index. Lifeline is required and pinned from `https://download.dfinity.systems/ic/36ceffe4c47f4c3a881e75951178f5413f777f6c/canisters/lifeline_canister.wasm.gz` with compressed SHA-256 `0d9221e28781e8b627c0e0696b16c0301424d4387514ed5fdae4fa74ad4b696b` and uncompressed SHA-256 `f43f8c231644359423bfb755e9c1b477e3d0cd6cb3beb3d45905fdec6b3ba188`.

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

The direct governance layer now runs real SNS governance plus real SNS ledger staking calls when pinned artifacts are supplied. The full IO E2E test name runs the real-ledger exact-economics layer when pinned ledger/index artifacts are supplied; it does not claim full SNS/NNS coverage:

- `real_sns_governance_staking_smoke` — direct-installs real SNS governance and real SNS ledger, stakes through the normal `manage_neuron` claim/refresh path, proves top-up increases the listed neuron stake, proves below-minimum stake rejection, and observes dissolve delay below and at the two-week eligibility boundary
- `real_canister_e2e_icp_to_io_stake_reward_redemption` — real-ledger exact-economics E2E using pinned SNS ledger/index artifacts; no real SNS governance/root or NNS maturity yet
- `real_nns_sns_wasm_canister_responds_to_basic_queries` — installs real SNS-W and queries local SNS subnet configuration; no NNS governance/root install yet
- `real_nns_sns_wasm_bootstrapped_by_pocketic_icp_features_contains_all_sns_wasm_slots` — uses PocketIC ICP features to initialize registry, ICP ledger, NNS governance/root, and SNS-W, then queries `get_latest_sns_version_pretty` to prove all six SNS Wasm slots exist; this is PocketIC bootstrap proof, not NNS proposal publication proof
- `real_sns_governance_direct_empty_state_lists_no_neurons_or_proposals` — installs real SNS governance with empty state and queries list APIs; no voting
- `real_sns_user_stakes_io_normal_path_and_list_neurons_observes_it_direct_governance_path`, `real_sns_user_topup_increases_existing_neuron_stake_direct_governance_path`, `real_sns_minimum_stake_is_enforced_direct_governance_path`, and `real_sns_dissolve_delay_boundaries_are_visible_direct_governance_path` — direct real governance/ledger staking proofs; no SNS-W deployment, swap-created neurons, voting, or finalized root control
- `real_sns_w_accepts_root_ledger_index_swap_archive_wasms_direct_test_path` and related SNS-W tests — direct-call publication/readback for Wasms; lower-level coverage, not finalized lifecycle proof
- `real_sns_w_governance_wasm_publication_payload_sizes_are_understood` — proves compressed governance publication fits PocketIC ingress and that the old 6,724,190-byte blocker was the decompressed proposal path
- `real_sns_w_publishes_large_governance_wasm_via_gzipped_nns_proposal` and `real_sns_w_publishes_root_governance_ledger_index_swap_archive_via_nns` — publish compressed source Wasms through real NNS proposals and verify SNS-W readback
- `real_sns_lifecycle_deploys_sns_via_sns_w` and `real_sns_swap_opens_with_expected_parameters` — deploy SNS through real SNS-W and observe the real swap open with expected local parameters
- `real_sns_root_control_uses_application_subnet_canister_direct_root_path` — direct root install/list smoke; no finalized SNS root control

## First Runnable Layer To Build Next

1. Fund at least one swap participant, call `refresh_buyer_tokens`, commit, and finalize the SNS swap under PocketIC time.
2. Prove finalized direct-participation neurons through real SNS governance `list_neurons`.
3. Move direct real SNS governance staking/top-up/dissolve-delay assertions behind SNS-W finalization.
4. Add voting/following and finalized root app-control tests.
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


## Current Real-Framework Stride

`tools/scripts/run-real-framework-e2e` is the opt-in local operator path for pinned real framework artifacts. It fetches/verifies/decompresses configured artifacts, runs the real ledger/index tests, the PocketIC ICP-feature SNS-W bootstrap proof, gzipped NNS proposal publication for all six SNS Wasm slots, SNS-W deploy/swap-open tests, and the real-ledger exact-economics E2E. Governance/root/SNS-W normal staking remains blocked until swap participation/finalization and list-neurons drivers are implemented.
