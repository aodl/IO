use io_core_model::{StreamKind, E8S_PER_TOKEN};
use io_reward_policy::NeuronSnapshot;
use io_stream_manager::state::{
    IO_NNS_NEURON_MANAGER_SOURCE, JUPITER_FAUCET_SOURCE, TWO_WEEK_MATURITY_MEMO,
    TWO_YEAR_MATURITY_MEMO,
};
use io_stream_manager::{ModelError, StreamManager, StreamManagerError};

fn t(n: u128) -> u128 {
    n * E8S_PER_TOKEN
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct DebugMintArgs {
    to: String,
    amount_e8s: u128,
    memo: String,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct DebugRejectAccountArgs {
    account: String,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct DebugLagArgs {
    lag_blocks: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct DebugArchiveRequiredArgs {
    archive_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct LedgerTransaction {
    from: String,
    to: String,
    amount_e8s: u128,
    memo: String,
    block_index: u64,
    timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct MockSnsNeuron {
    neuron_id: u64,
    staked_io_e8s: u128,
    eligible_seconds: u64,
    eligible_closed_proposals: u64,
    voted_closed_proposals: u64,
    is_genesis_governance_neuron: bool,
    is_protocol_owned: bool,
    is_dissolving: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct IndexInitArgs {
    ledger_principal_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct FaucetSendArgs {
    ledger_principal_text: String,
    from: String,
    to: String,
    amount_e8s: u128,
    memo: String,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct DebugTickOutcome {
    scanned_icp_transactions: u64,
    scanned_io_transactions: u64,
    processed_authorized_streams: u64,
    processed_redemptions: u64,
    io_issued_e8s: u128,
    icp_paid_e8s: u128,
    errors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct NnsDebugTickOutcome {
    disbursed_two_year_maturity_e8s: u128,
    disbursed_two_week_maturity_e8s: u128,
    disbursed_unwind_principal_e8s: u128,
    planned_pool_rebalances: u64,
    errors: Vec<String>,
}

fn pocketic_available() -> bool {
    std::env::var_os("POCKET_IC_BIN").is_some()
}

fn wasm(path: &str) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

#[cfg(test)]
mod live {
    use super::*;
    use candid::{decode_one, encode_one, Nat, Principal};
    use io_ledger_types::{
        map_icrc_transfer_result, Account, IcrcAccount, IcrcTransferArg, IcrcTransferError,
        Subaccount,
    };
    use pocket_ic::PocketIc;

    const CYCLES: u128 = 2_000_000_000_000;

    struct StreamFixture {
        pic: PocketIc,
        stream: Principal,
        stream_wasm: Vec<u8>,
        icp_ledger: Principal,
        icp_index: Principal,
        io_ledger: Principal,
        io_index: Principal,
        jupiter_faucet: Principal,
        sns_governance: Option<Principal>,
    }

    fn required_wasm(path: &str) -> Option<Vec<u8>> {
        match wasm(path) {
            Some(bytes) => Some(bytes),
            None => {
                eprintln!("skipping real PocketIC test because {path} is missing");
                None
            }
        }
    }

    fn create_canister(pic: &PocketIc, wasm: Vec<u8>, arg: Vec<u8>) -> Principal {
        let canister = pic.create_canister();
        pic.add_cycles(canister, CYCLES);
        pic.install_canister(canister, wasm, arg, None);
        canister
    }

    fn setup_stream(with_sns: bool) -> Option<StreamFixture> {
        setup_stream_with_payout_ledger(with_sns, true)
    }

    fn setup_stream_with_payout_ledger(
        with_sns: bool,
        configure_icp_payout_ledger: bool,
    ) -> Option<StreamFixture> {
        if !pocketic_available() {
            eprintln!("skipping real PocketIC test because POCKET_IC_BIN is not set");
            return None;
        }

        let stream_wasm =
            required_wasm("target/wasm32-unknown-unknown/debug/io_stream_manager.wasm")?;
        let icp_ledger_wasm =
            required_wasm("target/wasm32-unknown-unknown/debug/mock_icp_ledger.wasm")?;
        let io_ledger_wasm =
            required_wasm("target/wasm32-unknown-unknown/debug/mock_io_ledger.wasm")?;
        let icp_index_wasm =
            required_wasm("target/wasm32-unknown-unknown/debug/mock_icp_index.wasm")?;
        let io_index_wasm =
            required_wasm("target/wasm32-unknown-unknown/debug/mock_io_index.wasm")?;
        let faucet_wasm =
            required_wasm("target/wasm32-unknown-unknown/debug/mock_jupiter_faucet.wasm")?;
        let sns_wasm = if with_sns {
            Some(required_wasm(
                "target/wasm32-unknown-unknown/debug/mock_sns_governance.wasm",
            )?)
        } else {
            None
        };

        let pic = PocketIc::new();
        let icp_ledger = create_canister(&pic, icp_ledger_wasm, vec![]);
        let io_ledger = create_canister(&pic, io_ledger_wasm, vec![]);
        let icp_index = create_canister(
            &pic,
            icp_index_wasm,
            encode_one(IndexInitArgs {
                ledger_principal_text: Some(icp_ledger.to_text()),
            })
            .unwrap(),
        );
        let io_index = create_canister(
            &pic,
            io_index_wasm,
            encode_one(IndexInitArgs {
                ledger_principal_text: Some(io_ledger.to_text()),
            })
            .unwrap(),
        );
        let jupiter_faucet = create_canister(&pic, faucet_wasm, vec![]);
        let sns_governance = sns_wasm.map(|wasm| create_canister(&pic, wasm, vec![]));
        let stream_args = io_stream_manager::InitArgs {
            icp_ledger_principal_text: configure_icp_payout_ledger.then(|| icp_ledger.to_text()),
            icp_index_principal_text: Some(icp_index.to_text()),
            io_ledger_principal_text: Some(io_ledger.to_text()),
            io_index_principal_text: Some(io_index.to_text()),
            sns_governance_principal_text: sns_governance.map(|p| p.to_text()),
            ..Default::default()
        };
        let stream = create_canister(&pic, stream_wasm.clone(), encode_one(stream_args).unwrap());
        mint(
            &pic,
            io_ledger,
            "protocol_reserve",
            t(900_000),
            "initial_reserve",
        );

        Some(StreamFixture {
            pic,
            stream,
            stream_wasm,
            icp_ledger,
            icp_index,
            io_ledger,
            io_index,
            jupiter_faucet,
            sns_governance,
        })
    }

    fn mint(pic: &PocketIc, ledger: Principal, to: &str, amount_e8s: u128, memo: &str) -> u64 {
        let bytes = pic
            .update_call(
                ledger,
                Principal::anonymous(),
                "debug_mint",
                encode_one(DebugMintArgs {
                    to: to.to_string(),
                    amount_e8s,
                    memo: memo.to_string(),
                })
                .unwrap(),
            )
            .expect("mint");
        decode_one::<u64>(&bytes).unwrap()
    }

    fn transfer(
        pic: &PocketIc,
        ledger: Principal,
        from: &str,
        to: &str,
        amount_e8s: u128,
        memo: &str,
    ) -> u64 {
        let bytes = pic
            .update_call(
                ledger,
                Principal::anonymous(),
                "icrc1_transfer",
                encode_one(IcrcTransferArg {
                    from_subaccount: Some(mock_subaccount(from).0.to_vec()),
                    to: mock_account(to).into(),
                    amount: Nat::from(amount_e8s),
                    fee: None,
                    memo: Some(memo.as_bytes().to_vec()),
                    created_at_time: None,
                })
                .unwrap(),
            )
            .expect("transfer");
        map_icrc_transfer_result(decode_one::<Result<Nat, IcrcTransferError>>(&bytes).unwrap())
            .expect("ledger transfer result")
            .block_index
            .0
    }

    fn balance(pic: &PocketIc, ledger: Principal, account: &str) -> u128 {
        let bytes = pic
            .query_call(
                ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                encode_one(IcrcAccount::from(mock_account(account))).unwrap(),
            )
            .expect("balance");
        decode_one::<Nat>(&bytes)
            .unwrap()
            .0
            .to_str_radix(10)
            .parse()
            .unwrap()
    }

    fn mock_subaccount(label: &str) -> Subaccount {
        let bytes = label.as_bytes();
        let mut subaccount = [0; 32];
        let len = bytes.len().min(31);
        subaccount[0] = len as u8;
        subaccount[1..=len].copy_from_slice(&bytes[..len]);
        Subaccount(subaccount)
    }

    fn mock_account(label: &str) -> Account {
        Account::new(Principal::anonymous(), Some(mock_subaccount(label)))
    }

    fn transactions(pic: &PocketIc, ledger: Principal) -> Vec<LedgerTransaction> {
        let bytes = pic
            .query_call(
                ledger,
                Principal::anonymous(),
                "debug_get_transactions",
                encode_one(()).unwrap(),
            )
            .expect("transactions");
        decode_one::<Vec<LedgerTransaction>>(&bytes).unwrap()
    }

    fn reject_to(pic: &PocketIc, ledger: Principal, account: &str) {
        pic.update_call(
            ledger,
            Principal::anonymous(),
            "debug_reject_to",
            encode_one(DebugRejectAccountArgs {
                account: account.to_string(),
            })
            .unwrap(),
        )
        .expect("reject account");
    }

    fn clear_rejections(pic: &PocketIc, ledger: Principal) {
        pic.update_call(
            ledger,
            Principal::anonymous(),
            "debug_clear_rejections",
            encode_one(()).unwrap(),
        )
        .expect("clear rejections");
    }

    fn set_index_lag(pic: &PocketIc, index: Principal, lag_blocks: u64) {
        pic.update_call(
            index,
            Principal::anonymous(),
            "debug_set_lag",
            encode_one(DebugLagArgs { lag_blocks }).unwrap(),
        )
        .expect("set index lag");
    }

    fn set_archive_required(pic: &PocketIc, index: Principal, archive_required: bool) {
        pic.update_call(
            index,
            Principal::anonymous(),
            "debug_set_archive_required",
            encode_one(DebugArchiveRequiredArgs { archive_required }).unwrap(),
        )
        .expect("set archive required");
    }

    fn tick(fixture: &StreamFixture) -> DebugTickOutcome {
        let bytes = fixture
            .pic
            .update_call(
                fixture.stream,
                Principal::anonymous(),
                "debug_tick",
                encode_one(()).unwrap(),
            )
            .expect("stream tick");
        decode_one::<DebugTickOutcome>(&bytes).unwrap()
    }

    fn upgrade_stream(fixture: &StreamFixture) {
        fixture
            .pic
            .upgrade_canister(fixture.stream, fixture.stream_wasm.clone(), vec![], None)
            .expect("upgrade stream manager");
    }

    fn faucet_send(
        fixture: &StreamFixture,
        from: &str,
        to: &str,
        amount_e8s: u128,
        memo: &str,
    ) -> u64 {
        let bytes = fixture
            .pic
            .update_call(
                fixture.jupiter_faucet,
                Principal::anonymous(),
                "debug_send_icp",
                encode_one(FaucetSendArgs {
                    ledger_principal_text: fixture.icp_ledger.to_text(),
                    from: from.to_string(),
                    to: to.to_string(),
                    amount_e8s,
                    memo: memo.to_string(),
                })
                .unwrap(),
            )
            .expect("faucet send");
        decode_one::<Result<u64, String>>(&bytes)
            .unwrap()
            .expect("faucet send result")
    }

    fn state(fixture: &StreamFixture) -> io_stream_manager::ApiState {
        let bytes = fixture
            .pic
            .query_call(
                fixture.stream,
                Principal::anonymous(),
                "debug_get_state",
                encode_one(()).unwrap(),
            )
            .expect("state");
        decode_one::<io_stream_manager::ApiState>(&bytes).unwrap()
    }

    fn process_stream_event(
        fixture: &StreamFixture,
        kind: io_stream_manager::ApiStreamKind,
        amount_e8s: u128,
        transaction_id: &str,
    ) {
        fixture
            .pic
            .update_call(
                fixture.stream,
                Principal::anonymous(),
                "debug_process_stream_event",
                encode_one(io_stream_manager::ProcessStreamEventRequest {
                    kind,
                    amount_e8s,
                    transaction_id: transaction_id.to_string(),
                })
                .unwrap(),
            )
            .expect("process stream event");
    }

    fn add_sns_neuron(pic: &PocketIc, sns: Principal, neuron: MockSnsNeuron) {
        pic.update_call(
            sns,
            Principal::anonymous(),
            "debug_add_neuron",
            encode_one(neuron).unwrap(),
        )
        .expect("add sns neuron");
    }

    fn install_nns_manager(
        fixture: &StreamFixture,
        args: io_nns_neuron_manager::InitArgs,
    ) -> Option<Principal> {
        let nns_wasm =
            required_wasm("target/wasm32-unknown-unknown/debug/io_nns_neuron_manager.wasm")?;
        Some(create_canister(
            &fixture.pic,
            nns_wasm,
            encode_one(args).unwrap(),
        ))
    }

    fn nns_tick(pic: &PocketIc, manager: Principal) -> NnsDebugTickOutcome {
        let bytes = pic
            .update_call(
                manager,
                Principal::anonymous(),
                "debug_tick",
                encode_one(()).unwrap(),
            )
            .expect("nns tick");
        decode_one::<NnsDebugTickOutcome>(&bytes).unwrap()
    }

    fn advance_nns_model_time(pic: &PocketIc, manager: Principal, seconds: u64, annual_bps: u128) {
        pic.update_call(
            manager,
            Principal::anonymous(),
            "debug_advance_model_time",
            encode_one(io_nns_neuron_manager::AdvanceModelTimeRequest {
                elapsed_seconds: seconds,
                annual_bps: Some(annual_bps),
            })
            .unwrap(),
        )
        .expect("advance nns model time");
    }

    fn sns_neuron(id: u64, stake: u128, voted: u64, total: u64) -> MockSnsNeuron {
        MockSnsNeuron {
            neuron_id: id,
            staked_io_e8s: stake,
            eligible_seconds: 100,
            eligible_closed_proposals: total,
            voted_closed_proposals: voted,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: false,
        }
    }

    #[test]
    fn pocketic_live_jupiter_faucet_stream_moves_mock_ledger_balances_once() {
        let Some(fixture) = setup_stream(false) else {
            return;
        };

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            JUPITER_FAUCET_SOURCE,
            t(100),
            "fund_faucet",
        );
        faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            t(100),
            "faucet",
        );

        let outcome = tick(&fixture);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.processed_authorized_streams, 1);
        assert_eq!(outcome.scanned_icp_transactions, 2);
        assert_eq!(outcome.io_issued_e8s, t(60));
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, JUPITER_FAUCET_SOURCE),
            t(60)
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.icp_index)
                .iter()
                .filter(|tx| tx.to == "stream_manager_deposit")
                .count(),
            1
        );
        let protocol = state(&fixture).protocol;
        assert_eq!(protocol.two_year_staked_icp_e8s, t(40));
        assert_eq!(protocol.liquid_icp_e8s, t(60));

        let replay = tick(&fixture);
        assert_eq!(replay.processed_authorized_streams, 0);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, JUPITER_FAUCET_SOURCE),
            t(60)
        );
    }

    #[test]
    fn pocketic_live_jupiter_faucet_io_transfer_failure_is_retryable() {
        let Some(fixture) = setup_stream(false) else {
            return;
        };

        reject_to(&fixture.pic, fixture.io_ledger, JUPITER_FAUCET_SOURCE);
        mint(
            &fixture.pic,
            fixture.icp_ledger,
            JUPITER_FAUCET_SOURCE,
            t(100),
            "fund_faucet",
        );
        faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            t(100),
            "faucet",
        );

        let failed = tick(&fixture);
        assert!(!failed.errors.is_empty());
        assert_eq!(failed.processed_authorized_streams, 0);
        assert_eq!(state(&fixture).processed_transaction_count, 0);
        let protocol = state(&fixture).protocol;
        assert_eq!(protocol.liquid_icp_e8s, 0);
        assert_eq!(protocol.two_year_staked_icp_e8s, 0);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, JUPITER_FAUCET_SOURCE),
            0
        );

        clear_rejections(&fixture.pic, fixture.io_ledger);
        upgrade_stream(&fixture);
        let retry = tick(&fixture);
        assert!(retry.errors.is_empty(), "{:?}", retry.errors);
        assert_eq!(retry.processed_authorized_streams, 1);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, JUPITER_FAUCET_SOURCE),
            t(60)
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == JUPITER_FAUCET_SOURCE && tx.amount_e8s == t(60))
                .count(),
            1
        );
    }

    #[test]
    fn pocketic_live_index_lag_blocks_scan_then_resolves_once() {
        let Some(fixture) = setup_stream(false) else {
            return;
        };

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            JUPITER_FAUCET_SOURCE,
            t(200),
            "fund_faucet",
        );
        faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            t(100),
            "faucet",
        );
        assert!(tick(&fixture).errors.is_empty());

        faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            t(100),
            "faucet",
        );
        set_index_lag(&fixture.pic, fixture.icp_index, 10);
        let lagged = tick(&fixture);
        assert!(lagged.errors.iter().any(|err| err.contains("IndexLag")));
        assert_eq!(lagged.processed_authorized_streams, 0);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, JUPITER_FAUCET_SOURCE),
            t(60)
        );

        set_index_lag(&fixture.pic, fixture.icp_index, 0);
        let resolved = tick(&fixture);
        assert!(resolved.errors.is_empty(), "{:?}", resolved.errors);
        assert_eq!(resolved.processed_authorized_streams, 1);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, JUPITER_FAUCET_SOURCE),
            t(120)
        );
        let replay = tick(&fixture);
        assert_eq!(replay.processed_authorized_streams, 0);
    }

    #[test]
    fn pocketic_live_archive_required_blocks_redemption_scan_without_mutation() {
        let Some(fixture) = setup_stream(false) else {
            return;
        };

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            JUPITER_FAUCET_SOURCE,
            t(100),
            "fund_faucet",
        );
        faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            t(100),
            "faucet",
        );
        assert!(tick(&fixture).errors.is_empty());
        let before = state(&fixture).protocol;
        mint(&fixture.pic, fixture.io_ledger, "user", t(10), "user_io");
        transfer(
            &fixture.pic,
            fixture.io_ledger,
            "user",
            "redemption",
            t(10),
            "redeem",
        );

        set_archive_required(&fixture.pic, fixture.io_index, true);
        let blocked = tick(&fixture);
        assert!(blocked
            .errors
            .iter()
            .any(|err| err.contains("ArchiveRequired")));
        assert_eq!(blocked.processed_redemptions, 0);
        assert_eq!(balance(&fixture.pic, fixture.icp_ledger, "user"), 0);
        assert_eq!(state(&fixture).protocol, before);

        set_archive_required(&fixture.pic, fixture.io_index, false);
        let resolved = tick(&fixture);
        assert!(resolved.errors.is_empty(), "{:?}", resolved.errors);
        assert_eq!(resolved.processed_redemptions, 1);
        assert_eq!(balance(&fixture.pic, fixture.icp_ledger, "user"), t(10));
    }

    #[test]
    fn pocketic_live_two_year_maturity_issues_no_io() {
        let Some(fixture) = setup_stream(false) else {
            return;
        };

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            "stream_manager_deposit",
            t(100),
            TWO_YEAR_MATURITY_MEMO,
        );

        let outcome = tick(&fixture);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.processed_authorized_streams, 1);
        assert_eq!(outcome.io_issued_e8s, 0);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, JUPITER_FAUCET_SOURCE),
            0
        );
        let protocol = state(&fixture).protocol;
        assert_eq!(protocol.two_year_staked_icp_e8s, t(40));
        assert_eq!(protocol.liquid_icp_e8s, t(60));
    }

    #[test]
    fn pocketic_live_two_week_maturity_allocates_io_from_mock_sns_snapshot() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture.pic, sns, sns_neuron(10, t(10), 2, 2));
        add_sns_neuron(&fixture.pic, sns, sns_neuron(11, t(10), 1, 2));
        let mut non_voter = sns_neuron(12, t(10), 0, 2);
        non_voter.is_genesis_governance_neuron = false;
        add_sns_neuron(&fixture.pic, sns, non_voter);
        let mut genesis = sns_neuron(13, t(10), 2, 2);
        genesis.is_genesis_governance_neuron = true;
        add_sns_neuron(&fixture.pic, sns, genesis);

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            "stream_manager_deposit",
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );

        let outcome = tick(&fixture);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.processed_authorized_streams, 1);
        assert_eq!(outcome.io_issued_e8s, t(60));
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(40)
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_11"),
            t(20)
        );
        assert_eq!(balance(&fixture.pic, fixture.io_ledger, "sns_neuron_12"), 0);
        assert_eq!(balance(&fixture.pic, fixture.io_ledger, "sns_neuron_13"), 0);
        let protocol = state(&fixture).protocol;
        assert_eq!(protocol.two_week_staked_icp_e8s, t(40));
        assert_eq!(protocol.liquid_icp_e8s, t(60));
    }

    #[test]
    fn pocketic_live_two_week_partial_allocation_failure_does_not_double_pay_retry() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture.pic, sns, sns_neuron(10, t(10), 2, 2));
        add_sns_neuron(&fixture.pic, sns, sns_neuron(11, t(10), 1, 2));
        reject_to(&fixture.pic, fixture.io_ledger, "sns_neuron_11");

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            "stream_manager_deposit",
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );

        let failed = tick(&fixture);
        assert!(!failed.errors.is_empty());
        assert_eq!(failed.processed_authorized_streams, 0);
        assert_eq!(state(&fixture).processed_transaction_count, 0);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(40)
        );
        assert_eq!(balance(&fixture.pic, fixture.io_ledger, "sns_neuron_11"), 0);
        let protocol = state(&fixture).protocol;
        assert_eq!(protocol.two_week_staked_icp_e8s, 0);
        assert_eq!(protocol.liquid_icp_e8s, 0);

        clear_rejections(&fixture.pic, fixture.io_ledger);
        upgrade_stream(&fixture);
        let retry = tick(&fixture);
        assert!(retry.errors.is_empty(), "{:?}", retry.errors);
        assert_eq!(retry.processed_authorized_streams, 1);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(40)
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_11"),
            t(20)
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10")
                .count(),
            1
        );
    }

    #[test]
    fn pocketic_live_redemption_pays_icp_and_returns_io_to_reserve_once() {
        let Some(fixture) = setup_stream(false) else {
            return;
        };

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            JUPITER_FAUCET_SOURCE,
            t(100),
            "fund_faucet",
        );
        faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            t(100),
            "faucet",
        );
        assert!(tick(&fixture).errors.is_empty());

        mint(&fixture.pic, fixture.io_ledger, "user", t(10), "user_io");
        transfer(
            &fixture.pic,
            fixture.io_ledger,
            "user",
            "redemption",
            t(10),
            "redeem",
        );

        let outcome = tick(&fixture);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.processed_redemptions, 1);
        assert_eq!(outcome.icp_paid_e8s, t(10));
        assert_eq!(balance(&fixture.pic, fixture.icp_ledger, "user"), t(10));
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "protocol_reserve"),
            t(899_950)
        );

        let replay = tick(&fixture);
        assert_eq!(replay.processed_redemptions, 0);
        assert_eq!(balance(&fixture.pic, fixture.icp_ledger, "user"), t(10));
        let txs = transactions(&fixture.pic, fixture.icp_ledger);
        assert_eq!(
            txs.iter()
                .filter(|tx| tx.memo == "redemption_payout")
                .count(),
            1
        );
        assert!(transactions(&fixture.pic, fixture.io_index)
            .iter()
            .any(|tx| tx.to == "redemption" && tx.amount_e8s == t(10)));
    }

    #[test]
    fn pocketic_live_redemption_icp_payout_failure_is_retryable() {
        let Some(fixture) = setup_stream(false) else {
            return;
        };

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            JUPITER_FAUCET_SOURCE,
            t(100),
            "fund_faucet",
        );
        faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            t(100),
            "faucet",
        );
        assert!(tick(&fixture).errors.is_empty());
        let before = state(&fixture).protocol;

        mint(&fixture.pic, fixture.io_ledger, "user", t(10), "user_io");
        transfer(
            &fixture.pic,
            fixture.io_ledger,
            "user",
            "redemption",
            t(10),
            "redeem",
        );
        reject_to(&fixture.pic, fixture.icp_ledger, "user");

        let failed = tick(&fixture);
        assert!(!failed.errors.is_empty());
        assert_eq!(failed.processed_redemptions, 0);
        assert_eq!(balance(&fixture.pic, fixture.icp_ledger, "user"), 0);
        assert_eq!(state(&fixture).protocol, before);

        clear_rejections(&fixture.pic, fixture.icp_ledger);
        let retry = tick(&fixture);
        assert!(retry.errors.is_empty(), "{:?}", retry.errors);
        assert_eq!(retry.processed_redemptions, 1);
        assert_eq!(balance(&fixture.pic, fixture.icp_ledger, "user"), t(10));
        assert_eq!(
            transactions(&fixture.pic, fixture.icp_ledger)
                .iter()
                .filter(|tx| tx.memo == "redemption_payout")
                .count(),
            1
        );
    }

    #[test]
    fn pocketic_live_redemption_missing_icp_payout_ledger_is_retryable_failure() {
        let Some(fixture) = setup_stream_with_payout_ledger(false, false) else {
            return;
        };

        process_stream_event(
            &fixture,
            io_stream_manager::ApiStreamKind::JupiterFaucet,
            t(100),
            "seed-liquid-icp",
        );
        let before = state(&fixture).protocol;

        mint(&fixture.pic, fixture.io_ledger, "user", t(10), "user_io");
        transfer(
            &fixture.pic,
            fixture.io_ledger,
            "user",
            "redemption",
            t(10),
            "redeem",
        );

        let failed = tick(&fixture);
        assert!(!failed.errors.is_empty());
        assert!(failed
            .errors
            .iter()
            .any(|err| err.contains("missing ICP payout ledger principal")));
        assert_eq!(failed.processed_redemptions, 0);
        assert_eq!(balance(&fixture.pic, fixture.icp_ledger, "user"), 0);
        assert_eq!(state(&fixture).protocol, before);
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.memo == "redeemed_io_to_reserve")
                .count(),
            0
        );

        upgrade_stream(&fixture);
        let retry = tick(&fixture);
        assert!(!retry.errors.is_empty());
        assert_eq!(retry.processed_redemptions, 0);
        assert_eq!(balance(&fixture.pic, fixture.icp_ledger, "user"), 0);
        assert_eq!(state(&fixture).protocol, before);
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.memo == "redeemed_io_to_reserve")
                .count(),
            0
        );
    }

    #[test]
    fn pocketic_live_redemption_io_return_failure_does_not_double_pay_icp() {
        let Some(fixture) = setup_stream(false) else {
            return;
        };

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            JUPITER_FAUCET_SOURCE,
            t(100),
            "fund_faucet",
        );
        faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            t(100),
            "faucet",
        );
        assert!(tick(&fixture).errors.is_empty());
        let before = state(&fixture).protocol;

        mint(&fixture.pic, fixture.io_ledger, "user", t(10), "user_io");
        transfer(
            &fixture.pic,
            fixture.io_ledger,
            "user",
            "redemption",
            t(10),
            "redeem",
        );
        reject_to(&fixture.pic, fixture.io_ledger, "protocol_reserve");

        let failed = tick(&fixture);
        assert!(!failed.errors.is_empty());
        assert_eq!(failed.processed_redemptions, 0);
        assert_eq!(balance(&fixture.pic, fixture.icp_ledger, "user"), t(10));
        assert_eq!(state(&fixture).protocol, before);

        clear_rejections(&fixture.pic, fixture.io_ledger);
        upgrade_stream(&fixture);
        let retry = tick(&fixture);
        assert!(retry.errors.is_empty(), "{:?}", retry.errors);
        assert_eq!(retry.processed_redemptions, 1);
        assert_eq!(balance(&fixture.pic, fixture.icp_ledger, "user"), t(10));
        assert_eq!(
            transactions(&fixture.pic, fixture.icp_ledger)
                .iter()
                .filter(|tx| tx.memo == "redemption_payout")
                .count(),
            1
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "protocol_reserve"),
            t(899_950)
        );
    }

    #[test]
    fn pocketic_live_nns_manager_maturity_feeds_stream_manager_rewards() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture.pic, sns, sns_neuron(10, t(10), 1, 1));
        add_sns_neuron(&fixture.pic, sns, sns_neuron(11, t(10), 1, 1));
        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(10_000),
            "fund_nns_manager",
        );

        let Some(nns_manager) = install_nns_manager(
            &fixture,
            io_nns_neuron_manager::InitArgs {
                initial_two_year_principal_e8s: t(1_000),
                initial_two_week_principal_e8s: t(500),
                model_annual_bps: 12_000,
                icp_ledger_principal_text: Some(fixture.icp_ledger.to_text()),
                ..Default::default()
            },
        ) else {
            return;
        };

        fixture
            .pic
            .advance_time(std::time::Duration::from_secs(30 * 86_400));
        advance_nns_model_time(&fixture.pic, nns_manager, 30 * 86_400, 12_000);
        let nns_outcome = nns_tick(&fixture.pic, nns_manager);
        assert!(nns_outcome.errors.is_empty(), "{:?}", nns_outcome.errors);
        assert!(nns_outcome.disbursed_two_year_maturity_e8s > 0);
        assert!(nns_outcome.disbursed_two_week_maturity_e8s > 0);

        let stream_outcome = tick(&fixture);
        assert!(
            stream_outcome.errors.is_empty(),
            "{:?}",
            stream_outcome.errors
        );
        assert_eq!(stream_outcome.processed_authorized_streams, 2);
        assert!(stream_outcome.io_issued_e8s > 0);
        assert!(balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10") > 0);
        assert!(balance(&fixture.pic, fixture.io_ledger, "sns_neuron_11") > 0);
        let protocol = state(&fixture).protocol;
        assert!(protocol.two_year_staked_icp_e8s > 0);
        assert!(protocol.two_week_staked_icp_e8s > 0);
        assert!(protocol.liquid_icp_e8s > 0);
    }
}

