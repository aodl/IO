use crate::artifacts::{resolve_from_env, ArtifactStatus};
use crate::icrc;
use crate::pocketic_env;
use candid::{Nat, Principal};
use io_ledger_types::{IcrcAccount, IcrcTransferError};
use std::time::Duration;

const RESERVE_E8S: u64 = 1_000_000_000_000;
const GOVERNANCE_E8S: u64 = 250_000_000_000;
const USER_TRANSFER_E8S: u64 = 100_000_000;
const CREATED_AT_OFFSET: u64 = 1_000;

struct LedgerIndexFixture {
    pic: pocket_ic::PocketIc,
    ledger: Principal,
    index: Principal,
    ledger_wasm: Vec<u8>,
    index_wasm: Vec<u8>,
    reserve_owner: Principal,
    user_owner: Principal,
    reserve: IcrcAccount,
    user: IcrcAccount,
    governance: IcrcAccount,
}

fn maybe_artifacts(required: bool) -> Option<crate::artifacts::ArtifactSet> {
    match resolve_from_env(required) {
        Ok(ArtifactStatus::Ready(set)) => Some(set),
        Ok(ArtifactStatus::Skipped(message)) => {
            eprintln!("skipping real-framework PocketIC test: {message}");
            None
        }
        Err(err) if !required => {
            panic!("real-framework artifacts are configured but invalid: {err}");
        }
        Err(err) => panic!("{err}"),
    }
}

fn setup(required: bool) -> Option<LedgerIndexFixture> {
    let artifacts = maybe_artifacts(required)?;
    if !pocketic_env::pocketic_available() {
        if required {
            panic!("POCKET_IC_BIN is required for this real-canister gate");
        }
        panic!("real-framework artifacts are configured but POCKET_IC_BIN is not set");
    }
    let ledger_wasm = artifacts.load_required("sns_ledger").unwrap();
    let index_wasm = artifacts.load_required("sns_index").unwrap();

    let pic = pocketic_env::new_sns_pic();
    let reserve_owner = Principal::from_slice(&[1; 29]);
    let user_owner = Principal::from_slice(&[2; 29]);
    let governance_owner = Principal::from_slice(&[3; 29]);
    let minting_owner = Principal::from_slice(&[4; 29]);
    let reserve = icrc::account(reserve_owner, Some(icrc::subaccount("protocol_reserve")));
    let user = icrc::account(user_owner, None);
    let governance = icrc::account(governance_owner, Some(icrc::subaccount("governance")));
    let minting = icrc::account(minting_owner, None);

    let ledger = pocketic_env::create_sns_canister(
        &pic,
        ledger_wasm.clone(),
        icrc::ledger_init_arg(
            Principal::anonymous(),
            minting,
            vec![
                (reserve.clone(), RESERVE_E8S),
                (governance.clone(), GOVERNANCE_E8S),
            ],
        ),
    );
    let index =
        pocketic_env::create_sns_canister(&pic, index_wasm.clone(), icrc::index_init_arg(ledger));
    for _ in 0..10 {
        pic.tick();
    }

    Some(LedgerIndexFixture {
        pic,
        ledger,
        index,
        ledger_wasm,
        index_wasm,
        reserve_owner,
        user_owner,
        reserve,
        user,
        governance,
    })
}

fn assert_metadata_and_initial_balances(f: &LedgerIndexFixture) {
    assert_eq!(icrc::icrc1_name(&f.pic, f.ledger), icrc::TOKEN_NAME);
    assert_eq!(icrc::icrc1_symbol(&f.pic, f.ledger), icrc::TOKEN_SYMBOL);
    assert_eq!(icrc::icrc1_decimals(&f.pic, f.ledger), icrc::DECIMALS);
    assert_eq!(icrc::icrc1_fee(&f.pic, f.ledger), Nat::from(icrc::FEE_E8S));
    assert_eq!(
        icrc::icrc1_total_supply(&f.pic, f.ledger),
        Nat::from(RESERVE_E8S + GOVERNANCE_E8S)
    );
    assert_eq!(
        icrc::icrc1_balance_of(&f.pic, f.ledger, f.reserve.clone()),
        Nat::from(RESERVE_E8S)
    );
    assert_eq!(
        icrc::icrc1_balance_of(&f.pic, f.ledger, f.user.clone()),
        Nat::from(0_u64)
    );
    assert_eq!(
        icrc::icrc1_balance_of(&f.pic, f.ledger, f.governance.clone()),
        Nat::from(GOVERNANCE_E8S)
    );
}

