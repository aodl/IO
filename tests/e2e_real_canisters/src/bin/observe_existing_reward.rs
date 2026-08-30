use candid::{decode_one, encode_args, encode_one, Principal};
use e2e_real_canisters::sns_governance_setup::{
    Action, Command, CommandResponse, ListNeurons, ListNeuronsResponse, ManageNeuron,
    ManageNeuronResponse, Motion, Proposal,
};
use io_governance_types::SnsRewardEvent;
use io_stream_manager::{
    ApiError, RewardEventClassification, RewardEventObservation, Status, StreamProgress,
};
use pocket_ic::PocketIc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const REWARD_OBSERVATION_MARGIN_SECONDS: u64 = 300;
const REWARD_SCHEDULER_WAKE_EPSILON_SECONDS: u64 = 1;

fn reward_margin_wait_seconds(now_seconds: u64, event_end_seconds: u64) -> u64 {
    event_end_seconds
        .checked_add(REWARD_OBSERVATION_MARGIN_SECONDS)
        .expect("canonical reward observation deadline overflow")
        .saturating_sub(now_seconds)
}

fn reward_scheduler_advance_seconds(now_seconds: u64, event_end_seconds: u64) -> u64 {
    let wait = reward_margin_wait_seconds(now_seconds, event_end_seconds);
    if wait > 0
        || now_seconds
            == event_end_seconds
                .checked_add(REWARD_OBSERVATION_MARGIN_SECONDS)
                .expect("canonical reward observation deadline overflow")
    {
        wait.checked_add(REWARD_SCHEDULER_WAKE_EPSILON_SECONDS)
            .expect("canonical reward scheduler advance overflow")
    } else {
        0
    }
}

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
}

fn principal(name: &str) -> Principal {
    Principal::from_text(required(name)).unwrap_or_else(|error| panic!("invalid {name}: {error}"))
}

fn stream_status(pic: &PocketIc, stream: Principal) -> Status {
    decode_one(
        &pic.query_call(
            stream,
            Principal::anonymous(),
            "get_status",
            encode_args(()).expect("encode get_status request"),
        )
        .expect("query stream status"),
    )
    .expect("decode stream status")
}

fn restore_stream_readiness(pic: &PocketIc, stream: Principal, governance: Principal) {
    let paused: Result<(), ApiError> = decode_one(
        &pic.update_call(
            stream,
            governance,
            "set_paused",
            encode_one(true).expect("encode controlled reward recovery pause"),
        )
        .expect("pause stream for controlled reward recovery"),
    )
    .expect("decode controlled reward recovery pause");
    println!("controlled_reward_recovery_pause={paused:#?}");
    assert_eq!(paused, Ok(()));

    let ready: Result<(), ApiError> = decode_one(
        &pic.update_call(
            stream,
            governance,
            "set_paused",
            encode_one(false).expect("encode controlled reward recovery readiness"),
        )
        .expect("restore stream readiness for controlled reward recovery"),
    )
    .expect("decode controlled reward recovery readiness");
    println!("controlled_reward_recovery_ready={ready:#?}");
    assert_eq!(ready, Ok(()));
}

fn resume_reward(pic: &PocketIc, stream: Principal) -> Result<RewardEventObservation, ApiError> {
    decode_one(
        &pic.update_call(
            stream,
            Principal::anonymous(),
            "resume_reward_work",
            encode_args(()).expect("encode resume_reward_work request"),
        )
        .expect("resume reward work"),
    )
    .expect("decode reward observation")
}

fn drive_reconciliation(pic: &PocketIc, stream: Principal) {
    for attempt in 0..32 {
        let status = stream_status(pic, stream);
        if status.operation_kind.is_none() {
            println!("canonical_reconciliation_idle_after_attempt={attempt}");
            return;
        }
        assert_eq!(
            status.operation_kind.as_deref(),
            Some("BackingReconciliation"),
            "canonical reward warm-up left an unrelated operation active: {status:#?}"
        );
        let result: Result<StreamProgress, ApiError> = decode_one(
            &pic.update_call(
                stream,
                Principal::anonymous(),
                "resume",
                encode_args(()).expect("encode reconciliation resume"),
            )
            .expect("resume canonical reconciliation"),
        )
        .expect("decode reconciliation resume");
        println!("canonical_reconciliation_resume[{attempt}]={result:#?}");
        match result {
            Ok(StreamProgress::BackingReconciliation) | Err(ApiError::Pending(_)) => {}
            Err(ApiError::Invalid(reason)) if reason == "NNS resume rejected" => {
                println!("canonical_reconciliation_retryable_nns_rejection={reason}");
            }
            other => panic!("canonical reconciliation returned an unexpected result: {other:#?}"),
        }
        for _ in 0..10 {
            pic.tick();
        }
    }
    panic!("canonical reconciliation did not become idle within 32 attempts");
}

