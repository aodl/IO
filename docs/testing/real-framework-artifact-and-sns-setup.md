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
- Lifecycle participation from `sns_lifecycle.rs`: fund participant ICP accounts, transfer ICP to the swap subaccount, optionally create sale tickets, call `refresh_buyer_tokens`, await committed/open/finalized lifecycle, and assert direct-participation SNS neurons via finalized governance `list_neurons`, including participant permissions, basket dissolve delays, multi-participant distinctness, duplicate-ID absence, and limit-constrained listing subsets.

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

### NNS Lifeline Resolution

`nns_lifeline` is required for the minimal NNS/SNS-W path. DFINITY's current `NnsInstaller` installs NNS Root with Lifeline as controller, installs NNS Governance under Root, then installs Lifeline before SNS-W. The matching NNS constants list the artifact filename pattern as `lifeline_canister`, not `lifeline-canister`.

Pinned IO artifact entry:

- source URL: `https://download.dfinity.systems/ic/36ceffe4c47f4c3a881e75951178f5413f777f6c/canisters/lifeline_canister.wasm.gz`
- upstream revision: `36ceffe4c47f4c3a881e75951178f5413f777f6c`
- compressed SHA-256: `0d9221e28781e8b627c0e0696b16c0301424d4387514ed5fdae4fa74ad4b696b`
- uncompressed SHA-256: `f43f8c231644359423bfb755e9c1b477e3d0cd6cb3beb3d45905fdec6b3ba188`
- license: `Apache-2.0`

The full-framework preflight treats Lifeline as required, not optional. A missing Lifeline artifact should fail required gates before the SNS-W deployment driver runs.

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

The IO harness now has direct opt-in layers:

