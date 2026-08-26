use candid::{decode_one, encode_args, encode_one, Principal};
use e2e_real_canisters::sns_governance_setup::{ListNeurons, ListNeuronsResponse};
use io_governance_types::SnsRewardEvent;
use io_stream_manager::{ApiError, RewardEventObservation, Status};
use pocket_ic::PocketIc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
}

fn principal(name: &str) -> Principal {
    Principal::from_text(required(name)).unwrap_or_else(|error| panic!("invalid {name}: {error}"))
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

    if let Ok(seconds) = std::env::var("IO_LOCAL_REWARD_ADVANCE_SECONDS") {
        let seconds = seconds
            .parse::<u64>()
            .expect("invalid IO_LOCAL_REWARD_ADVANCE_SECONDS");
        pic.advance_time(Duration::from_secs(seconds));
        for _ in 0..20 {
            pic.tick();
        }
        println!("advanced_pocketic_seconds={seconds}");
    }

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
    println!("latest_reward_event={event:#?}");

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
            "neuron={} stake_e8s={} maturity_e8s={} dissolve_state={:?} participation={:?}",
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

    let before: Status = decode_one(
        &pic.query_call(
            stream,
            caller,
            "get_status",
            encode_args(()).expect("encode get_status request"),
        )
        .expect("query stream status"),
    )
    .expect("decode stream status");
    println!("stream_status_before={before:#?}");

    if std::env::var_os("IO_LOCAL_REWARD_RESUME").is_some() {
        if before.reward_processing_paused {
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
        let result: Result<RewardEventObservation, ApiError> = decode_one(
            &pic.update_call(
                stream,
                caller,
                "resume_reward_work",
                encode_args(()).expect("encode resume_reward_work request"),
            )
            .expect("resume reward work"),
        )
        .expect("decode reward observation");
        println!("resume_reward_work={result:#?}");
    }

    let after: Status = decode_one(
        &pic.query_call(
            stream,
            caller,
            "get_status",
            encode_args(()).expect("encode get_status request"),
        )
        .expect("query stream status"),
    )
    .expect("decode stream status");
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
            Some(2),
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