fn neuron(id: u64, stake: u128, voted: u64, total: u64) -> NeuronSnapshot {
    NeuronSnapshot {
        neuron_id: id,
        staked_io_e8s: stake,
        eligible_seconds: 100,
        eligible_closed_proposals: total,
        voted_closed_proposals: voted,
        is_genesis_governance_neuron: false,
        is_protocol_owned: false,
        is_dissolving: false,
    }
}

#[test]
fn pocketic_model_full_stream_and_redemption_flow() {
    let mut manager = StreamManager::default_for_tests();
    let faucet = manager
        .process_authorized_stream(StreamKind::JupiterFaucet, t(100), "faucet-1")
        .unwrap();
    assert_eq!(faucet.io_issued_e8s, t(60));

    let two_year = manager
        .process_authorized_stream(StreamKind::TwoYearMaturity, t(100), "2y-1")
        .unwrap();
    assert_eq!(two_year.io_issued_e8s, 0);
    assert_eq!(
        manager
            .state
            .redemption_rate()
            .unwrap()
            .icp_for_io(t(1))
            .unwrap(),
        t(2)
    );

    let two_week = manager
        .process_authorized_stream(StreamKind::TwoWeekMaturity, t(100), "2w-1")
        .unwrap();
    assert_eq!(two_week.io_issued_e8s, t(30));

    let neurons = vec![neuron(10, t(10), 2, 2), neuron(11, t(10), 1, 2)];
    let alloc = manager.allocate_two_week_maturity_io(two_week.io_issued_e8s, &neurons);
    assert_eq!(alloc.allocations[0].io_e8s, t(20));
    assert_eq!(alloc.allocations[1].io_e8s, t(10));

    let redemption = manager.redeem(t(5)).unwrap();
    assert_eq!(redemption.icp_paid_e8s, t(10));
}

