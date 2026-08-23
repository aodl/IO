use candid::{decode_one, encode_one, CandidType, Principal};
use io_nns_neuron_manager::{
    state::Account, ApiError, InitArgs, JupiterProgress, Lifecycle, MaturityProgress, NnsConfig,
    PrepareTwoWeekMaturityArgs, Status,
};
use pocket_ic::PocketIc;
use serde::Deserialize;
use std::time::Duration;

const CYCLES: u128 = 2_000_000_000_000;

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DebugMintAccountArgs {
    to: Account,
    amount_e8s: u128,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct CreateNeuronArgs {
    neuron_id: u64,
    principal_e8s: u128,
    dissolve_delay_seconds: u64,
}

#[derive(Clone, Copy, Debug, Default, CandidType, Deserialize)]
struct LedgerCallCounters {
    query_blocks: u64,
}

fn wasm(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/wasm32-unknown-unknown/debug/{name}.wasm"
        )),
    )
    .unwrap_or_else(|error| panic!("missing {name} debug Wasm: {error}"))
}

fn install(pic: &PocketIc, name: &str, arg: Vec<u8>) -> Principal {
    let canister = pic.create_canister();
    pic.add_cycles(canister, CYCLES);
    pic.install_canister(canister, wasm(name), arg, None);
    canister
}

fn update<A: CandidType, R: for<'de> Deserialize<'de> + CandidType>(
    pic: &PocketIc,
    canister: Principal,
    caller: Principal,
    method: &str,
    arg: A,
) -> R {
    decode_one(
        &pic.update_call(canister, caller, method, encode_one(arg).unwrap())
            .unwrap_or_else(|error| panic!("{method}: {error}")),
    )
    .unwrap()
}

