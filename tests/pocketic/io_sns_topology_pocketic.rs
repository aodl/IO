use candid::encode_one;
use pocket_ic::PocketIcBuilder;

const CYCLES: u128 = 2_000_000_000_000;

fn pocketic_available() -> bool {
    std::env::var_os("POCKET_IC_BIN").is_some()
}

fn required_wasm(path: &str) -> Option<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!("skipping SNS topology PocketIC test because {path} is missing");
            None
        }
    }
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

    let sns_init = std::fs::read_to_string("tools/sns/sns_init.io.local.yaml")
        .expect("local SNS init fixture should be readable");
    assert!(sns_init.contains("name: \"IO\""));
    assert!(sns_init.contains("sns_governance_principal_text"));
    assert!(sns_init.contains("not production-ready"));
    assert!(!sns_init.contains("--network ic"));

    let stream_did = std::fs::read_to_string("canisters/io_stream_manager/io_stream_manager.did")
        .expect("stream production DID should be readable");
    assert!(stream_did.contains("service : (InitArgs) -> {}"));
    assert!(!stream_did.contains("debug_"));
    assert!(!stream_did.contains(" get_state :"));

    let nns_did =
        std::fs::read_to_string("canisters/io_nns_neuron_manager/io_nns_neuron_manager.did")
            .expect("nns production DID should be readable");
    assert!(nns_did.contains("service : (InitArgs) -> {}"));
    assert!(!nns_did.contains("debug_"));
    assert!(!nns_did.contains(" get_state :"));

    let stream_debug_did =
        std::fs::read_to_string("canisters/io_stream_manager/io_stream_manager_debug.did")
            .expect("stream debug DID should be readable");
    assert!(stream_debug_did.contains("debug_get_state"));

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
            jupiter_faucet_principal_text: Some(sns_root.to_text()),
            io_nns_neuron_manager_principal_text: Some(nns_manager.to_text()),
            icp_ledger_principal_text: Some(icp_ledger.to_text()),
            icp_index_principal_text: Some(icp_index.to_text()),
            io_ledger_principal_text: Some(io_ledger.to_text()),
            io_index_principal_text: Some(io_index.to_text()),
            io_sns_ledger_principal_text: Some(io_sns_ledger.to_text()),
            io_sns_index_principal_text: Some(io_sns_index.to_text()),
            sns_governance_principal_text: Some(sns_governance.to_text()),
            ..Default::default()
        })
        .expect("encode stream init args"),
        None,
    );

    pic.install_canister(
        nns_manager,
        nns_manager_wasm,
        encode_one(io_nns_neuron_manager::InitArgs {
            controller_canister_principal_text: sns_root.to_text(),
            two_year_nns_neuron_id: 42,
            io_stream_manager_principal_text: Some(stream.to_text()),
            nns_governance_principal_text: Some(nns_governance.to_text()),
            icp_ledger_principal_text: Some(icp_ledger.to_text()),
            icp_index_principal_text: Some(icp_index.to_text()),
            ..Default::default()
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