#[test]
fn pocketic_scanner_classifies_sources_and_memos() {
    let mut manager = StreamManager::default_for_tests();
    assert_eq!(
        manager
            .process_scanned_icp(JUPITER_FAUCET_SOURCE, "faucet", t(100), "faucet-block")
            .unwrap()
            .io_issued_e8s,
        t(60)
    );
    assert_eq!(
        manager
            .process_scanned_icp(
                IO_NNS_NEURON_MANAGER_SOURCE,
                TWO_YEAR_MATURITY_MEMO,
                t(100),
                "2y-block"
            )
            .unwrap()
            .io_issued_e8s,
        0
    );
    assert_eq!(
        manager
            .process_scanned_icp(
                IO_NNS_NEURON_MANAGER_SOURCE,
                TWO_WEEK_MATURITY_MEMO,
                t(100),
                "2w-block"
            )
            .unwrap()
            .recipient_policy,
        io_core_model::IoRecipientPolicy::EligibleIoSnsNeurons
    );
}

#[test]
fn pocketic_unknown_sender_cannot_issue_io_and_does_not_mark_tx() {
    let mut manager = StreamManager::default_for_tests();
    let err = manager
        .process_scanned_icp("attacker", "faucet", t(100), "attack-block")
        .unwrap_err();
    assert!(matches!(
        err,
        StreamManagerError::UnknownOrUnauthorizedStream { .. }
    ));
    assert!(!manager.processed_transactions.contains("attack-block"));
    assert_eq!(manager.state.redeemable_io_supply_e8s().unwrap(), 0);
}

