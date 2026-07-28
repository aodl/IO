use io_core_model::{StreamKind, E8S_PER_TOKEN};
use io_reward_policy::RewardParticipant;
use io_stream_manager::state::{
    IO_NNS_NEURON_MANAGER_SOURCE, JUPITER_FAUCET_SOURCE, TWO_WEEK_MATURITY_MEMO,
    TWO_YEAR_MATURITY_MEMO,
};
use io_stream_manager::{DebugFailpoint, ModelError, StreamManager, StreamManagerError};

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
struct DebugOrderArgs {
    descending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct DebugFeeArgs {
    fee_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct LedgerTransaction {
    from: String,
    to: String,
    from_account: Option<io_ledger_types::Account>,
    to_account: Option<io_ledger_types::Account>,
    amount_e8s: u128,
    memo: String,
    memo_bytes: Option<Vec<u8>>,
    block_index: u64,
    timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct MockSnsNeuron {
    neuron_id: u64,
    staked_io_e8s: u128,
    dissolve_delay_seconds: u64,
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
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir.parent().and_then(|path| path.parent());
    let debug_artifact = std::path::Path::new(path)
        .file_name()
        .and_then(|name| workspace.map(|root| root.join("debug-artifacts").join(name)));
    std::fs::read(path)
        .ok()
        .or_else(|| workspace.and_then(|root| std::fs::read(root.join(path)).ok()))
        .or_else(|| debug_artifact.and_then(|path| std::fs::read(path).ok()))
}

#[cfg(test)]
mod live {
    use super::*;
    use candid::{decode_one, encode_one, Nat, Principal};
    use io_governance_types::{
        EmptyRecord, SnsBallot, SnsClaimOrRefresh, SnsClaimOrRefreshBy, SnsManageNeuronCommand,
        SnsProductionManageNeuronRequest, SnsProductionManageNeuronResponse, SnsProposal,
        SnsProposalId, SnsProposalRewardStatus, SnsProposalStatus, SnsVote,
    };
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
        setup_stream_configured(with_sns, configure_icp_payout_ledger, |_| {})
    }

    fn setup_stream_configured(
        with_sns: bool,
        configure_icp_payout_ledger: bool,
        configure: impl FnOnce(&mut io_stream_manager::InitArgs),
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
        if let Some(sns) = sns_governance {
            pic.update_call(
                sns,
                Principal::anonymous(),
                "debug_set_io_ledger_principal",
                encode_one(io_ledger).unwrap(),
            )
            .expect("configure mock SNS governance IO ledger");
        }
        let mut stream_args = io_stream_manager::InitArgs {
            icp_ledger_principal_text: configure_icp_payout_ledger.then(|| icp_ledger.to_text()),
            icp_index_principal_text: Some(icp_index.to_text()),
            io_ledger_principal_text: Some(io_ledger.to_text()),
            io_index_principal_text: Some(io_index.to_text()),
            sns_governance_principal_text: sns_governance.map(|p| p.to_text()),
            ..Default::default()
        };
        configure(&mut stream_args);
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

    fn transfer_to_account(
        pic: &PocketIc,
        ledger: Principal,
        from: &str,
        to: Account,
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
                    to: to.into(),
                    amount: Nat::from(amount_e8s),
                    fee: None,
                    memo: Some(memo.as_bytes().to_vec()),
                    created_at_time: None,
                })
                .unwrap(),
            )
            .expect("transfer to account");
        map_icrc_transfer_result(decode_one::<Result<Nat, IcrcTransferError>>(&bytes).unwrap())
            .expect("ledger transfer result")
            .block_index
            .0
    }

    fn transfer_to_stream_deposit(
        fixture: &StreamFixture,
        from: &str,
        amount_e8s: u128,
        memo: &str,
    ) -> u64 {
        transfer_to_account(
            &fixture.pic,
            fixture.icp_ledger,
            from,
            Account::new(
                fixture.stream,
                Some(mock_subaccount("stream_manager_deposit")),
            ),
            amount_e8s,
            memo,
        )
    }

    fn balance(pic: &PocketIc, ledger: Principal, account: &str) -> u128 {
        balance_of_account(pic, ledger, mock_account(account))
    }

