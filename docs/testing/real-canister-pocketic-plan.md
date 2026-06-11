# Real-Canister PocketIC Plan

Goal: run all-real or as-real-as-possible canister stacks under the Real-framework PocketIC layer without mainnet calls, without unpinned downloads in CI, and without treating IO-owned mocks as real SNS framework proof. The ledger/index layer is executable when pinned local artifacts are supplied. The full NNS/SNS artifact set is pinned, including NNS Lifeline. SNS-W publication uses the DFINITY compressed-source payload shape: `sns_governance.wasm.gz` is 1,695,732 bytes, the decompressed installable Wasm is 6,723,691 bytes, and the compressed full NNS proposal Candid payload is 1,696,127 bytes versus 6,724,088 bytes for the decompressed path. SNS-W deployment, swap-open, direct participation, commit, finalization, finalized governance `list_neurons`, finalized root dapp listing/control/upgrade, post-finalization user staking/top-up/multiple-neuron/minimum-stake/dissolve-delay/start-dissolving/stop-dissolving eligibility checks, no-closed-proposals participation weighting, finalized yes/no voting, finalized following vote propagation, finalized non-voter reward-policy weighting, IO canister installation against finalized local SNS IDs, stream-manager finalized SNS active-stake refresh/top-up refresh, stream-manager local ICP plus finalized SNS index no-traffic scan safety, real local ICP-indexed Jupiter deposit issuance from the finalized SNS ledger reserve, exact backed two-week reward-pool transfer through the finalized SNS ledger, real dissolving-neuron reward exclusion, and one finalized-SNS IO redemption scan with local ICP payout plus SNS IO reserve return are executable; broader participation-weighted stream-manager reward allocation variants remain blocked.

## Current Feasibility