fn query<R: for<'de> Deserialize<'de> + CandidType>(
    pic: &PocketIc,
    canister: Principal,
    method: &str,
) -> R {
    decode_one(
        &pic.query_call(
            canister,
            Principal::anonymous(),
            method,
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

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
                pooled_parent_memo: 2,
                pooled_parent_followee_id: 3,
                minimum_parent_stake_e8s: 100_000_000,
                jupiter_account: Account {
                    owner: Principal::from_slice(&[4; 29]),
                    subaccount: None,
                },
                jupiter_staging: Account {
                    owner: canister,
                    subaccount: None,
                },
                maturity_staging: staging(2),
                stream_liquid_account: Account {
                    owner: Principal::from_slice(&[3; 29]),
                    subaccount: None,
                },
                expected_io_fee_e8s: 10_000,
                expected_icp_fee_e8s: 10_000,
                jupiter_activation_block_floor: 1,
                audited_permanent_principal_e8s: 1,
                transfer_retry_delay_nanos: 1_000_000_000,
                ledger_deduplication_window_nanos: 86_400_000_000_000,
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
    let rendered: Result<String, String> = decode_one(
        &pic.query_call(
            canister,
            Principal::anonymous(),
            "validate_set_paused",
            encode_one(true).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(rendered.unwrap(), "Set IO NNS manager paused: true");
    assert!(pic
        .query_call(
            canister,
            Principal::anonymous(),
            "validate_set_paused",
            encode_one(()).unwrap(),
        )
        .is_err());
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
    let rendered_after_upgrade: Result<String, String> = decode_one(
        &pic.query_call(
            canister,
            Principal::anonymous(),
            "validate_set_paused",
            encode_one(false).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        rendered_after_upgrade.unwrap(),
        "Set IO NNS manager paused: false"
    );
    let result: Result<MaturityProgress, ApiError> = decode_one(
        &pic.update_call(
            canister,
            principal,
            "prepare_two_week_maturity",
            encode_one(PrepareTwoWeekMaturityArgs {
                entitlement_batch_generation: 1,
                target_e8s: 1,
            })
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(result, Err(ApiError::Unauthorized));
}

#[test]
fn jupiter_floor_baselines_and_upgrade_replay_boundaries_hold() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping NNS boundary PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let pic = PocketIc::new();
    let ledger = install(&pic, "mock_icp_ledger", Vec::new());
    let governance = install(&pic, "mock_nns_governance", Vec::new());
    let sns_governance = Principal::from_slice(&[21; 29]);
    let stream = Principal::from_slice(&[22; 29]);
    let jupiter = Principal::from_slice(&[23; 29]);

    let manager = pic.create_canister();
    pic.add_cycles(manager, CYCLES);
    let staging = |value: u8| Account {
        owner: manager,
        subaccount: (value != 0).then(|| vec![value; 32]),
    };
    let jupiter_account = Account {
        owner: jupiter,
        subaccount: None,
    };
    let _: u64 = update(
        &pic,
        ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: jupiter_account.clone(),
            amount_e8s: 10_000_000,
        },
    );
    let old_block: io_ledger_boundary::IcrcTransferResult = update(
        &pic,
        ledger,
        jupiter,
        "icrc1_transfer",
        io_ledger_boundary::IcrcTransferArg {
            from_subaccount: None,
            to: staging(0),
            amount: candid::Nat::from(1_000_000_u64),
            fee: Some(candid::Nat::from(10_000_u64)),
            memo: Some(vec![1]),
            created_at_time: Some(1),
        },
    );
    let old_block: u128 = old_block.unwrap().0.try_into().unwrap();
    assert_eq!(old_block, 1);
    let _: u64 = update(
        &pic,
        ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: staging(2),
            amount_e8s: 10_000,
        },
    );
    for neuron_id in [41_u64, 42] {
        let _: u64 = update(
            &pic,
            governance,
            Principal::anonymous(),
            "debug_create_neuron",
            CreateNeuronArgs {
                neuron_id,
                principal_e8s: 1_000_000,
                dissolve_delay_seconds: 63_115_200,
            },
        );
    }
    let config = NnsConfig {
        sns_governance,
        stream_manager: stream,
        jupiter,
        icp_ledger: ledger,
        nns_governance: governance,
        two_year_neuron_id: 41,
        pooled_parent_memo: 42,
        pooled_parent_followee_id: 43,
        minimum_parent_stake_e8s: 100_000_000,
        jupiter_account,
        jupiter_staging: staging(0),
        maturity_staging: staging(2),
        stream_liquid_account: Account {
            owner: stream,
            subaccount: None,
        },
        expected_io_fee_e8s: 10_000,
        expected_icp_fee_e8s: 10_000,
        jupiter_activation_block_floor: old_block + 1,
        audited_permanent_principal_e8s: 1_000_000,
        transfer_retry_delay_nanos: 1_000_000_000,
        ledger_deduplication_window_nanos: 86_400_000_000_000,
    };
    pic.install_canister(
        manager,
        wasm("io_nns_neuron_manager"),
        encode_one(InitArgs { config }).unwrap(),
        None,
    );

    let initial: Status = query(&pic, manager, "get_status");
    assert!(!initial.two_year_maturity_baseline_reconciled);
    let ready: Result<(), ApiError> = update(&pic, manager, sns_governance, "set_paused", false);
    ready.unwrap();
    let ready_status: Status = query(&pic, manager, "get_status");
    assert!(ready_status.two_year_maturity_baseline_reconciled);

    let governance_calls_before_unauthorized: u64 =
        query(&pic, governance, "debug_get_full_neuron_call_count");
    let unauthorized_assets: Result<io_nns_types::backing::ClaimAssetObservation, ApiError> =
        update(
            &pic,
            manager,
            Principal::anonymous(),
            "observe_claim_assets",
            (),
        );
    assert_eq!(unauthorized_assets, Err(ApiError::Unauthorized));
    assert_eq!(
        query::<u64>(&pic, governance, "debug_get_full_neuron_call_count"),
        governance_calls_before_unauthorized,
        "unauthorized asset observation must not call Governance"
    );
    let unauthorized_policy: Result<io_nns_types::backing::PoolPolicyObservation, ApiError> =
        update(
            &pic,
            manager,
            Principal::anonymous(),
            "observe_pool_policy",
            (),
        );
    assert_eq!(unauthorized_policy, Err(ApiError::Unauthorized));
    assert_eq!(
        query::<u64>(&pic, governance, "debug_get_full_neuron_call_count"),
        governance_calls_before_unauthorized,
        "unauthorized policy observation must not call Governance"
    );

    let idle: Result<io_nns_neuron_manager::api::NnsProgress, ApiError> =
        update(&pic, manager, Principal::anonymous(), "resume", ());
    assert_eq!(idle, Ok(io_nns_neuron_manager::api::NnsProgress::Idle));

    let before_floor: LedgerCallCounters = query(&pic, ledger, "debug_get_call_counters");
    let rejected: Result<JupiterProgress, ApiError> = update(
        &pic,
        manager,
        Principal::anonymous(),
        "notify_jupiter_deposit",
        io_nns_neuron_manager::NotifyJupiterDepositArgs {
            block_index: old_block,
        },
    );
    assert!(
        matches!(rejected, Err(ApiError::Invalid(message)) if message.contains("predates activation floor"))
    );
    assert_eq!(
        query::<LedgerCallCounters>(&pic, ledger, "debug_get_call_counters").query_blocks,
        before_floor.query_blocks
    );
    let _: u64 = update(
        &pic,
        ledger,
        Principal::anonymous(),
        "debug_mint_account",
        DebugMintAccountArgs {
            to: staging(0),
            amount_e8s: 5_000_000,
        },
    );
    let rejected_again: Result<JupiterProgress, ApiError> = update(
        &pic,
        manager,
        Principal::anonymous(),
        "notify_jupiter_deposit",
        io_nns_neuron_manager::NotifyJupiterDepositArgs {
            block_index: old_block,
        },
    );
    assert!(matches!(rejected_again, Err(ApiError::Invalid(_))));
    assert_eq!(
        query::<LedgerCallCounters>(&pic, ledger, "debug_get_call_counters").query_blocks,
        before_floor.query_blocks
    );

    let public_invalid: Result<JupiterProgress, ApiError> = update(
        &pic,
        manager,
        Principal::anonymous(),
        "notify_jupiter_deposit",
        io_nns_neuron_manager::NotifyJupiterDepositArgs {
            block_index: old_block + 1,
        },
    );
    assert!(matches!(public_invalid, Err(ApiError::Invalid(_))));
    let after_public =
        query::<LedgerCallCounters>(&pic, ledger, "debug_get_call_counters").query_blocks;
    assert_eq!(after_public, before_floor.query_blocks + 1);

    pic.upgrade_canister(
        manager,
        wasm("io_nns_neuron_manager"),
        encode_one(()).unwrap(),
        None,
    )
    .unwrap();
    let reopened: Status = query(&pic, manager, "get_status");
    assert_eq!(reopened.lifecycle, Lifecycle::Paused);
    assert!(reopened.two_year_maturity_baseline_reconciled);
    let ready: Result<(), ApiError> = update(&pic, manager, sns_governance, "set_paused", false);
    ready.unwrap();
    let invalid_after_upgrade: Result<JupiterProgress, ApiError> = update(
        &pic,
        manager,
        Principal::anonymous(),
        "notify_jupiter_deposit",
        io_nns_neuron_manager::NotifyJupiterDepositArgs {
            block_index: old_block + 2,
        },
    );
    assert!(matches!(invalid_after_upgrade, Err(ApiError::Invalid(_))));
    assert_eq!(
        query::<LedgerCallCounters>(&pic, ledger, "debug_get_call_counters").query_blocks,
        after_public + 1
    );

    let valid_block: io_ledger_boundary::IcrcTransferResult = update(
        &pic,
        ledger,
        jupiter,
        "icrc1_transfer",
        io_ledger_boundary::IcrcTransferArg {
            from_subaccount: None,
            to: staging(0),
            amount: candid::Nat::from(2_000_000_u64),
            fee: Some(candid::Nat::from(10_000_u64)),
            memo: Some(vec![2]),
            created_at_time: Some(2),
        },
    );
    let valid_block: u128 = valid_block.unwrap().0.try_into().unwrap();
    let legitimate: Result<JupiterProgress, ApiError> = update(
        &pic,
        manager,
        jupiter,
        "notify_jupiter_deposit",
        io_nns_neuron_manager::NotifyJupiterDepositArgs {
            block_index: valid_block,
        },
    );
    assert_eq!(legitimate, Ok(JupiterProgress::DepositProved));
    assert_eq!(
        query::<LedgerCallCounters>(&pic, ledger, "debug_get_call_counters").query_blocks,
        after_public + 2
    );

    pic.advance_time(Duration::from_secs(1));
}
