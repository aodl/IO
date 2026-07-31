use candid::{decode_one, encode_one, Principal};
use io_stream_manager::{Account, ApiError, InitArgs, Lifecycle, RedeemArgs, Status, StreamConfig};
use pocket_ic::PocketIc;

const CYCLES: u128 = 2_000_000_000_000;

#[test]
fn simplified_stream_installs_inert_and_rejects_anonymous_before_funds_move() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping stream-manager PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let wasm = match std::fs::read("target/wasm32-unknown-unknown/debug/io_stream_manager.wasm") {
        Ok(wasm) => wasm,
        Err(_) => {
            eprintln!("skipping stream-manager PocketIC test because debug Wasm is missing");
            return;
        }
    };
    let pic = PocketIc::new();
    let canister = pic.create_canister();
    pic.add_cycles(canister, CYCLES);
    let ledger = Principal::from_slice(&[1; 29]);
    let manager = Principal::from_slice(&[2; 29]);
    let governance = Principal::from_slice(&[3; 29]);
    let account = Account {
        owner: canister,
        subaccount: None,
    };
    pic.install_canister(
        canister,
        wasm,
        encode_one(InitArgs {
            config: StreamConfig {
                io_ledger: ledger,
                icp_ledger: ledger,
                nns_manager: manager,
                nns_receipt_source: Account {
                    owner: manager,
                    subaccount: Some(vec![7; 32]),
                },
                sns_governance: governance,
                io_reserve: account.clone(),
                liquid_icp: account,
                excluded_io_accounts: Vec::new(),
                minimum_redemption_io_e8s: 1,
                expected_io_fee_e8s: 10_000,
                expected_icp_fee_e8s: 10_000,
            },
            initial_lifecycle: Lifecycle::Inert,
            next_cohort_timestamp_seconds: 1_209_600,
        })
        .unwrap(),
        None,
    );
    let status: Status = decode_one(
        &pic.query_call(
            canister,
            Principal::anonymous(),
            "get_status",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(status.lifecycle, Lifecycle::Inert);
    assert!(status.operation_kind.is_none());
    let result: Result<io_stream_manager::state::RedemptionResult, ApiError> = decode_one(
        &pic.update_call(
            canister,
            Principal::anonymous(),
            "redeem",
            encode_one(RedeemArgs {
                from_subaccount: None,
                io_amount_e8s: 1,
                min_icp_out_e8s: 1,
                max_io_fee_e8s: 10_000,
                max_icp_fee_e8s: 10_000,
                expires_at_nanos: u64::MAX,
                nonce: 0,
            })
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(result, Err(ApiError::Anonymous));
}
