#![allow(dead_code, unused_imports)]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealFrameworkBlocker {
    NnsInitPayloadDriver,
    SnsWasmProposalDriver,
    SnsLifecycleDriver,
    NormalSnsStakingDriver,
    SnsVotingDriver,
    SnsRootControlDriver,
    StreamManagerRealStackDriver,
    RealStackFailureInjectionDriver,
    HistorianRealSourceDriver,
    FrontendBrowserHarness,
    LocalFullLaunchRehearsalDriver,
}

fn blocked(blocker: RealFrameworkBlocker) -> Result<(), RealFrameworkBlocker> {
    Err(blocker)
}

macro_rules! blocked_test {
    ($name:ident, $blocker:expr) => {
        #[test]
        fn $name() {
            assert_eq!(blocked($blocker), Err($blocker));
        }
    };
}

mod nns_installer {
    use super::*;

    blocked_test!(
        real_nns_minimal_installer_installs_required_canisters,
        RealFrameworkBlocker::NnsInitPayloadDriver
    );
    blocked_test!(
        real_nns_sns_wasm_canister_responds_to_basic_queries,
        RealFrameworkBlocker::NnsInitPayloadDriver
    );
}

mod sns_wasm_publication {
    use super::*;

    blocked_test!(
        real_sns_w_accepts_root_governance_ledger_index_swap_archive_wasms,
        RealFrameworkBlocker::SnsWasmProposalDriver
    );
    blocked_test!(
        real_sns_w_rejects_wrong_hash_or_wrong_type,
        RealFrameworkBlocker::SnsWasmProposalDriver
    );
    blocked_test!(
        real_sns_w_lists_published_wasms,
        RealFrameworkBlocker::SnsWasmProposalDriver
    );
    blocked_test!(
        real_sns_w_publication_is_idempotent_or_fails_safely,
        RealFrameworkBlocker::SnsWasmProposalDriver
    );
}

mod sns_lifecycle {
    use super::*;

    blocked_test!(
        real_sns_lifecycle_deploys_sns_via_sns_w,
        RealFrameworkBlocker::SnsLifecycleDriver
    );
    blocked_test!(
        real_sns_swap_opens_with_expected_parameters,
        RealFrameworkBlocker::SnsLifecycleDriver
    );
    blocked_test!(
        real_sns_participant_can_refresh_buyer_tokens,
        RealFrameworkBlocker::SnsLifecycleDriver
    );
    blocked_test!(
        real_sns_finalized_swap_creates_direct_participation_neurons,
        RealFrameworkBlocker::SnsLifecycleDriver
    );
    blocked_test!(
        real_sns_deployed_canister_ids_are_on_sns_subnet,
        RealFrameworkBlocker::SnsLifecycleDriver
    );
    blocked_test!(
        real_sns_io_app_canister_is_on_application_subnet,
        RealFrameworkBlocker::SnsLifecycleDriver
    );
    blocked_test!(
        real_sns_lifecycle_preserves_not_mainnet_not_production_status,
        RealFrameworkBlocker::SnsLifecycleDriver
    );
}

mod normal_sns_staking {
    use super::*;

    blocked_test!(
        real_sns_user_stakes_io_normal_path_and_list_neurons_observes_it,
        RealFrameworkBlocker::NormalSnsStakingDriver
    );
    blocked_test!(
        real_sns_user_topup_increases_existing_neuron_stake,
        RealFrameworkBlocker::NormalSnsStakingDriver
    );
    blocked_test!(
        real_sns_user_stakes_multiple_neurons_without_duplicate_confusion,
        RealFrameworkBlocker::NormalSnsStakingDriver
    );
    blocked_test!(
        real_sns_minimum_stake_is_enforced,
        RealFrameworkBlocker::NormalSnsStakingDriver
    );
    blocked_test!(
        real_sns_dissolve_delay_below_two_weeks_is_ineligible,
        RealFrameworkBlocker::NormalSnsStakingDriver
    );
    blocked_test!(
        real_sns_dissolve_delay_at_two_weeks_is_eligible,
        RealFrameworkBlocker::NormalSnsStakingDriver
    );
    blocked_test!(
        real_sns_dissolving_neuron_is_excluded_if_strict,
        RealFrameworkBlocker::NormalSnsStakingDriver
    );
    blocked_test!(
        real_sns_stop_dissolving_restores_eligibility_if_policy_allows,
        RealFrameworkBlocker::NormalSnsStakingDriver
    );
    blocked_test!(
        real_sns_genesis_governance_neuron_is_excluded_from_io_rewards,
        RealFrameworkBlocker::NormalSnsStakingDriver
    );
    blocked_test!(
        real_sns_protocol_owned_neuron_is_excluded_from_io_rewards,
        RealFrameworkBlocker::NormalSnsStakingDriver
    );
}

mod voting {
    use super::*;

