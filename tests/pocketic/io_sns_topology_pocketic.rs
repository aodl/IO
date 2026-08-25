use candid::{encode_one, CandidType, Principal};
use pocket_ic::PocketIcBuilder;
use serde::Deserialize;
use std::path::PathBuf;

const CYCLES: u128 = 2_000_000_000_000;

#[derive(CandidType, Deserialize)]
struct NnsInitArgs {
    config: NnsConfig,
}

#[derive(CandidType, Deserialize)]
struct NnsConfig {
    sns_governance: Principal,
    stream_manager: Principal,
    jupiter: Principal,
    icp_ledger: Principal,
    nns_governance: Principal,
    two_year_neuron_id: u64,
    pooled_parent_memo: u64,
    pooled_parent_followee_id: u64,
    minimum_parent_stake_e8s: u128,
    jupiter_account: io_stream_manager::Account,
    jupiter_staging: io_stream_manager::Account,
    stream_liquid_account: io_stream_manager::Account,
    expected_io_fee_e8s: u128,
    expected_icp_fee_e8s: u128,
    jupiter_activation_block_floor: u128,
    audited_permanent_principal_e8s: u128,
    transfer_retry_delay_nanos: u64,
    ledger_deduplication_window_nanos: u64,
}

fn pocketic_available() -> bool {
    std::env::var_os("POCKET_IC_BIN").is_some()
}

fn required_wasm(path: &str) -> Option<Vec<u8>> {
    match std::fs::read(workspace_relative_path(path)) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!("skipping SNS topology PocketIC test because {path} is missing");
            None
        }
    }
}

fn workspace_relative_path(path: &str) -> PathBuf {
    let direct = PathBuf::from(path);
    if direct.exists() {
        return direct;
    }
    PathBuf::from("../../").join(path)
}

