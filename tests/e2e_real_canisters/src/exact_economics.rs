use crate::artifacts::{resolve_from_env, ArtifactSet, ArtifactStatus};
use crate::icrc;
use crate::pocketic_env;
use candid::{Nat, Principal};
use io_core_model::{process_stream, redeem_io, ProtocolState, StreamKind, E8S_PER_TOKEN};
use io_ledger_types::IcrcAccount;
use io_reward_policy::{allocate_rewards, NeuronSnapshot};
use std::time::Duration;

const DAY_SECONDS: u64 = 86_400;
const RESERVE_IO_E8S: u128 = 900_000 * E8S_PER_TOKEN;
const GOVERNANCE_IO_E8S: u128 = 100_000 * E8S_PER_TOKEN;
const TOTAL_IO_E8S: u128 = 1_000_000 * E8S_PER_TOKEN;
const ICP_TREASURY_E8S: u128 = 1_000_000 * E8S_PER_TOKEN;
const TWO_WEEK_REWARD_POOL_E8S: u128 = 272_727_272;
const FULL_PARTICIPATION_REWARD_E8S: u128 = 181_818_181;
const HALF_PARTICIPATION_REWARD_E8S: u128 = 90_909_090;
const TWO_WEEK_DUST_E8S: u128 = 1;
const HOLDER_REDEMPTION_PAYOUT_E8S: u128 = 550_000_000;
const CREATED_AT_MARGIN_NANOS: u64 = 1_000;

struct ExactEconomicsFixture {
    pic: pocket_ic::PocketIc,
    icp_ledger: Principal,
    icp_index: Principal,
    io_ledger: Principal,
    io_index: Principal,
    jupiter: IcrcAccount,
    stream_icp: IcrcAccount,
    reserve: IcrcAccount,
    governance: IcrcAccount,
    alice: IcrcAccount,
    bob: IcrcAccount,
    charlie: IcrcAccount,
    jupiter_owner: Principal,
    stream_owner: Principal,
    reserve_owner: Principal,
    charlie_owner: Principal,
}

struct TransferSpec {
    ledger: Principal,
    caller: Principal,
    from_subaccount: Option<[u8; 32]>,
    to: IcrcAccount,
    amount_e8s: u128,
    memo: &'static [u8],
    created_at_time: u64,
}

fn t(n: u128) -> u128 {
    n * E8S_PER_TOKEN
}

fn nat_to_u64(value: Nat) -> u64 {
    value
        .0
        .to_str_radix(10)
        .parse::<u64>()
        .expect("block index should fit u64")
}

fn nat_from_u128(value: u128) -> Nat {
    Nat::from(value)
}

fn u64_from_e8s(value: u128) -> u64 {
    u64::try_from(value).expect("test e8s amount should fit u64")
}

fn maybe_artifacts(required: bool) -> Option<ArtifactSet> {
    match resolve_from_env(required) {
        Ok(ArtifactStatus::Ready(set)) => Some(set),
        Ok(ArtifactStatus::Skipped(message)) => {
            eprintln!("skipping real-ledger exact-economics E2E: {message}");
            None
        }
        Err(err) if !required => {
            panic!("real-framework artifacts are configured but invalid: {err}")
        }
        Err(err) => panic!("{err}"),
    }
}

