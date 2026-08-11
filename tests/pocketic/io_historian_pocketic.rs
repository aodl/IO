use candid::{decode_one, encode_one, Principal};
use io_historian::{
    CanisterRole, ExpectedModule, NamedAccount, ObservationConfig, ObservationFreshness,
};
use io_ledger_types::{Account, Subaccount};
use pocket_ic::PocketIc;

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
        reward_backing_neuron_id: 1,
        two_year_neuron_id: 2,
        protocol_io_reserve: account(40),
        liquid_icp_reserve: account(41),
        excluded_io_accounts: vec![NamedAccount {
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