fn transfer_reserve_to_user(f: &LedgerIndexFixture) -> (u64, u64) {
    let created_at_time = created_at_time(f, 0);
    let result = icrc::icrc1_transfer(
        &f.pic,
        f.ledger,
        f.reserve_owner,
        icrc::transfer_arg(
            Some(icrc::subaccount("protocol_reserve")),
            f.user.clone(),
            USER_TRANSFER_E8S,
            Some(icrc::FEE_E8S),
            Some(b"reserve-to-user"),
            Some(created_at_time),
        ),
    )
    .expect("reserve-to-user transfer should succeed");
    let block = result.0.to_str_radix(10).parse::<u64>().unwrap();
    assert_eq!(
        icrc::icrc1_balance_of(&f.pic, f.ledger, f.reserve.clone()),
        Nat::from(RESERVE_E8S - USER_TRANSFER_E8S - icrc::FEE_E8S)
    );
    assert_eq!(
        icrc::icrc1_balance_of(&f.pic, f.ledger, f.user.clone()),
        Nat::from(USER_TRANSFER_E8S)
    );
    assert_eq!(
        icrc::icrc1_total_supply(&f.pic, f.ledger),
        Nat::from(RESERVE_E8S + GOVERNANCE_E8S - icrc::FEE_E8S)
    );
    (block, created_at_time)
}

fn created_at_time(f: &LedgerIndexFixture, offset: u64) -> u64 {
    f.pic
        .get_time()
        .as_nanos_since_unix_epoch()
        .saturating_sub(CREATED_AT_OFFSET)
        .saturating_add(offset)
}

fn assert_index_has_transfer(f: &LedgerIndexFixture, block: u64, created_at_time: u64) {
    for _ in 0..200 {
        f.pic.advance_time(Duration::from_secs(1));
        f.pic.tick();
    }
    let reserve_history =
        icrc::get_account_transactions(&f.pic, f.index, f.reserve.clone(), None, 20)
            .expect("reserve account history should be readable");
    let user_history = icrc::get_account_transactions(&f.pic, f.index, f.user.clone(), None, 20)
        .expect("user account history should be readable");
    for history in [&reserve_history, &user_history] {
        let observed = history
            .transactions
            .iter()
            .find(|tx| tx.id == block)
            .and_then(|tx| tx.transaction.transfer.as_ref())
            .unwrap_or_else(|| {
                panic!(
                    "account history should include transfer block {block}; observed ids {:?}",
                    history
                        .transactions
                        .iter()
                        .map(|tx| tx.id.clone())
                        .collect::<Vec<_>>()
                )
            });
        assert_eq!(observed.from, f.reserve);
        assert_eq!(observed.to, f.user);
        assert_eq!(observed.amount, Nat::from(USER_TRANSFER_E8S));
        assert_eq!(observed.fee, Some(Nat::from(icrc::FEE_E8S)));
        assert_eq!(observed.memo.as_deref(), Some(&b"reserve-to-user"[..]));
        assert_eq!(observed.created_at_time, Some(created_at_time));
    }
}

fn assert_error_paths(f: &LedgerIndexFixture, duplicate_block: u64, created_at_time: u64) {
    let bad_fee = icrc::icrc1_transfer(
        &f.pic,
        f.ledger,
        f.reserve_owner,
        icrc::transfer_arg(
            Some(icrc::subaccount("protocol_reserve")),
            f.user.clone(),
            1,
            Some(1),
            Some(b"bad-fee"),
            Some(created_at_time + 1),
        ),
    )
    .unwrap_err();
    assert!(matches!(bad_fee, IcrcTransferError::BadFee { .. }));

    let insufficient = icrc::icrc1_transfer(
        &f.pic,
        f.ledger,
        f.user_owner,
        icrc::transfer_arg(
            None,
            f.reserve.clone(),
            USER_TRANSFER_E8S * 10,
            Some(icrc::FEE_E8S),
            Some(b"insufficient"),
            Some(created_at_time + 2),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        insufficient,
        IcrcTransferError::InsufficientFunds { .. }
    ));

    let duplicate = icrc::icrc1_transfer(
        &f.pic,
        f.ledger,
        f.reserve_owner,
        icrc::transfer_arg(
            Some(icrc::subaccount("protocol_reserve")),
            f.user.clone(),
            USER_TRANSFER_E8S,
            Some(icrc::FEE_E8S),
            Some(b"reserve-to-user"),
            Some(created_at_time),
        ),
    )
    .unwrap_err();
    match duplicate {
        IcrcTransferError::Duplicate { duplicate_of } => {
            assert_eq!(duplicate_of, Nat::from(duplicate_block));
        }
        other => panic!("expected duplicate transfer, got {other:?}"),
    }
}