    fn balance_of_account(pic: &PocketIc, ledger: Principal, account: Account) -> u128 {
        let bytes = pic
            .update_call(
                ledger,
                Principal::anonymous(),
                "icrc1_balance_of",
                encode_one(IcrcAccount::from(account)).unwrap(),
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

    fn sns_neuron_subaccount(id: u64) -> Subaccount {
        let mut subaccount = [0; 32];
        subaccount[24..].copy_from_slice(&id.to_be_bytes());
        Subaccount(subaccount)
    }

    fn sns_neuron_staking_account(sns_governance: Principal, id: u64) -> Account {
        Account::new(sns_governance, Some(sns_neuron_subaccount(id)))
    }

    fn mock_account(label: &str) -> Account {
        Account::new(Principal::anonymous(), Some(mock_subaccount(label)))
    }

    fn transactions(pic: &PocketIc, ledger: Principal) -> Vec<LedgerTransaction> {
        let bytes = pic
            .update_call(
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

    fn set_index_order(pic: &PocketIc, index: Principal, descending: bool) {
        pic.update_call(
            index,
            Principal::anonymous(),
            "debug_set_order",
            encode_one(DebugOrderArgs { descending }).unwrap(),
        )
        .expect("set index order");
    }

    fn set_ledger_fee(pic: &PocketIc, ledger: Principal, fee_e8s: u128) {
        pic.update_call(
            ledger,
            Principal::anonymous(),
            "debug_set_fee",
            encode_one(DebugFeeArgs { fee_e8s }).unwrap(),
        )
        .expect("set ledger fee");
    }

    fn set_sns_available(pic: &PocketIc, sns: Principal, available: bool) {
        pic.update_call(
            sns,
            Principal::anonymous(),
            "debug_set_available",
            encode_one(available).unwrap(),
        )
        .expect("set mock SNS governance availability");
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

    fn seed_reward_cohort_and_advance_one_second(fixture: &StreamFixture) {
        let seeded = tick(fixture);
        assert!(seeded.errors.is_empty(), "{:?}", seeded.errors);
        assert_eq!(seeded.processed_authorized_streams, 0);
        fixture.pic.advance_time(std::time::Duration::from_secs(1));
    }

    fn upgrade_stream(fixture: &StreamFixture) {
        fixture
            .pic
            .upgrade_canister(fixture.stream, fixture.stream_wasm.clone(), vec![], None)
            .expect("upgrade stream manager");
    }

    fn set_stream_failpoint(fixture: &StreamFixture, failpoint: Option<DebugFailpoint>) {
        fixture
            .pic
            .update_call(
                fixture.stream,
                Principal::anonymous(),
                "debug_set_failpoint",
                encode_one(failpoint).unwrap(),
            )
            .expect("set stream failpoint");
    }

    fn stable_state(fixture: &StreamFixture) -> io_stream_manager::StableState {
        let bytes = fixture
            .pic
            .query_call(
                fixture.stream,
                Principal::anonymous(),
                "debug_get_stable_state",
                encode_one(()).unwrap(),
            )
            .expect("stable state");
        decode_one::<io_stream_manager::StableState>(&bytes).unwrap()
    }

    fn faucet_send(
        fixture: &StreamFixture,
        from: &str,
        to: &str,
        amount_e8s: u128,
        memo: &str,
    ) -> u64 {
        if to == "stream_manager_deposit" {
            return transfer_to_stream_deposit(fixture, from, amount_e8s, memo);
        }
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

    fn add_sns_neuron(fixture: &StreamFixture, sns: Principal, neuron: MockSnsNeuron) {
        let initial_stake = neuron.staked_io_e8s;
        let neuron_id = neuron.neuron_id;
        update_sns_neuron(fixture, sns, neuron);
        if initial_stake > 0 {
            transfer_to_account(
                &fixture.pic,
                fixture.io_ledger,
                "protocol_reserve",
                sns_neuron_staking_account(sns, neuron_id),
                initial_stake,
                "existing-stake",
            );
        }
    }

    fn update_sns_neuron(fixture: &StreamFixture, sns: Principal, neuron: MockSnsNeuron) {
        fixture
            .pic
            .update_call(
                sns,
                Principal::anonymous(),
                "debug_add_neuron",
                encode_one(neuron).unwrap(),
            )
            .expect("add sns neuron");
    }

    fn set_sns_proposals(fixture: &StreamFixture, sns: Principal, proposals: Vec<SnsProposal>) {
        fixture
            .pic
            .update_call(
                sns,
                Principal::anonymous(),
                "debug_set_proposals",
                encode_one(proposals).unwrap(),
            )
            .expect("set sns proposals");
    }

    fn proposal(value: u64, decided: u64, votes: &[(u64, SnsVote)]) -> SnsProposal {
        fn id(value: u64) -> io_governance_types::SnsNeuronId {
            let mut bytes = [0_u8; 32];
            bytes[24..].copy_from_slice(&value.to_be_bytes());
            io_governance_types::SnsNeuronId(bytes.to_vec())
        }

        SnsProposal {
            id: SnsProposalId(value),
            topic: Some(1),
            status: SnsProposalStatus::Adopted,
            reward_status: SnsProposalRewardStatus::Settled,
            decided_timestamp_seconds: Some(decided),
            ballots: votes
                .iter()
                .map(|(neuron_id, vote)| SnsBallot {
                    neuron_id: id(*neuron_id),
                    vote: *vote,
                })
                .collect(),
        }
    }

    fn sns_neurons(pic: &PocketIc, sns: Principal) -> Vec<MockSnsNeuron> {
        let bytes = pic
            .query_call(
                sns,
                Principal::anonymous(),
                "debug_list_neurons",
                encode_one(()).unwrap(),
            )
            .expect("list sns neurons");
        decode_one::<Vec<MockSnsNeuron>>(&bytes).unwrap()
    }

    fn claim_or_refresh_sns_neuron(pic: &PocketIc, sns: Principal, neuron_id: u64) {
        let bytes = pic
            .update_call(
                sns,
                Principal::anonymous(),
                "manage_neuron",
                encode_one(SnsProductionManageNeuronRequest {
                    subaccount: sns_neuron_subaccount(neuron_id).0.to_vec(),
                    command: Some(SnsManageNeuronCommand::ClaimOrRefresh(SnsClaimOrRefresh {
                        by: Some(SnsClaimOrRefreshBy::NeuronId(EmptyRecord {})),
                    })),
                })
                .unwrap(),
            )
            .expect("claim or refresh SNS neuron");
        let response = decode_one::<SnsProductionManageNeuronResponse>(&bytes).unwrap();
        assert!(
            matches!(
                response.command,
                Some(io_governance_types::SnsManageNeuronCommandResponse::ClaimOrRefresh(_))
            ),
            "{response:?}"
        );
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
            dissolve_delay_seconds: io_core_model::TWO_WEEK_SECONDS,
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
            t(150),
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
        assert_eq!(outcome.scanned_icp_transactions, 1);
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
        assert_eq!(replay.scanned_icp_transactions, 0);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, JUPITER_FAUCET_SOURCE),
            t(60)
        );
    }

    #[test]
    fn pocketic_live_jupiter_faucet_stream_accepts_descending_index_pages() {
        let Some(fixture) = setup_stream(false) else {
            return;
        };

        set_index_order(&fixture.pic, fixture.icp_index, true);
        mint(
            &fixture.pic,
            fixture.icp_ledger,
            JUPITER_FAUCET_SOURCE,
            t(150),
            "fund_faucet",
        );
        faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            t(100),
            "faucet",
        );
        faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            t(50),
            "faucet_second",
        );

        let outcome = tick(&fixture);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.processed_authorized_streams, 2);
        assert_eq!(outcome.io_issued_e8s, t(90));
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, JUPITER_FAUCET_SOURCE),
            t(90)
        );