fn submit_canonical_motion(pic: &PocketIc, governance: Principal) -> u64 {
    let proposer = principal("IO_LOCAL_REWARD_PROPOSER_PRINCIPAL");
    let subaccount = hex::decode(required("IO_LOCAL_REWARD_NEURON_SUBACCOUNT_HEX"))
        .expect("invalid IO_LOCAL_REWARD_NEURON_SUBACCOUNT_HEX");
    assert_eq!(
        subaccount.len(),
        32,
        "SNS proposer subaccount must be 32 bytes"
    );
    let response: ManageNeuronResponse = decode_one(
        &pic.update_call(
            governance,
            proposer,
            "manage_neuron",
            encode_one(ManageNeuron {
                subaccount,
                command: Some(Command::MakeProposal(Proposal {
                    url: "https://forum.dfinity.org/t/io-local-rehearsal/0".into(),
                    title: "Prospective IO reward eligibility observation".into(),
                    summary: "Local-only proposal after exact Dynamic-parent reconciliation."
                        .into(),
                    action: Some(Action::Motion(Motion {
                        motion_text: "Observe one proposal-bearing eligible IO reward event."
                            .into(),
                    })),
                })),
            })
            .expect("encode canonical reward proposal"),
        )
        .expect("submit canonical reward proposal through real SNS Governance"),
    )
    .expect("decode canonical reward proposal response");
    match response.command {
        Some(CommandResponse::MakeProposal(response)) => {
            response
                .proposal_id
                .expect("canonical reward proposal response lacks proposal ID")
                .id
        }
        other => panic!("canonical reward proposal failed: {other:#?}"),
    }
}

fn print_reward_state(pic: &PocketIc, governance: Principal, label: &str) -> SnsRewardEvent {
    let caller = Principal::anonymous();
    let event: SnsRewardEvent = decode_one(
        &pic.query_call(
            governance,
            caller,
            "get_latest_reward_event",
            encode_one(()).expect("encode reward event request"),
        )
        .expect("query latest reward event"),
    )
    .expect("decode latest reward event");
    println!("{label}_latest_reward_event={event:#?}");

    let neurons: ListNeuronsResponse = decode_one(
        &pic.query_call(
            governance,
            caller,
            "list_neurons",
            encode_one(ListNeurons {
                of_principal: None,
                limit: 1_000,
                start_page_at: None,
            })
            .expect("encode list_neurons request"),
        )
        .expect("query neurons"),
    )
    .expect("decode neurons");
    for neuron in neurons.neurons {
        println!(
            "{label}_neuron={} stake_e8s={} maturity_e8s={} dissolve_state={:?} participation={:?}",
            neuron
                .id
                .map(|id| hex::encode(id.id))
                .unwrap_or_else(|| "missing".into()),
            neuron.cached_neuron_stake_e8s,
            neuron.maturity_e8s_equivalent,
            neuron.dissolve_state,
            neuron.latest_reward_event_participation,
        );
    }
    event
}

fn advance_to_reward_observation_margin(pic: &PocketIc, stream: Principal, event: &SnsRewardEvent) {
    let event_end = event
        .end_timestamp_seconds
        .expect("canonical reward event lacks an end timestamp");
    let deadline = event_end
        .checked_add(REWARD_OBSERVATION_MARGIN_SECONDS)
        .expect("canonical reward observation deadline overflow");
    let now = pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000;
    let wait_seconds = reward_margin_wait_seconds(now, event_end);
    let advance_seconds = reward_scheduler_advance_seconds(now, event_end);

    if wait_seconds > 0 {
        let processed_before = stream_status(pic, stream).processed_reward_event_count;
        let mut pending_proved = false;
        for attempt in 0..4 {
            let early = resume_reward(pic, stream);
            println!("pre_margin_resume_reward_work={early:#?}");
            match early {
                Err(ApiError::Pending(_)) => {
                    println!("pre_margin_pending_proved_after_attempt={attempt}");
                    pending_proved = true;
                    break;
                }
                Ok(RewardEventObservation {
                    classification: RewardEventClassification::StructuralOnly,
                    proposal_count: 0,
                    policy_credit: 0,
                    eligible_credit_total: 0,
                    ..
                }) => {
                    let status = stream_status(pic, stream);
                    assert_eq!(
                        status.processed_reward_event_count, processed_before,
                        "pre-margin structural work must not consume a reward event"
                    );
                    println!("pre_margin_structural_only_attempt={attempt}");
                }
                other => panic!(
                    "reward processing must remain zero-credit structural or Pending before the canonical event margin: {other:#?}"
                ),
            }
        }
        assert!(
            pending_proved,
            "reward processing did not prove Pending before the canonical event margin"
        );
    }
    if advance_seconds > 0 {
        pic.advance_time(Duration::from_secs(advance_seconds));
        for _ in 0..20 {
            pic.tick();
        }
    }

    println!("warmup_reward_margin_wait_seconds={wait_seconds}");
    println!("warmup_reward_scheduler_epsilon_seconds={REWARD_SCHEDULER_WAKE_EPSILON_SECONDS}");
    println!("warmup_reward_observation_deadline_seconds={deadline}");
}