1. `real_sns_ledger_index_smoke` installs pinned real SNS ledger/index Wasms on the SNS subnet and verifies ICRC metadata, transfers, errors, duplicate handling, index history, and same-Wasm upgrade behaviour.
2. `real_canister_e2e_icp_to_io_stake_reward_redemption` uses pinned real ICRC ledger/index canisters for the ledger movement slice and the pure IO accounting/reward policy crates for exact expected economics: Jupiter Faucet ICP input, 40/60 split, backed IO issuance, holder compounding via rate increase, two-week staker rewards, participation-weighted higher staking returns, and redemption at the current rate.
3. `real_sns_governance_staking_smoke` direct-installs real SNS governance and a real SNS ledger, transfers liquid IO into the governance staking subaccount, claims/refreshed a neuron through `manage_neuron`, verifies `list_neurons`, verifies top-up increases the same neuron's cached stake, verifies the real below-minimum stake rejection, and observes dissolve-delay state below and at the two-week eligibility boundary.
4. `real_nns_sns_wasm_canister_responds_to_basic_queries` installs real SNS-W on the PocketIC NNS subnet and queries configured SNS subnet IDs.
5. `real_nns_sns_wasm_bootstrapped_by_pocketic_icp_features_contains_all_sns_wasm_slots` uses PocketIC ICP features to initialize registry, ICP ledger, NNS governance/root, and SNS-W, then verifies SNS-W reports Root, Governance, Ledger, Ledger Index, Swap, and Ledger Archive hashes. This is a PocketIC bootstrap proof, not NNS proposal publication proof.
6. `real_sns_governance_direct_empty_state_lists_no_neurons_or_proposals` direct-installs real SNS governance with empty local state and verifies `list_neurons`, `list_proposals`, and nervous-system parameters.
7. The direct SNS-W publication tests call real `add_wasm`/`get_wasm` for compressed source Wasms and verify wrong-hash rejection plus duplicate publication safety. These are lower-level direct tests, not finalized lifecycle proof.
8. `real_sns_w_governance_wasm_publication_payload_sizes_are_understood` proves the prior 6,724,190-byte SNS governance publication blocker was built from decompressed `sns_governance.wasm` plus update-call overhead. The compressed source artifact is 1,695,732 bytes, the decompressed installable Wasm is 6,723,691 bytes, the compressed full NNS proposal Candid payload is 1,696,051 bytes, and the decompressed full proposal Candid payload is 6,724,012 bytes.
9. `real_sns_w_publishes_large_governance_wasm_via_gzipped_nns_proposal` and `real_sns_w_publishes_root_governance_ledger_index_swap_archive_via_nns` publish compressed `.wasm.gz` SNS Wasm payloads through real NNS `manage_neuron(MakeProposal(ExecuteNnsFunction(AddSnsWasm)))` and verify SNS-W `get_wasm` readback.
10. `real_sns_lifecycle_deploys_sns_via_sns_w` and `real_sns_swap_opens_with_expected_parameters` publish all six Wasms through NNS proposals, deploy SNS through real SNS-W, verify `list_deployed_snses`, and observe the real swap open with expected local sale parameters.
11. `real_sns_participant_can_fund_swap_account_and_refresh_buyer_tokens` and `real_sns_swap_observes_direct_participation` create real sale tickets, fund the swap escrow subaccount on the local NNS ledger, call real `refresh_buyer_tokens`, and verify buyer/direct-participant state.
12. `real_sns_swap_commits_after_minimum_participation`, `real_sns_swap_finalizes_successfully_and_preserves_canister_ids`, and `real_sns_finalized_governance_*` drive real swap commit/finalization and verify finalized governance `list_neurons`, participant controller permissions, `[0, 1]` second direct-participation basket dissolve delays, multi-participant distinct neurons, duplicate-ID absence, and limit-constrained listing subsets.
13. `real_sns_user_stakes_io_normal_path_after_sns_w_finalization`, `real_sns_user_topup_increases_existing_neuron_after_sns_w_finalization`, `real_sns_user_can_stake_multiple_neurons_after_finalization`, `real_sns_minimum_stake_is_enforced_after_finalization`, `real_sns_dissolve_delay_below_two_weeks_is_ineligible_after_finalization`, and `real_sns_dissolve_delay_at_two_weeks_is_eligible_after_finalization` use the finalized SNS ledger/governance canisters after swap finalization. Liquid test funding comes from a real `Disburse` of a zero-delay direct-participation neuron created by the swap, and staking/top-up/multiple-neuron paths use normal ledger transfers plus `manage_neuron(ClaimOrRefresh)`.
14. `real_sns_no_closed_proposals_participation_factor_defaults_to_one_after_finalization` queries finalized governance `list_proposals`, observes no closed reward proposals before voting setup, and feeds the finalized staked neuron into `io_reward_policy` to prove full participation weight.
15. `real_sns_user_votes_yes_and_ballot_is_observed_after_finalization` submits a finalized SNS motion proposal, observes the proposer ballot through finalized governance `list_proposals`, and asserts duplicate proposer voting fails safely.
16. `real_sns_user_votes_no_and_ballot_is_observed_after_finalization` finalizes two direct participants, registers a no vote from the second finalized neuron, and reads the voter ballot through caller-aware finalized governance `list_proposals`.
17. `real_sns_non_voter_gets_lower_participation_factor_after_finalization` finalizes two direct participants within the local 10 ICP sale cap, closes a finalized governance proposal through PocketIC time advancement, converts finalized proposal/ballot observations into `io_reward_policy`, and proves the non-voter receives lower reward weight than the proposer.
18. `real_sns_proposal_rejection_fee_is_100_io_if_configured_after_finalization` queries finalized governance parameters and a finalized proposal record to prove the local SNS proposal rejection fee is the configured 100 IO.
19. `real_sns_root_lists_application_subnet_dapp_after_finalization`, `real_sns_root_can_upgrade_test_app_canister_after_finalization`, and `real_sns_root_rejects_non_dapp_canister_after_finalization` verify finalized root `list_sns_canisters`, application-subnet dapp placement, root controller transfer, finalized governance/root-mediated dapp module upgrade, and that a separate application-subnet non-dapp is neither listed nor controlled by finalized root.
20. `io_real_stack_installs_stream_manager_on_application_subnet`, `io_real_stack_installs_nns_neuron_manager_on_application_subnet`, and `io_real_stack_installs_historian_on_application_subnet` reuse the finalized SNS fixture and install `io_stream_manager`, `io_nns_neuron_manager`, and `io_historian` Wasms on the application subnet with local NNS ledger/governance and finalized SNS ledger/index/governance IDs. The companion unit/static tests reject production fiduciary, DevMainnet, protected canister, and protected neuron values before install.
21. `io_stream_manager_real_finalized_sns_list_neurons_updates_active_staked_io` uses locally built debug IO Wasms for evidence, calls stream-manager `debug_tick`, consumes finalized SNS governance `list_neurons`, and verifies `active_staked_io_e8s` equals finalized non-dissolving two-week-or-longer SNS neuron stake.
22. `io_stream_manager_real_jupiter_deposit_scanned_from_real_icp_index` funds the stream-manager protocol reserve with finalized SNS ledger tokens, transfers 100 ICP through the local NNS ledger from a Jupiter-labelled subaccount into the stream-manager deposit account, waits for the local real ICP index, then verifies stream-manager scans one deposit, issues exactly 60 IO from the finalized SNS ledger reserve, records 40 ICP two-year principal plus 60 ICP liquid reserve, and does not replay the issuance.
23. `io_stream_manager_real_two_week_maturity_5_icp_issues_exact_backed_reward_pool` grants the stream-manager finalized governance visibility over participant neurons, consumes finalized SNS `list_neurons` for reward snapshots, scans a 5 ICP two-week maturity deposit through the local ICP ledger/index, records the 2 ICP stake-accounting and 3 ICP liquid split, transfers exactly 3 IO of backed reward pool through the finalized SNS ledger, and records successful reward journal entries.
24. `io_stream_manager_real_two_week_maturity_rewards_only_eligible_stakers` creates normal post-finalization finalized-SNS staking neurons, starts one otherwise eligible neuron dissolving through finalized governance, then proves the non-dissolving neuron receives a positive finalized SNS ledger reward transfer while the dissolving neuron is absent from the reward journal and receives zero.
25. `io_stream_manager_real_sns_topup_increases_active_staked_io` creates a normal post-finalization finalized-SNS neuron, consumes it through stream-manager governance refresh, tops up the same neuron through finalized SNS governance, and proves stream-manager active stake increases by the exact top-up amount.
26. `io_stream_manager_real_redemption_pays_icp_on_real_local_ledger` funds a user redemption account through the finalized SNS ledger/index, verifies direct index visibility, runs stream-manager `debug_tick`, pays ICP on the local NNS ledger, returns redeemed IO minus the SNS ledger fee to the stream-manager protocol reserve account on the finalized SNS ledger, records the fee in the operation journal, and does not replay the redemption.
27. `real_sns_root_control_uses_application_subnet_canister_direct_root_path` direct-installs real SNS root with a local application-subnet dapp and verifies `list_sns_canisters`.