| Canister/framework | Can be installed today? | Reason |
| --- | --- | --- |
| real SNS ledger | Opt-in executable | `tests/e2e_real_canisters` installs a locally supplied pinned SNS ledger Wasm in PocketIC and exercises ICRC metadata, balances, transfer, BadFee, InsufficientFunds, Duplicate, same-Wasm upgrade, and constant total supply. No pinned real SNS ledger Wasm is present in the repo. |
| real SNS index | Opt-in executable | `tests/e2e_real_canisters` installs a locally supplied pinned SNS index Wasm in PocketIC, points it at the ledger, checks account history for reserve/user transfers, and verifies same-Wasm upgrade. No pinned real SNS index Wasm is present in the repo. |
| real SNS governance | Narrow opt-in executable | The real governance Wasm is pinned and verified locally. A direct-governance PocketIC smoke installs empty governance state and queries `list_neurons`, `list_proposals`, and nervous-system parameters. The finalized SNS lifecycle fixture queries finalized governance `list_neurons` after direct swap participation and `finalize_swap`, verifies participant controller permissions, `[0, 1]` second basket dissolve delays, distinct multi-participant neurons, no duplicate neuron IDs, and limit-constrained listing subsets, then disburses a zero-delay direct-participation neuron into a participant liquid account and proves normal staking, top-up, multiple distinct post-finalization neurons, minimum-stake rejection, below/two-week dissolve-delay eligibility, start-dissolving exclusion, stop-dissolving restored eligibility, no-closed-proposals participation weighting, yes/no voting, following vote propagation, and non-voter reward-policy weighting through finalized governance. Stream-manager consumes finalized governance `list_neurons` for active stake, exact active-stake increase after finalized SNS top-up, one exact backed two-week reward-pool transfer, and real dissolving-neuron reward exclusion; broader participation-weighted stream-manager reward variants remain pending. |
| real SNS root | Narrow opt-in executable | The real root Wasm is pinned and verified locally. A direct-root PocketIC smoke installs root and verifies `list_sns_canisters` against a local application-subnet dapp. The finalized SNS lifecycle fixture now verifies `set_dapp_controllers`, root `list_sns_canisters`, dapp controller transfer to the finalized root, and a finalized governance `UpgradeSnsControlledCanister` proposal that changes the registered dapp module hash through root. |
| real SNS swap | Narrow opt-in executable | The real swap Wasm is pinned and verified locally. The lifecycle fixture deploys SNS through SNS-W, observes swap open, creates sale tickets, funds the swap escrow subaccount on the local NNS ledger, refreshes buyer tokens, observes direct participants, awaits commit, and calls real `finalize_swap`. |
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
- `real_sns_participant_can_fund_swap_account_and_refresh_buyer_tokens` and `real_sns_swap_observes_direct_participation` — fund the real swap escrow account through the local NNS ledger, call real `refresh_buyer_tokens`, and verify buyer/direct-participant state
- `real_sns_swap_commits_after_minimum_participation`, `real_sns_swap_finalizes_successfully_and_preserves_canister_ids`, and `real_sns_finalized_governance_*` — drive commit/finalization through the real swap and verify finalized governance direct-participation neurons, participant permissions, basket dissolve delays, multi-participant distinctness, duplicate-ID absence, and limit-constrained listing behavior
- `real_sns_user_stakes_io_normal_path_after_sns_w_finalization`, `real_sns_user_topup_increases_existing_neuron_after_sns_w_finalization`, `real_sns_user_can_stake_multiple_neurons_after_finalization`, `real_sns_minimum_stake_is_enforced_after_finalization`, `real_sns_dissolve_delay_below_two_weeks_is_ineligible_after_finalization`, and `real_sns_dissolve_delay_at_two_weeks_is_eligible_after_finalization` — use finalized SNS governance and ledger after swap finalization; liquid test funding comes from disbursing a zero-delay direct-participation neuron created by the real swap, then staking/top-up/multiple-neuron paths use normal ledger transfers plus `manage_neuron(ClaimOrRefresh)`
- `real_sns_no_closed_proposals_participation_factor_defaults_to_one_after_finalization` — queries finalized governance `list_proposals`, observes a fresh finalized local SNS has no closed reward proposals before voting setup, and feeds the finalized staked neuron into `io_reward_policy` to prove participation defaults to full weight
- `real_sns_user_votes_yes_and_ballot_is_observed_after_finalization` — submits a finalized SNS motion proposal through real `manage_neuron(MakeProposal)`, observes the proposer ballot through `list_proposals`, and proves duplicate proposer voting fails safely with `Neuron already voted on proposal`
- `real_sns_user_votes_no_and_ballot_is_observed_after_finalization` — finalizes two direct participants, submits a motion with one finalized neuron, registers a no vote from the second finalized neuron, and observes the voter ballot through caller-aware `list_proposals`
- `real_sns_non_voter_gets_lower_participation_factor_after_finalization` — finalizes two direct participants within the local 10 ICP sale cap, closes a finalized governance proposal through PocketIC time advancement, converts finalized proposal/ballot observations into `io_reward_policy`, and proves the non-voter receives lower reward weight than the proposer
- `real_sns_proposal_rejection_fee_is_100_io_if_configured_after_finalization` — queries finalized governance parameters and a finalized proposal record to prove the local SNS proposal rejection fee is the configured 100 IO
- `real_sns_root_lists_application_subnet_dapp_after_finalization`, `real_sns_root_can_upgrade_test_app_canister_after_finalization`, and `real_sns_root_rejects_non_dapp_canister_after_finalization` — verify finalized root `list_sns_canisters`, dapp application-subnet placement, finalized root controller transfer, finalized governance/root-mediated dapp module upgrade, and that a separate application-subnet non-dapp is neither listed nor controlled by finalized root
- `real_sns_root_control_uses_application_subnet_canister_direct_root_path` — direct root install/list smoke; no finalized SNS root control

## First Runnable Layer To Build Next

1. Extend stream-manager reward allocation to handle real SNS byte-vector neuron IDs instead of the current `NeuronSnapshot { neuron_id: u64 }` boundary.
2. Extend stream-manager value-moving real ledger/index scanner processing beyond the proven Jupiter deposit/issuance and single redemption payout/return paths to reward distribution, current-rate redemption variants, and retry/upgrade cases against the local ICP index and finalized SNS index already wired into the fixture.
3. Add upgrade/retry/idempotency tests around the finalized-governance active-stake refresh and future real scanner operations.

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

`tools/scripts/run-real-framework-e2e` is the opt-in local operator path for pinned real framework artifacts. It fetches/verifies/decompresses configured artifacts, runs the real ledger/index tests, the PocketIC ICP-feature SNS-W bootstrap proof, gzipped NNS proposal publication for all six SNS Wasm slots, SNS-W deploy/swap-open/participation/commit/finalization/finalized-governance/finalized staking/finalized no-closed-proposals weighting/finalized yes/no voting/finalized following/finalized non-voter reward-policy weighting/finalized-root tests, builds local debug IO Wasms, installs IO canisters against the finalized local SNS with local ICP index and finalized SNS index scanner principals configured, verifies stream-manager finalized-governance active-stake refresh plus no-traffic local ICP/SNS index scan safety, proves real local ICP index deposit scanning and finalized SNS reserve issuance for a 100 ICP Jupiter Faucet deposit, proves one finalized-SNS IO redemption scan with local ICP payout and SNS IO reserve return, and runs the real-ledger exact-economics E2E. Participation-weighted stream-manager reward allocation plus real reward scanner processing remain separate implementation work.
