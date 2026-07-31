use candid::{decode_one, encode_one, Principal};
use io_nns_neuron_manager::{
    state::Account, ApiError, InitArgs, Lifecycle, NnsConfig, SetTwoWeekTargetArgs, Status,
};
use pocket_ic::PocketIc;

const CYCLES: u128 = 2_000_000_000_000;

#[test]
fn simplified_nns_installs_inert_and_rejects_unauthorized_target() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping NNS-manager PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let wasm = match std::fs::read("target/wasm32-unknown-unknown/debug/io_nns_neuron_manager.wasm")
    {
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
    let account = Account {
        owner: canister,
        subaccount: None,
    };
    pic.install_canister(
        canister,
        wasm,
        encode_one(InitArgs {
            config: NnsConfig {
                sns_governance: Principal::from_slice(&[2; 29]),
                stream_manager: Principal::from_slice(&[3; 29]),
                jupiter: Principal::from_slice(&[4; 29]),
                icp_ledger: principal,
                nns_governance: Principal::from_slice(&[5; 29]),
                two_year_neuron_id: 1,
                two_week_neuron_id: 2,
                jupiter_account: account.clone(),
                staging_account: account.clone(),
                operational_fee_account: account.clone(),
                stream_liquid_account: account,
                expected_icp_fee_e8s: 10_000,
            },
            initial_lifecycle: Lifecycle::Inert,
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
    let result: Result<(), ApiError> = decode_one(
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
    assert_eq!(result, Err(ApiError::Inert));
}
