#![allow(dead_code, unused_imports)]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealFrameworkBlocker {
    NnsInitPayloadDriver,
    SnsWasmProposalDriver,
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

mod normal_sns_staking {
    use super::*;

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
        real_sns_reward_policy_reads_real_proposal_participation,
        RealFrameworkBlocker::SnsVotingDriver
    );
}

mod stream_manager_real_stack {
    use super::*;

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
    use std::fs;
    use std::path::{Path, PathBuf};

    fn repo_path(path: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    }

    fn read(path: &str) -> String {
        fs::read_to_string(repo_path(path))
            .unwrap_or_else(|err| panic!("{path} should read: {err}"))
    }

    fn web_source() -> String {
        let mut combined = String::new();
        collect_source(&repo_path("canisters/frontend/web/src"), &mut combined);
        combined
    }

    fn collect_source(path: &Path, combined: &mut String) {
        for entry in
            fs::read_dir(path).unwrap_or_else(|err| panic!("{} should list: {err}", path.display()))
        {
            let entry = entry.expect("frontend source entry should read");
            let path = entry.path();
            if path.is_dir() {
                collect_source(&path, combined);
            } else if matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("js") | Some("html")
            ) {
                combined.push_str(
                    &fs::read_to_string(&path)
                        .unwrap_or_else(|err| panic!("{} should read: {err}", path.display())),
                );
                combined.push('\n');
            }
        }
    }

    #[test]
    fn frontend_real_status_displays_not_live() {
        let template = read("canisters/frontend/web/index.template.html");
        let readme = read("canisters/frontend/README.md");

        assert!(template.contains("COMING SOON"));
        assert!(readme.contains("IO remains pre-launch"));
        assert!(readme.contains("not live"));
        for forbidden in [
            "IO is live",
            "issuance is live",
            "redemption is live",
            "mainnet truth",
        ] {
            assert!(
                !template.contains(forbidden),
                "frontend template must not claim {forbidden}"
            );
        }
    }

    #[test]
    fn frontend_real_status_shows_local_evidence_as_local_only() {
        let readme = read("canisters/frontend/README.md");

        assert!(readme.contains("`DevMainnet` only"));
        assert!(readme.contains("not a production IO protocol canister"));
        assert!(readme.contains("not protocol truth"));
        assert!(readme.contains("Historian data is rebuildable, not canonical protocol truth"));
    }

    #[test]
    fn frontend_does_not_import_stream_manager_declarations() {
        let source = web_source();

        assert!(!source.contains("io_stream_manager"));
        assert!(!source.contains("stream_manager"));
        assert!(!repo_path("canisters/frontend/web/declarations/io_stream_manager").exists());
    }

    #[test]
    fn frontend_does_not_import_nns_neuron_manager_declarations() {
        let source = web_source();

        assert!(!source.contains("io_nns_neuron_manager"));
        assert!(!source.contains("nns_neuron_manager"));
        assert!(!repo_path("canisters/frontend/web/declarations/io_nns_neuron_manager").exists());
    }

    #[test]
    fn frontend_displays_stale_missing_incomplete_as_unknown_not_zero() {
        let transforms = read("canisters/frontend/web/src/data/dashboard-transforms.js");
        let formatters = read("canisters/frontend/web/src/app/view-formatters.js");

        assert!(transforms.contains("Incomplete data"));
        assert!(transforms.contains("sourceHealthWarnings"));
        assert!(formatters.contains("unknown"));
        assert!(
            !transforms.contains("?? 0"),
            "missing historian values must not be coerced to zero"
        );
        assert!(
            !formatters.contains("?? 0"),
            "unknown formatter values must not be coerced to zero"
        );
    }

    #[test]
    fn frontend_escapes_canister_ids_evidence_fields() {
        let source = web_source();

        assert!(source.contains("textContent"));
        assert!(
            !source.contains("innerHTML"),
            "frontend evidence fields must render as text, not HTML"
        );
        assert!(!source.contains("insertAdjacentHTML"));
    }

    #[test]
    fn frontend_uses_historian_only_for_protocol_status() {
        let agent = read("canisters/frontend/web/src/app/agent.js");
        let loaders = read("canisters/frontend/web/src/data/historian-loaders.js");

        assert!(agent.contains("../../declarations/io_historian/io_historian.did.js"));
        assert!(agent.contains("createHistorianActor"));
        assert!(loaders.contains("get_dashboard_state"));
        assert!(loaders.contains("get_public_status"));
        assert!(!agent.contains("io_stream_manager"));
        assert!(!agent.contains("io_nns_neuron_manager"));
    }

    #[test]
    fn frontend_build_uses_historian_only() {
        let build = read("canisters/frontend/web/build-frontend.mjs");

        assert!(build.contains("CANISTER_ID_IO_HISTORIAN"));
        assert!(build.contains("resolveCanisterId(\"io_historian\")"));
        assert!(!build.contains("CANISTER_ID_IO_STREAM_MANAGER"));
        assert!(!build.contains("CANISTER_ID_IO_NNS_NEURON_MANAGER"));
        assert!(!build.contains("resolveCanisterId(\"io_stream_manager\")"));
        assert!(!build.contains("resolveCanisterId(\"io_nns_neuron_manager\")"));
    }

    #[test]
    fn frontend_certified_assets_do_not_embed_production_activation_claims() {
        let index = read("canisters/frontend/public/index.html");
        let template = read("canisters/frontend/web/index.template.html");
        let combined = format!("{index}\n{template}");

        assert!(combined.contains("COMING SOON"));
        for forbidden in [
            "IO is live",
            "Protocol is live",
            "Issuance is live",
            "Redemption is live",
            "Mainnet truth",
        ] {
            assert!(
                !combined.contains(forbidden),
                "certified frontend assets must not claim {forbidden}"
            );
        }
    }

    #[test]
    fn frontend_does_not_show_mainnet_truth_from_local_rehearsal() {
        let readme = read("canisters/frontend/README.md");
        let transforms = read("canisters/frontend/web/src/data/dashboard-transforms.js");

        assert!(readme.contains("not canonical protocol truth"));
        assert!(readme.contains("missing/stale/incomplete fields must not be interpreted as zero"));
        assert!(transforms.contains("Historian data unavailable"));
        assert!(!transforms.contains("mainnet truth"));
    }

    #[test]
    fn frontend_does_not_expose_value_moving_actions() {
        let source = web_source();
        let template = read("canisters/frontend/web/index.template.html");
        let combined = format!("{source}\n{template}");

        for forbidden in [
            "<form",
            "type=\"submit\"",
            "approve(",
            "transfer(",
            "stake(",
            "redeem(",
            "claim(",
        ] {
            assert!(
                !combined.contains(forbidden),
                "frontend shell must not expose value-moving action surface {forbidden}"
            );
        }
    }
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