#[test]
fn pocketic_duplicate_ledger_event_is_idempotently_rejected() {
    let mut manager = StreamManager::default_for_tests();
    manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "block-1")
        .unwrap();
    let before = manager.state;
    let err = manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "block-1")
        .unwrap_err();
    assert_eq!(err, StreamManagerError::DuplicateTransaction);
    assert_eq!(manager.state, before);
}

#[test]
fn pocketic_failed_issuance_is_atomic_and_retryable() {
    let mut manager = StreamManager::default_for_tests();
    manager.state.protocol_reserve_io_e8s = t(1);
    let before = manager.state;
    let err = manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "reserve-fail")
        .unwrap_err();
    assert!(matches!(
        err,
        StreamManagerError::Model(ModelError::InsufficientProtocolReserve { .. })
    ));
    assert_eq!(manager.state, before);
    assert!(!manager.processed_transactions.contains("reserve-fail"));
}

#[test]
fn pocketic_active_stake_snapshot_drives_two_week_target() {
    let mut manager = StreamManager::default_for_tests();
    manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet")
        .unwrap();
    manager
        .process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_YEAR_MATURITY_MEMO,
            t(100),
            "2y",
        )
        .unwrap(); // rate = 2
    let mut dissolving = neuron(12, t(10), 1, 1);
    dissolving.is_dissolving = true;
    let mut genesis = neuron(13, t(10), 1, 1);
    genesis.is_genesis_governance_neuron = true;
    manager.refresh_active_staked_io_from_neurons(&[neuron(10, t(10), 1, 1), dissolving, genesis]);
    assert_eq!(manager.active_staked_io_e8s, t(10));
    assert_eq!(manager.target_two_week_pool_e8s().unwrap(), t(20));
}

