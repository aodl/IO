use candid::{decode_one, encode_one, CandidType, Principal};
use io_nns_neuron_manager::{
    jupiter::{
        JupiterDeposit, JupiterOperation, JupiterPhase, NeuronSnapshot, StakeTransferSucceeded,
    },
    maturity::{
        ClaimReceiptDeliveryOperation, DisburseMaturitySubmission, DisburseMaturitySucceeded,
        MaturityCommandOperation, MaturityCommandPhase, MaturityEvidenceSource, MaturityKind,
        MaturityPlan, MintEvidence, MintProofState, PendingMaturityDisbursement,
        StakeMaturitySucceeded,
    },
    pool::{PassiveCohort, UnwindOperation, UnwindPhase},
    state::{Account, NnsOperation, NnsStateV1, PooledTarget},
    ApiError, InitArgs, JupiterProgress, Lifecycle, NnsConfig, NnsProgress, PooledTargetStatus,
};
use io_nns_types::backing::{
    CohortProofState, CompletedPoolCommand, PoolCommand, PoolCommandKind, PoolCommandPhase,
    PoolProgress, PoolReconciliationAction, PreparePoolReconciliationArgs, TopUpPermit,
    POOLED_PARENT_DELAY_SECONDS,
};
use pocket_ic::PocketIc;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const CYCLES: u128 = 2_000_000_000_000;