Use `tools/scripts/run-real-framework-e2e` for the local all-in-one operator path after copying this file to `tests/e2e_real_canisters/wasms.local.toml` and setting `POCKET_IC_BIN`. The script fetches pinned artifacts, verifies compressed source hashes, decompresses to installable Wasms, fills local uncompressed hashes, and runs the ignored real-framework tests. It does not use `--network ic` and must not be run against production fiduciary canisters.

### Remaining Real SNS-W Driver Work

The exact-economics E2E is a real-ledger test, and the direct governance staking smoke is a real-governance/direct-install test. The SNS-W-deployed swap now reaches participation, commit, finalization, finalized governance `list_neurons`, post-finalization user staking/top-up/multiple-neuron/minimum-stake/below-and-at-two-week dissolve-delay/start-dissolving/stop-dissolving checks, no-closed-proposals reward weighting, yes/no voting, finalized non-voter reward-policy weighting, finalized root dapp listing/control/upgrade/non-dapp exclusion, local IO canister installation against finalized SNS IDs, stream-manager finalized-governance active-stake refresh, real local ICP-indexed Jupiter deposit issuance from the finalized SNS ledger reserve, one exact 5 ICP two-week maturity backed reward-pool transfer through the finalized SNS ledger, real stream-manager exclusion of a finalized dissolving neuron from rewards, and one finalized-SNS IO redemption scan with local ICP payout plus SNS IO reserve return. The remaining implementation steps are:

- complete broader participation-weighted stream-manager reward-policy variants behind SNS-W-finalized governance. Following is now proved by a three-neuron finalized SNS path: a follower calls `manage_neuron(Follow { function_id: Motion, followees: [leader] })` and `manage_neuron(SetFollowing { topic_following: all known SNS topics })`, a separate proposer creates a motion proposal, the leader explicitly votes yes, and finalized governance lists a follower-visible yes ballot;
- extend stream-manager reward/current-rate redemption and historian real-stack checks against the finalized local SNS canister IDs, and add browser E2E on top of the existing frontend source-backed static honesty checks.

The direct SNS-W/governance/root smokes and PocketIC ICP-feature bootstrap are intentionally not treated as SNS-W lifecycle proof. SNS-W publication proof uses compressed source `.wasm.gz` payloads and source SHA-256 semantics, while PocketIC install paths use decompressed `.wasm` bytes. The direct governance staking smoke remains lower-layer coverage; finalized post-swap staking, yes/no voting, following, non-voter weighting, and root-controlled app upgrade coverage lives in `sns_lifecycle.rs` and still does not prove broad stream-manager reward-snapshot consumption.
