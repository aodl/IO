# Current Test Inventory

This inventory records proof strength for tests relevant to real SNS and full protocol coverage. It is not a full list of every unit test.

| Test or command | File/crate | Layer | Proves | Does not prove |
| --- | --- | --- | --- | --- |
| `icrc_error_mapping_preserves_duplicate_and_bad_fee`, `icrc_error_mapping_preserves_insufficient_funds_and_generic_error`, `duplicate_transfer_proof_checks_operation_and_ledger_kind_when_available` | `crates/io_ledger_types/src/lib.rs` | Unit | Ledger boundary error and duplicate-proof mapping | Real SNS ledger behavior |
| `account_history_scan_*`, `icrc_index_result_*`, `archive_traversal_requires_complete_contiguous_ranges` | `crates/io_ledger_types/src/lib.rs` | Unit | Index ordering, lag/archive flags, cursor safety | Real SNS index account history |
| `local_sns_*` ledger model tests | `crates/io_ledger_types/src/lib.rs` | Unit | Reserve-transfer issuance/redemption model, constant supply assumptions | Real SNS-created ledger |
| `sns_candid_records_convert_to_domain`, `sns_production_list_neurons_fixture_maps_page_and_malformed_id`, `sns_production_list_proposals_fixture_maps_participation_inputs`, `sns_eligibility_excludes_expected_neurons`, `sns_participation_*` | `crates/io_governance_types/src/lib.rs` | Unit | Production-shaped SNS governance DTO mapping, eligibility, participation | Normal SNS staking, real governance canister rewards |
| `eligible_sns_staking_increases_io_reward_entitlement`, `increasing_staked_io_increases_reward_weight_without_double_counting`, `ineligible_sns_staking_does_not_increase_reward_entitlement` | `crates/io_reward_policy/src/lib.rs` | Unit | IO reward/APY policy response to eligible/ineligible SNS-shaped neuron state and stake increase | Real SNS staking/top-up |
| `governance_source_drives_equal_two_week_allocation`, `participation_and_stake_time_weighting_flow_into_allocation`, `governance_exclusions_and_invalid_ids_are_reported` | `canisters/io_stream_manager/src/governance_snapshot.rs` | Unit | Snapshotting paged SNS-shaped governance data into reward snapshots | Real SNS governance canister |
| `pocketic_live_sns_governance_reads_drive_two_week_allocation` | `tests/pocketic/io_sns_governance_read_pocketic.rs` | SNS-shaped PocketIC | IO reads mock SNS-shaped governance pages in PocketIC and allocates rewards | Real SNS governance, staking, maturity |
| `pocketic_live_two_week_maturity_allocates_io_from_mock_sns_snapshot`, `pocketic_live_nns_manager_maturity_feeds_stream_manager_rewards` | `tests/pocketic/io_stream_manager_pocketic.rs` | SNS-shaped PocketIC | Mock SNS snapshot can drive two-week IO rewards through IO canister stack | Real SNS governance/ledger |
| `pocketic_live_jupiter_faucet_stream_moves_mock_ledger_balances_once`, `pocketic_live_tiny_authorized_icp_deposit_does_not_block_later_valid_deposit`, `pocketic_live_index_lag_blocks_scan_then_resolves_once`, `pocketic_live_archive_required_blocks_redemption_scan_without_mutation` | `tests/pocketic/io_stream_manager_pocketic.rs` | Mock/PocketIC | Deposit scanning, dust rejection, index lag/archive behavior with mock ledgers/indexes | Real ICP/SNS ledger/index |
| `pocketic_live_redemption_pays_icp_and_returns_io_to_reserve_once`, `pocketic_live_redemption_*failure*` | `tests/pocketic/io_stream_manager_pocketic.rs` | Mock/PocketIC | Redemption payout/return retry safety with mock ledgers | Real ledger duplicate/idempotency behavior |
| `pocketic_live_sns_topology_installs_io_canisters_with_local_principals` | `tests/pocketic/io_sns_topology_pocketic.rs` | SNS-shaped PocketIC | IO canisters can be placed in a PocketIC SNS/application topology with local principals | Real SNS root/governance/ledger |
| `pocketic_live_sns_root_*` lifecycle tests | `tests/pocketic/io_sns_root_lifecycle_pocketic.rs` | SNS-shaped PocketIC | Mock SNS root/governance upgrade intent and outcome tracking | Real SNS root proposal execution |
| `e2e_jupiter_to_staking_to_maturity_to_redemption` and related `io_e2e` tests | `tests/e2e/io_e2e.rs` | Mock/model | Full model lifecycle through issuance, staking target, maturity, rewards, unwind, redemption | Any real canister behavior |
| `cargo run -p xtask -- validate_local_sns_rehearsal` | `tools/xtask` | Static/local-manual guard | Local SNS rehearsal package shape and safety | Completed real local SNS run |
| `cargo run -p xtask -- validate_local_sns_ledger` | `tools/xtask` | Official local SNS rehearsal evidence, if file exists | Parses completed local evidence for real local SNS ledger/index/governance/root observations | Does not call canisters; skips if evidence absent |
| `cargo run -p xtask -- local_sns_evidence_tests` | `tools/xtask` | Official local SNS evidence parser | Env-gated parse/policy gate for completed local evidence | No canister calls by default |
| `cargo test -p e2e-real-canisters` | `tests/e2e_real_canisters` | Unit/static harness | Manifest parsing, missing artifact skip semantics, required-artifact failure, SHA-256 mismatch failure | Does not install real canisters without ignored env-gated tests |
| `real_sns_ledger_index_smoke` | `tests/e2e_real_canisters/src/lib.rs` | Real-framework PocketIC, ignored/opt-in | With pinned local SNS ledger/index Wasms: installs real ledger/index, queries ICRC metadata/balances/fee/total supply, transfers reserve-to-user, checks BadFee/InsufficientFunds/Duplicate, verifies index account history and constant supply | Not run in default CI; no governance/root/staking |
| `real_sns_ledger_index_same_wasm_upgrade_preserves_balances_history_and_duplicates` | `tests/e2e_real_canisters/src/lib.rs` | Real-framework PocketIC, ignored/opt-in | With pinned local SNS ledger/index Wasms: same-Wasm ledger/index upgrades preserve balances, history, and duplicate proof behavior | Not run in default CI; archive behavior not induced |
| `real_sns_governance_staking_smoke` | `tests/e2e_real_canisters/src/lib.rs` | Real-framework PocketIC blocker | Registers the required normal SNS staking proof target and skips with an explicit blocker | No real governance behavior yet |
| `real_canister_e2e_icp_to_io_stake_reward_redemption` | `tests/e2e_real_canisters/src/lib.rs` | Real-framework PocketIC blocker | Registers the full ICP/SNS/NNS E2E proof target and skips with an explicit blocker | No all-real E2E behavior yet |

Explicit current gaps:

- real SNS neuron staking is not proved.
- real SNS governance maturity/rewards are not proved.
- real SNS index account history is proved only by opt-in `e2e-real-canisters` ignored tests when pinned SNS ledger/index Wasms are supplied; it is not proved in default CI.
- IO APY increase from real SNS staked IO is not proved.
- full ICP -> IO -> stake -> APY -> redemption E2E with real SNS ledger/index/governance/root is not proved.
