use candid::{decode_one, encode_one, Principal};
use io_nns_neuron_manager::{
    state::Account, ApiError, InitArgs, Lifecycle, NnsConfig, SetTwoWeekTargetArgs, Status,
    TwoWeekTargetStatus,
};
use pocket_ic::PocketIc;

const CYCLES: u128 = 2_000_000_000_000;

#[test]
fn simplified_nns_installs_paused_and_rejects_unauthorized_target() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping NNS-manager PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let wasm_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/debug/io_nns_neuron_manager.wasm");
    let wasm = match std::fs::read(wasm_path) {
        Ok(wasm) => wasm,
        Err(_) => {
            eprintln!("skipping NNS-manager PocketIC test because debug Wasm is missing");
            return;
        }
    };
    let pic = PocketIc::new();
    let canister = pic.create_canister();
    pic.add_cycles(canister, CYCLES);
    let principal = Principal::from_slice(&[1; 29]);
    let staging = |value: u8| Account {
        owner: canister,
        subaccount: Some(vec![value; 32]),
    };
    pic.install_canister(
        canister,
        wasm.clone(),
        encode_one(InitArgs {
            config: NnsConfig {
                sns_governance: Principal::from_slice(&[2; 29]),
                stream_manager: Principal::from_slice(&[3; 29]),
                jupiter: Principal::from_slice(&[4; 29]),
                icp_ledger: principal,
                nns_governance: Principal::from_slice(&[5; 29]),
                two_year_neuron_id: 1,
                two_week_neuron_id: 2,
                jupiter_account: Account {
                    owner: Principal::from_slice(&[4; 29]),
                    subaccount: None,
                },
                jupiter_staging: Account {
                    owner: canister,
                    subaccount: None,
                },
                two_week_maturity_staging: staging(2),
                stream_liquid_account: Account {
                    owner: Principal::from_slice(&[3; 29]),
                    subaccount: None,
                },
                expected_io_fee_e8s: 10_000,
                expected_icp_fee_e8s: 10_000,
                jupiter_fee_float_e8s: 20_000,
                two_week_fee_float_e8s: 10_000,
                seeded_two_week_principal_e8s: 1,
            },
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
    pic.upgrade_canister(canister, wasm, encode_one(()).unwrap(), None)
        .unwrap();
    let upgraded: Status = decode_one(
        &pic.query_call(
            canister,
            Principal::anonymous(),
            "get_status",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(upgraded.lifecycle, Lifecycle::Paused);
    let result: Result<TwoWeekTargetStatus, ApiError> = decode_one(
        &pic.update_call(
            canister,
            principal,
            "set_two_week_target",
            encode_one(SetTwoWeekTargetArgs {
                target_e8s: 1,
                generation: 1,
            })
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(result, Err(ApiError::Unauthorized));
}