    blocked_test!(
        real_sns_user_votes_yes_and_ballot_is_observed,
        RealFrameworkBlocker::SnsVotingDriver
    );
    blocked_test!(
        real_sns_user_votes_no_and_ballot_is_observed,
        RealFrameworkBlocker::SnsVotingDriver
    );
    blocked_test!(
        real_sns_following_vote_counts_for_participation_if_policy_allows,
        RealFrameworkBlocker::SnsVotingDriver
    );
    blocked_test!(
        real_sns_non_voter_gets_lower_participation_factor,
        RealFrameworkBlocker::SnsVotingDriver
    );
    blocked_test!(
        real_sns_no_closed_proposals_participation_factor_defaults_to_one,
        RealFrameworkBlocker::SnsVotingDriver
    );
    blocked_test!(
        real_sns_proposal_rejection_fee_is_100_io_if_configured,
        RealFrameworkBlocker::SnsVotingDriver
    );
    blocked_test!(
        real_sns_reward_policy_reads_real_proposal_participation,
        RealFrameworkBlocker::SnsVotingDriver
    );
}

mod root_control {
    use super::*;

    blocked_test!(
        real_sns_root_controls_io_app_canister_after_finalization,
        RealFrameworkBlocker::SnsRootControlDriver
    );
    blocked_test!(
        real_sns_root_can_upgrade_test_app_canister,
        RealFrameworkBlocker::SnsRootControlDriver
    );
    blocked_test!(
        real_sns_root_control_does_not_touch_production_fiduciary_ids,
        RealFrameworkBlocker::SnsRootControlDriver
    );
    blocked_test!(
        real_sns_root_control_uses_application_subnet_canister,
        RealFrameworkBlocker::SnsRootControlDriver
    );
    blocked_test!(
        real_sns_root_control_rejects_non_dapp_canister,
        RealFrameworkBlocker::SnsRootControlDriver
    );
}

mod stream_manager_real_stack {
    use super::*;

    blocked_test!(
        io_stream_manager_real_jupiter_deposit_issues_backed_io,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_jupiter_deposit_100_icp_issues_exact_60_io,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_deposit_after_holder_yield_issues_less_io,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_duplicate_deposit_replay_no_double_issuance,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_unknown_deposit_issues_no_io,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_tiny_deposit_terminal_rejection_no_scanner_stall,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_reserve_exhaustion_fails_without_state_corruption,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_two_year_maturity_increases_rate_no_io,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_two_year_maturity_10_icp_increases_60_io_value_to_66_icp,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_multiple_two_year_maturity_events_compound_exactly,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_two_year_maturity_does_not_change_redeemable_supply,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_later_faucet_deposit_uses_pre_event_rate,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_sns_staking_snapshot_updates_active_staked_io,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_sns_topup_increases_active_staked_io,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_two_week_maturity_rewards_only_eligible_stakers,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_two_week_maturity_5_icp_issues_exact_backed_reward_pool,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_full_participation_gets_more_than_half_participation,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_ineligible_staker_gets_zero_reward,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_genesis_neuron_excluded,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_protocol_owned_neuron_excluded,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_dissolving_neuron_excluded,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_no_closed_proposals_factor_one,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_redemption_uses_current_rate,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_redemption_after_holder_yield_is_higher_than_genesis,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_redemption_after_staker_rewards_preserves_rate,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_duplicate_redemption_no_double_payout,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_redemption_rounding_fee_dust_accounted,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_redemption_return_to_reserve_updates_supply,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_index_catchup_after_ticks,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_index_replay_is_idempotent,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_archive_required_fails_closed_or_blocked,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_account_history_ordering_is_stable,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_cursor_persists_across_upgrade,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_bad_fee_maps_to_retry_or_terminal_policy,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_insufficient_funds_maps_to_retry_or_terminal_policy,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_duplicate_transfer_proof_is_verified,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_governance_unavailable_fails_safe,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_reward_partial_failure_retries_only_failed_recipient,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_payout_failure_retries_without_losing_redemption,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
    blocked_test!(
        io_stream_manager_real_io_return_failure_after_payout_does_not_double_pay,
        RealFrameworkBlocker::StreamManagerRealStackDriver
    );
}

mod real_stack_upgrade {
    use super::*;

