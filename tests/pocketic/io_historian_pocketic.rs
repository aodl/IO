use candid::{decode_one, encode_one, CandidType, Principal};
use io_historian::{
    CanisterRole, ExpectedModule, Lifecycle, NamedAccount, ObservationConfig, ObservationFreshness,
    RewardEventId, StreamStatus,
};
use io_ledger_types::{Account, Subaccount};
use pocket_ic::PocketIc;
use serde::Deserialize;

const CYCLES: u128 = 2_000_000_000_000;

fn pocketic_available() -> bool {
    std::env::var_os("POCKET_IC_BIN").is_some()
}

fn account(seed: u8) -> Account {
    Account::new(Principal::from_slice(&[seed]), Some(Subaccount([seed; 32])))
}

fn config(historian: Principal) -> ObservationConfig {
    let ids = (10..=17)
        .map(|seed| Principal::from_slice(&[seed]))
        .collect::<Vec<_>>();
    let modules = [
        (CanisterRole::StreamManager, ids[0]),
        (CanisterRole::NnsManager, ids[1]),
        (CanisterRole::SnsRoot, ids[2]),
        (CanisterRole::SnsGovernance, ids[3]),
        (CanisterRole::SnsLedger, ids[4]),
        (CanisterRole::SnsIndex, ids[5]),
        (CanisterRole::Historian, historian),
        (CanisterRole::Frontend, Principal::from_slice(&[30])),
    ];
    ObservationConfig {
        stream_manager: ids[0],
        nns_manager: ids[1],
        sns_root: ids[2],
        sns_governance: ids[3],
        sns_ledger: ids[4],
        sns_index: ids[5],
        icp_ledger: ids[6],
        nns_governance: ids[7],
        two_year_neuron_id: 2,
        protocol_io_reserve: account(40),
        liquid_icp_reserve: account(41),
        nonredeemable_governance_io_accounts: vec![NamedAccount {
            name: "governance".into(),
            account: account(42),
        }],
        history_accounts: vec![NamedAccount {
            name: "protocol-reserve".into(),
            account: account(40),
        }],
        expected_modules: modules
            .into_iter()
            .enumerate()
            .map(|(index, (role, canister_id))| ExpectedModule {
                role,
                canister_id,
                wasm_sha256: vec![index as u8; 32],
            })
            .collect(),
        reward_share_capable_governance_sha256: Some(vec![3; 32]),
        refresh_interval_seconds: 60,
    }
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize)]
enum RawRewardEventClassification {
    StructuralOnly,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct RawStructuralStreamStatus {
    lifecycle: Lifecycle,
    operation_kind: Option<String>,
    operation_phase: Option<String>,
    latest_entitlement_batch_generation: u64,
    latest_processed_reward_event: Option<RewardEventId>,
    latest_reward_event_classification: Option<RawRewardEventClassification>,
    accumulated_eligible_credit: u128,
    accumulated_policy_credit: u128,
    processed_reward_event_count: u64,
    missed_reward_event_count: u64,
    reward_work_due: bool,
    reward_processing_paused: bool,
    governance_parameters_fresh: bool,
    pending_entitlement_batch_eligible_credit: Option<u128>,
    pending_entitlement_batch_policy_credit: Option<u128>,
    latest_reconciliation_checkpoint: Option<io_historian::ReconciliationProjection>,
}

#[test]
fn pocketic_historian_decodes_structural_stream_status_without_discarding_it() {
    if !pocketic_available() {
        eprintln!("skipping historian structural decode because POCKET_IC_BIN is not set");
        return;
    }
    let wasm = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/wasm32-unknown-unknown/debug/io_historian.wasm"),
    )
    .expect("build historian debug Wasm before this required regression");
    let pic = PocketIc::new();
    let historian = pic.create_canister();
    pic.add_cycles(historian, CYCLES);
    pic.install_canister(
        historian,
        wasm,
        encode_one(None::<ObservationConfig>).unwrap(),
        None,
    );
    let raw = RawStructuralStreamStatus {
        lifecycle: Lifecycle::Ready,
        operation_kind: Some("Redemption".into()),
        operation_phase: Some("PayoutSucceeded".into()),
        latest_entitlement_batch_generation: 0,
        latest_processed_reward_event: Some(RewardEventId {
            end_timestamp_seconds: 1,
            round: 0,
        }),
        latest_reward_event_classification: Some(RawRewardEventClassification::StructuralOnly),
        accumulated_eligible_credit: 0,
        accumulated_policy_credit: 0,
        processed_reward_event_count: 0,
        missed_reward_event_count: 0,
        reward_work_due: false,
        reward_processing_paused: false,
        governance_parameters_fresh: true,
        pending_entitlement_batch_eligible_credit: None,
        pending_entitlement_batch_policy_credit: None,
        latest_reconciliation_checkpoint: None,
    };
    let decoded: Result<StreamStatus, String> = decode_one(
        &pic.update_call(
            historian,
            Principal::anonymous(),
            "debug_decode_stream_status",
            encode_one(encode_one(raw).unwrap()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let decoded = decoded.expect("StructuralOnly must not discard the Stream observation");
    assert_eq!(decoded.latest_processed_reward_event.unwrap().round, 0);
    assert_eq!(decoded.operation_phase.as_deref(), Some("PayoutSucceeded"));
    assert_eq!(decoded.latest_reward_event_classification, None);
}

#[test]
fn pocketic_historian_upgrade_controls_config_and_preserves_honest_freshness() {
    if !pocketic_available() {
        eprintln!("skipping historian PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let wasm_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/debug/io_historian.wasm");
    let wasm = match std::fs::read(&wasm_path) {
        Ok(wasm) => wasm,
        Err(_) => {
            eprintln!("skipping historian PocketIC test because debug Wasm is missing");
            return;
        }
    };
    let production_did = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("io_historian.did"),
    )
    .expect("read historian production DID");
    assert!(production_did.contains("service : (opt ObservationConfig)"));
    assert!(!production_did.contains("debug_"));
    assert!(!production_did.contains(" configure :"));
    assert!(!production_did.contains(" ingest :"));

    let pic = PocketIc::new();
    let historian = pic.create_canister();
    pic.add_cycles(historian, CYCLES);
    pic.install_canister(
        historian,
        wasm.clone(),
        encode_one(None::<ObservationConfig>).unwrap(),
        None,
    );
    let dashboard = || {
        decode_one::<io_historian::Dashboard>(
            &pic.query_call(
                historian,
                Principal::anonymous(),
                "get_dashboard_state",
                encode_one(()).unwrap(),
            )
            .expect("query dashboard"),
        )
        .unwrap()
    };
    let initial = dashboard();
    assert!(!initial.status.configured);
    assert!(initial
        .source_health
        .iter()
        .all(|health| { health.freshness == ObservationFreshness::PrelaunchNotConfigured }));

    pic.upgrade_canister(
        historian,
        wasm.clone(),
        encode_one(Some(config(historian))).unwrap(),
        None,
    )
    .expect("upgrade historian with typed observation config");
    let configured = dashboard();
    assert!(configured.status.configured);
    assert!(configured
        .source_health
        .iter()
        .all(|health| health.freshness == ObservationFreshness::Missing));

    pic.upgrade_canister(
        historian,
        wasm,
        encode_one(None::<ObservationConfig>).unwrap(),
        None,
    )
    .expect("same-Wasm upgrade preserves observation config");
    assert!(dashboard().status.configured);
}