#[test]
fn pocketic_live_sns_topology_installs_io_canisters_with_local_principals() {
    if !pocketic_available() {
        eprintln!("skipping SNS topology PocketIC test because POCKET_IC_BIN is not set");
        return;
    }

    let stream_wasm =
        match required_wasm("target/wasm32-unknown-unknown/debug/io_stream_manager.wasm") {
            Some(wasm) => wasm,
            None => return,
        };
    let nns_manager_wasm =
        match required_wasm("target/wasm32-unknown-unknown/debug/io_nns_neuron_manager.wasm") {
            Some(wasm) => wasm,
            None => return,
        };
    let sns_governance_wasm =
        match required_wasm("target/wasm32-unknown-unknown/debug/mock_sns_governance.wasm") {
            Some(wasm) => wasm,
            None => return,
        };
    let sns_root_wasm =
        match required_wasm("target/wasm32-unknown-unknown/debug/mock_sns_root.wasm") {
            Some(wasm) => wasm,
            None => return,
        };
    let nns_governance_wasm =
        match required_wasm("target/wasm32-unknown-unknown/debug/mock_nns_governance.wasm") {
            Some(wasm) => wasm,
            None => return,
        };

    let sns_init =
        std::fs::read_to_string(workspace_relative_path("tools/sns/sns_init.io.local.yaml"))
            .expect("local SNS init fixture should be readable");
    assert!(sns_init.contains("name: \"IO\""));
    assert!(sns_init.contains("sns_governance_principal_text"));
    assert!(sns_init.contains("not production-ready"));
    assert!(!sns_init.contains("--network ic"));

    let stream_did = std::fs::read_to_string(workspace_relative_path(
        "canisters/io_stream_manager/io_stream_manager.did",
    ))
    .expect("stream production DID should be readable");
    assert!(stream_did.contains("service : (InitArgs) -> {"));
    assert!(!stream_did.contains("debug_"));
    assert!(!stream_did.contains(" get_state :"));

    let nns_did = std::fs::read_to_string(workspace_relative_path(
        "canisters/io_nns_neuron_manager/io_nns_neuron_manager.did",
    ))
    .expect("nns production DID should be readable");
    assert!(nns_did.contains("service : (InitArgs) -> {"));
    assert!(!nns_did.contains("debug_"));
    assert!(!nns_did.contains(" get_state :"));

    let pic = PocketIcBuilder::new()
        .with_nns_subnet()
        .with_sns_subnet()
        .with_application_subnet()
        .build();
    let topology = pic.topology();
    let nns_subnet = topology.get_nns().expect("NNS subnet should exist");
    let sns_subnet = topology.get_sns().expect("SNS subnet should exist");
    let app_subnet = topology
        .get_app_subnets()
        .first()
        .copied()
        .expect("application subnet should exist");

    let icp_ledger = pic.create_canister_on_subnet(None, None, app_subnet);
    let icp_index = pic.create_canister_on_subnet(None, None, app_subnet);
    let io_ledger = pic.create_canister_on_subnet(None, None, app_subnet);
    let io_index = pic.create_canister_on_subnet(None, None, app_subnet);
    let io_sns_ledger = pic.create_canister_on_subnet(None, None, sns_subnet);
    let io_sns_index = pic.create_canister_on_subnet(None, None, sns_subnet);
    let sns_governance = pic.create_canister_on_subnet(None, None, sns_subnet);
    let sns_root = pic.create_canister_on_subnet(None, None, sns_subnet);
    let nns_governance = pic.create_canister_on_subnet(None, None, nns_subnet);
    let nns_manager = pic.create_canister_on_subnet(None, None, app_subnet);
    let historian = pic.create_canister_on_subnet(None, None, app_subnet);
    let frontend = pic.create_canister_on_subnet(None, None, app_subnet);

    for canister in [
        icp_ledger,
        icp_index,
        io_ledger,
        io_index,
        io_sns_ledger,
        io_sns_index,
        sns_governance,
        sns_root,
        nns_governance,
        nns_manager,
        historian,
        frontend,
    ] {
        pic.add_cycles(canister, CYCLES);
    }

    pic.install_canister(sns_governance, sns_governance_wasm, Vec::new(), None);
    pic.install_canister(sns_root, sns_root_wasm, Vec::new(), None);
    pic.install_canister(nns_governance, nns_governance_wasm, Vec::new(), None);

    let stream = pic.create_canister_on_subnet(None, None, app_subnet);
    pic.add_cycles(stream, CYCLES);
    pic.install_canister(
        stream,
        stream_wasm,
        encode_one(io_stream_manager::InitArgs {
            config: io_stream_manager::StreamConfig {
                io_ledger: io_sns_ledger,
                icp_ledger,
                nns_manager,
                jupiter_io_account: io_stream_manager::Account {
                    owner: sns_root,
                    subaccount: None,
                },
                sns_governance,
                sns_root,
                expected_sns_governance_module_hash: vec![0; 32],
                approved_reward_event_duration_seconds: 86_400,
                io_reserve: io_stream_manager::Account {
                    owner: stream,
                    subaccount: None,
                },
                liquid_icp: io_stream_manager::Account {
                    owner: stream,
                    subaccount: Some(vec![1; 32]),
                },
                nonredeemable_governance_io_accounts: Vec::new(),
                minimum_redemption_io_e8s: 20_000,
                expected_io_fee_e8s: 10_000,
                expected_icp_fee_e8s: 10_000,
                maximum_request_lifetime_nanos: 900_000_000_000,
                retry_delay_nanos: 1_000_000_000,
                ledger_deduplication_window_nanos: 86_400_000_000_000,
            },
        })
        .expect("encode stream init args"),
        None,
    );

    pic.install_canister(
        nns_manager,
        nns_manager_wasm,
        encode_one(NnsInitArgs {
            config: NnsConfig {
                sns_governance,
                stream_manager: stream,
                jupiter: sns_root,
                icp_ledger,
                nns_governance,
                two_year_neuron_id: 42,
                pooled_parent_memo: 43,
                pooled_parent_followee_id: 44,
                minimum_parent_stake_e8s: 100_000_000,
                jupiter_account: io_stream_manager::Account {
                    owner: sns_root,
                    subaccount: None,
                },
                jupiter_staging: io_stream_manager::Account {
                    owner: nns_manager,
                    subaccount: None,
                },
                stream_liquid_account: io_stream_manager::Account {
                    owner: stream,
                    subaccount: Some(vec![1; 32]),
                },
                expected_io_fee_e8s: 10_000,
                expected_icp_fee_e8s: 10_000,
                jupiter_activation_block_floor: 1,
                audited_permanent_principal_e8s: 1,
                transfer_retry_delay_nanos: 1_000_000_000,
                ledger_deduplication_window_nanos: 86_400_000_000_000,
            },
        })
        .expect("encode nns manager init args"),
        None,
    );

    assert_ne!(stream, nns_manager);
    assert_ne!(sns_governance, nns_governance);
    assert_eq!(pic.get_subnet(stream), Some(app_subnet));
    assert_eq!(pic.get_subnet(nns_manager), Some(app_subnet));
    assert_eq!(pic.get_subnet(historian), Some(app_subnet));
    assert_eq!(pic.get_subnet(frontend), Some(app_subnet));
    assert_eq!(pic.get_subnet(sns_governance), Some(sns_subnet));
    assert_eq!(pic.get_subnet(sns_root), Some(sns_subnet));
    assert_eq!(pic.get_subnet(io_sns_ledger), Some(sns_subnet));
    assert_eq!(pic.get_subnet(io_sns_index), Some(sns_subnet));
    assert_eq!(pic.get_subnet(nns_governance), Some(nns_subnet));
}