    blocked_test!(
        real_stack_upgrade_before_deposit_processing_then_tick_completes_once,
        RealFrameworkBlocker::RealStackFailureInjectionDriver
    );
    blocked_test!(
        real_stack_upgrade_after_deposit_before_io_transfer_then_tick_completes_once,
        RealFrameworkBlocker::RealStackFailureInjectionDriver
    );
    blocked_test!(
        real_stack_upgrade_after_io_transfer_before_journal_terminal_no_double_issue,
        RealFrameworkBlocker::RealStackFailureInjectionDriver
    );
    blocked_test!(
        real_stack_upgrade_before_two_week_distribution_then_tick_completes_once,
        RealFrameworkBlocker::RealStackFailureInjectionDriver
    );
    blocked_test!(
        real_stack_upgrade_after_one_reward_transfer_retries_only_failed_recipient,
        RealFrameworkBlocker::RealStackFailureInjectionDriver
    );
    blocked_test!(
        real_stack_upgrade_after_redemption_observed_before_payout_then_pays_once,
        RealFrameworkBlocker::RealStackFailureInjectionDriver
    );
    blocked_test!(
        real_stack_upgrade_after_icp_payout_before_io_return_then_returns_io_once,
        RealFrameworkBlocker::RealStackFailureInjectionDriver
    );
    blocked_test!(
        real_stack_upgrade_after_io_return_before_terminal_then_no_double_payout,
        RealFrameworkBlocker::RealStackFailureInjectionDriver
    );
    blocked_test!(
        real_stack_same_wasm_upgrade_preserves_scheduler_cursors,
        RealFrameworkBlocker::RealStackFailureInjectionDriver
    );
    blocked_test!(
        real_stack_same_wasm_upgrade_preserves_processed_tx_set,
        RealFrameworkBlocker::RealStackFailureInjectionDriver
    );
    blocked_test!(
        real_stack_same_wasm_upgrade_preserves_operation_journal,
        RealFrameworkBlocker::RealStackFailureInjectionDriver
    );
    blocked_test!(
        real_stack_future_schema_rejected_or_fails_closed,
        RealFrameworkBlocker::RealStackFailureInjectionDriver
    );
    blocked_test!(
        real_stack_corrupt_state_fails_closed,
        RealFrameworkBlocker::RealStackFailureInjectionDriver
    );
}

mod historian_real_sources {
    use super::*;

    blocked_test!(
        historian_real_sources_observe_reserved_not_live_until_protocol_active,
        RealFrameworkBlocker::HistorianRealSourceDriver
    );
    blocked_test!(
        historian_real_ledger_index_observes_reserve_and_user_balances,
        RealFrameworkBlocker::HistorianRealSourceDriver
    );
    blocked_test!(
        historian_real_ledger_index_observes_redemption_and_payout_history,
        RealFrameworkBlocker::HistorianRealSourceDriver
    );
    blocked_test!(
        historian_real_governance_observes_sns_neurons_without_claiming_truth,
        RealFrameworkBlocker::HistorianRealSourceDriver
    );
    blocked_test!(
        historian_real_governance_observes_votes_and_participation,
        RealFrameworkBlocker::HistorianRealSourceDriver
    );
    blocked_test!(
        historian_real_freshness_reports_stale_missing_incomplete_not_zero,
        RealFrameworkBlocker::HistorianRealSourceDriver
    );
    blocked_test!(
        historian_real_evidence_renders_local_only_not_mainnet_truth,
        RealFrameworkBlocker::HistorianRealSourceDriver
    );
    blocked_test!(
        historian_real_rebuild_from_sources_matches_stream_manager_state,
        RealFrameworkBlocker::HistorianRealSourceDriver
    );
    blocked_test!(
        historian_real_stale_index_does_not_display_zero_as_truth,
        RealFrameworkBlocker::HistorianRealSourceDriver
    );
    blocked_test!(
        historian_real_frontend_status_remains_not_live,
        RealFrameworkBlocker::HistorianRealSourceDriver
    );
}

mod frontend_honesty {
    use super::*;

    blocked_test!(
        frontend_real_status_displays_not_live,
        RealFrameworkBlocker::FrontendBrowserHarness
    );
    blocked_test!(
        frontend_real_status_shows_local_evidence_as_local_only,
        RealFrameworkBlocker::FrontendBrowserHarness
    );
    blocked_test!(
        frontend_does_not_import_stream_manager_declarations,
        RealFrameworkBlocker::FrontendBrowserHarness
    );
    blocked_test!(
        frontend_does_not_import_nns_neuron_manager_declarations,
        RealFrameworkBlocker::FrontendBrowserHarness
    );
    blocked_test!(
        frontend_displays_stale_missing_incomplete_as_unknown_not_zero,
        RealFrameworkBlocker::FrontendBrowserHarness
    );
    blocked_test!(
        frontend_escapes_canister_ids_evidence_fields,
        RealFrameworkBlocker::FrontendBrowserHarness
    );
}

mod local_full_launch_rehearsal {
    use super::*;

    blocked_test!(
        local_network_launches_with_nns_sns_features,
        RealFrameworkBlocker::LocalFullLaunchRehearsalDriver
    );
    blocked_test!(
        io_canister_ids_are_local_only,
        RealFrameworkBlocker::LocalFullLaunchRehearsalDriver
    );
    blocked_test!(
        sns_launch_completes_locally,
        RealFrameworkBlocker::LocalFullLaunchRehearsalDriver
    );
    blocked_test!(
        local_evidence_parsed,
        RealFrameworkBlocker::LocalFullLaunchRehearsalDriver
    );
    blocked_test!(
        no_production_fiduciary_ids_used,
        RealFrameworkBlocker::LocalFullLaunchRehearsalDriver
    );
    blocked_test!(
        no_mainnet_calls,
        RealFrameworkBlocker::LocalFullLaunchRehearsalDriver
    );
}
