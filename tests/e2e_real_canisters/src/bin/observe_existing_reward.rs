use candid::{decode_one, encode_args, encode_one, Principal};
use e2e_real_canisters::sns_governance_setup::{ListNeurons, ListNeuronsResponse};
use io_governance_types::SnsRewardEvent;
use io_stream_manager::{ApiError, RewardEventObservation, Status};
use pocket_ic::PocketIc;
use std::time::Duration;

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
}
