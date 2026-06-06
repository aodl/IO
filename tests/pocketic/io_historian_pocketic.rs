use candid::{decode_one, encode_one};
use pocket_ic::PocketIc;

const CYCLES: u128 = 2_000_000_000_000;

fn pocketic_available() -> bool {
    std::env::var_os("POCKET_IC_BIN").is_some()
}

fn required_wasm(path: &str) -> Option<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!("skipping historian PocketIC test because {path} is missing");
            None
        }
    }
}

#[test]
fn pocketic_historian_debug_ingestion_queries_and_upgrade_persist() {
    if !pocketic_available() {
        eprintln!("skipping historian PocketIC test because POCKET_IC_BIN is not set");
        return;
    }

    let wasm = match required_wasm("target/wasm32-unknown-unknown/debug/io_historian.wasm") {
        Some(wasm) => wasm,
        None => return,
    };

    let production_did = std::fs::read_to_string("canisters/io_historian/io_historian.did")
        .expect("historian production DID should be readable");
    assert!(production_did.contains("get_dashboard_state"));
    assert!(production_did.contains("list_streams"));
    assert!(!production_did.contains("debug_"));
    assert!(!production_did.contains("get_all"));

    let stream_did = std::fs::read_to_string("canisters/io_stream_manager/io_stream_manager.did")
        .expect("stream production DID should be readable");
    let nns_did =
        std::fs::read_to_string("canisters/io_nns_neuron_manager/io_nns_neuron_manager.did")
            .expect("nns production DID should be readable");
    for did in [stream_did, nns_did] {
        assert!(did.contains("service : (InitArgs) -> {}"));
        assert!(!did.contains("debug_"));
        assert!(!did.contains(" get_state :"));
    }

    let pic = PocketIc::new();
    let historian = pic.create_canister();
    pic.add_cycles(historian, CYCLES);
    pic.install_canister(historian, wasm.clone(), encode_one(()).unwrap(), None);

    let status = pic
        .query_call(
            historian,
            candid::Principal::anonymous(),
            "get_public_status",
            encode_one(()).unwrap(),
        )
        .expect("query public status");
    let status: io_historian::PublicStatus = decode_one(&status).unwrap();
    assert_eq!(status.ingestion.stream_record_count, 0);

    let record = io_historian::StreamHistoryRecord {
        record_id: "stream:0001".to_string(),
        source_ledger: "icp".to_string(),
        source_block_index: Some(1),
        stream_kind: io_historian::PublicStreamKind::JupiterFaucet,
        amount_e8s: 100,
        recipient_policy: io_historian::PublicRecipientPolicy::JupiterFaucet,
        io_issued_e8s: Some(100),
        phase: io_historian::PublicOperationPhase::Completed,
        timestamp_nanos: Some(1),
        memo_label: Some("test".to_string()),
        safe_subaccount_label: None,
        terminal_rejection_reason: None,
    };
    pic.update_call(
        historian,
        candid::Principal::anonymous(),
        "debug_ingest_stream_record",
        encode_one(record).unwrap(),
    )
    .expect("ingest stream");

    let page = pic
        .query_call(
            historian,
            candid::Principal::anonymous(),
            "list_streams",
            encode_one(io_historian::ListStreamsRequest {
                start_after: None,
                limit: Some(10),
            })
            .unwrap(),
        )
        .expect("list streams");
    let page: io_historian::ListStreamsResponse = decode_one(&page).unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].record_id, "stream:0001");

    let dashboard = pic
        .query_call(
            historian,
            candid::Principal::anonymous(),
            "get_dashboard_state",
            encode_one(()).unwrap(),
        )
        .expect("dashboard");
    let dashboard: io_historian::PublicDashboardState = decode_one(&dashboard).unwrap();
    assert_eq!(dashboard.status.ingestion.stream_record_count, 1);

    pic.upgrade_canister(historian, wasm, encode_one(()).unwrap(), None)
        .expect("upgrade historian");
    let page = pic
        .query_call(
            historian,
            candid::Principal::anonymous(),
            "list_streams",
            encode_one(io_historian::ListStreamsRequest {
                start_after: None,
                limit: Some(10),
            })
            .unwrap(),
        )
        .expect("list streams after upgrade");
    let page: io_historian::ListStreamsResponse = decode_one(&page).unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].record_id, "stream:0001");
}