#[test]
fn pocketic_two_week_maturity_fails_atomically_when_reward_reserve_is_exhausted() {
    let mut manager = StreamManager::default_for_tests();
    manager
        .process_authorized_stream(StreamKind::JupiterFaucet, t(100), "faucet")
        .unwrap();
    manager.state.protocol_reserve_io_e8s = 1;
    let before = manager.state;
    let err = manager
        .process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_WEEK_MATURITY_MEMO,
            t(100),
            "2w-reserve-fail",
        )
        .unwrap_err();
    assert!(matches!(
        err,
        StreamManagerError::Model(ModelError::InsufficientProtocolReserve { .. })
    ));
    assert_eq!(manager.state, before);
    assert!(!manager.processed_transactions.contains("2w-reserve-fail"));
}

#[test]
fn pocketic_small_amount_streams_preserve_e8s_totals_and_do_not_panic() {
    let mut manager = StreamManager::default_for_tests();
    for amount in 1..100u128 {
        let tx = format!("tiny-{amount}");
        let out = manager
            .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", amount, tx)
            .unwrap();
        assert_eq!(out.split.stake_e8s + out.split.liquid_e8s, amount);
    }
}

#[test]
fn pocketic_later_faucet_stream_after_two_year_maturity_is_not_dilutive() {
    let mut manager = StreamManager::default_for_tests();
    manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet-1")
        .unwrap();
    manager
        .process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_YEAR_MATURITY_MEMO,
            t(100),
            "2y-1",
        )
        .unwrap();
    let rate_before = manager.state.redemption_rate().unwrap();
    let out = manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet-2")
        .unwrap();
    assert_eq!(out.io_issued_e8s, t(30));
    assert_eq!(manager.state.redemption_rate().unwrap(), rate_before);
}