pub fn run_ledger_index_smoke(required: bool) {
    let Some(fixture) = setup(required) else {
        return;
    };
    assert_metadata_and_initial_balances(&fixture);
    let (block, created_at_time) = transfer_reserve_to_user(&fixture);
    assert_index_has_transfer(&fixture, block, created_at_time);
    assert_error_paths(&fixture, block, created_at_time);
    for _ in 0..10 {
        fixture.pic.tick();
    }
    assert_index_has_transfer(&fixture, block, created_at_time);
}

pub fn run_ledger_index_same_wasm_upgrade(required: bool) {
    let Some(fixture) = setup(required) else {
        return;
    };
    let (block, created_at_time) = transfer_reserve_to_user(&fixture);
    assert_index_has_transfer(&fixture, block, created_at_time);
    pocketic_env::upgrade_canister(
        &fixture.pic,
        fixture.ledger,
        fixture.ledger_wasm.clone(),
        icrc::ledger_upgrade_arg(),
    );
    pocketic_env::upgrade_canister(
        &fixture.pic,
        fixture.index,
        fixture.index_wasm.clone(),
        icrc::index_upgrade_arg(),
    );
    for _ in 0..10 {
        fixture.pic.tick();
    }
    assert_eq!(
        icrc::icrc1_balance_of(&fixture.pic, fixture.ledger, fixture.user.clone()),
        Nat::from(USER_TRANSFER_E8S)
    );
    assert_index_has_transfer(&fixture, block, created_at_time);
    assert_error_paths(&fixture, block, created_at_time);
}

pub fn run_icrc2_direct_reserve_pull(required: bool) {
    let Some(fixture) = setup(required) else {
        return;
    };
    let standard_names = icrc::supported_standards(&fixture.pic, fixture.ledger)
        .into_iter()
        .map(|standard| standard.name)
        .collect::<std::collections::BTreeSet<_>>();
    for required_standard in ["ICRC-1", "ICRC-2", "ICRC-3"] {
        assert!(standard_names.contains(required_standard));
    }

    let (_funding_block, funding_time) = transfer_reserve_to_user(&fixture);
    let spender = Principal::from_slice(&[5; 29]);
    let spender_account = icrc::account(spender, None);
    let allowance_e8s = 20_000_000u64 + icrc::FEE_E8S;
    let expires_at = funding_time + 1_000_000_000;
    let approve_block = icrc::icrc2_approve(
        &fixture.pic,
        fixture.ledger,
        fixture.user_owner,
        icrc::ApproveArgs {
            from_subaccount: None,
            spender: spender_account.clone(),
            amount: Nat::from(allowance_e8s),
            expected_allowance: Some(Nat::from(0u8)),
            expires_at: Some(expires_at),
            fee: Some(Nat::from(icrc::FEE_E8S)),
            memo: Some(b"exact-short-lived-allowance".to_vec()),
            created_at_time: Some(funding_time + 1),
        },
    )
    .expect("real SNS ledger approval should succeed");
    let allowance = icrc::icrc2_allowance(
        &fixture.pic,
        fixture.ledger,
        icrc::AllowanceArgs {
            account: fixture.user.clone(),
            spender: spender_account,
        },
    );
    assert_eq!(allowance.allowance, Nat::from(allowance_e8s));
    assert_eq!(allowance.expires_at, Some(expires_at));

    let reserve_before =
        icrc::icrc1_balance_of(&fixture.pic, fixture.ledger, fixture.reserve.clone());
    let supply_before = icrc::icrc1_total_supply(&fixture.pic, fixture.ledger);
    let pull_amount = 20_000_000u64;
    let pull_block = icrc::icrc2_transfer_from(
        &fixture.pic,
        fixture.ledger,
        spender,
        icrc::TransferFromArgs {
            spender_subaccount: None,
            from: fixture.user.clone(),
            to: fixture.reserve.clone(),
            amount: Nat::from(pull_amount),
            fee: Some(Nat::from(icrc::FEE_E8S)),
            memo: Some(b"direct-reserve-redemption".to_vec()),
            created_at_time: Some(funding_time + 2),
        },
    )
    .expect("real SNS ledger transfer_from should succeed");
    assert!(pull_block > approve_block);
    assert_eq!(
        icrc::icrc1_balance_of(&fixture.pic, fixture.ledger, fixture.reserve.clone()),
        reserve_before + Nat::from(pull_amount)
    );
    assert_eq!(
        icrc::icrc1_total_supply(&fixture.pic, fixture.ledger),
        supply_before - Nat::from(icrc::FEE_E8S)
    );

    let changed = icrc::icrc2_approve(
        &fixture.pic,
        fixture.ledger,
        fixture.user_owner,
        icrc::ApproveArgs {
            from_subaccount: None,
            spender: icrc::account(spender, None),
            amount: Nat::from(1u8),
            expected_allowance: Some(Nat::from(1u8)),
            expires_at: Some(expires_at),
            fee: Some(Nat::from(icrc::FEE_E8S)),
            memo: None,
            created_at_time: Some(funding_time + 3),
        },
    )
    .unwrap_err();
    assert!(matches!(
        changed,
        icrc::ApproveError::AllowanceChanged { .. }
    ));
}

