use candid::{decode_one, encode_one, Principal};
use io_stream_manager::{
    Account, ApiError, InitArgs, Lifecycle, RedeemArgs, RedemptionProgress, Status, StreamConfig,
};
use pocket_ic::PocketIc;

const CYCLES: u128 = 2_000_000_000_000;

#[test]
fn simplified_stream_installs_paused_and_rejects_anonymous_before_funds_move() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping stream-manager PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let wasm_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/debug/io_stream_manager.wasm");
    let wasm = match std::fs::read(wasm_path) {
        Ok(wasm) => wasm,
        Err(_) => {
            eprintln!("skipping stream-manager PocketIC test because debug Wasm is missing");
            return;
        }
    };
    let pic = PocketIc::new();
    let canister = pic.create_canister();
    pic.add_cycles(canister, CYCLES);
    let io_ledger = Principal::from_slice(&[1; 29]);
    let icp_ledger = Principal::from_slice(&[4; 29]);
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
                io_ledger,
                icp_ledger,
                nns_manager: manager,
                jupiter_receipt_source: Account {
                    owner: manager,
                    subaccount: Some(vec![7; 32]),
                },
                two_week_receipt_source: Account {
                    owner: manager,
                    subaccount: Some(vec![8; 32]),
                },
                jupiter_io_account: Account {
                    owner: manager,
                    subaccount: Some(vec![9; 32]),
                },
                sns_governance: governance,
                io_reserve: account.clone(),
                liquid_icp: Account {
                    owner: canister,
                    subaccount: Some(vec![1; 32]),
                },
                excluded_io_accounts: Vec::new(),
                minimum_redemption_io_e8s: 20_000,
                expected_io_fee_e8s: 10_000,
                expected_icp_fee_e8s: 10_000,
                maximum_request_lifetime_nanos: 900_000_000_000,
                retry_delay_nanos: 1_000_000_000,
                ledger_deduplication_window_nanos: 86_400_000_000_000,
            },
            next_cohort_timestamp_seconds: 0,
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
    assert_eq!(status.lifecycle, Lifecycle::Paused);
    assert!(status.operation_kind.is_none());
    let result: Result<RedemptionProgress, ApiError> = decode_one(
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