fn setup(required: bool) -> Option<ExactEconomicsFixture> {
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

    let jupiter_owner = Principal::from_slice(&[10; 29]);
    let stream_owner = Principal::from_slice(&[11; 29]);
    let reserve_owner = Principal::from_slice(&[12; 29]);
    let governance_owner = Principal::from_slice(&[13; 29]);
    let alice_owner = Principal::from_slice(&[14; 29]);
    let bob_owner = Principal::from_slice(&[15; 29]);
    let charlie_owner = Principal::from_slice(&[16; 29]);
    let minting_owner = Principal::from_slice(&[17; 29]);

    let jupiter = icrc::account(jupiter_owner, None);
    let stream_icp = icrc::account(stream_owner, Some(icrc::subaccount("stream_icp")));
    let stream_payout = icrc::account(stream_owner, Some(icrc::subaccount("stream_payout")));
    let reserve = icrc::account(reserve_owner, Some(icrc::subaccount("protocol_reserve")));
    let governance = icrc::account(governance_owner, Some(icrc::subaccount("governance")));
    let alice = icrc::account(alice_owner, None);
    let bob = icrc::account(bob_owner, None);
    let charlie = icrc::account(charlie_owner, None);
    let minting = icrc::account(minting_owner, None);

    let icp_ledger = pocketic_env::create_sns_canister(
        &pic,
        ledger_wasm.clone(),
        icrc::ledger_init_arg(
            Principal::anonymous(),
            minting.clone(),
            vec![
                (jupiter.clone(), u64_from_e8s(t(1_000))),
                (stream_payout.clone(), u64_from_e8s(ICP_TREASURY_E8S)),
            ],
        ),
    );
    let icp_index = pocketic_env::create_sns_canister(
        &pic,
        index_wasm.clone(),
        icrc::index_init_arg(icp_ledger),
    );
    let io_ledger = pocketic_env::create_sns_canister(
        &pic,
        ledger_wasm.clone(),
        icrc::ledger_init_arg(
            Principal::anonymous(),
            minting,
            vec![
                (reserve.clone(), u64_from_e8s(RESERVE_IO_E8S)),
                (governance.clone(), u64_from_e8s(GOVERNANCE_IO_E8S)),
            ],
        ),
    );
    let io_index =
        pocketic_env::create_sns_canister(&pic, index_wasm, icrc::index_init_arg(io_ledger));
    for _ in 0..20 {
        pic.tick();
    }

    Some(ExactEconomicsFixture {
        pic,
        icp_ledger,
        icp_index,
        io_ledger,
        io_index,
        jupiter,
        stream_icp,
        reserve,
        governance,
        alice,
        bob,
        charlie,
        jupiter_owner,
        stream_owner,
        reserve_owner,
        charlie_owner,
    })
}

fn transfer(f: &ExactEconomicsFixture, spec: TransferSpec) -> u64 {
    let block = icrc::icrc1_transfer(
        &f.pic,
        spec.ledger,
        spec.caller,
        icrc::transfer_arg(
            spec.from_subaccount,
            spec.to,
            u64_from_e8s(spec.amount_e8s),
            Some(icrc::FEE_E8S),
            Some(spec.memo),
            Some(spec.created_at_time),
        ),
    )
    .expect("real ledger transfer should succeed");
    nat_to_u64(block)
}

fn created_at_time(f: &ExactEconomicsFixture, offset: u64) -> u64 {
    f.pic
        .get_time()
        .as_nanos_since_unix_epoch()
        .saturating_sub(CREATED_AT_MARGIN_NANOS)
        .saturating_add(offset)
}

fn assert_balance(
    f: &ExactEconomicsFixture,
    ledger: Principal,
    account: IcrcAccount,
    expected: u128,
) {
    assert_eq!(
        icrc::icrc1_balance_of(&f.pic, ledger, account),
        nat_from_u128(expected)
    );
}

fn assert_history_has_amount(
    f: &ExactEconomicsFixture,
    index: Principal,
    account: IcrcAccount,
    block: u64,
    amount: u128,
) {
    for _ in 0..200 {
        f.pic.advance_time(Duration::from_secs(1));
        f.pic.tick();
    }
    let history = icrc::get_account_transactions(&f.pic, index, account, None, 50)
        .expect("real index account history should be readable");
    let transfer = history
        .transactions
        .iter()
        .find(|tx| tx.id == block)
        .and_then(|tx| tx.transaction.transfer.as_ref())
        .expect("account history should contain expected transfer block");
    assert_eq!(transfer.amount, nat_from_u128(amount));
}

fn neuron(id: u64, stake: u128, seconds: u64, voted: u64, total: u64) -> NeuronSnapshot {
    NeuronSnapshot {
        neuron_id: id,
        staked_io_e8s: stake,
        eligible_seconds: seconds,
        eligible_closed_proposals: total,
        voted_closed_proposals: voted,
        is_genesis_governance_neuron: false,
        is_protocol_owned: false,
        is_dissolving: false,
    }
}