pub fn run_installed_stream_redemption(required: bool) {
    use candid::{decode_one, encode_one};
    use io_stream_manager::{
        Account, ApiError, InitArgs, Lifecycle, RedeemArgs, RedemptionProgress, Status,
        StreamConfig, StreamProgress,
    };

    let Some(artifacts) = maybe_artifacts(required) else {
        return;
    };
    if !pocketic_env::pocketic_available() {
        panic!("POCKET_IC_BIN is required for installed stream redemption");
    }
    let stream_wasm_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/debug/io_stream_manager.wasm");
    let stream_wasm = std::fs::read(&stream_wasm_path).unwrap_or_else(|error| {
        panic!(
            "build debug stream-manager Wasm before the installed real-ledger test ({}): {error}",
            stream_wasm_path.display()
        )
    });
    let ledger_wasm = artifacts.load_required("sns_ledger").unwrap();
    let pic = pocketic_env::new_pic_with_icp_sns_features();
    let stream = pocketic_env::create_empty_application_canister(&pic);
    let user = Principal::from_slice(&[21; 29]);
    let excluded_user = Principal::from_slice(&[25; 29]);
    let governance = Principal::from_slice(&[22; 29]);
    let nns_manager = Principal::from_slice(&[23; 29]);
    let minting = icrc::account(Principal::from_slice(&[24; 29]), None);
    let reserve_subaccount = icrc::subaccount("simplified-io-reserve");
    let liquid_subaccount = icrc::subaccount("simplified-liquid-icp");
    let reserve = icrc::account(stream, Some(reserve_subaccount));
    let liquid = icrc::account(stream, Some(liquid_subaccount));
    let user_account = icrc::account(user, None);
    let excluded_account = icrc::account(excluded_user, None);
    let io_reserve_e8s = 1_000_000_000_000u64;
    let user_io_e8s = 100_000_000u64;
    let liquid_icp_e8s = 1_000_000_000_000u64;
    let io_ledger = pocketic_env::create_sns_canister(
        &pic,
        ledger_wasm.clone(),
        icrc::ledger_init_arg(
            Principal::anonymous(),
            minting.clone(),
            vec![
                (reserve.clone(), io_reserve_e8s),
                (user_account.clone(), user_io_e8s),
                (excluded_account.clone(), user_io_e8s),
            ],
        ),
    );
    let icp_ledger = Principal::from_text(crate::nns_setup::install_nns_ledger().canister_id)
        .expect("official ICP ledger ID should parse");
    icrc::icrc1_transfer(
        &pic,
        icp_ledger,
        Principal::anonymous(),
        icrc::transfer_arg(
            None,
            liquid.clone(),
            liquid_icp_e8s,
            Some(icrc::FEE_E8S),
            Some(b"fund-stream-liquid"),
            None,
        ),
    )
    .expect("default ICP ledger account should fund stream liquid backing");
    let init = InitArgs {
        config: StreamConfig {
            io_ledger,
            icp_ledger,
            nns_manager,
            jupiter_io_account: Account {
                owner: nns_manager,
                subaccount: Some(vec![10; 32]),
            },
            sns_governance: governance,
            sns_root: Principal::from_slice(&[6; 29]),
            expected_sns_governance_module_hash: vec![0; 32],
            approved_reward_event_duration_seconds: 86_400,
            io_reserve: Account {
                owner: stream,
                subaccount: Some(reserve_subaccount.to_vec()),
            },
            liquid_icp: Account {
                owner: stream,
                subaccount: Some(liquid_subaccount.to_vec()),
            },
            nonredeemable_governance_io_accounts: vec![Account {
                owner: excluded_user,
                subaccount: None,
            }],
            minimum_redemption_io_e8s: 20_000,
            expected_io_fee_e8s: icrc::FEE_E8S as u128,
            expected_icp_fee_e8s: icrc::FEE_E8S as u128,
            maximum_request_lifetime_nanos: 900_000_000_000,
            retry_delay_nanos: 1_000_000_000,
            ledger_deduplication_window_nanos: 86_400_000_000_000,
        },
    };
    pic.install_canister(stream, stream_wasm.clone(), encode_one(init).unwrap(), None);
    let status: Status = decode_one(
        &pic.query_call(stream, user, "get_status", encode_one(()).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(status.lifecycle, Lifecycle::Paused);
    let unpause: Result<(), ApiError> = decode_one(
        &pic.update_call(stream, governance, "set_paused", encode_one(false).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(unpause, Ok(()));

    let now = pic.get_time().as_nanos_since_unix_epoch();
    let amount = 20_000_000u64;
    let rejected_excluded: Result<RedemptionProgress, ApiError> = decode_one(
        &pic.update_call(
            stream,
            excluded_user,
            "redeem",
            encode_one(RedeemArgs {
                from_subaccount: None,
                io_amount_e8s: amount as u128,
                min_icp_out_e8s: 0,
                max_io_fee_e8s: icrc::FEE_E8S as u128,
                max_icp_fee_e8s: icrc::FEE_E8S as u128,
                expires_at_nanos: now + 800_000_000_000,
                nonce: 0,
            })
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        rejected_excluded,
        Err(ApiError::Invalid("excluded account cannot redeem".into()))
    );
    let rejected_reserve: Result<RedemptionProgress, ApiError> = decode_one(
        &pic.update_call(
            stream,
            stream,
            "redeem",
            encode_one(RedeemArgs {
                from_subaccount: Some(reserve_subaccount.to_vec()),
                io_amount_e8s: amount as u128,
                min_icp_out_e8s: 0,
                max_io_fee_e8s: icrc::FEE_E8S as u128,
                max_icp_fee_e8s: icrc::FEE_E8S as u128,
                expires_at_nanos: now + 800_000_000_000,
                nonce: 0,
            })
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        rejected_reserve,
        Err(ApiError::Invalid("reserve account cannot redeem".into()))
    );
    icrc::icrc2_approve(
        &pic,
        io_ledger,
        user,
        icrc::ApproveArgs {
            from_subaccount: None,
            spender: icrc::account(stream, None),
            amount: Nat::from(amount + icrc::FEE_E8S),
            expected_allowance: Some(Nat::from(0u8)),
            expires_at: Some(now + 800_000_000_000),
            fee: Some(Nat::from(icrc::FEE_E8S)),
            memo: Some(b"stream-redemption-approval".to_vec()),
            created_at_time: Some(now),
        },
    )
    .expect("approval should succeed");
    let supply_before: u128 = icrc::icrc1_total_supply(&pic, io_ledger)
        .0
        .try_into()
        .unwrap();
    let reserve_before: u128 = icrc::icrc1_balance_of(&pic, io_ledger, reserve.clone())
        .0
        .try_into()
        .unwrap();
    let liquid_before: u128 = icrc::icrc1_balance_of(&pic, icp_ledger, liquid.clone())
        .0
        .try_into()
        .unwrap();
    let quote = io_core_model::redemption_quote(
        io_core_model::EconomicState {
            backing: io_core_model::Backing {
                liquid: liquid_before,
                ..Default::default()
            },
            claims: supply_before - reserve_before - user_io_e8s as u128,
            active_backing: 0,
            active_reward: 0,
        },
        amount as u128,
        icrc::FEE_E8S as u128,
        icrc::FEE_E8S as u128,
    )
    .unwrap();
    let args = RedeemArgs {
        from_subaccount: None,
        io_amount_e8s: amount as u128,
        min_icp_out_e8s: quote.net_icp,
        max_io_fee_e8s: icrc::FEE_E8S as u128,
        max_icp_fee_e8s: icrc::FEE_E8S as u128,
        expires_at_nanos: now + 800_000_000_000,
        nonce: 0,
    };
    let pulled: Result<RedemptionProgress, ApiError> = decode_one(
        &pic.update_call(stream, user, "redeem", encode_one(args.clone()).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        pulled,
        Ok(RedemptionProgress::Pending | RedemptionProgress::Completed(_))
    ));
    assert_eq!(
        icrc::icrc1_balance_of(&pic, io_ledger, reserve.clone()),
        Nat::from(io_reserve_e8s + amount),
    );
    assert_eq!(
        icrc::icrc1_total_supply(&pic, io_ledger),
        Nat::from(supply_before - icrc::FEE_E8S as u128)
    );
    assert_eq!(
        icrc::icrc1_balance_of(&pic, icp_ledger, user_account.clone()),
        Nat::from(0u8)
    );
    pocketic_env::upgrade_canister(&pic, stream, stream_wasm.clone(), encode_one(()).unwrap());
    let paused_after_pull_upgrade: Status = decode_one(
        &pic.query_call(stream, user, "get_status", encode_one(()).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(paused_after_pull_upgrade.lifecycle, Lifecycle::Paused);
    pic.advance_time(Duration::from_secs(2 * 60 * 60));
    assert!(pic.get_time().as_nanos_since_unix_epoch() > args.expires_at_nanos);

    let paid: Result<StreamProgress, ApiError> = decode_one(
        &pic.update_call(
            stream,
            Principal::anonymous(),
            "resume",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        paid,
        Ok(StreamProgress::Redemption(RedemptionProgress::Pending))
    );
    assert_eq!(
        icrc::icrc1_balance_of(&pic, icp_ledger, user_account),
        Nat::from(quote.net_icp)
    );
    assert_eq!(
        icrc::icrc1_balance_of(&pic, icp_ledger, liquid.clone()),
        Nat::from(liquid_before - quote.gross_icp)
    );
    pocketic_env::upgrade_canister(&pic, stream, stream_wasm.clone(), encode_one(()).unwrap());
    let paused_after_payout_upgrade: Status = decode_one(
        &pic.query_call(stream, user, "get_status", encode_one(()).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(paused_after_payout_upgrade.lifecycle, Lifecycle::Paused);
    let completed: Result<StreamProgress, ApiError> = decode_one(
        &pic.update_call(
            stream,
            Principal::anonymous(),
            "resume",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let result = match completed {
        Ok(StreamProgress::Redemption(RedemptionProgress::Completed(result))) => result,
        other => panic!("expected completion, got {other:?}"),
    };
    assert_eq!(result.gross_icp_e8s, quote.gross_icp);
    assert_eq!(result.net_icp_e8s, quote.net_icp);
    let replay: Result<RedemptionProgress, ApiError> = decode_one(
        &pic.update_call(stream, user, "redeem", encode_one(args.clone()).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(replay, Ok(RedemptionProgress::Completed(result)));
    let zero_subaccount_replay: Result<RedemptionProgress, ApiError> = decode_one(
        &pic.update_call(
            stream,
            user,
            "redeem",
            encode_one(RedeemArgs {
                from_subaccount: Some(vec![0; 32]),
                ..args.clone()
            })
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(zero_subaccount_replay, replay);
    let liquid_after_completion = icrc::icrc1_balance_of(&pic, icp_ledger, liquid.clone());
    let idle: Result<StreamProgress, ApiError> = decode_one(
        &pic.update_call(
            stream,
            Principal::anonymous(),
            "resume",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(idle, Ok(StreamProgress::Idle));
    assert_eq!(
        icrc::icrc1_balance_of(
            &pic,
            icp_ledger,
            icrc::account(stream, Some(liquid_subaccount))
        ),
        liquid_after_completion
    );
    let conflict: Result<RedemptionProgress, ApiError> = decode_one(
        &pic.update_call(
            stream,
            user,
            "redeem",
            encode_one(RedeemArgs {
                min_icp_out_e8s: args.min_icp_out_e8s.saturating_sub(1),
                ..args
            })
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(conflict, Err(ApiError::NonceAlreadyUsed));
    // Jupiter receipt replay is exercised by the installed NNS/Stream harness,
    // where the receipt can bind to an exact NNS claim-backing fingerprint.
}