        let replay = tick(&fixture);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.processed_authorized_streams, 0);
        assert_eq!(replay.scanned_icp_transactions, 0);
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

        faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            t(50),
            "faucet_second",
        );
        let blocked_by_retry = tick(&fixture);
        assert!(!blocked_by_retry.errors.is_empty());
        assert_eq!(blocked_by_retry.processed_authorized_streams, 0);
        assert_eq!(blocked_by_retry.scanned_icp_transactions, 0);

        clear_rejections(&fixture.pic, fixture.io_ledger);
        upgrade_stream(&fixture);
        let retry = tick(&fixture);
        assert!(retry.errors.is_empty(), "{:?}", retry.errors);
        assert_eq!(retry.processed_authorized_streams, 2);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, JUPITER_FAUCET_SOURCE),
            t(90)
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
    fn pocketic_live_icp_preview_failure_does_not_commit_scan_state() {
        let Some(fixture) = setup_stream_configured(false, true, |args| {
            args.initial_total_io_supply_e8s = 1;
            args.initial_protocol_reserve_io_e8s = 1;
            args.non_redeemable_governance_io_e8s = 0;
        }) else {
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

        let failed = tick(&fixture);
        assert!(!failed.errors.is_empty());
        assert_eq!(failed.scanned_icp_transactions, 1);
        assert_eq!(failed.processed_authorized_streams, 0);

        let replay = tick(&fixture);
        assert!(!replay.errors.is_empty());
        assert_eq!(replay.scanned_icp_transactions, 1);
        assert_eq!(replay.processed_authorized_streams, 0);
    }

    #[test]
    fn pocketic_live_unknown_icp_deposit_is_terminally_journaled_and_advances_scan_state() {
        let Some(fixture) = setup_stream(false) else {
            return;
        };

        mint(&fixture.pic, fixture.icp_ledger, "attacker", t(100), "fund");
        transfer_to_stream_deposit(&fixture, "attacker", t(100), "unknown");

        let rejected = tick(&fixture);
        assert!(rejected.errors.is_empty(), "{:?}", rejected.errors);
        assert_eq!(rejected.scanned_icp_transactions, 1);
        assert_eq!(rejected.processed_authorized_streams, 0);
        assert_eq!(state(&fixture).processed_transaction_count, 0);

        let replay = tick(&fixture);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.scanned_icp_transactions, 0);
        assert_eq!(replay.processed_authorized_streams, 0);
    }

    #[test]
    fn pocketic_live_tiny_authorized_icp_deposit_does_not_block_later_valid_deposit() {
        let Some(fixture) = setup_stream(false) else {
            return;
        };

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            JUPITER_FAUCET_SOURCE,
            t(150),
            "fund_faucet",
        );
        faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            1,
            "tiny",
        );
        faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            t(100),
            "valid",
        );

        let outcome = tick(&fixture);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.processed_authorized_streams, 1);
        assert_eq!(outcome.io_issued_e8s, t(60));
        assert_eq!(state(&fixture).processed_transaction_count, 1);

        let replay = tick(&fixture);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.scanned_icp_transactions, 0);
        assert_eq!(replay.processed_authorized_streams, 0);
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
        assert!(lagged.errors.is_empty(), "{:?}", lagged.errors);
        assert_eq!(lagged.scanned_icp_transactions, 0);
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
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
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

    struct ZeroRecipientRewardObservation {
        reward_pool_e8s: u128,
        reserve_before: u128,
        reserve_after: u128,
        model_supply_before: u128,
        model_supply_after: u128,
        io_transaction_count_before: usize,
        io_transaction_count_after: usize,
        stable_after_completion: io_stream_manager::StableState,
    }

    fn drive_zero_recipient_reward(fixture: &StreamFixture) -> ZeroRecipientRewardObservation {
        seed_reward_cohort_and_advance_one_second(fixture);
        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        let deposit_block = transfer_to_stream_deposit(
            fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        let before = state(fixture).protocol;
        let reserve_before = balance(&fixture.pic, fixture.io_ledger, "protocol_reserve");
        let io_transaction_count_before = transactions(&fixture.pic, fixture.io_ledger).len();

        let outcome = tick(fixture);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.processed_authorized_streams, 1);
        assert!(outcome.io_issued_e8s > 0);
        let after = state(fixture).protocol;
        let stable_after_completion = stable_state(fixture);
        let op = stable_after_completion
            .operation_journal
            .iter()
            .find(|op| op.source_transaction_id == format!("icp:{deposit_block}"))
            .expect("zero-recipient reward operation should be journaled");
        let preflight = op
            .reward_preflight
            .as_ref()
            .expect("zero-recipient reward should have preflight");
        assert_eq!(op.phase, io_stream_manager::OperationPhase::Completed);
        assert!(op.two_week_recipients.is_empty());
        assert_eq!(preflight.recipient_count, 0);
        assert_eq!(preflight.total_reward_e8s, 0);
        assert_eq!(preflight.total_fee_e8s, 0);
        assert_eq!(preflight.total_reserve_debit_e8s, 0);
        assert_eq!(preflight.dust_e8s, outcome.io_issued_e8s);
        assert!(preflight.canonical_recipient_ids.is_empty());
        assert!(preflight.compatibility_keys.is_empty());
        assert_eq!(
            op.reward_reservation,
            Some(io_stream_manager::RewardReservation::default())
        );
        assert_eq!(op.reserved_reward_debit_e8s, Some(0));
        assert_eq!(
            after.two_week_staked_icp_e8s,
            before.two_week_staked_icp_e8s + t(40)
        );
        assert_eq!(after.liquid_icp_e8s, before.liquid_icp_e8s + t(60));
        assert_eq!(
            after.protocol_reserve_io_e8s,
            before.protocol_reserve_io_e8s
        );
        assert_eq!(after.total_io_supply_e8s, before.total_io_supply_e8s);

        ZeroRecipientRewardObservation {
            reward_pool_e8s: outcome.io_issued_e8s,
            reserve_before,
            reserve_after: balance(&fixture.pic, fixture.io_ledger, "protocol_reserve"),
            model_supply_before: before.total_io_supply_e8s,
            model_supply_after: after.total_io_supply_e8s,
            io_transaction_count_before,
            io_transaction_count_after: transactions(&fixture.pic, fixture.io_ledger).len(),
            stable_after_completion,
        }
    }

    #[test]
    fn zero_recipient_reward_creates_no_io_ledger_transfer() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };

        let observed = drive_zero_recipient_reward(&fixture);
        let model_reserve = state(&fixture).protocol.protocol_reserve_io_e8s;

        assert!(observed.reward_pool_e8s > 0);
        assert_eq!(
            observed.io_transaction_count_after,
            observed.io_transaction_count_before
        );
        assert_eq!(observed.model_supply_after, observed.model_supply_before);
        assert_eq!(observed.reserve_after, observed.reserve_before);
        assert_eq!(model_reserve, observed.reserve_after);
        assert_eq!(observed.stable_after_completion.operation_journal.len(), 1);
    }

    #[test]
    fn mock_claim_or_refresh_sets_exact_staking_balance() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, 0, 1, 1));
        transfer_to_account(
            &fixture.pic,
            fixture.io_ledger,
            "protocol_reserve",
            sns_neuron_staking_account(sns, 10),
            t(42),
            "stake",
        );

        claim_or_refresh_sns_neuron(&fixture.pic, sns, 10);

        assert_eq!(sns_neurons(&fixture.pic, sns)[0].staked_io_e8s, t(42));
    }

    #[test]
    fn mock_claim_or_refresh_is_idempotent() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, 0, 1, 1));
        transfer_to_account(
            &fixture.pic,
            fixture.io_ledger,
            "protocol_reserve",
            sns_neuron_staking_account(sns, 10),
            t(42),
            "stake",
        );

        claim_or_refresh_sns_neuron(&fixture.pic, sns, 10);
        claim_or_refresh_sns_neuron(&fixture.pic, sns, 10);

        assert_eq!(sns_neurons(&fixture.pic, sns)[0].staked_io_e8s, t(42));
    }

    #[test]
    fn mock_claim_or_refresh_wrong_destination_does_not_increase_stake() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, 0, 1, 1));
        transfer_to_account(
            &fixture.pic,
            fixture.io_ledger,
            "protocol_reserve",
            sns_neuron_staking_account(sns, 11),
            t(42),
            "wrong-destination",
        );

        claim_or_refresh_sns_neuron(&fixture.pic, sns, 10);

        assert_eq!(sns_neurons(&fixture.pic, sns)[0].staked_io_e8s, 0);
    }

    #[test]
    fn pocketic_live_two_week_maturity_allocates_io_from_mock_sns_snapshot() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 2, 2));
        add_sns_neuron(&fixture, sns, sns_neuron(11, t(10), 1, 2));
        let mut non_voter = sns_neuron(12, t(10), 0, 2);
        non_voter.is_genesis_governance_neuron = false;
        add_sns_neuron(&fixture, sns, non_voter);
        let mut genesis = sns_neuron(13, t(10), 2, 2);
        genesis.is_genesis_governance_neuron = true;
        add_sns_neuron(&fixture, sns, genesis);
        seed_reward_cohort_and_advance_one_second(&fixture);
        let proposal_time = stable_state(&fixture)
            .reward_cohort
            .as_ref()
            .expect("seeded reward cohort")
            .captured_at_timestamp_seconds
            + 1;
        set_sns_proposals(
            &fixture,
            sns,
            vec![
                proposal(1, proposal_time, &[(10, SnsVote::Yes), (11, SnsVote::Yes)]),
                proposal(2, proposal_time, &[(10, SnsVote::Yes)]),
            ],
        );

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );

        let outcome = tick(&fixture);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.processed_authorized_streams, 1);
        assert_eq!(outcome.io_issued_e8s, t(60));
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(50)
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_11"),
            t(30)
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_12"),
            t(10)
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_13"),
            t(10)
        );
        let protocol = state(&fixture).protocol;
        assert_eq!(protocol.two_week_staked_icp_e8s, t(40));
        assert_eq!(protocol.liquid_icp_e8s, t(60));
    }

    #[test]
    fn governance_snapshot_unavailable_does_not_convert_reward_to_dust() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );

        let before_state = state(&fixture);
        let before_stable = stable_state(&fixture);
        let before_io_reserve = balance(&fixture.pic, fixture.io_ledger, "protocol_reserve");
        let before_io_transactions = transactions(&fixture.pic, fixture.io_ledger).len();
        set_sns_available(&fixture.pic, sns, false);

        let failed = tick(&fixture);
        assert_eq!(failed.processed_authorized_streams, 0);
        assert_eq!(failed.io_issued_e8s, 0);
        assert!(
            failed
                .errors
                .iter()
                .any(|err| err.contains("reward snapshot unavailable")),
            "{:?}",
            failed.errors
        );
        assert_eq!(state(&fixture), before_state);
        assert_eq!(
            stable_state(&fixture).operation_journal,
            before_stable.operation_journal
        );
        assert_eq!(
            stable_state(&fixture)
                .scheduler_cursors
                .last_scanned_icp_index_block,
            before_stable.scheduler_cursors.last_scanned_icp_index_block
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "protocol_reserve"),
            before_io_reserve
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger).len(),
            before_io_transactions
        );

        set_sns_available(&fixture.pic, sns, true);

        let recovered = tick(&fixture);
        assert!(recovered.errors.is_empty(), "{:?}", recovered.errors);
        assert_eq!(recovered.processed_authorized_streams, 1);
        assert_eq!(recovered.io_issued_e8s, t(60));
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(70)
        );
        assert_eq!(state(&fixture).processed_transaction_count, 1);
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(60))
                .count(),
            1
        );

        let replay = tick(&fixture);
        assert_eq!(replay.processed_authorized_streams, 0);
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(60))
                .count(),
            1
        );
    }

    #[test]
    fn governance_outage_cannot_skip_blocked_reward_via_newer_transaction() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        mint(
            &fixture.pic,
            fixture.icp_ledger,
            JUPITER_FAUCET_SOURCE,
            t(100),
            "fund_faucet",
        );
        let reward_block = transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        let faucet_block = faucet_send(
            &fixture,
            JUPITER_FAUCET_SOURCE,
            "stream_manager_deposit",
            t(100),
            "faucet",
        );
        assert_eq!(faucet_block, reward_block + 1);
        let reward_tx = format!("icp:{reward_block}");
        let faucet_tx = format!("icp:{faucet_block}");

        let before_state = state(&fixture);
        let before_stable = stable_state(&fixture);
        let before_io_reserve = balance(&fixture.pic, fixture.io_ledger, "protocol_reserve");
        let before_total_supply = before_state.protocol.total_io_supply_e8s;
        let before_io_transactions = transactions(&fixture.pic, fixture.io_ledger).len();
        set_sns_available(&fixture.pic, sns, false);

        let failed = tick(&fixture);
        assert_eq!(failed.processed_authorized_streams, 0);
        assert_eq!(failed.io_issued_e8s, 0);
        assert!(
            failed
                .errors
                .iter()
                .any(|err| err.contains("reward snapshot unavailable")),
            "{:?}",
            failed.errors
        );
        let outage_state = state(&fixture);
        let outage_stable = stable_state(&fixture);
        assert_eq!(outage_state, before_state);
        assert_eq!(
            outage_stable.operation_journal,
            before_stable.operation_journal
        );
        assert_eq!(
            outage_stable.processed_transactions,
            before_stable.processed_transactions
        );
        assert!(!outage_stable.processed_transactions.contains(&reward_tx));
        assert!(!outage_stable.processed_transactions.contains(&faucet_tx));
        assert_eq!(
            outage_stable.scheduler_cursors.icp_account_history_scan,
            before_stable.scheduler_cursors.icp_account_history_scan
        );
        assert_eq!(
            outage_stable.scheduler_cursors.last_scanned_icp_index_block,
            before_stable.scheduler_cursors.last_scanned_icp_index_block
        );
        assert!(
            outage_stable
                .scheduler_cursors
                .last_scanned_icp_index_block
                .is_none_or(|block| block < reward_block),
            "{:?}",
            outage_stable.scheduler_cursors.last_scanned_icp_index_block
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "protocol_reserve"),
            before_io_reserve
        );
        assert_eq!(
            state(&fixture).protocol.total_io_supply_e8s,
            before_total_supply
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger).len(),
            before_io_transactions
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(10)
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, JUPITER_FAUCET_SOURCE),
            0
        );

        set_sns_available(&fixture.pic, sns, true);

        let recovered = tick(&fixture);
        assert!(recovered.errors.is_empty(), "{:?}", recovered.errors);
        assert_eq!(recovered.processed_authorized_streams, 2);
        assert_eq!(recovered.io_issued_e8s, t(120));

        let recovered_stable = stable_state(&fixture);
        assert!(recovered_stable.processed_transactions.contains(&reward_tx));
        assert!(recovered_stable.processed_transactions.contains(&faucet_tx));
        assert_eq!(
            recovered_stable
                .operation_journal
                .iter()
                .filter(|op| op.source_transaction_id == reward_tx)
                .count(),
            1
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(70)
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, JUPITER_FAUCET_SOURCE),
            t(60)
        );
        let protocol = state(&fixture).protocol;
        assert_eq!(protocol.two_week_staked_icp_e8s, t(40));
        assert_eq!(protocol.two_year_staked_icp_e8s, t(40));
        assert_eq!(protocol.liquid_icp_e8s, t(120));
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(60))
                .count(),
            1
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == JUPITER_FAUCET_SOURCE && tx.amount_e8s == t(60))
                .count(),
            1
        );

        let replay_transactions = transactions(&fixture.pic, fixture.io_ledger).len();
        let replay = tick(&fixture);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.processed_authorized_streams, 0);
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger).len(),
            replay_transactions
        );
    }

    #[test]
    fn pocketic_live_two_week_partial_allocation_failure_does_not_double_pay_retry() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 2, 2));
        add_sns_neuron(&fixture, sns, sns_neuron(11, t(10), 1, 2));
        reject_to(&fixture.pic, fixture.io_ledger, "sns_neuron_11");
        seed_reward_cohort_and_advance_one_second(&fixture);

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
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
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_11"),
            t(10)
        );
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
            t(40)
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(30))
                .count(),
            1
        );
    }

    #[test]
    fn proof_found_after_submitted_upgrade_completes_once() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        set_stream_failpoint(
            &fixture,
            Some(DebugFailpoint::AfterTwoWeekRewardTransferBeforeJournalUpdate),
        );

        let trapped = fixture.pic.update_call(
            fixture.stream,
            Principal::anonymous(),
            "debug_tick",
            encode_one(()).unwrap(),
        );
        if let Ok(bytes) = &trapped {
            let outcome = decode_one::<DebugTickOutcome>(bytes).unwrap();
            panic!("reward failpoint should trap the tick; outcome was {outcome:?}");
        }
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(70)
        );
        assert_eq!(state(&fixture).processed_transaction_count, 0);
        let before_upgrade = stable_state(&fixture);
        let recipient = &before_upgrade.operation_journal[0].two_week_recipients[0];
        assert!(matches!(
            recipient
                .reward_transfer_attempt
                .as_ref()
                .and_then(|attempt| attempt.lifecycle.as_ref()),
            Some(io_stream_manager::RewardTransferAttemptLifecycle::SubmittedAwaitingResult { .. })
        ));
        assert!(recipient.ledger_transfer_proof_scan_state.is_none());

        upgrade_stream(&fixture);
        let recovered = tick(&fixture);
        assert!(recovered.errors.is_empty(), "{:?}", recovered.errors);
        assert_eq!(recovered.processed_authorized_streams, 1);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(70)
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(60))
                .count(),
            1
        );
        assert_eq!(state(&fixture).processed_transaction_count, 1);
        let after_upgrade = stable_state(&fixture);
        let recipient = &after_upgrade.operation_journal[0].two_week_recipients[0];
        assert!(matches!(
            recipient
                .reward_transfer_attempt
                .as_ref()
                .and_then(|attempt| attempt.lifecycle.as_ref()),
            Some(io_stream_manager::RewardTransferAttemptLifecycle::Proven { .. })
        ));
    }

    #[test]
    fn submitted_upgrade_recovery_asserts_exact_stake_delta() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);
        let staking_account = sns_neuron_staking_account(sns, 10);

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        set_stream_failpoint(
            &fixture,
            Some(DebugFailpoint::AfterTwoWeekRewardTransferBeforeJournalUpdate),
        );

        assert!(fixture
            .pic
            .update_call(
                fixture.stream,
                Principal::anonymous(),
                "debug_tick",
                encode_one(()).unwrap(),
            )
            .is_err());
        let before_upgrade_neurons = sns_neurons(&fixture.pic, sns);
        assert_eq!(before_upgrade_neurons[0].staked_io_e8s, t(10));
        assert_eq!(
            balance_of_account(&fixture.pic, fixture.io_ledger, staking_account.clone()),
            t(70)
        );

        upgrade_stream(&fixture);
        let recovered = tick(&fixture);
        assert!(recovered.errors.is_empty(), "{:?}", recovered.errors);

        let after_upgrade_neurons = sns_neurons(&fixture.pic, sns);
        assert_eq!(after_upgrade_neurons[0].staked_io_e8s, t(70));
        assert_eq!(
            after_upgrade_neurons[0].staked_io_e8s - before_upgrade_neurons[0].staked_io_e8s,
            t(60)
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(60))
                .count(),
            1
        );
    }

    #[test]
    fn pending_fee_repreflight_survives_actual_same_wasm_upgrade() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        set_stream_failpoint(
            &fixture,
            Some(DebugFailpoint::AfterTwoWeekRewardPreflightBeforeTransfer),
        );
        let preflight_tick = tick(&fixture);
        assert!(preflight_tick.errors.iter().any(|err| err.contains(
            "AfterTwoWeekRewardPreflightBeforeTransfer triggered after two-week reward preflight"
        )));

        let preflighted = stable_state(&fixture);
        let op = &preflighted.operation_journal[0];
        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.ledger_fee_e8s),
            Some(10_000)
        );
        assert_eq!(op.reserved_reward_debit_e8s, Some(t(60) + 10_000));

        set_ledger_fee(&fixture.pic, fixture.io_ledger, 20_000);
        let bad_fee = tick(&fixture);
        assert!(!bad_fee.errors.is_empty());
        let pending = stable_state(&fixture);
        let op = &pending.operation_journal[0];
        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.status),
            Some(io_stream_manager::RewardPreflightStatus::Pending)
        );
        assert_eq!(
            op.reward_fee_repreflight
                .as_ref()
                .map(|evidence| evidence.observed_current_fee_e8s),
            Some(20_000)
        );
        assert_eq!(op.reserved_reward_debit_e8s, Some(t(60) + 10_000));

        upgrade_stream(&fixture);
        let recovered = tick(&fixture);
        assert!(recovered.errors.is_empty(), "{:?}", recovered.errors);
        assert_eq!(recovered.processed_authorized_streams, 1);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(70)
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(60))
                .count(),
            1
        );
    }

    #[test]
    fn submitted_attempt_without_callback_recovers_after_actual_same_wasm_upgrade() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        set_stream_failpoint(
            &fixture,
            Some(DebugFailpoint::AfterTwoWeekRewardTransferBeforeJournalUpdate),
        );

        assert!(fixture
            .pic
            .update_call(
                fixture.stream,
                Principal::anonymous(),
                "debug_tick",
                encode_one(()).unwrap(),
            )
            .is_err());
        let before_upgrade = stable_state(&fixture);
        let recipient = &before_upgrade.operation_journal[0].two_week_recipients[0];
        assert!(matches!(
            recipient
                .reward_transfer_attempt
                .as_ref()
                .and_then(|attempt| attempt.lifecycle.as_ref()),
            Some(io_stream_manager::RewardTransferAttemptLifecycle::SubmittedAwaitingResult { .. })
        ));
        assert!(recipient.ledger_transfer_proof_scan_state.is_none());

        upgrade_stream(&fixture);
        let recovered = tick(&fixture);
        assert!(recovered.errors.is_empty(), "{:?}", recovered.errors);
        assert_eq!(recovered.processed_authorized_streams, 1);
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(60))
                .count(),
            1
        );
    }

    #[test]
    fn proof_required_without_cursor_recovers_after_actual_same_wasm_upgrade() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        set_stream_failpoint(
            &fixture,
            Some(DebugFailpoint::AfterTwoWeekRewardTransferBeforeJournalUpdate),
        );

        assert!(fixture
            .pic
            .update_call(
                fixture.stream,
                Principal::anonymous(),
                "debug_tick",
                encode_one(()).unwrap(),
            )
            .is_err());
        set_stream_failpoint(&fixture, None);
        set_index_lag(&fixture.pic, fixture.io_index, 10);

        let proof_tick = tick(&fixture);
        assert!(
            !proof_tick.errors.is_empty(),
            "index lag should force proof-required uncertainty"
        );
        let proof_required = stable_state(&fixture);
        let recipient = &proof_required.operation_journal[0].two_week_recipients[0];
        assert!(matches!(
            recipient
                .reward_transfer_attempt
                .as_ref()
                .and_then(|attempt| attempt.lifecycle.as_ref()),
            Some(io_stream_manager::RewardTransferAttemptLifecycle::ProofRequired { .. })
        ));
        assert!(recipient
            .ledger_transfer_proof_scan_state
            .as_ref()
            .is_none_or(|state| state.cursor.latest_cursor.is_none()));
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(60))
                .count(),
            1
        );
        assert_eq!(sns_neurons(&fixture.pic, sns)[0].staked_io_e8s, t(10));

        set_index_lag(&fixture.pic, fixture.io_index, 0);
        upgrade_stream(&fixture);
        let recovered = tick(&fixture);
        assert!(recovered.errors.is_empty(), "{:?}", recovered.errors);
        assert_eq!(recovered.processed_authorized_streams, 1);
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(60))
                .count(),
            1
        );
        assert_eq!(sns_neurons(&fixture.pic, sns)[0].staked_io_e8s, t(70));
        let completed = stable_state(&fixture);
        assert_eq!(completed.processed_transactions.len(), 1);
        assert_eq!(
            completed.operation_journal[0].reserved_reward_debit_e8s,
            Some(0)
        );
    }

    #[test]
    fn spent_uncommitted_reservation_survives_actual_same_wasm_upgrade() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        set_stream_failpoint(
            &fixture,
            Some(DebugFailpoint::AfterTwoWeekRewardTransferBeforeGovernanceRefresh),
        );

        let interrupted = tick(&fixture);
        assert!(interrupted.errors.iter().any(|err| err.contains(
            "AfterTwoWeekRewardTransferBeforeGovernanceRefresh triggered after two-week reward transfer"
        )));
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(70)
        );
        let pending = stable_state(&fixture);
        let reservation = pending.operation_journal[0].reward_reservation.unwrap();
        assert_eq!(reservation.unspent_reserved_reward_debit_e8s, 0);
        assert_eq!(
            reservation.externally_spent_but_uncommitted_reward_debit_e8s,
            t(60) + 10_000
        );
        assert_eq!(pending.processed_transactions.len(), 0);

        upgrade_stream(&fixture);
        let recovered = tick(&fixture);
        assert!(recovered.errors.is_empty(), "{:?}", recovered.errors);
        assert_eq!(recovered.processed_authorized_streams, 1);
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(60))
                .count(),
            1
        );
        let neurons = sns_neurons(&fixture.pic, sns);
        assert_eq!(neurons[0].staked_io_e8s, t(70));
        let completed = stable_state(&fixture);
        assert_eq!(
            completed.operation_journal[0].reserved_reward_debit_e8s,
            Some(0)
        );
        assert_eq!(completed.processed_transactions.len(), 1);
    }

    #[test]
    fn partial_distribution_fee_change_never_retransfers_prior_recipient() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        add_sns_neuron(&fixture, sns, sns_neuron(11, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        set_stream_failpoint(
            &fixture,
            Some(DebugFailpoint::AfterTwoWeekRewardTransferBeforeGovernanceRefresh),
        );

        let interrupted = tick(&fixture);
        assert!(interrupted.errors.iter().any(|err| err.contains(
            "AfterTwoWeekRewardTransferBeforeGovernanceRefresh triggered after two-week reward transfer"
        )));
        set_stream_failpoint(&fixture, None);

        let after_first = stable_state(&fixture);
        let op = &after_first.operation_journal[0];
        let first_before = op.two_week_recipients[0].clone();
        assert!(matches!(
            first_before
                .reward_transfer_attempt
                .as_ref()
                .and_then(|attempt| attempt.lifecycle.as_ref()),
            Some(io_stream_manager::RewardTransferAttemptLifecycle::Proven {
                block,
                ..
            }) if Some(*block) == first_before.transfer_block_index
        ));
        assert_eq!(
            first_before.ledger_transfer_status,
            Some(io_stream_manager::TransferStatus::Succeeded)
        );
        assert_eq!(
            first_before.ledger_transfer_block,
            first_before.transfer_block_index
        );
        assert_eq!(first_before.ledger_transfer_fee_e8s, Some(10_000));
        assert_eq!(first_before.reward_amount_received_e8s, Some(t(30)));
        assert_eq!(first_before.reserve_debit_e8s, Some(t(30) + 10_000));
        let initial_reservation = op.reward_reservation.unwrap();
        assert_eq!(
            initial_reservation.externally_spent_but_uncommitted_reward_debit_e8s,
            t(30) + 10_000
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(30))
                .count(),
            1
        );

        set_ledger_fee(&fixture.pic, fixture.io_ledger, 20_000);
        let bad_fee = tick(&fixture);
        assert!(!bad_fee.errors.is_empty());
        let manual = stable_state(&fixture);
        let op = &manual.operation_journal[0];
        let manual_preflight = op.reward_preflight.clone();
        let manual_reservation = op.reward_reservation;
        let first_manual = op.two_week_recipients[0].clone();
        let second_manual = op.two_week_recipients[1].clone();
        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.status),
            Some(io_stream_manager::RewardPreflightStatus::ManualReconciliationRequired)
        );
        assert!(op.reward_fee_repreflight.is_none());
        assert_eq!(op.two_week_recipients[0], first_before);
        assert!(matches!(
            op.two_week_recipients[1]
                .reward_transfer_attempt
                .as_ref()
                .and_then(|attempt| attempt.lifecycle.as_ref()),
            Some(io_stream_manager::RewardTransferAttemptLifecycle::SubmittedAwaitingResult { .. })
        ));
        assert_eq!(
            op.two_week_recipients[1].ledger_transfer_status,
            Some(io_stream_manager::TransferStatus::FailedTerminal)
        );
        assert_eq!(
            op.reward_reservation,
            Some(io_stream_manager::RewardReservation {
                unspent_reserved_reward_debit_e8s: t(30) + 10_000,
                externally_spent_but_uncommitted_reward_debit_e8s: t(30) + 10_000,
            })
        );
        assert_eq!(op.reserved_reward_debit_e8s, Some(t(60) + 20_000));
        assert_eq!(manual.processed_transactions.len(), 0);
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(30))
                .count(),
            1
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_11" && tx.amount_e8s == t(30))
                .count(),
            0
        );
        let model_before_upgrade = state(&fixture).protocol;

        upgrade_stream(&fixture);
        let replay = tick(&fixture);
        assert_eq!(replay.processed_authorized_streams, 0);
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s == t(30))
                .count(),
            1
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_11" && tx.amount_e8s == t(30))
                .count(),
            0
        );
        assert_eq!(state(&fixture).protocol, model_before_upgrade);
        let after_upgrade = stable_state(&fixture);
        let op = &after_upgrade.operation_journal[0];
        assert_eq!(op.reward_preflight, manual_preflight);
        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.status),
            Some(io_stream_manager::RewardPreflightStatus::ManualReconciliationRequired)
        );
        assert_eq!(op.reward_reservation, manual_reservation);
        assert_eq!(op.two_week_recipients[0], first_manual);
        assert_eq!(op.two_week_recipients[1], second_manual);
        assert_eq!(
            op.two_week_recipients[0].ledger_transfer_block,
            first_before.transfer_block_index
        );
        assert_eq!(
            op.two_week_recipients[0].ledger_transfer_fee_e8s,
            Some(10_000)
        );
        assert_eq!(
            op.two_week_recipients[0].reward_amount_received_e8s,
            Some(t(30))
        );
        assert_eq!(
            op.two_week_recipients[0].reserve_debit_e8s,
            Some(t(30) + 10_000)
        );
        assert_eq!(after_upgrade.processed_transactions.len(), 0);
    }

    #[test]
    fn pocketic_live_second_two_week_reward_after_completed_reward_succeeds() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(250),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );

        let first = tick(&fixture);
        assert!(first.errors.is_empty(), "{:?}", first.errors);
        assert_eq!(first.processed_authorized_streams, 1);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(70)
        );

        let refreshed = tick(&fixture);
        assert!(refreshed.errors.is_empty(), "{:?}", refreshed.errors);
        assert_eq!(refreshed.processed_authorized_streams, 0);
        assert_eq!(
            stable_state(&fixture)
                .reward_cohort
                .as_ref()
                .map(|cohort| cohort.generation),
            Some(2)
        );
        fixture.pic.advance_time(std::time::Duration::from_secs(1));

        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(50),
            TWO_WEEK_MATURITY_MEMO,
        );

        let second = tick(&fixture);
        assert!(second.errors.is_empty(), "{:?}", second.errors);
        assert_eq!(second.processed_authorized_streams, 1);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(100)
        );
        assert_eq!(state(&fixture).processed_transaction_count, 2);
        assert_eq!(
            transactions(&fixture.pic, fixture.io_ledger)
                .iter()
                .filter(|tx| tx.to == "sns_neuron_10" && tx.amount_e8s != t(10))
                .count(),
            2
        );
    }

    #[test]
    fn post_capture_topup_waits_until_next_reward_cohort() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        add_sns_neuron(&fixture, sns, sns_neuron(11, t(10), 1, 1));

        seed_reward_cohort_and_advance_one_second(&fixture);
        assert_eq!(
            stable_state(&fixture)
                .reward_cohort
                .as_ref()
                .map(|cohort| cohort.generation),
            Some(1)
        );

        transfer_to_account(
            &fixture.pic,
            fixture.io_ledger,
            "protocol_reserve",
            sns_neuron_staking_account(sns, 10),
            t(30),
            "post-capture-topup",
        );
        claim_or_refresh_sns_neuron(&fixture.pic, sns, 10);
        assert_eq!(sns_neurons(&fixture.pic, sns)[0].staked_io_e8s, t(40));

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(250),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        let first = tick(&fixture);
        assert!(first.errors.is_empty(), "{:?}", first.errors);
        assert_eq!(first.processed_authorized_streams, 1);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(70)
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_11"),
            t(40)
        );
        assert!(tick(&fixture).errors.is_empty());
        fixture.pic.advance_time(std::time::Duration::from_secs(1));

        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        let second = tick(&fixture);
        assert!(second.errors.is_empty(), "{:?}", second.errors);
        assert_eq!(second.processed_authorized_streams, 1);
        let second_a = t(60) * 70 / 110;
        let second_b = t(60) * 40 / 110;
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(70) + second_a
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_11"),
            t(40) + second_b
        );

        upgrade_stream(&fixture);
        let replay = tick(&fixture);
        assert_eq!(replay.processed_authorized_streams, 0);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
    }

    #[test]
    fn new_stake_after_capture_cannot_claim_prior_maturity() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        add_sns_neuron(&fixture, sns, sns_neuron(11, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);

        add_sns_neuron(&fixture, sns, sns_neuron(12, t(10), 1, 1));
        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(250),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        let first = tick(&fixture);
        assert!(first.errors.is_empty(), "{:?}", first.errors);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(40)
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_11"),
            t(40)
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_12"),
            t(10)
        );

        assert!(tick(&fixture).errors.is_empty());
        fixture.pic.advance_time(std::time::Duration::from_secs(1));
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        let second = tick(&fixture);
        assert!(second.errors.is_empty(), "{:?}", second.errors);
        assert!(balance(&fixture.pic, fixture.io_ledger, "sns_neuron_12") > t(10));
    }

    #[test]
    fn dissolving_cohort_member_share_remains_protocol_dust() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        add_sns_neuron(&fixture, sns, sns_neuron(11, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);

        let mut dissolving = sns_neuron(11, t(10), 1, 1);
        dissolving.is_dissolving = true;
        update_sns_neuron(&fixture, sns, dissolving);
        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        let outcome = tick(&fixture);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(40)
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_11"),
            t(10)
        );
        let completed = stable_state(&fixture)
            .operation_journal
            .into_iter()
            .find(|op| op.kind == io_stream_manager::StreamOperationKind::TwoWeekMaturityStream)
            .expect("completed reward operation");
        assert_eq!(
            completed
                .reward_preflight
                .as_ref()
                .map(|preflight| preflight.dust_e8s),
            Some(t(30))
        );
    }

    #[test]
    fn stale_reward_cohort_cannot_process_maturity() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);
        let cursor_before = stable_state(&fixture)
            .scheduler_cursors
            .last_scanned_icp_index_block;

        fixture.pic.advance_time(std::time::Duration::from_secs(
            io_core_model::TWO_WEEK_SECONDS + 1,
        ));
        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );
        let blocked = tick(&fixture);
        assert!(blocked
            .errors
            .iter()
            .any(|err| err.contains("beyond reward cohort expiry")));
        assert_eq!(blocked.processed_authorized_streams, 0);
        assert_eq!(state(&fixture).processed_transaction_count, 0);
        assert_eq!(
            stable_state(&fixture)
                .scheduler_cursors
                .last_scanned_icp_index_block,
            cursor_before
        );
    }

    #[test]
    fn consumed_reward_cohort_survives_actual_same_wasm_upgrade() {
        let Some(fixture) = setup_stream(true) else {
            return;
        };
        let sns = fixture.sns_governance.expect("sns governance installed");
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        add_sns_neuron(&fixture, sns, sns_neuron(11, t(10), 1, 1));
        reject_to(&fixture.pic, fixture.io_ledger, "sns_neuron_11");
        seed_reward_cohort_and_advance_one_second(&fixture);

        mint(
            &fixture.pic,
            fixture.icp_ledger,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            "fund_nns_manager",
        );
        transfer_to_stream_deposit(
            &fixture,
            IO_NNS_NEURON_MANAGER_SOURCE,
            t(100),
            TWO_WEEK_MATURITY_MEMO,
        );

        let interrupted = tick(&fixture);
        assert!(!interrupted.errors.is_empty());
        assert_eq!(interrupted.processed_authorized_streams, 0);
        let before_upgrade = stable_state(&fixture);
        let consumed = before_upgrade
            .reward_cohort
            .as_ref()
            .expect("reward cohort should exist after operation creation");
        let consumed_operation_id = consumed
            .consumed_by_operation_id
            .as_ref()
            .expect("reward cohort should be consumed by the pending operation")
            .clone();
        let pending_op = before_upgrade
            .operation_journal
            .iter()
            .find(|op| op.operation_id == consumed_operation_id)
            .expect("consumed operation should be journaled");
        assert_eq!(
            pending_op.kind,
            io_stream_manager::StreamOperationKind::TwoWeekMaturityStream
        );
        assert_ne!(
            pending_op.phase,
            io_stream_manager::OperationPhase::Completed
        );
        assert_eq!(pending_op.two_week_recipients.len(), 2);
        let recipient_plan = pending_op
            .two_week_recipients
            .iter()
            .map(|recipient| {
                (
                    recipient.sns_neuron_id.clone(),
                    recipient.neuron_id,
                    recipient.amount_e8s,
                )
            })
            .collect::<Vec<_>>();

        upgrade_stream(&fixture);
        let after_upgrade = stable_state(&fixture);
        let upgraded_cohort = after_upgrade
            .reward_cohort
            .as_ref()
            .expect("reward cohort should survive upgrade");
        assert_eq!(upgraded_cohort.generation, consumed.generation);
        assert_eq!(
            upgraded_cohort.consumed_by_operation_id.as_ref(),
            Some(&consumed_operation_id)
        );
        assert_eq!(
            after_upgrade
                .operation_journal
                .iter()
                .find(|op| op.operation_id == consumed_operation_id)
                .unwrap()
                .two_week_recipients
                .iter()
                .map(|recipient| {
                    (
                        recipient.sns_neuron_id.clone(),
                        recipient.neuron_id,
                        recipient.amount_e8s,
                    )
                })
                .collect::<Vec<_>>(),
            recipient_plan
        );

        clear_rejections(&fixture.pic, fixture.io_ledger);
        let completed = tick(&fixture);
        assert!(completed.errors.is_empty(), "{:?}", completed.errors);
        assert_eq!(completed.processed_authorized_streams, 1);
        let after_completion = stable_state(&fixture);
        let completed_ops = after_completion
            .operation_journal
            .iter()
            .filter(|op| op.kind == io_stream_manager::StreamOperationKind::TwoWeekMaturityStream)
            .collect::<Vec<_>>();
        assert_eq!(completed_ops.len(), 1);
        assert_eq!(completed_ops[0].operation_id, consumed_operation_id);
        assert_eq!(
            completed_ops[0].phase,
            io_stream_manager::OperationPhase::Completed
        );
        assert!(after_completion
            .reward_cohort
            .as_ref()
            .is_some_and(|cohort| {
                cohort.generation > consumed.generation
                    || cohort.consumed_by_operation_id.as_ref() == Some(&consumed_operation_id)
            }));
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_10"),
            t(40)
        );
        assert_eq!(
            balance(&fixture.pic, fixture.io_ledger, "sns_neuron_11"),
            t(40)
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
        assert_eq!(failed.scanned_io_transactions, 1);
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
        assert_eq!(retry.scanned_io_transactions, 0);
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
    fn pocketic_live_redemption_preview_failure_does_not_commit_scan_state() {
        let Some(fixture) = setup_stream(false) else {
            return;
        };

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
        assert_eq!(failed.scanned_io_transactions, 1);
        assert_eq!(failed.processed_redemptions, 0);

        let replay = tick(&fixture);
        assert!(!replay.errors.is_empty());
        assert_eq!(replay.scanned_io_transactions, 1);
        assert_eq!(replay.processed_redemptions, 0);
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
        add_sns_neuron(&fixture, sns, sns_neuron(10, t(10), 1, 1));
        add_sns_neuron(&fixture, sns, sns_neuron(11, t(10), 1, 1));
        seed_reward_cohort_and_advance_one_second(&fixture);
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

fn neuron(id: u64, stake: u128, voted: u64, total: u64) -> RewardParticipant {
    RewardParticipant {
        sns_neuron_id: io_reward_policy::compatibility_sns_neuron_id_from_u64(id),
        neuron_id: id,
        frozen_stake_e8s: stake,
        eligible_closed_proposals: total,
        voted_closed_proposals: voted,
        destination_is_currently_eligible: true,
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
    let alloc = manager
        .allocate_two_week_maturity_io(two_week.io_issued_e8s, &neurons)
        .unwrap();
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
    dissolving.destination_is_currently_eligible = false;
    let mut genesis = neuron(13, t(10), 1, 1);
    genesis.destination_is_currently_eligible = false;
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
    for amount in 1..3u128 {
        let tx = format!("tiny-rejected-{amount}");
        let before = manager.state;
        let err = manager
            .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", amount, tx)
            .unwrap_err();
        assert!(matches!(
            err,
            StreamManagerError::Model(ModelError::BelowMinimumStreamDeposit { .. })
        ));
        assert_eq!(manager.state, before);
    }
    for amount in 3..100u128 {
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
    let out = manager
        .allocate_two_week_maturity_io(two_week.io_issued_e8s, &neurons)
        .unwrap();
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