fn main() {
    let pic = PocketIc::new_from_existing_instance(
        required("IO_POCKET_IC_SERVER_URL")
            .parse()
            .expect("invalid IO_POCKET_IC_SERVER_URL"),
        required("IO_POCKET_IC_INSTANCE_ID")
            .parse()
            .expect("invalid IO_POCKET_IC_INSTANCE_ID"),
        Some(300_000),
    );
    if std::env::var_os("IO_LOCAL_ASSERT_FRESH_HOST_TIME_ONLY").is_some() {
        let pocket_nanos = pic.get_time().as_nanos_since_unix_epoch();
        let host_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("host time predates the Unix epoch")
            .as_nanos() as u64;
        let drift = pocket_nanos.abs_diff(host_nanos);
        assert!(
            drift <= 300_000_000_000,
            "fresh lifecycle topology time differs from host time by more than 300 seconds: pocket={pocket_nanos} host={host_nanos}"
        );
        println!("fresh_topology_time_drift_nanos={drift}");
        return;
    }
    let governance = principal("IO_LOCAL_SNS_GOVERNANCE_ID");
    let stream = principal("IO_LOCAL_STREAM_MANAGER_ID");
    let caller = Principal::anonymous();
    let canonical_two_event = std::env::var_os("IO_LOCAL_REWARD_CANONICAL_TWO_EVENT").is_some();
    let initial_status = stream_status(&pic, stream);

    if let Ok(seconds) = std::env::var("IO_LOCAL_REWARD_ADVANCE_SECONDS") {
        let seconds = seconds
            .parse::<u64>()
            .expect("invalid IO_LOCAL_REWARD_ADVANCE_SECONDS");
        if !canonical_two_event || initial_status.processed_reward_event_count == 0 {
            pic.advance_time(Duration::from_secs(seconds));
            for _ in 0..20 {
                pic.tick();
            }
            println!("advanced_pocketic_seconds={seconds}");
        } else {
            println!("canonical_reward_warmup_reused=true");
        }
    }

    let warmup_event = print_reward_state(&pic, governance, "warmup");

    let before = stream_status(&pic, stream);
    println!("stream_status_before={before:#?}");

    if std::env::var_os("IO_LOCAL_REWARD_RESUME").is_some() {
        if before.reward_processing_paused {
            restore_stream_readiness(&pic, stream, governance);
        }
        if !canonical_two_event || before.processed_reward_event_count == 0 {
            // A structural observation is allowed to start ordinary backing
            // reconciliation independently of reward credit. Resolve that exact
            // generation first so the pre-margin call proves the reward deadline,
            // rather than merely observing an unrelated Busy operation.
            drive_reconciliation(&pic, stream);
            let margin_ready = stream_status(&pic, stream);
            assert_eq!(
                margin_ready.operation_kind, None,
                "reward-margin proof requires canonical structural reconciliation to be idle"
            );
            println!("stream_status_before_reward_margin={margin_ready:#?}");
            advance_to_reward_observation_margin(&pic, stream, &warmup_event);
            let post_margin = stream_status(&pic, stream);
            println!("stream_status_after_warmup_margin={post_margin:#?}");
            if canonical_two_event
                && post_margin.processed_reward_event_count == 1
                && post_margin.latest_reward_event_classification
                    == Some(RewardEventClassification::ZeroEligibleParticipation)
            {
                println!("warmup_reward_processed_by_timer=true");
            } else {
                let result = resume_reward(&pic, stream);
                println!("resume_reward_work={result:#?}");
                if canonical_two_event {
                    assert!(matches!(
                        result,
                        Ok(RewardEventObservation {
                            classification: RewardEventClassification::ZeroEligibleParticipation,
                            ..
                        })
                    ));
                }
            }
            if canonical_two_event {
                let committed = stream_status(&pic, stream);
                assert_eq!(committed.processed_reward_event_count, 1);
                assert!(matches!(
                    committed.latest_reward_event_classification,
                    Some(RewardEventClassification::ZeroEligibleParticipation)
                ));
            }
        }

        if canonical_two_event {
            drive_reconciliation(&pic, stream);
            restore_stream_readiness(&pic, stream, governance);
            let structural = resume_reward(&pic, stream);
            println!("canonical_structural_refresh={structural:#?}");
            assert!(matches!(
                structural,
                Ok(RewardEventObservation {
                    classification: RewardEventClassification::StructuralOnly,
                    ..
                })
            ));
            let proposal_id = submit_canonical_motion(&pic, governance);
            println!("canonical_reward_proposal_id={proposal_id}");
            let seconds = required("IO_LOCAL_REWARD_ADVANCE_SECONDS")
                .parse::<u64>()
                .expect("invalid IO_LOCAL_REWARD_ADVANCE_SECONDS");
            let canonical_advance = seconds
                .checked_add(REWARD_OBSERVATION_MARGIN_SECONDS)
                .expect("canonical reward observation advance overflow");
            pic.advance_time(Duration::from_secs(canonical_advance));
            for _ in 0..20 {
                pic.tick();
            }
            println!("canonical_reward_advanced_pocketic_seconds={seconds}");
            println!(
                "canonical_reward_observation_margin_seconds={REWARD_OBSERVATION_MARGIN_SECONDS}"
            );
            print_reward_state(&pic, governance, "canonical");
            let final_before = stream_status(&pic, stream);
            println!("canonical_stream_status_before={final_before:#?}");
            if final_before.reward_processing_paused {
                restore_stream_readiness(&pic, stream, governance);
            }
            if final_before.processed_reward_event_count >= 2
                && final_before.latest_reward_event_classification
                    == Some(RewardEventClassification::ProposalBearing)
            {
                assert!(final_before.accumulated_eligible_credit > 0);
                println!("canonical_reward_already_processed=true");
            } else {
                let result =
                    resume_reward(&pic, stream).expect("canonical reward observation failed");
                println!("canonical_resume_reward_work={result:#?}");
                assert_eq!(
                    result.classification,
                    RewardEventClassification::ProposalBearing
                );
                assert!(result.eligible_credit_total > 0);
            }
        }
    }

    let after = stream_status(&pic, stream);
    println!("stream_status_after={after:#?}");

    if let Ok(historian) = std::env::var("IO_LOCAL_HISTORIAN_ID") {
        let historian = Principal::from_text(historian).expect("invalid IO_LOCAL_HISTORIAN_ID");
        let settle_seconds = std::env::var("IO_LOCAL_HISTORIAN_SETTLE_SECONDS")
            .unwrap_or_else(|_| "60".into())
            .parse::<u64>()
            .expect("invalid IO_LOCAL_HISTORIAN_SETTLE_SECONDS");
        pic.advance_time(Duration::from_secs(settle_seconds));
        for _ in 0..200 {
            pic.tick();
        }
        let dashboard: io_historian::Dashboard = decode_one(
            &pic.query_call(
                historian,
                caller,
                "get_dashboard_state",
                encode_one(()).expect("encode historian dashboard request"),
            )
            .expect("query historian dashboard"),
        )
        .expect("decode historian dashboard");
        assert!(dashboard.status.configured, "historian must be configured");
        assert!(
            dashboard
                .source_health
                .iter()
                .all(|health| { health.freshness == io_historian::ObservationFreshness::Fresh }),
            "historian sources are not all fresh: {:#?}",
            dashboard.source_health
        );
        assert!(
            dashboard.canisters.iter().all(|canister| {
                canister.module_match == io_historian::ModuleMatch::Matching
                    && canister.controllers.is_some()
            }),
            "historian module/controller observations are incomplete: {:#?}",
            dashboard.canisters
        );
        assert_eq!(
            dashboard.stream.as_ref().map(|status| status.lifecycle),
            Some(io_historian::Lifecycle::Ready),
        );
        assert_eq!(
            dashboard.nns_manager.as_ref().map(|status| (
                status.lifecycle,
                status.permanent_maturity_baseline_reconciled,
                status.latest_pooled_target.is_some()
            )),
            Some((io_historian::Lifecycle::Ready, true, true)),
        );
        assert_eq!(
            dashboard
                .nns_governance
                .as_ref()
                .map(|status| status.neurons.len()),
            Some(1),
        );
        assert!(dashboard.protocol.claim_rate.is_some());
        assert!(dashboard
            .index
            .as_ref()
            .is_some_and(|status| !status.accounts.is_empty()));
        println!("historian_settle_seconds={settle_seconds}");
        println!("historian_dashboard={dashboard:#?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reward_margin_waits_only_for_the_exact_remaining_margin() {
        assert_eq!(reward_margin_wait_seconds(1_220, 1_000), 80);
        assert_eq!(reward_margin_wait_seconds(1_300, 1_000), 0);
        assert_eq!(reward_margin_wait_seconds(1_301, 1_000), 0);
        assert_eq!(reward_scheduler_advance_seconds(1_220, 1_000), 81);
        assert_eq!(reward_scheduler_advance_seconds(1_300, 1_000), 1);
        assert_eq!(reward_scheduler_advance_seconds(1_301, 1_000), 0);
    }
}