#[test]
fn pocketic_participation_snapshot_penalizes_non_voters_in_two_week_distribution() {
    let mut manager = StreamManager::default_for_tests();
    manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet")
        .unwrap();
    let two_week = manager
        .process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_WEEK_MATURITY_MEMO,
            t(30),
            "2w",
        )
        .unwrap();
    let neurons = vec![
        neuron(1, t(10), 3, 3),
        neuron(2, t(10), 0, 3),
        neuron(3, t(10), 1, 3),
    ];
    let out = manager.allocate_two_week_maturity_io(two_week.io_issued_e8s, &neurons);
    assert_eq!(
        out.allocations
            .iter()
            .map(|a| a.neuron_id)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
    assert!(out.allocations[0].io_e8s > out.allocations[1].io_e8s);
}

#[test]
fn pocketic_blank_transaction_id_is_rejected_and_not_recorded() {
    let mut manager = StreamManager::default_for_tests();
    let before = manager.state;
    assert_eq!(
        manager
            .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "")
            .unwrap_err(),
        StreamManagerError::InvalidTransactionId
    );
    assert_eq!(manager.state, before);
    assert!(manager.processed_transactions.is_empty());
}

#[test]
fn pocketic_backing_fraction_above_one_hundred_percent_is_rejected() {
    let mut manager = StreamManager::default_for_tests();
    manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet")
        .unwrap();
    manager.two_week_pool_backing_bps = 20_000;
    assert_eq!(
        manager.target_two_week_pool_e8s().unwrap_err(),
        StreamManagerError::Model(ModelError::InvalidBasisPoints { bps: 20_000 })
    );
}
