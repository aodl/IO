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