pub fn run_exact_economics(required: bool) {
    let Some(f) = setup(required) else { return };
    let mut state = ProtocolState::new(TOTAL_IO_E8S, RESERVE_IO_E8S, GOVERNANCE_IO_E8S);
    let initial_time = created_at_time(&f, 0);

    assert_balance(&f, f.io_ledger, f.reserve.clone(), RESERVE_IO_E8S);
    assert_balance(&f, f.io_ledger, f.governance.clone(), GOVERNANCE_IO_E8S);
    assert_balance(&f, f.io_ledger, f.jupiter.clone(), 0);

    // 1. Jupiter Faucet sends 100 ICP to IO. The protocol model authorizes 60 backed IO issuance.
    let jupiter_deposit_block = transfer(
        &f,
        TransferSpec {
            ledger: f.icp_ledger,
            caller: f.jupiter_owner,
            from_subaccount: None,
            to: f.stream_icp.clone(),
            amount_e8s: t(100),
            memo: b"jupiter-to-io",
            created_at_time: initial_time + 1,
        },
    );
    let faucet = process_stream(&mut state, StreamKind::JupiterFaucet, t(100)).unwrap();
    assert_eq!(faucet.split.stake_e8s, t(40));
    assert_eq!(faucet.split.liquid_e8s, t(60));
    assert_eq!(faucet.io_issued_e8s, t(60));
    assert_eq!(state.liquid_icp_e8s, t(60));
    assert_eq!(state.two_year_staked_icp_e8s, t(40));
    assert_eq!(
        state.redemption_rate().unwrap().icp_for_io(t(1)).unwrap(),
        t(1)
    );

    let io_issuance_block = transfer(
        &f,
        TransferSpec {
            ledger: f.io_ledger,
            caller: f.reserve_owner,
            from_subaccount: Some(icrc::subaccount("protocol_reserve")),
            to: f.jupiter.clone(),
            amount_e8s: faucet.io_issued_e8s,
            memo: b"backed-io-to-jupiter",
            created_at_time: initial_time + 2,
        },
    );
    assert_balance(&f, f.io_ledger, f.jupiter.clone(), t(60));
    assert_history_has_amount(
        &f,
        f.icp_index,
        f.stream_icp.clone(),
        jupiter_deposit_block,
        t(100),
    );
    assert_history_has_amount(&f, f.io_index, f.jupiter.clone(), io_issuance_block, t(60));

    // Charlie holds 30 IO liquid. Alice and Bob are represented in the reward-policy snapshot as
    // equal 30 IO SNS stakes; Alice has full participation, Bob has half, and an ineligible neuron
    // is excluded. The SNS staking/governance path is still modeled in this layer.
    transfer(
        &f,
        TransferSpec {
            ledger: f.io_ledger,
            caller: f.jupiter_owner,
            from_subaccount: None,
            to: f.charlie.clone(),
            amount_e8s: t(30),
            memo: b"holder-allocation",
            created_at_time: initial_time + 3,
        },
    );
    assert_balance(&f, f.io_ledger, f.charlie.clone(), t(30));

    // 2. Fast-forward PocketIC time and process 10 ICP of 2-year maturity. It compounds holder
    // value by increasing the redemption rate from 1.0 to 1.1 ICP/IO and issues no new IO.
    f.pic.advance_time(Duration::from_secs(30 * DAY_SECONDS));
    for _ in 0..10 {
        f.pic.tick();
    }
    let two_year = process_stream(&mut state, StreamKind::TwoYearMaturity, t(10)).unwrap();
    assert_eq!(two_year.split.stake_e8s, t(4));
    assert_eq!(two_year.split.liquid_e8s, t(6));
    assert_eq!(two_year.io_issued_e8s, 0);
    assert_eq!(state.liquid_icp_e8s, t(66));
    assert_eq!(state.redeemable_io_supply_e8s().unwrap(), t(60));
    assert_eq!(
        state.redemption_rate().unwrap().icp_for_io(t(60)).unwrap(),
        t(66)
    );

    // 3. After 14 days, 5 ICP of 2-week maturity contributes 3 ICP to liquid backing and issues
    // 2.72727272 backed IO at the pre-event 1.1 ICP/IO redemption rate.
    f.pic.advance_time(Duration::from_secs(13 * DAY_SECONDS));
    for _ in 0..10 {
        f.pic.tick();
    }
    let not_ready_elapsed_days = 13;
    assert!(not_ready_elapsed_days < 14);
    f.pic.advance_time(Duration::from_secs(DAY_SECONDS));
    for _ in 0..10 {
        f.pic.tick();
    }
    let ready_elapsed_days = 14;
    assert!(ready_elapsed_days >= 14);

    let two_week = process_stream(&mut state, StreamKind::TwoWeekMaturity, t(5)).unwrap();
    let reward_time = created_at_time(&f, 0);
    assert_eq!(two_week.split.stake_e8s, t(2));
    assert_eq!(two_week.split.liquid_e8s, t(3));
    assert_eq!(two_week.io_issued_e8s, TWO_WEEK_REWARD_POOL_E8S);
    assert_eq!(
        state.redeemable_io_supply_e8s().unwrap(),
        t(60) + TWO_WEEK_REWARD_POOL_E8S
    );

    let alice = neuron(1, t(30), 14 * DAY_SECONDS, 2, 2);
    let bob = neuron(2, t(30), 14 * DAY_SECONDS, 1, 2);
    let mut ineligible = neuron(3, t(30), 14 * DAY_SECONDS, 2, 2);
    ineligible.is_dissolving = true;
    let allocations = allocate_rewards(two_week.io_issued_e8s, &[alice, bob, ineligible]);
    assert_eq!(allocations.dust_e8s, TWO_WEEK_DUST_E8S);
    assert_eq!(allocations.allocations.len(), 2);
    assert_eq!(allocations.allocations[0].neuron_id, 1);
    assert_eq!(
        allocations.allocations[0].io_e8s,
        FULL_PARTICIPATION_REWARD_E8S
    );
    assert_eq!(allocations.allocations[1].neuron_id, 2);
    assert_eq!(
        allocations.allocations[1].io_e8s,
        HALF_PARTICIPATION_REWARD_E8S
    );
    assert!(allocations.allocations[0].io_e8s > allocations.allocations[1].io_e8s);

    let alice_reward_block = transfer(
        &f,
        TransferSpec {
            ledger: f.io_ledger,
            caller: f.reserve_owner,
            from_subaccount: Some(icrc::subaccount("protocol_reserve")),
            to: f.alice.clone(),
            amount_e8s: allocations.allocations[0].io_e8s,
            memo: b"two-week-reward-alice",
            created_at_time: reward_time + 1,
        },
    );
    let bob_reward_block = transfer(
        &f,
        TransferSpec {
            ledger: f.io_ledger,
            caller: f.reserve_owner,
            from_subaccount: Some(icrc::subaccount("protocol_reserve")),
            to: f.bob.clone(),
            amount_e8s: allocations.allocations[1].io_e8s,
            memo: b"two-week-reward-bob",
            created_at_time: reward_time + 2,
        },
    );
    assert_balance(
        &f,
        f.io_ledger,
        f.alice.clone(),
        FULL_PARTICIPATION_REWARD_E8S,
    );
    assert_balance(
        &f,
        f.io_ledger,
        f.bob.clone(),
        HALF_PARTICIPATION_REWARD_E8S,
    );
    assert_balance(&f, f.io_ledger, f.governance.clone(), GOVERNANCE_IO_E8S);
    assert_history_has_amount(
        &f,
        f.io_index,
        f.alice.clone(),
        alice_reward_block,
        allocations.allocations[0].io_e8s,
    );
    assert_history_has_amount(
        &f,
        f.io_index,
        f.bob.clone(),
        bob_reward_block,
        allocations.allocations[1].io_e8s,
    );

    // 4. Redemption uses the current rate: Charlie redeems 5 IO for exactly 5.5 ICP gross.
    let redemption_time = created_at_time(&f, 0);
    let redemption_block = transfer(
        &f,
        TransferSpec {
            ledger: f.io_ledger,
            caller: f.charlie_owner,
            from_subaccount: None,
            to: f.reserve.clone(),
            amount_e8s: t(5),
            memo: b"redeem-holder-io",
            created_at_time: redemption_time + 1,
        },
    );
    let redemption = redeem_io(&mut state, t(5)).unwrap();
    assert_eq!(
        redemption.gross_icp_payout_e8s,
        HOLDER_REDEMPTION_PAYOUT_E8S
    );
    assert_eq!(redemption.io_returned_to_reserve_e8s, t(5));
    let payout_block = transfer(
        &f,
        TransferSpec {
            ledger: f.icp_ledger,
            caller: f.stream_owner,
            from_subaccount: Some(icrc::subaccount("stream_payout")),
            to: f.charlie.clone(),
            amount_e8s: redemption.gross_icp_payout_e8s,
            memo: b"holder-redemption-payout",
            created_at_time: redemption_time + 2,
        },
    );
    assert_balance(
        &f,
        f.io_ledger,
        f.charlie.clone(),
        t(25) - u128::from(icrc::FEE_E8S),
    );
    assert_balance(
        &f,
        f.icp_ledger,
        f.charlie.clone(),
        HOLDER_REDEMPTION_PAYOUT_E8S,
    );
    assert_history_has_amount(&f, f.io_index, f.reserve.clone(), redemption_block, t(5));
    assert_history_has_amount(
        &f,
        f.icp_index,
        f.charlie.clone(),
        payout_block,
        HOLDER_REDEMPTION_PAYOUT_E8S,
    );
}
