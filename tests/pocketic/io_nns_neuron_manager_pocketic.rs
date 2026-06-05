use io_nns_neuron_manager::{
    ManagerError, NnsNeuronManagerModel, RebalanceAction, TwoWeekPoolState,
    CONTROLLER_CANISTER_PRINCIPAL_TEXT, SECONDS_PER_DAY, TWO_WEEK_DISSOLVE_SECONDS,
    TWO_YEAR_NNS_NEURON_ID,
};

fn t(n: u128) -> u128 {
    n * 100_000_000
}

fn pocketic_available() -> bool {
    std::env::var_os("POCKET_IC_BIN").is_some()
}

fn wasm(path: &str) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
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
struct LedgerTransaction {
    from: String,
    to: String,
    amount_e8s: u128,
    memo: String,
    block_index: u64,
    timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct CreateNeuronArgs {
    neuron_id: u64,
    principal_e8s: u128,
    dissolve_delay_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct NeuronAmountArgs {
    neuron_id: u64,
    amount_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct NeuronIdArgs {
    neuron_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct MockNeuron {
    neuron_id: u64,
    principal_e8s: u128,
    maturity_e8s: u128,
    dissolve_delay_seconds: u64,
    is_dissolving: bool,
    dissolve_started_at_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
struct DebugTickOutcome {
    disbursed_two_year_maturity_e8s: u128,
    disbursed_two_week_maturity_e8s: u128,
    disbursed_unwind_principal_e8s: u128,
    planned_pool_rebalances: u64,
    errors: Vec<String>,
}

#[cfg(test)]
mod live {
    use super::*;
    use candid::{decode_one, encode_one, Principal};
    use pocket_ic::PocketIc;

    const CYCLES: u128 = 2_000_000_000_000;

    struct NnsFixture {
        pic: PocketIc,
        manager: Principal,
        manager_wasm: Vec<u8>,
        ledger: Principal,
        governance: Option<Principal>,
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

    fn setup_nns_manager(
        with_governance: bool,
        args: io_nns_neuron_manager::InitArgs,
    ) -> Option<NnsFixture> {
        if !pocketic_available() {
            eprintln!("skipping real PocketIC test because POCKET_IC_BIN is not set");
            return None;
        }

        let manager_wasm =
            required_wasm("target/wasm32-unknown-unknown/debug/io_nns_neuron_manager.wasm")?;
        let ledger_wasm =
            required_wasm("target/wasm32-unknown-unknown/debug/mock_icp_ledger.wasm")?;
        let governance_wasm = if with_governance {
            Some(required_wasm(
                "target/wasm32-unknown-unknown/debug/mock_nns_governance.wasm",
            )?)
        } else {
            None
        };

        let pic = PocketIc::new();
        let ledger = create_canister(&pic, ledger_wasm, vec![]);
        let governance = governance_wasm.map(|wasm| create_canister(&pic, wasm, vec![]));
        mint(&pic, ledger, "io_nns_neuron_manager", t(10_000), "initial");

        let mut args = args;
        args.icp_ledger_principal_text = Some(ledger.to_text());
        args.nns_governance_principal_text = governance.map(|p| p.to_text());
        let manager = create_canister(&pic, manager_wasm.clone(), encode_one(args).unwrap());
        Some(NnsFixture {
            pic,
            manager,
            manager_wasm,
            ledger,
            governance,
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

    fn state(fixture: &NnsFixture) -> io_nns_neuron_manager::ApiState {
        let bytes = fixture
            .pic
            .query_call(
                fixture.manager,
                Principal::anonymous(),
                "debug_get_state",
                encode_one(()).unwrap(),
            )
            .expect("state");
        decode_one::<io_nns_neuron_manager::ApiState>(&bytes).unwrap()
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

    fn tick(fixture: &NnsFixture) -> DebugTickOutcome {
        let bytes = fixture
            .pic
            .update_call(
                fixture.manager,
                Principal::anonymous(),
                "debug_tick",
                encode_one(()).unwrap(),
            )
            .expect("manager tick");
        decode_one::<DebugTickOutcome>(&bytes).unwrap()
    }

    fn upgrade_manager(fixture: &NnsFixture) {
        fixture
            .pic
            .upgrade_canister(fixture.manager, fixture.manager_wasm.clone(), vec![], None)
            .expect("upgrade nns neuron manager");
    }

    fn advance_model_time(fixture: &NnsFixture, seconds: u64, annual_bps: u128) {
        fixture
            .pic
            .update_call(
                fixture.manager,
                Principal::anonymous(),
                "debug_advance_model_time",
                encode_one(io_nns_neuron_manager::AdvanceModelTimeRequest {
                    elapsed_seconds: seconds,
                    annual_bps: Some(annual_bps),
                })
                .unwrap(),
            )
            .expect("advance model time");
    }

    fn create_neuron(pic: &PocketIc, governance: Principal, neuron_id: u64, principal_e8s: u128) {
        pic.update_call(
            governance,
            Principal::anonymous(),
            "debug_create_neuron",
            encode_one(CreateNeuronArgs {
                neuron_id,
                principal_e8s,
                dissolve_delay_seconds: TWO_WEEK_DISSOLVE_SECONDS,
            })
            .unwrap(),
        )
        .expect("create mock nns neuron");
    }

    fn add_maturity(pic: &PocketIc, governance: Principal, neuron_id: u64, amount_e8s: u128) {
        let bytes = pic
            .update_call(
                governance,
                Principal::anonymous(),
                "debug_add_maturity",
                encode_one(NeuronAmountArgs {
                    neuron_id,
                    amount_e8s,
                })
                .unwrap(),
            )
            .expect("add maturity");
        decode_one::<Result<u128, String>>(&bytes)
            .unwrap()
            .expect("add maturity result");
    }

    fn advance_governance_time(pic: &PocketIc, governance: Principal, seconds: u64) {
        pic.update_call(
            governance,
            Principal::anonymous(),
            "debug_advance_time",
            encode_one(seconds).unwrap(),
        )
        .expect("advance mock governance time");
    }

    fn get_neuron(pic: &PocketIc, governance: Principal, neuron_id: u64) -> Option<MockNeuron> {
        let bytes = pic
            .query_call(
                governance,
                Principal::anonymous(),
                "debug_get_neuron",
                encode_one(NeuronIdArgs { neuron_id }).unwrap(),
            )
            .expect("get neuron");
        decode_one::<Option<MockNeuron>>(&bytes).unwrap()
    }

    #[test]
    fn pocketic_live_mock_nns_governance_maturity_emits_two_year_stream_transfer() {
        let Some(fixture) = setup_nns_manager(true, io_nns_neuron_manager::InitArgs::default())
        else {
            return;
        };
        let governance = fixture.governance.expect("governance installed");
        create_neuron(&fixture.pic, governance, TWO_YEAR_NNS_NEURON_ID, t(1_000));
        add_maturity(&fixture.pic, governance, TWO_YEAR_NNS_NEURON_ID, t(100));

        let outcome = tick(&fixture);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.disbursed_two_year_maturity_e8s, t(100));
        assert_eq!(
            get_neuron(&fixture.pic, governance, TWO_YEAR_NNS_NEURON_ID)
                .unwrap()
                .maturity_e8s,
            0
        );
        let txs = transactions(&fixture.pic, fixture.ledger);
        assert!(txs.iter().any(|tx| {
            tx.from == "io_nns_neuron_manager"
                && tx.to == "stream_manager_deposit"
                && tx.amount_e8s == t(100)
                && tx.memo == "two_year_maturity"
        }));
    }

    #[test]
    fn pocketic_live_model_two_week_maturity_emits_two_week_stream_transfer() {
        let Some(fixture) = setup_nns_manager(
            false,
            io_nns_neuron_manager::InitArgs {
                initial_two_week_principal_e8s: t(500),
                model_annual_bps: 12_000,
                ..Default::default()
            },
        ) else {
            return;
        };
        fixture
            .pic
            .advance_time(std::time::Duration::from_secs(30 * SECONDS_PER_DAY));
        advance_model_time(&fixture, 30 * SECONDS_PER_DAY, 12_000);

        let outcome = tick(&fixture);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert!(outcome.disbursed_two_week_maturity_e8s > 0);
        let two_week = outcome.disbursed_two_week_maturity_e8s;
        let txs = transactions(&fixture.pic, fixture.ledger);
        assert!(txs.iter().any(|tx| {
            tx.from == "io_nns_neuron_manager"
                && tx.to == "stream_manager_deposit"
                && tx.amount_e8s == two_week
                && tx.memo == "two_week_maturity"
        }));
    }

    #[test]
    fn pocketic_live_model_maturity_transfer_failure_keeps_maturity_retryable() {
        let Some(fixture) = setup_nns_manager(
            false,
            io_nns_neuron_manager::InitArgs {
                initial_two_year_principal_e8s: t(1_000),
                model_annual_bps: 12_000,
                ..Default::default()
            },
        ) else {
            return;
        };
        fixture
            .pic
            .advance_time(std::time::Duration::from_secs(30 * SECONDS_PER_DAY));
        advance_model_time(&fixture, 30 * SECONDS_PER_DAY, 12_000);
        let maturity_before = state(&fixture).two_year_neuron.maturity_e8s;
        assert!(maturity_before > 0);
        reject_to(&fixture.pic, fixture.ledger, "stream_manager_deposit");

        let failed = tick(&fixture);
        assert!(!failed.errors.is_empty());
        assert_eq!(failed.disbursed_two_year_maturity_e8s, 0);
        assert_eq!(
            state(&fixture).two_year_neuron.maturity_e8s,
            maturity_before
        );
        assert_eq!(
            transactions(&fixture.pic, fixture.ledger)
                .iter()
                .filter(|tx| tx.memo == "two_year_maturity")
                .count(),
            0
        );

        clear_rejections(&fixture.pic, fixture.ledger);
        upgrade_manager(&fixture);
        let retry = tick(&fixture);
        assert!(retry.errors.is_empty(), "{:?}", retry.errors);
        assert_eq!(retry.disbursed_two_year_maturity_e8s, maturity_before);
        assert_eq!(state(&fixture).two_year_neuron.maturity_e8s, 0);
        assert_eq!(
            transactions(&fixture.pic, fixture.ledger)
                .iter()
                .filter(|tx| {
                    tx.to == "stream_manager_deposit"
                        && tx.amount_e8s == maturity_before
                        && tx.memo == "two_year_maturity"
                })
                .count(),
            1
        );
    }

    #[test]
    fn pocketic_live_unwind_disburses_only_after_two_week_fast_forward() {
        let Some(fixture) = setup_nns_manager(
            false,
            io_nns_neuron_manager::InitArgs {
                initial_two_week_principal_e8s: t(100),
                ..Default::default()
            },
        ) else {
            return;
        };

        fixture
            .pic
            .update_call(
                fixture.manager,
                Principal::anonymous(),
                "debug_plan_rebalance",
                encode_one(io_nns_neuron_manager::ApiTwoWeekPoolState {
                    target_staked_e8s: 0,
                    active_staked_e8s: t(100),
                    pending_unwind_e8s: 0,
                    pending_restake_e8s: 0,
                })
                .unwrap(),
            )
            .expect("plan unwind");
        let split = tick(&fixture);
        assert!(split.errors.is_empty(), "{:?}", split.errors);
        assert_eq!(split.planned_pool_rebalances, 1);
        assert_eq!(split.disbursed_unwind_principal_e8s, 0);

        advance_model_time(&fixture, TWO_WEEK_DISSOLVE_SECONDS - 1, 0);
        let early = tick(&fixture);
        assert_eq!(early.disbursed_unwind_principal_e8s, 0);

        advance_model_time(&fixture, 1, 0);
        let ready = tick(&fixture);
        assert!(ready.errors.is_empty(), "{:?}", ready.errors);
        assert_eq!(ready.disbursed_unwind_principal_e8s, t(100));
        let txs = transactions(&fixture.pic, fixture.ledger);
        assert!(txs.iter().any(|tx| {
            tx.from == "io_nns_neuron_manager"
                && tx.to == "stream_manager_deposit"
                && tx.amount_e8s == t(100)
                && tx.memo == "principal_unwind"
        }));
    }

    #[test]
    fn pocketic_live_scheduler_drives_mock_governance_split_start_and_principal_disburse() {
        let Some(fixture) = setup_nns_manager(
            true,
            io_nns_neuron_manager::InitArgs {
                initial_two_week_principal_e8s: t(100),
                ..Default::default()
            },
        ) else {
            return;
        };
        let governance = fixture.governance.expect("governance installed");
        create_neuron(&fixture.pic, governance, 2, t(100));

        fixture
            .pic
            .update_call(
                fixture.manager,
                Principal::anonymous(),
                "debug_plan_rebalance",
                encode_one(io_nns_neuron_manager::ApiTwoWeekPoolState {
                    target_staked_e8s: 0,
                    active_staked_e8s: t(100),
                    pending_unwind_e8s: 0,
                    pending_restake_e8s: 0,
                })
                .unwrap(),
            )
            .expect("plan unwind");
        let split = tick(&fixture);
        assert!(split.errors.is_empty(), "{:?}", split.errors);
        assert_eq!(split.planned_pool_rebalances, 1);
        let child = get_neuron(&fixture.pic, governance, 10_000).expect("mock child neuron");
        assert_eq!(child.principal_e8s, t(100));
        assert!(child.is_dissolving);

        advance_model_time(&fixture, TWO_WEEK_DISSOLVE_SECONDS, 0);
        advance_governance_time(&fixture.pic, governance, TWO_WEEK_DISSOLVE_SECONDS);
        let ready = tick(&fixture);
        assert!(ready.errors.is_empty(), "{:?}", ready.errors);
        assert_eq!(ready.disbursed_unwind_principal_e8s, t(100));
        assert!(get_neuron(&fixture.pic, governance, 10_000).is_none());
    }

    #[test]
    fn pocketic_live_scheduler_stops_and_merges_mock_governance_child_on_restake() {
        let Some(fixture) = setup_nns_manager(
            true,
            io_nns_neuron_manager::InitArgs {
                initial_two_week_principal_e8s: t(100),
                ..Default::default()
            },
        ) else {
            return;
        };
        let governance = fixture.governance.expect("governance installed");
        create_neuron(&fixture.pic, governance, 2, t(100));

        fixture
            .pic
            .update_call(
                fixture.manager,
                Principal::anonymous(),
                "debug_plan_rebalance",
                encode_one(io_nns_neuron_manager::ApiTwoWeekPoolState {
                    target_staked_e8s: 0,
                    active_staked_e8s: t(100),
                    pending_unwind_e8s: 0,
                    pending_restake_e8s: 0,
                })
                .unwrap(),
            )
            .expect("plan unwind");
        assert!(tick(&fixture).errors.is_empty());

        fixture
            .pic
            .update_call(
                fixture.manager,
                Principal::anonymous(),
                "debug_plan_rebalance",
                encode_one(io_nns_neuron_manager::ApiTwoWeekPoolState {
                    target_staked_e8s: t(100),
                    active_staked_e8s: 0,
                    pending_unwind_e8s: 0,
                    pending_restake_e8s: 0,
                })
                .unwrap(),
            )
            .expect("plan restake");
        let restake = tick(&fixture);
        assert!(restake.errors.is_empty(), "{:?}", restake.errors);
        assert_eq!(restake.planned_pool_rebalances, 1);
        assert_eq!(
            get_neuron(&fixture.pic, governance, 2)
                .expect("pool neuron")
                .principal_e8s,
            t(100)
        );
        let child = get_neuron(&fixture.pic, governance, 10_000).expect("merged child remains");
        assert!(!child.is_dissolving);
    }
}

#[test]
fn pocketic_model_nns_manager_constants_and_rebalance() {
    assert_eq!(TWO_YEAR_NNS_NEURON_ID, 6_345_890_886_899_317_159);
    assert_eq!(
        CONTROLLER_CANISTER_PRINCIPAL_TEXT,
        "oae4c-3iaaa-aaaar-qb5qq-cai"
    );
    let pool = TwoWeekPoolState {
        target_staked_e8s: 1_000,
        active_staked_e8s: 1_500,
        pending_unwind_e8s: 0,
        pending_restake_e8s: 0,
    };
    assert_eq!(
        pool.plan_rebalance(),
        RebalanceAction::SplitAndDissolve { amount_e8s: 500 }
    );
}

#[test]
fn pocketic_fast_forward_maturity_can_feed_downstream_streams() {
    let mut manager = NnsNeuronManagerModel::new(1_000_000_000, 500_000_000);
    manager.advance_time(30 * SECONDS_PER_DAY, 12_000); // exaggerated 120% APY for deterministic fast-forward testing.
    assert!(manager.two_year_neuron.maturity_e8s > 0);
    assert!(manager.two_week_pool.maturity_e8s > 0);

    let two_year_maturity = manager.disburse_two_year_maturity();
    let two_week_maturity = manager.disburse_two_week_maturity();
    assert!(two_year_maturity > two_week_maturity);
    assert_eq!(manager.two_year_neuron.maturity_e8s, 0);
    assert_eq!(manager.two_week_pool.maturity_e8s, 0);
}

#[test]
fn pocketic_fast_forward_unwind_principal_after_two_weeks() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000);
    let child = manager.split_and_start_unwind(250_000).unwrap();
    assert_eq!(
        manager.disburse_ready_unwind(child),
        Err(ManagerError::NeuronNotReady)
    );
    manager.advance_time(TWO_WEEK_DISSOLVE_SECONDS - 1, 0);
    assert_eq!(
        manager.disburse_ready_unwind(child),
        Err(ManagerError::NeuronNotReady)
    );
    manager.advance_time(1, 0);
    assert_eq!(manager.disburse_ready_unwind(child).unwrap(), 250_000);
}

#[test]
fn pocketic_cancel_dissolve_merges_unwind_back_before_it_becomes_liquid() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000);
    let child = manager.split_and_start_unwind(400_000).unwrap();
    manager.advance_time(7 * SECONDS_PER_DAY, 0);
    let merged = manager.cancel_unwind_and_merge_back(child).unwrap();
    assert_eq!(merged, 400_000);
    assert_eq!(manager.two_week_pool.principal_e8s, 1_000_000);
    assert!(manager.unwind_neurons.is_empty());
}

#[test]
fn pocketic_multiple_unwind_children_can_mature_and_disburse_independently() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000);
    let first = manager.split_and_start_unwind(100_000).unwrap();
    manager.advance_time(SECONDS_PER_DAY, 0);
    let second = manager.split_and_start_unwind(200_000).unwrap();

    manager.advance_time(TWO_WEEK_DISSOLVE_SECONDS - SECONDS_PER_DAY, 0);
    assert_eq!(manager.disburse_ready_unwind(first).unwrap(), 100_000);
    assert_eq!(
        manager.disburse_ready_unwind(second),
        Err(ManagerError::NeuronNotReady)
    );

    manager.advance_time(SECONDS_PER_DAY, 0);
    assert_eq!(manager.disburse_ready_unwind(second).unwrap(), 200_000);
    assert_eq!(manager.two_week_pool.principal_e8s, 700_000);
}

#[test]
fn pocketic_dissolving_child_does_not_receive_fast_forward_maturity() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000_000);
    let child = manager.split_and_start_unwind(500_000_000).unwrap();
    manager.advance_time(365 * SECONDS_PER_DAY, 10_000);
    let unwind = manager
        .unwind_neurons
        .iter()
        .find(|n| n.neuron_id == child)
        .unwrap();
    assert_eq!(unwind.maturity_e8s, 0);
    assert_eq!(manager.two_week_pool.maturity_e8s, 500_000_000);
}

#[test]
fn pocketic_rebalance_plan_handles_cancel_dissolve_batching() {
    let after_user_started_dissolving = TwoWeekPoolState {
        target_staked_e8s: 600_000,
        active_staked_e8s: 1_000_000,
        pending_unwind_e8s: 400_000,
        pending_restake_e8s: 0,
    };
    assert_eq!(
        after_user_started_dissolving.plan_rebalance(),
        RebalanceAction::None
    );

    let after_cancel_before_execution = TwoWeekPoolState {
        target_staked_e8s: 1_000_000,
        active_staked_e8s: 1_000_000,
        pending_unwind_e8s: 400_000,
        pending_restake_e8s: 400_000,
    };
    assert_eq!(
        after_cancel_before_execution.plan_rebalance(),
        RebalanceAction::None
    );
}

#[test]
fn pocketic_maturity_disbursement_is_idempotent_until_more_time_passes() {
    let mut manager = NnsNeuronManagerModel::new(1_000_000_000, 0);
    manager.advance_time(30 * SECONDS_PER_DAY, 12_000);
    let first = manager.disburse_two_year_maturity();
    assert!(first > 0);
    assert_eq!(manager.disburse_two_year_maturity(), 0);
    manager.advance_time(SECONDS_PER_DAY, 12_000);
    assert!(manager.disburse_two_year_maturity() > 0);
}

#[test]
fn pocketic_can_split_entire_two_week_pool_but_not_more() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000);
    let child = manager.split_and_start_unwind(1_000_000).unwrap();
    assert_eq!(manager.two_week_pool.principal_e8s, 0);
    assert_eq!(
        manager.split_and_start_unwind(1),
        Err(ManagerError::SplitExceedsMainPool)
    );
    manager.advance_time(TWO_WEEK_DISSOLVE_SECONDS, 0);
    assert_eq!(manager.disburse_ready_unwind(child).unwrap(), 1_000_000);
}

#[test]
fn pocketic_cancel_after_child_disbursed_is_unknown_and_requires_restake_path() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000);
    let child = manager.split_and_start_unwind(250_000).unwrap();
    manager.advance_time(TWO_WEEK_DISSOLVE_SECONDS, 0);
    assert_eq!(manager.disburse_ready_unwind(child).unwrap(), 250_000);
    assert_eq!(
        manager.cancel_unwind_and_merge_back(child),
        Err(ManagerError::UnknownUnwindNeuron)
    );
}

#[test]
fn pocketic_zero_time_advance_does_not_create_maturity() {
    let mut manager = NnsNeuronManagerModel::new(1_000_000_000, 1_000_000_000);
    manager.advance_time(0, 100_000);
    assert_eq!(manager.two_year_neuron.maturity_e8s, 0);
    assert_eq!(manager.two_week_pool.maturity_e8s, 0);
}