#[derive(Clone, Debug, CandidType, Deserialize)]
struct CreateNeuronArgs {
    neuron_id: u64,
    principal_e8s: u128,
    dissolve_delay_seconds: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct NeuronAmountArgs {
    neuron_id: u64,
    amount_e8s: u128,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct SetFolloweeArgs {
    neuron_id: u64,
    followee: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
struct LedgerCallCounters {
    fee: u64,
    total_supply: u64,
    balance: u64,
    allowance: u64,
    transfer: u64,
    transfer_from: u64,
    query_blocks: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum ControlledCommand {
    ClaimOrRefresh,
    IncreaseDissolveDelay,
    SetFollowing,
    StartDissolving,
    Merge,
    RefreshVotingPower,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct CommandControl {
    command: ControlledCommand,
    reject_before_effect: u64,
    malformed_after_effect: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
struct GovernanceCommandCounters {
    claim_or_refresh: u64,
    increase_dissolve_delay: u64,
    set_following: u64,
    start_dissolving: u64,
    merge: u64,
    refresh_voting_power: u64,
}

fn wasm(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/wasm32-unknown-unknown/debug/{name}.wasm"
        )),
    )
    .unwrap_or_else(|error| panic!("missing {name} debug Wasm: {error}"))
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
        .unwrap_or_else(|error| panic!("{method}: {error}")),
    )
    .unwrap()
}

struct RecoveryFixture {
    pic: PocketIc,
    ledger: Principal,
    governance: Principal,
    manager: Principal,
    stream: Principal,
}

impl RecoveryFixture {
    fn new() -> Self {
        let pic = PocketIc::new();
        let install = |name: &str| {
            let canister = pic.create_canister();
            pic.add_cycles(canister, CYCLES);
            pic.install_canister(canister, wasm(name), Vec::new(), None);
            canister
        };
        let ledger = install("mock_icp_ledger");
        let governance = install("mock_nns_governance");
        let manager = pic.create_canister();
        pic.add_cycles(manager, CYCLES);
        let sns_governance = Principal::from_slice(&[31; 29]);
        let stream = Principal::from_slice(&[32; 29]);
        let jupiter = Principal::from_slice(&[33; 29]);
        let staging = |byte| Account {
            owner: manager,
            subaccount: (byte != 0).then(|| vec![byte; 32]),
        };
        pic.install_canister(
            manager,
            wasm("io_nns_neuron_manager"),
            encode_one(InitArgs {
                config: NnsConfig {
                    sns_governance,
                    stream_manager: stream,
                    jupiter,
                    icp_ledger: ledger,
                    nns_governance: governance,
                    two_year_neuron_id: 41,
                    pooled_parent_memo: 42,
                    pooled_parent_followee_id: 43,
                    minimum_parent_stake_e8s: 100_000_000,
                    jupiter_account: Account {
                        owner: jupiter,
                        subaccount: None,
                    },
                    jupiter_staging: staging(0),
                    maturity_staging: staging(2),
                    stream_liquid_account: Account {
                        owner: stream,
                        subaccount: Some(vec![3; 32]),
                    },
                    expected_io_fee_e8s: 10_000,
                    expected_icp_fee_e8s: 10_000,
                    jupiter_activation_block_floor: 1,
                    audited_permanent_principal_e8s: 1_000_000,
                    transfer_retry_delay_nanos: 1_000_000_000,
                    ledger_deduplication_window_nanos: 86_400_000_000_000,
                },
            })
            .unwrap(),
            None,
        );
        let fixture = Self {
            pic,
            ledger,
            governance,
            manager,
            stream,
        };
        fixture.create_neuron(41, 1_000_000, 63_115_200);
        fixture.create_neuron(42, 1_000_000, POOLED_PARENT_DELAY_SECONDS);
        fixture
    }

    fn state(&self) -> NnsStateV1 {
        let mut state: NnsStateV1 = query(&self.pic, self.manager, "debug_get_state");
        state.lifecycle = Lifecycle::Ready;
        state.two_year_maturity_baseline_reconciled = true;
        state.active_operation = None;
        state.pooled_parent_id = None;
        state.pooled_parent_staking_account = None;
        state.live_cohorts.clear();
        state.last_completed_pool = None;
        state.last_completed_unwind = None;
        state.last_held_reconciliation = None;
        state.latest_reconciliation_generation = 0;
        state.latest_pooled_target = None;
        state.latest_started_two_week_generation = 0;
        state.latest_completed_two_week_generation = 0;
        state.pending_two_year_maturity = None;
        state.pending_two_week_maturity = None;
        state.last_two_year_maturity = None;
        state.last_two_week_maturity = None;
        state.next_operation_sequence = 2;
        state.control_epoch = 1;
        state
    }

    fn replace(&self, state: NnsStateV1) {
        update::<_, Result<(), String>>(
            &self.pic,
            self.manager,
            Principal::anonymous(),
            "debug_replace_state",
            state,
        )
        .unwrap();
    }

    fn upgrade(&self) {
        let before: NnsStateV1 = query(&self.pic, self.manager, "debug_get_state");
        self.pic
            .upgrade_canister(
                self.manager,
                wasm("io_nns_neuron_manager"),
                encode_one(()).unwrap(),
                None,
            )
            .unwrap();
        let after: NnsStateV1 = query(&self.pic, self.manager, "debug_get_state");
        let mut expected = before;
        expected.lifecycle = Lifecycle::Paused;
        assert_eq!(after, expected);
    }

    fn resume(&self) -> Result<NnsProgress, ApiError> {
        update(
            &self.pic,
            self.manager,
            Principal::anonymous(),
            "resume",
            (),
        )
    }

    fn assert_pending(&self) {
        let result = self.resume();
        assert!(
            matches!(result, Err(ApiError::Pending(_))),
            "expected Pending, got {result:?}"
        );
    }

    fn create_neuron(&self, neuron_id: u64, principal_e8s: u128, delay: u64) {
        let _: u64 = update(
            &self.pic,
            self.governance,
            Principal::anonymous(),
            "debug_create_neuron",
            CreateNeuronArgs {
                neuron_id,
                principal_e8s,
                dissolve_delay_seconds: delay,
            },
        );
    }

    fn add_maturity(&self, neuron_id: u64, amount_e8s: u128) {
        update::<_, Result<u128, String>>(
            &self.pic,
            self.governance,
            Principal::anonymous(),
            "debug_add_maturity",
            NeuronAmountArgs {
                neuron_id,
                amount_e8s,
            },
        )
        .unwrap();
    }

    fn refresh_credit(&self, neuron_id: u64, amount_e8s: u128) {
        update::<_, Result<(), String>>(
            &self.pic,
            self.governance,
            Principal::anonymous(),
            "debug_set_refresh_credit",
            NeuronAmountArgs {
                neuron_id,
                amount_e8s,
            },
        )
        .unwrap();
    }

    fn followee(&self, neuron_id: u64, followee: Option<u64>) {
        update::<_, Result<(), String>>(
            &self.pic,
            self.governance,
            Principal::anonymous(),
            "debug_set_followee",
            SetFolloweeArgs {
                neuron_id,
                followee,
            },
        )
        .unwrap();
    }

    fn control(&self, command: ControlledCommand, reject: u64, malformed: u64) {
        let _: () = update(
            &self.pic,
            self.governance,
            Principal::anonymous(),
            "debug_set_command_control",
            CommandControl {
                command,
                reject_before_effect: reject,
                malformed_after_effect: malformed,
            },
        );
    }

    fn governance_calls(&self) -> GovernanceCommandCounters {
        query(&self.pic, self.governance, "debug_get_command_counters")
    }
}

fn pool_operation(
    state: &NnsStateV1,
    parent_neuron_id: u64,
    parent_e8s: u128,
    credit_e8s: u128,
    phase: PoolCommandPhase,
) -> PoolCommand {
    PoolCommand {
        kind: if parent_e8s == 0 {
            PoolCommandKind::Bootstrap
        } else {
            PoolCommandKind::TopUp
        },
        permit: TopUpPermit {
            generation: 1,
            operation_sequence: 1,
            expected_parent_principal_e8s: parent_e8s,
            destination: Account {
                owner: state.config.nns_governance,
                subaccount: Some(vec![9; 32]),
            },
            expected_credit_e8s: credit_e8s,
            fee_e8s: 10_000,
            memo: vec![1; 32],
            prepared_at_nanos: 1,
            snapshot_fingerprint: vec![2; 32],
        },
        transfer_block_index: (!matches!(phase, PoolCommandPhase::AwaitingTransfer)).then_some(7),
        parent_neuron_id: Some(parent_neuron_id),
        phase,
    }
}

fn pool_state(
    fixture: &RecoveryFixture,
    parent_id: u64,
    parent_e8s: u128,
    credit_e8s: u128,
    phase: PoolCommandPhase,
) -> NnsStateV1 {
    let mut state = fixture.state();
    state.latest_reconciliation_generation = 1;
    state.latest_pooled_target = Some(PooledTarget {
        target_e8s: parent_e8s + credit_e8s,
        status: PooledTargetStatus::UnderTarget,
    });
    if parent_e8s > 0 {
        state.pooled_parent_id = Some(parent_id);
        state.pooled_parent_staking_account = Some(Account {
            owner: fixture.governance,
            subaccount: Some(vec![9; 32]),
        });
    }
    state.active_operation = Some(NnsOperation::Pool(pool_operation(
        &state, parent_id, parent_e8s, credit_e8s, phase,
    )));
    state
}

fn unwind_operation(phase: UnwindPhase, child_neuron_id: u64) -> UnwindOperation {
    let cleanup = matches!(
        phase,
        UnwindPhase::DelayIncreaseSubmitted
            | UnwindPhase::DelayIncreaseProved
            | UnwindPhase::MergePrepared
            | UnwindPhase::MergeSubmitted
            | UnwindPhase::MergeProved
    );
    UnwindOperation {
        operation_sequence: 1,
        generation: 1,
        reconciliation_request_fingerprint: vec![3; 32],
        target_e8s: 1_000_000,
        gross_e8s: 120_000,
        child_neuron_id,
        principal_e8s: 110_000,
        child_staking_subaccount: vec![4; 32],
        submitted_at_seconds: 1,
        expected_block_index: Some(9),
        child_maturity_e8s: if cleanup { 50_000 } else { 0 },
        parent_maturity_e8s: if cleanup { 20_000 } else { 0 },
        parent_principal_e8s: if cleanup { 1_000_000 } else { 0 },
        phase,
    }
}

fn unwind_state(fixture: &RecoveryFixture, phase: UnwindPhase, child_neuron_id: u64) -> NnsStateV1 {
    let mut state = fixture.state();
    state.active_operation = Some(NnsOperation::Unwind(unwind_operation(
        phase,
        child_neuron_id,
    )));
    state.pooled_parent_id = Some(42);
    state.pooled_parent_staking_account = Some(Account {
        owner: fixture.governance,
        subaccount: Some(vec![8; 32]),
    });
    state.latest_reconciliation_generation = 1;
    state.latest_pooled_target = Some(PooledTarget {
        target_e8s: 1_000_000,
        status: PooledTargetStatus::OverTarget,
    });
    state
}

fn delivering_maturity(state: &NnsStateV1, kind: MaturityKind) -> PendingMaturityDisbursement {
    let (neuron_id, retained, remaining, generation) = match kind {
        MaturityKind::TwoYear => (41, 80_000_000, 120_000_000, None),
        MaturityKind::TwoWeek => (42, 0, 200_000_000, Some(1)),
    };
    let plan = MaturityPlan {
        neuron: NeuronSnapshot {
            neuron_id,
            staking_subaccount: [6; 32],
            cached_stake_e8s: 1_000_000,
        },
        original_maturity_e8s: 200_000_000,
        original_staked_maturity_e8s: 0,
        stake_maturity_e8s: retained,
        remaining_maturity_e8s: remaining,
        destination: state.config.maturity_staging.clone(),
        requested_at_seconds: 1,
        entitlement_batch_generation: generation,
    };
    let stake = StakeMaturitySucceeded {
        plan,
        remaining_maturity_e8s: remaining,
        staked_maturity_e8s: retained,
        evidence_source: MaturityEvidenceSource::CommandResponse,
    };
    let submission = DisburseMaturitySubmission {
        stake: stake.clone(),
        submitted_at_seconds: 1,
    };
    PendingMaturityDisbursement {
        kind,
        neuron_id,
        nominal_disbursed_maturity_e8s: remaining,
        destination: state.config.maturity_staging.clone(),
        initiation_timestamp_seconds: 1,
        scheduled_finalization_timestamp_seconds: 604_801,
        stake_evidence: stake,
        disburse_evidence: DisburseMaturitySucceeded {
            submission,
            amount_disbursed_e8s: remaining,
            evidence_source: MaturityEvidenceSource::CommandResponse,
        },
        mint_proof: MintProofState::Delivering(MintEvidence {
            mint_block: 11,
            actual_minted_icp_e8s: u128::from(remaining),
            native_memo_u64: 604_801,
            created_at_time_nanos: 604_801_000_000_000,
        }),
    }
}

#[test]
fn every_persisted_governance_phase_recovers_and_exact_replay_is_call_free() {
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping Governance recovery PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = RecoveryFixture::new();
    let monetary_before: LedgerCallCounters =
        query(&fixture.pic, fixture.ledger, "debug_get_call_counters");

    let mut state = fixture.state();
    state.active_operation = Some(NnsOperation::Jupiter(Box::new(JupiterOperation {
        operation_sequence: 1,
        dispatch_epoch: 1,
        captured_control_epoch: 1,
        deposit: JupiterDeposit {
            block_index: 2,
            gross_e8s: 100_000,
            stake_e8s: 30_000,
            liquid_e8s: 50_000,
            fee_e8s: 10_000,
            created_at_time_nanos: 1,
        },
        phase: JupiterPhase::RefreshSubmitted(StakeTransferSucceeded {
            before: NeuronSnapshot {
                neuron_id: 41,
                staking_subaccount: {
                    let mut account = [0; 32];
                    account[24..].copy_from_slice(&41_u64.to_be_bytes());
                    account
                },
                cached_stake_e8s: 1_000_000,
            },
            block_index: 3,
        }),
    })));
    fixture.replace(state);
    fixture.refresh_credit(41, 30_000);
    fixture.control(ControlledCommand::ClaimOrRefresh, 1, 0);
    fixture.upgrade();
    fixture.assert_pending();
    fixture.assert_pending();
    assert_eq!(
        fixture.resume(),
        Ok(NnsProgress::Jupiter(JupiterProgress::StakeIncreaseProved))
    );
    assert_eq!(fixture.governance_calls().claim_or_refresh, 2);

    fixture.create_neuron(101, 100_000_000, 0);
    fixture.replace(pool_state(
        &fixture,
        101,
        0,
        100_000_000,
        PoolCommandPhase::DelaySubmitted {
            expected_delay_seconds: POOLED_PARENT_DELAY_SECONDS,
        },
    ));
    fixture.control(ControlledCommand::IncreaseDissolveDelay, 1, 0);
    let calls = fixture.governance_calls().increase_dissolve_delay;
    fixture.upgrade();
    fixture.assert_pending();
    fixture.assert_pending();
    fixture.assert_pending();
    assert_eq!(
        fixture.governance_calls().increase_dissolve_delay,
        calls + 2
    );

    fixture.create_neuron(102, 100_000_000, POOLED_PARENT_DELAY_SECONDS);
    fixture.followee(102, None);
    fixture.replace(pool_state(
        &fixture,
        102,
        0,
        100_000_000,
        PoolCommandPhase::FollowingSubmitted,
    ));
    fixture.control(ControlledCommand::SetFollowing, 1, 0);
    let calls = fixture.governance_calls().set_following;
    fixture.upgrade();
    fixture.assert_pending();
    fixture.assert_pending();
    fixture.assert_pending();
    assert_eq!(fixture.governance_calls().set_following, calls + 2);

    fixture.create_neuron(103, 100_000_000, POOLED_PARENT_DELAY_SECONDS);
    fixture.refresh_credit(103, 10_000_000);
    fixture.replace(pool_state(
        &fixture,
        103,
        100_000_000,
        10_000_000,
        PoolCommandPhase::RefreshSubmitted,
    ));
    fixture.control(ControlledCommand::ClaimOrRefresh, 1, 0);
    let calls = fixture.governance_calls().claim_or_refresh;
    fixture.upgrade();
    fixture.assert_pending();
    fixture.assert_pending();
    assert!(matches!(
        fixture.resume(),
        Ok(NnsProgress::Pool(PoolProgress::Completed { .. }))
    ));
    assert_eq!(fixture.governance_calls().claim_or_refresh, calls + 2);

    fixture.create_neuron(201, 110_000, 100);
    fixture.replace(unwind_state(
        &fixture,
        UnwindPhase::StartDissolvingSubmitted,
        201,
    ));
    fixture.control(ControlledCommand::StartDissolving, 0, 1);
    let calls = fixture.governance_calls().start_dissolving;
    fixture.upgrade();
    fixture.assert_pending();
    assert!(matches!(fixture.resume(), Ok(NnsProgress::Unwind(_))));
    assert_eq!(fixture.governance_calls().start_dissolving, calls + 1);

    fixture.create_neuron(202, 110_000, 100);
    fixture.replace(unwind_state(
        &fixture,
        UnwindPhase::StartDissolvingSubmitted,
        202,
    ));
    fixture.control(ControlledCommand::StartDissolving, 1, 0);
    let calls = fixture.governance_calls().start_dissolving;
    fixture.assert_pending();
    fixture.assert_pending();
    assert!(matches!(fixture.resume(), Ok(NnsProgress::Unwind(_))));
    assert_eq!(fixture.governance_calls().start_dissolving, calls + 2);

    fixture.create_neuron(203, 0, 0);
    fixture.add_maturity(203, 50_000);
    fixture.replace(unwind_state(
        &fixture,
        UnwindPhase::DelayIncreaseSubmitted,
        203,
    ));
    fixture.control(ControlledCommand::IncreaseDissolveDelay, 1, 0);
    let calls = fixture.governance_calls().increase_dissolve_delay;
    fixture.upgrade();
    fixture.assert_pending();
    fixture.assert_pending();
    assert!(matches!(fixture.resume(), Ok(NnsProgress::Unwind(_))));
    assert_eq!(
        fixture.governance_calls().increase_dissolve_delay,
        calls + 2
    );

    fixture.create_neuron(204, 0, 1);
    fixture.add_maturity(204, 50_000);
    fixture.add_maturity(42, 20_000);
    fixture.replace(unwind_state(&fixture, UnwindPhase::MergeSubmitted, 204));
    fixture.control(ControlledCommand::Merge, 0, 1);
    let calls = fixture.governance_calls().merge;
    fixture.upgrade();
    fixture.assert_pending();
    assert!(matches!(fixture.resume(), Ok(NnsProgress::Unwind(_))));
    assert_eq!(fixture.governance_calls().merge, calls + 1);

    for kind in [MaturityKind::TwoWeek, MaturityKind::TwoYear] {
        let mut state = fixture.state();
        let pending = delivering_maturity(&state, kind);
        if kind == MaturityKind::TwoWeek {
            state.pooled_parent_id = Some(42);
            state.pooled_parent_staking_account = Some(Account {
                owner: fixture.governance,
                subaccount: Some(vec![8; 32]),
            });
            state.latest_started_two_week_generation = 1;
            state.latest_pooled_target = Some(PooledTarget {
                target_e8s: 1_000_000,
                status: PooledTargetStatus::AtTarget,
            });
            state.pending_two_week_maturity = Some(pending.clone());
        } else {
            state.pending_two_year_maturity = Some(pending.clone());
        }
        state.active_operation = Some(NnsOperation::Maturity(Box::new(MaturityCommandOperation {
            operation_sequence: 1,
            dispatch_epoch: 1,
            kind,
            phase: MaturityCommandPhase::ClaimReceiptDelivery(ClaimReceiptDeliveryOperation {
                pending,
                permit: None,
                permanent_credit: None,
                claim_transfer: None,
            }),
        })));
        fixture.replace(state);
        fixture.upgrade();
    }

    let mut state = pool_state(
        &fixture,
        301,
        100_000_000,
        10_000_000,
        PoolCommandPhase::AwaitingTransfer,
    );
    state.lifecycle = Lifecycle::Paused;
    let Some(NnsOperation::Pool(replay_operation)) = state.active_operation.clone() else {
        unreachable!()
    };
    fixture.replace(state);
    fixture.control(ControlledCommand::RefreshVotingPower, 1, 0);
    let governance_before_replay = fixture.governance_calls();
    let queries_before_replay: u64 = query(
        &fixture.pic,
        fixture.governance,
        "debug_get_full_neuron_call_count",
    );
    let replay_args = PreparePoolReconciliationArgs {
        generation: 1,
        target_e8s: 110_000_000,
        action: PoolReconciliationAction::TopUp {
            expected_credit_e8s: 10_000_000,
        },
        fee_e8s: 10_000,
        snapshot_fingerprint: vec![2; 32],
        memo: vec![1; 32],
        created_at_time_nanos: 1,
    };
    assert_eq!(
        update::<_, Result<PoolProgress, ApiError>>(
            &fixture.pic,
            fixture.manager,
            fixture.stream,
            "prepare_pool_reconciliation",
            replay_args.clone()
        ),
        Ok(PoolProgress::AwaitingTransfer(
            replay_operation.permit.clone()
        ))
    );

    let mut completed_state = fixture.state();
    completed_state.lifecycle = Lifecycle::Paused;
    completed_state.latest_reconciliation_generation = 1;
    completed_state.latest_pooled_target = Some(PooledTarget {
        target_e8s: 110_000_000,
        status: PooledTargetStatus::AtTarget,
    });
    completed_state.last_completed_pool = Some(CompletedPoolCommand {
        permit: replay_operation.permit,
        transfer_block_index: 7,
        parent_neuron_id: 301,
        principal_e8s: 110_000_000,
    });
    fixture.replace(completed_state);
    assert!(matches!(
        update::<_, Result<PoolProgress, ApiError>>(
            &fixture.pic,
            fixture.manager,
            fixture.stream,
            "prepare_pool_reconciliation",
            replay_args
        ),
        Ok(PoolProgress::Completed { .. })
    ));

    let passive_args = PreparePoolReconciliationArgs {
        generation: 2,
        target_e8s: 1_000_000,
        action: PoolReconciliationAction::Unwind {
            expected_gross_e8s: 120_000,
        },
        fee_e8s: 10_000,
        snapshot_fingerprint: vec![7; 32],
        memo: vec![8; 32],
        created_at_time_nanos: 9,
    };
    let fingerprint = Sha256::digest(encode_one(passive_args.clone()).unwrap()).to_vec();
    let mut state = fixture.state();
    state.lifecycle = Lifecycle::Paused;
    state.latest_reconciliation_generation = 2;
    state.latest_pooled_target = Some(PooledTarget {
        target_e8s: 1_000_000,
        status: PooledTargetStatus::OverTarget,
    });
    state.live_cohorts = vec![PassiveCohort {
        generation: 2,
        reconciliation_request_fingerprint: fingerprint,
        child_neuron_id: 302,
        principal_e8s: 110_000,
        committed_fee_e8s: 10_000,
        child_staking_subaccount: vec![5; 32],
        ready_at_seconds: u64::MAX,
        proof: CohortProofState::Dissolving,
        disbursement_block: None,
    }];
    fixture.replace(state);
    assert_eq!(
        update::<_, Result<PoolProgress, ApiError>>(
            &fixture.pic,
            fixture.manager,
            fixture.stream,
            "prepare_pool_reconciliation",
            passive_args
        ),
        Ok(PoolProgress::UnwindCommitted {
            generation: 2,
            principal_e8s: 110_000,
        })
    );
    assert_eq!(fixture.governance_calls(), governance_before_replay);
    assert_eq!(
        query::<u64>(
            &fixture.pic,
            fixture.governance,
            "debug_get_full_neuron_call_count"
        ),
        queries_before_replay,
        "prepared and passive exact replay must make zero Governance calls"
    );

    let monetary_after: LedgerCallCounters =
        query(&fixture.pic, fixture.ledger, "debug_get_call_counters");
    assert_eq!(monetary_after.transfer, monetary_before.transfer);
    assert_eq!(monetary_after.transfer_from, monetary_before.transfer_from);
}
