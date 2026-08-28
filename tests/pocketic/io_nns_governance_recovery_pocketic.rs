use candid::{decode_one, encode_one, CandidType, Principal};
use io_nns_neuron_manager::{
    api::{MaturityProgress, PrepareTwoWeekMaturityArgs, UnwindProgress},
    jupiter::{
        JupiterDeposit, JupiterOperation, JupiterPhase, NeuronSnapshot, StakeTransferSucceeded,
    },
    maturity::{
        MaturityCommandOperation, MaturityCommandPhase, MaturityDeliveryOperation, MaturityKind,
        PendingMaturityDisbursement,
    },
    pool::{PassiveCohort, UnwindOperation, UnwindPhase},
    state::{Account, NnsOperation, NnsStateV1, PooledTarget},
    ApiError, InitArgs, JupiterProgress, Lifecycle, NnsConfig, NnsProgress, PooledTargetStatus,
};
use io_nns_types::backing::{
    ClaimAssetObservation, CohortProofState, CompletedPoolCommand, PoolCommand, PoolCommandKind,
    PoolCommandPhase, PoolProgress, PoolReconciliationAction, PreparePoolReconciliationArgs,
    TopUpPermit, TransitComponentKind, POOLED_PARENT_DELAY_SECONDS,
};
use pocket_ic::{PocketIc, PocketIcBuilder};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    process::{Child, Command, Stdio},
    sync::{Mutex, MutexGuard, OnceLock},
    thread,
    time::{Duration, Instant},
};

const CYCLES: u128 = 2_000_000_000_000;

struct RecoveryServer {
    url: String,
    _child: Mutex<Child>,
}

fn recovery_server() -> &'static RecoveryServer {
    static SERVER: OnceLock<RecoveryServer> = OnceLock::new();
    SERVER.get_or_init(|| {
        let binary = std::env::var_os("POCKET_IC_BIN")
            .expect("POCKET_IC_BIN must be set for live Governance recovery tests");
        let port_file = std::env::temp_dir().join(format!(
            "io_nns_governance_recovery_{}.port",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&port_file);
        let mut command = Command::new(binary);
        command
            .args(["--ttl", "120", "--hard-ttl", "1800", "--port-file"])
            .arg(&port_file);
        if std::env::var_os("POCKET_IC_MUTE_SERVER").is_some() {
            command.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let mut child = command
            .spawn()
            .expect("failed to start the dedicated Governance recovery PocketIC server");
        let started = Instant::now();
        let port = loop {
            if let Ok(value) = std::fs::read_to_string(&port_file) {
                if let Ok(port) = value.trim().parse::<u16>() {
                    break port;
                }
            }
            if let Some(status) = child.try_wait().expect("failed to inspect PocketIC server") {
                panic!("Governance recovery PocketIC server exited early: {status}");
            }
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "Governance recovery PocketIC server did not publish its port"
            );
            thread::sleep(Duration::from_millis(20));
        };
        let _ = std::fs::remove_file(port_file);
        RecoveryServer {
            url: format!("http://127.0.0.1:{port}/"),
            _child: Mutex::new(child),
        }
    })
}

fn recovery_pocket_ic() -> PocketIc {
    PocketIcBuilder::new()
        .with_application_subnet()
        .with_server_url(
            recovery_server()
                .url
                .parse()
                .expect("dedicated PocketIC URL must parse"),
        )
        .build()
}

fn lock_recovery_test() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct CreateNeuronArgs {
    neuron_id: u64,
    principal_e8s: u128,
    dissolve_delay_seconds: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct CreateSplitChildArgs {
    neuron_id: u64,
    principal_e8s: u128,
    dissolve_delay_seconds: u64,
    memo: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct CreateStakingNeuronArgs {
    neuron_id: u64,
    principal_e8s: u128,
    dissolve_delay_seconds: u64,
    memo: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct NeuronAmountArgs {
    neuron_id: u64,
    amount_e8s: u128,
}

#[derive(Clone, Copy, Debug, CandidType)]
struct DebugFeeArgs {
    fee_e8s: u128,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DebugNnsDisbursementArgs {
    from: Account,
    to: Account,
    amount_e8s: u128,
    native_memo_u64: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DebugMintAccountArgs {
    to: Account,
    amount_e8s: u128,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DebugLedgerTransaction {
    from: String,
    to: String,
    from_account: Option<Account>,
    to_account: Option<Account>,
    amount_e8s: u128,
    memo: String,
    memo_bytes: Option<Vec<u8>>,
    block_index: u64,
    timestamp: u64,
    native_memo_u64: u64,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize)]
struct SetNextDisburseBlockArgs {
    block_index: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct SetFolloweeArgs {
    neuron_id: u64,
    followee: Option<u64>,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize)]
struct SetVotingPowerTimestampArgs {
    neuron_id: u64,
    timestamp_seconds: u64,
}

#[derive(Clone, Copy, Debug, CandidType)]
struct NeuronIdArgs {
    neuron_id: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct MockNeuronObservation {
    followee_id: Option<u64>,
    voting_power_refreshed_timestamp_seconds: u64,
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
    Split,
    Disburse,
    DisburseMaturity,
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
    split: u64,
    disburse: u64,
    disburse_maturity: u64,
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

fn parent_staking_subaccount(manager: Principal, memo: u64) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update([0x0c]);
    hasher.update(b"neuron-stake");
    hasher.update(manager.as_slice());
    hasher.update(memo.to_be_bytes());
    hasher.finalize().to_vec()
}

struct RecoveryFixture {
    pic: PocketIc,
    ledger: Principal,
    governance: Principal,
    manager: Principal,
    stream: Principal,
    sns_governance: Principal,
}

impl RecoveryFixture {
    fn new() -> Self {
        Self::new_with_policy(42, false)
    }

    fn new_with_policy(pooled_parent_memo: u64, permanent_collision: bool) -> Self {
        let pic = recovery_pocket_ic();
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
                    pooled_parent_memo,
                    pooled_parent_followee_id: 41,
                    minimum_parent_stake_e8s: 100_000_000,
                    jupiter_account: Account {
                        owner: jupiter,
                        subaccount: None,
                    },
                    jupiter_staging: staging(0),
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
            sns_governance,
        };
        if permanent_collision {
            fixture.create_staking_neuron(41, 1_000_000, 63_115_200, pooled_parent_memo);
        } else {
            fixture.create_neuron(41, 1_000_000, 63_115_200);
        }
        if pooled_parent_memo == 42 {
            fixture.create_staking_neuron(42, 1_000_000, POOLED_PARENT_DELAY_SECONDS, 42);
        }
        if pooled_parent_memo == 42 {
            fixture.followee(42, Some(41));
        }
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
        state.pending_two_year_maturity = None;
        state.pending_two_week_maturity = None;
        state.last_two_year_maturity = None;
        state.last_two_week_maturity = None;
        state.next_operation_sequence = 2;
        state.control_epoch = 1;
        state
    }

    fn state_from_canister(&self) -> NnsStateV1 {
        query(&self.pic, self.manager, "debug_get_state")
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

    fn create_staking_neuron(&self, neuron_id: u64, principal_e8s: u128, delay: u64, memo: u64) {
        let _: u64 = update(
            &self.pic,
            self.governance,
            self.manager,
            "debug_create_staking_neuron",
            CreateStakingNeuronArgs {
                neuron_id,
                principal_e8s,
                dissolve_delay_seconds: delay,
                memo,
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

    fn mint_account(&self, account: Account, amount_e8s: u128) {
        let _: u64 = update(
            &self.pic,
            self.ledger,
            Principal::anonymous(),
            "debug_mint_account",
            DebugMintAccountArgs {
                to: account,
                amount_e8s,
            },
        );
    }

    fn balance(&self, account: Account) -> u128 {
        let balance: candid::Nat = update(
            &self.pic,
            self.ledger,
            Principal::anonymous(),
            "icrc1_balance_of",
            account,
        );
        balance.0.try_into().unwrap()
    }

    fn debit_remaining_maturity_capture(&self, kind: MaturityKind, captured_e8s: u128) {
        let split = io_nns_types::maturity::capture_40_60(captured_e8s, 10_000, 10_000)
            .expect("captured balance covers both fees");
        let source = match kind {
            MaturityKind::TwoYear => io_accounts::two_year_maturity_staging(self.manager),
            MaturityKind::TwoWeek => io_accounts::two_week_maturity_staging(self.manager),
        };
        // The driver already submitted the permanent net transfer before it
        // reached the canonical ClaimOrRefresh boundary. This mock Ledger does
        // not debit transfer fees, so debit that fee and the remaining gross
        // claim leg to simulate completion of the exact frozen capture.
        for (index, amount) in [split.permanent_fee, split.claim_gross]
            .into_iter()
            .enumerate()
        {
            let result: io_ledger_boundary::IcrcTransferResult = update(
                &self.pic,
                self.ledger,
                self.manager,
                "icrc1_transfer",
                io_ledger_boundary::IcrcTransferArg {
                    from_subaccount: source.subaccount.clone(),
                    to: Account {
                        owner: Principal::from_slice(&[60 + index as u8; 29]),
                        subaccount: None,
                    },
                    amount: candid::Nat::from(amount),
                    fee: Some(candid::Nat::from(10_000_u64)),
                    memo: Some(vec![index as u8]),
                    created_at_time: Some(10 + index as u64),
                },
            );
            result.unwrap();
        }
    }

    fn create_split_child(&self, neuron_id: u64, principal_e8s: u128, delay: u64, memo: u64) {
        let _: u64 = update(
            &self.pic,
            self.governance,
            self.manager,
            "debug_create_split_child",
            CreateSplitChildArgs {
                neuron_id,
                principal_e8s,
                dissolve_delay_seconds: delay,
                memo,
            },
        );
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

    fn voting_power_timestamp(&self, neuron_id: u64, timestamp_seconds: u64) {
        update::<_, Result<(), String>>(
            &self.pic,
            self.governance,
            Principal::anonymous(),
            "debug_set_voting_power_timestamp",
            SetVotingPowerTimestampArgs {
                neuron_id,
                timestamp_seconds,
            },
        )
        .unwrap();
    }

    fn neuron(&self, neuron_id: u64) -> MockNeuronObservation {
        decode_one::<Option<MockNeuronObservation>>(
            &self
                .pic
                .query_call(
                    self.governance,
                    Principal::anonymous(),
                    "debug_get_neuron",
                    encode_one(NeuronIdArgs { neuron_id }).unwrap(),
                )
                .unwrap(),
        )
        .unwrap()
        .unwrap()
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

    fn set_split_transport_rejection(&self, enabled: bool) {
        let _: () = update(
            &self.pic,
            self.governance,
            Principal::anonymous(),
            "debug_set_split_trap_before_effect",
            enabled,
        );
    }

    fn set_governance_fee(&self, fee_e8s: u64) {
        let _: () = update(
            &self.pic,
            self.governance,
            Principal::anonymous(),
            "debug_set_transaction_fee_e8s",
            fee_e8s,
        );
    }

    fn set_ledger_fee(&self, fee_e8s: u128) {
        let _: () = update(
            &self.pic,
            self.ledger,
            Principal::anonymous(),
            "debug_set_fee",
            DebugFeeArgs { fee_e8s },
        );
    }

    fn record_child_disbursement(&self, child_subaccount: Vec<u8>, amount_e8s: u128) -> u128 {
        let block: u64 = update(
            &self.pic,
            self.ledger,
            Principal::anonymous(),
            "debug_record_nns_disbursement",
            DebugNnsDisbursementArgs {
                from: Account {
                    owner: self.governance,
                    subaccount: Some(child_subaccount),
                },
                to: self.state_from_canister().config.stream_liquid_account,
                amount_e8s,
                native_memo_u64: u64::MAX,
            },
        );
        let _: () = update(
            &self.pic,
            self.governance,
            Principal::anonymous(),
            "debug_set_next_disburse_block",
            SetNextDisburseBlockArgs { block_index: block },
        );
        block.into()
    }

    fn create_controlled_neuron(&self, neuron_id: u64, principal_e8s: u128, delay: u64) {
        let _: u64 = update(
            &self.pic,
            self.governance,
            self.manager,
            "debug_create_neuron",
            CreateNeuronArgs {
                neuron_id,
                principal_e8s,
                dissolve_delay_seconds: delay,
            },
        );
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
    let before_child = matches!(
        phase,
        UnwindPhase::SplitPrepared | UnwindPhase::SplitSubmitted
    );
    let prepared = phase == UnwindPhase::SplitPrepared;
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
        split_fee_e8s: if prepared { 0 } else { 10_000 },
        committed_disbursement_fee_e8s: if prepared { 0 } else { 10_000 },
        parent_principal_before_split_e8s: if prepared { 0 } else { 1_120_000 },
        child_neuron_id: if before_child { 0 } else { child_neuron_id },
        principal_e8s: if before_child { 0 } else { 110_000 },
        child_staking_subaccount: if before_child {
            Vec::new()
        } else {
            vec![4; 32]
        },
        submitted_at_seconds: 1,
        expected_block_index: (phase == UnwindPhase::DisbursementSubmitted).then_some(9),
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

fn cleanup_unwind_state(
    fixture: &RecoveryFixture,
    phase: UnwindPhase,
    child_neuron_id: u64,
) -> NnsStateV1 {
    let mut state = unwind_state(fixture, phase, child_neuron_id);
    state.live_cohorts = vec![PassiveCohort {
        generation: 1,
        reconciliation_request_fingerprint: vec![3; 32],
        child_neuron_id,
        principal_e8s: 110_000,
        committed_fee_e8s: 10_000,
        child_staking_subaccount: vec![4; 32],
        ready_at_seconds: 1,
        proof: CohortProofState::PrincipalReturned,
        disbursement_block: Some(9),
    }];
    state
}

fn split_state(fixture: &RecoveryFixture, phase: UnwindPhase) -> NnsStateV1 {
    let mut state = unwind_state(fixture, phase, 0);
    state.config.minimum_parent_stake_e8s = 10_001;
    state.pooled_parent_staking_account = Some(Account {
        owner: fixture.governance,
        subaccount: Some(parent_staking_subaccount(fixture.manager, 42)),
    });
    let Some(NnsOperation::Unwind(operation)) = state.active_operation.as_mut() else {
        unreachable!()
    };
    operation.target_e8s = 880_000;
    if operation.phase == UnwindPhase::SplitSubmitted {
        operation.parent_principal_before_split_e8s = 1_000_000;
    }
    state.latest_pooled_target = Some(PooledTarget {
        target_e8s: 880_000,
        status: PooledTargetStatus::OverTarget,
    });
    state
}

fn unwind_phase(state: &NnsStateV1) -> &UnwindPhase {
    let Some(NnsOperation::Unwind(operation)) = state.active_operation.as_ref() else {
        panic!("expected active unwind")
    };
    &operation.phase
}

fn delivering_maturity(_state: &NnsStateV1, kind: MaturityKind) -> PendingMaturityDisbursement {
    let (remaining, generation, target) = match kind {
        MaturityKind::TwoYear => (120_000_000, None, None),
        MaturityKind::TwoWeek => (200_000_000, Some(1), Some(1_000_000)),
    };
    PendingMaturityDisbursement {
        nominal_disbursed_e8s: remaining,
        initiated_at_seconds: 1,
        scheduled_finalization_timestamp_seconds: 604_801,
        entitlement_batch_generation: generation,
        two_week_target_e8s: target,
        captured_e8s: Some(u128::from(remaining)),
    }
}

fn uncaptured_maturity(
    kind: MaturityKind,
    nominal_disbursed_e8s: u64,
    generation: u64,
) -> PendingMaturityDisbursement {
    let (entitlement_batch_generation, two_week_target_e8s) = match kind {
        MaturityKind::TwoYear => (None, None),
        MaturityKind::TwoWeek => (Some(generation), Some(1_000_000)),
    };
    PendingMaturityDisbursement {
        nominal_disbursed_e8s,
        initiated_at_seconds: 1,
        scheduled_finalization_timestamp_seconds: 604_801,
        entitlement_batch_generation,
        two_week_target_e8s,
        captured_e8s: None,
    }
}

fn claim_assets(fixture: &RecoveryFixture) -> ClaimAssetObservation {
    update::<_, Result<ClaimAssetObservation, ApiError>>(
        &fixture.pic,
        fixture.manager,
        fixture.stream,
        "observe_claim_assets",
        (),
    )
    .unwrap()
}

fn reconciliation_args(
    observation: &ClaimAssetObservation,
    action: PoolReconciliationAction,
    target_e8s: u128,
) -> PreparePoolReconciliationArgs {
    PreparePoolReconciliationArgs {
        generation: 1,
        target_e8s,
        action,
        fee_e8s: 10_000,
        snapshot_fingerprint: observation.fingerprint.clone(),
        memo: vec![1; 32],
        created_at_time_nanos: 1,
    }
}

#[test]
fn account_policy_voting_power_housekeeping_is_best_effort_and_never_gates_pool_policy() {
    let _serial = lock_recovery_test();
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        return;
    }
    let fixture = RecoveryFixture::new();
    let mut recorded = fixture.state();
    recorded.config.minimum_parent_stake_e8s = 1_000_000;
    recorded.pooled_parent_id = Some(42);
    recorded.pooled_parent_staking_account = Some(Account {
        owner: fixture.governance,
        subaccount: Some(parent_staking_subaccount(fixture.manager, 42)),
    });
    recorded.latest_pooled_target = Some(PooledTarget {
        target_e8s: 1_000_000,
        status: PooledTargetStatus::AtTarget,
    });
    fixture.replace(recorded);
    fixture.voting_power_timestamp(41, 0);
    fixture.voting_power_timestamp(42, u64::MAX);
    let permanent_followee = fixture.neuron(41).followee_id;
    let parent_followee = fixture.neuron(42).followee_id;
    fixture.control(ControlledCommand::RefreshVotingPower, 1, 0);
    let calls_before = fixture.governance_calls().refresh_voting_power;

    let observation: Result<io_nns_types::backing::PoolPolicyObservation, ApiError> = update(
        &fixture.pic,
        fixture.manager,
        fixture.stream,
        "observe_pool_policy",
        (),
    );
    assert!(observation.unwrap().parent.is_some());
    assert_eq!(
        fixture.governance_calls().refresh_voting_power,
        calls_before + 2,
        "a rejected permanent refresh must not suppress the parent attempt"
    );
    assert_eq!(fixture.neuron(41).followee_id, permanent_followee);
    assert_eq!(fixture.neuron(42).followee_id, parent_followee);

    fixture.control(ControlledCommand::RefreshVotingPower, 0, 1);
    let calls_before = fixture.governance_calls().refresh_voting_power;
    let malformed: Result<io_nns_types::backing::PoolPolicyObservation, ApiError> = update(
        &fixture.pic,
        fixture.manager,
        fixture.stream,
        "observe_pool_policy",
        (),
    );
    assert!(malformed.is_ok(), "{malformed:?}");
    assert_eq!(
        fixture.governance_calls().refresh_voting_power,
        calls_before + 2
    );

    let claim = claim_assets(&fixture);
    let hold = reconciliation_args(
        &claim,
        PoolReconciliationAction::Hold,
        claim.pooled_parent_principal_e8s,
    );
    fixture.control(ControlledCommand::RefreshVotingPower, 1, 0);
    let refreshes_before_hold = fixture.governance_calls().refresh_voting_power;
    assert_eq!(
        update::<_, Result<PoolProgress, ApiError>>(
            &fixture.pic,
            fixture.manager,
            fixture.stream,
            "prepare_pool_reconciliation",
            hold,
        ),
        Ok(PoolProgress::Held {
            principal_e8s: claim.pooled_parent_principal_e8s,
        })
    );
    assert_eq!(
        fixture.governance_calls().refresh_voting_power,
        refreshes_before_hold,
        "monetary reconciliation uses pure policy validation"
    );

    let absent = RecoveryFixture::new();
    absent.voting_power_timestamp(41, u64::MAX);
    absent.control(ControlledCommand::RefreshVotingPower, 1, 0);
    let calls_before = absent.governance_calls().refresh_voting_power;
    let ready = update::<_, Result<(), ApiError>>(
        &absent.pic,
        absent.manager,
        absent.sns_governance,
        "set_paused",
        false,
    );
    assert_eq!(ready, Ok(()));
    assert_eq!(
        absent.governance_calls().refresh_voting_power,
        calls_before + 1,
        "parent absence permits only the permanent housekeeping attempt"
    );
}

#[test]
fn account_policy_zero_memo_rejects_permanent_collision_and_accepts_candidate_dust() {
    let _serial = lock_recovery_test();
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        return;
    }
    let collision = RecoveryFixture::new_with_policy(0, true);
    let readiness = update::<_, Result<(), ApiError>>(
        &collision.pic,
        collision.manager,
        collision.sns_governance,
        "set_paused",
        false,
    );
    assert!(matches!(readiness, Err(ApiError::Invalid(message)) if message.contains("collides")));
    collision.replace(collision.state());
    let observation = claim_assets(&collision);
    let args = reconciliation_args(
        &observation,
        PoolReconciliationAction::TopUp {
            expected_credit_e8s: 100_000_000,
        },
        100_000_000,
    );
    let ledger_before: LedgerCallCounters =
        query(&collision.pic, collision.ledger, "debug_get_call_counters");
    let rejected = update::<_, Result<PoolProgress, ApiError>>(
        &collision.pic,
        collision.manager,
        collision.stream,
        "prepare_pool_reconciliation",
        args,
    );
    assert!(matches!(rejected, Err(ApiError::Invalid(message)) if message.contains("collides")));
    assert!(collision.state_from_canister().active_operation.is_none());
    let ledger_after: LedgerCallCounters =
        query(&collision.pic, collision.ledger, "debug_get_call_counters");
    assert_eq!(ledger_after.transfer, ledger_before.transfer);
    assert_eq!(ledger_after.transfer_from, ledger_before.transfer_from);

    const CREDIT_E8S: u128 = 100_000_000;
    const UNSOLICITED_E8S: u128 = 50_000_000;
    let occupied = RecoveryFixture::new_with_policy(0, false);
    let candidate = Account {
        owner: occupied.governance,
        subaccount: Some(parent_staking_subaccount(occupied.manager, 0)),
    };
    occupied.mint_account(candidate.clone(), UNSOLICITED_E8S);
    let readiness = update::<_, Result<(), ApiError>>(
        &occupied.pic,
        occupied.manager,
        occupied.sns_governance,
        "set_paused",
        false,
    );
    readiness.expect("publicly derivable candidate-account dust must not block readiness");
    let observation = claim_assets(&occupied);
    let mut args = reconciliation_args(
        &observation,
        PoolReconciliationAction::TopUp {
            expected_credit_e8s: CREDIT_E8S,
        },
        CREDIT_E8S,
    );
    args.created_at_time_nanos = occupied.pic.get_time().as_nanos_since_unix_epoch();
    let progress = update::<_, Result<PoolProgress, ApiError>>(
        &occupied.pic,
        occupied.manager,
        occupied.stream,
        "prepare_pool_reconciliation",
        args.clone(),
    )
    .unwrap();
    let PoolProgress::AwaitingTransfer(permit) = progress else {
        panic!("expected bootstrap permit, got {progress:?}")
    };
    assert_eq!(permit.expected_credit_e8s, CREDIT_E8S);
    assert_eq!(permit.destination, candidate);
    assert_eq!(occupied.state_from_canister().config.pooled_parent_memo, 0);

    let stream_liquid = occupied.state_from_canister().config.stream_liquid_account;
    occupied.mint_account(stream_liquid.clone(), CREDIT_E8S + permit.fee_e8s);
    let transfer: io_ledger_boundary::IcrcTransferResult = update(
        &occupied.pic,
        occupied.ledger,
        occupied.stream,
        "icrc1_transfer",
        io_ledger_boundary::IcrcTransferArg {
            from_subaccount: stream_liquid.subaccount,
            to: permit.destination.clone(),
            amount: candid::Nat::from(CREDIT_E8S),
            fee: Some(candid::Nat::from(permit.fee_e8s)),
            memo: Some(permit.memo.clone()),
            created_at_time: Some(permit.prepared_at_nanos),
        },
    );
    let transfer_block: u128 = transfer.unwrap().0.try_into().unwrap();
    let transactions: Vec<DebugLedgerTransaction> = update(
        &occupied.pic,
        occupied.ledger,
        Principal::anonymous(),
        "debug_get_transactions",
        (),
    );
    assert_eq!(
        transactions[transfer_block as usize].timestamp,
        permit.prepared_at_nanos
    );
    occupied.create_staking_neuron(0, CREDIT_E8S + UNSOLICITED_E8S, 0, 0);
    let proved = update::<_, Result<NnsProgress, ApiError>>(
        &occupied.pic,
        occupied.manager,
        occupied.stream,
        "prove_active_transfer",
        transfer_block,
    )
    .unwrap();
    let NnsProgress::Pool(PoolProgress::Completed {
        parent_neuron_id,
        principal_e8s,
        target_status,
    }) = proved
    else {
        panic!("definite bootstrap effects did not continue to completion: {proved:?}")
    };
    let completed = (parent_neuron_id, principal_e8s, target_status);
    assert_eq!(completed.0, 0);
    assert_eq!(completed.1, CREDIT_E8S + UNSOLICITED_E8S);
    assert_eq!(
        completed.2,
        io_nns_types::backing::PoolTargetResult::OverTarget
    );
    let final_state = occupied.state_from_canister();
    assert!(final_state.active_operation.is_none());
    assert_eq!(
        final_state.latest_pooled_target,
        Some(PooledTarget {
            target_e8s: CREDIT_E8S,
            status: PooledTargetStatus::OverTarget,
        })
    );
    assert_eq!(occupied.balance(candidate), CREDIT_E8S + UNSOLICITED_E8S);
    let ledger_calls: LedgerCallCounters =
        query(&occupied.pic, occupied.ledger, "debug_get_call_counters");
    assert_eq!(ledger_calls.transfer, 1);
    assert_eq!(ledger_calls.transfer_from, 0);

    assert_eq!(
        update::<_, Result<PoolProgress, ApiError>>(
            &occupied.pic,
            occupied.manager,
            occupied.stream,
            "prepare_pool_reconciliation",
            args,
        ),
        Ok(PoolProgress::Completed {
            parent_neuron_id: 0,
            principal_e8s: CREDIT_E8S + UNSOLICITED_E8S,
            target_status: io_nns_types::backing::PoolTargetResult::OverTarget,
        })
    );
    let replay_calls: LedgerCallCounters =
        query(&occupied.pic, occupied.ledger, "debug_get_call_counters");
    assert_eq!(replay_calls.transfer, ledger_calls.transfer);
    assert_eq!(replay_calls.transfer_from, ledger_calls.transfer_from);
}

#[test]
fn semantic_staging_carries_late_value_into_the_next_cycle_for_both_roles() {
    let _serial = lock_recovery_test();
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        return;
    }
    const UNIT: u128 = 2_000_000;
    for kind in [MaturityKind::TwoYear, MaturityKind::TwoWeek] {
        let fixture = RecoveryFixture::new();
        let mut state = match kind {
            MaturityKind::TwoYear => fixture.state(),
            MaturityKind::TwoWeek => ready_two_week_state(&fixture),
        };
        let staging = match kind {
            MaturityKind::TwoYear => io_accounts::two_year_maturity_staging(fixture.manager),
            MaturityKind::TwoWeek => io_accounts::two_week_maturity_staging(fixture.manager),
        };
        let other_staging = match kind {
            MaturityKind::TwoYear => io_accounts::two_week_maturity_staging(fixture.manager),
            MaturityKind::TwoWeek => io_accounts::two_year_maturity_staging(fixture.manager),
        };
        assert_eq!(fixture.balance(staging.clone()), 0);
        fixture.mint_account(other_staging.clone(), 33 * UNIT);

        let first_capture = 100 * UNIT;
        fixture.mint_account(staging.clone(), first_capture);
        match kind {
            MaturityKind::TwoYear => {
                state.pending_two_year_maturity =
                    Some(uncaptured_maturity(kind, first_capture as u64, 0));
            }
            MaturityKind::TwoWeek => {
                state.pending_two_week_maturity =
                    Some(uncaptured_maturity(kind, first_capture as u64, 1));
            }
        }
        fixture.replace(state);
        assert!(matches!(fixture.resume(), Err(ApiError::Pending(_))));

        let late_donation = 20 * UNIT;
        fixture.mint_account(staging.clone(), late_donation);
        fixture.debit_remaining_maturity_capture(kind, first_capture);
        assert_eq!(fixture.balance(staging.clone()), late_donation);

        let split = io_nns_types::maturity::capture_40_60(first_capture, 10_000, 10_000).unwrap();
        let mut next = fixture.state_from_canister();
        next.active_operation = None;
        let completed = io_nns_neuron_manager::maturity::CompletedMaturity {
            kind,
            captured_e8s: first_capture,
            permanent_credit_e8s: split.permanent_credit,
            claim_credit_e8s: split.claim_credit,
            entitlement_batch_generation: (kind == MaturityKind::TwoWeek).then_some(1),
            two_week_target_e8s: (kind == MaturityKind::TwoWeek).then_some(1_000_000),
            completed_at_nanos: 1,
        };
        match kind {
            MaturityKind::TwoYear => {
                next.pending_two_year_maturity = None;
                next.last_two_year_maturity = Some(completed);
            }
            MaturityKind::TwoWeek => {
                next.pending_two_week_maturity = None;
                next.last_two_week_maturity = Some(completed);
            }
        }

        let second_maturity = 50 * UNIT;
        fixture.mint_account(staging.clone(), second_maturity);
        let expected_second_capture = late_donation + second_maturity;
        match kind {
            MaturityKind::TwoYear => {
                next.pending_two_year_maturity =
                    Some(uncaptured_maturity(kind, second_maturity as u64, 0));
            }
            MaturityKind::TwoWeek => {
                next.pending_two_week_maturity =
                    Some(uncaptured_maturity(kind, second_maturity as u64, 2));
            }
        }
        fixture.replace(next);
        assert!(matches!(fixture.resume(), Err(ApiError::Pending(_))));
        fixture.debit_remaining_maturity_capture(kind, expected_second_capture);
        assert_eq!(fixture.balance(staging), 0);
        assert_eq!(fixture.balance(other_staging), 33 * UNIT);
        eprintln!(
            "account_semantic_carry_forward kind={kind:?} unit_e8s={UNIT} g1_capture_units=100 late_units=20 g2_maturity_units=50 g2_capture_units=70 final_staging_e8s=0 isolated_other_e8s={}",
            33 * UNIT
        );
    }
}

#[test]
fn ambiguous_split_is_discovered_after_upgrade_without_a_second_call() {
    let _serial = lock_recovery_test();
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping Governance recovery PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = RecoveryFixture::new();
    fixture.replace(split_state(&fixture, UnwindPhase::SplitPrepared));
    fixture.control(ControlledCommand::Split, 0, 1);

    fixture.assert_pending();
    let submitted = fixture.state_from_canister();
    assert_eq!(unwind_phase(&submitted), &UnwindPhase::SplitSubmitted);
    assert_eq!(fixture.governance_calls().split, 1);
    let observation: Result<ClaimAssetObservation, ApiError> = update(
        &fixture.pic,
        fixture.manager,
        fixture.stream,
        "observe_claim_assets",
        (),
    );
    assert!(matches!(observation, Err(ApiError::Pending(_))));

    fixture.upgrade();
    assert_eq!(
        fixture.resume(),
        Ok(NnsProgress::Unwind(UnwindProgress::Pending))
    );
    let identified = fixture.state_from_canister();
    assert_eq!(unwind_phase(&identified), &UnwindPhase::ChildIdentified);
    assert_eq!(fixture.governance_calls().split, 1);

    let observation: ClaimAssetObservation = update::<_, Result<_, ApiError>>(
        &fixture.pic,
        fixture.manager,
        fixture.stream,
        "observe_claim_assets",
        (),
    )
    .unwrap();
    let child = observation
        .transit_components
        .iter()
        .find(|component| component.kind == TransitComponentKind::ActiveUnwind)
        .expect("identified child must contribute active unwind transit");
    assert_eq!(child.backing_e8s, 100_000);
    assert_eq!(child.fee_basis_e8s, Some(10_000));
    assert_eq!(observation.pooled_parent_principal_e8s, 880_000);
}

#[test]
fn decoded_split_rejection_releases_intent_for_one_safe_retry() {
    let _serial = lock_recovery_test();
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping Governance recovery PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = RecoveryFixture::new();
    fixture.replace(split_state(&fixture, UnwindPhase::SplitPrepared));
    fixture.control(ControlledCommand::Split, 1, 0);
    fixture.assert_pending();
    assert_eq!(
        unwind_phase(&fixture.state_from_canister()),
        &UnwindPhase::SplitPrepared
    );
    assert_eq!(fixture.governance_calls().split, 1);

    assert_eq!(
        fixture.resume(),
        Ok(NnsProgress::Unwind(UnwindProgress::Pending))
    );
    let completed = fixture.state_from_canister();
    assert!(completed.active_operation.is_none());
    assert!(completed.live_cohorts.iter().any(|cohort| {
        cohort.child_neuron_id != 0 && cohort.proof == CohortProofState::Dissolving
    }));
    assert_eq!(fixture.governance_calls().split, 2);
}

#[test]
fn split_transport_ambiguity_and_duplicate_candidates_fail_closed() {
    let _serial = lock_recovery_test();
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping Governance recovery PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = RecoveryFixture::new();
    fixture.replace(split_state(&fixture, UnwindPhase::SplitPrepared));
    fixture.set_split_transport_rejection(true);
    fixture.assert_pending();
    assert_eq!(
        unwind_phase(&fixture.state_from_canister()),
        &UnwindPhase::SplitSubmitted
    );
    fixture.set_split_transport_rejection(false);
    fixture.create_controlled_neuron(901, 109_999, POOLED_PARENT_DELAY_SECONDS);
    fixture.assert_pending();
    assert_eq!(fixture.governance_calls().split, 0);

    fixture.replace(split_state(&fixture, UnwindPhase::SplitPrepared));
    fixture.control(ControlledCommand::Split, 0, 1);
    fixture.assert_pending();
    fixture.create_split_child(902, 110_000, POOLED_PARENT_DELAY_SECONDS, 1);
    let conflicting = fixture.resume();
    assert!(
        matches!(conflicting, Err(ApiError::Stuck(_))),
        "expected conflicting exact subaccount to fail closed, got {conflicting:?}"
    );
    assert_eq!(
        unwind_phase(&fixture.state_from_canister()),
        &UnwindPhase::SplitSubmitted
    );
    assert_eq!(fixture.governance_calls().split, 1);
}

#[test]
fn split_fee_drift_pauses_before_effect_and_is_resumable() {
    let _serial = lock_recovery_test();
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        eprintln!("skipping Governance recovery PocketIC test because POCKET_IC_BIN is not set");
        return;
    }
    let fixture = RecoveryFixture::new();
    fixture.replace(split_state(&fixture, UnwindPhase::SplitPrepared));
    fixture.set_governance_fee(10_001);
    assert!(matches!(fixture.resume(), Err(ApiError::Stuck(_))));
    let paused = fixture.state_from_canister();
    assert_eq!(paused.lifecycle, Lifecycle::Paused);
    assert_eq!(unwind_phase(&paused), &UnwindPhase::SplitPrepared);
    assert_eq!(fixture.governance_calls().split, 0);

    fixture.set_governance_fee(10_000);
    fixture.set_ledger_fee(10_000);
    let mut reviewed = paused;
    reviewed.lifecycle = Lifecycle::Ready;
    fixture.replace(reviewed);
    assert_eq!(
        fixture.resume(),
        Ok(NnsProgress::Unwind(UnwindProgress::Pending))
    );
    assert_eq!(fixture.governance_calls().split, 1);
}

#[test]
fn every_persisted_governance_phase_recovers_and_exact_replay_is_effect_safe() {
    let _serial = lock_recovery_test();
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
    let rejected = fixture.resume();
    assert!(
        matches!(rejected, Err(ApiError::Invalid(ref reason)) if reason.contains("rejected")),
        "definitive ClaimOrRefresh rejection was disguised: {rejected:?}"
    );
    let resumed = fixture.resume();
    assert!(
        matches!(
            resumed,
            Ok(NnsProgress::Jupiter(JupiterProgress::Pending)) | Err(ApiError::Pending(_))
        ),
        "proved permanent credit did not stop at the real Stream boundary: {resumed:?}"
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
    let rejected = fixture.resume();
    assert!(
        matches!(rejected, Err(ApiError::Invalid(ref reason)) if reason.contains("rejected")),
        "definitive delay rejection was disguised: {rejected:?}"
    );
    assert!(matches!(fixture.resume(), Ok(NnsProgress::Pool(_))));
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
    let rejected = fixture.resume();
    assert!(
        matches!(rejected, Err(ApiError::Invalid(ref reason)) if reason.contains("rejected")),
        "definitive following rejection was disguised: {rejected:?}"
    );
    assert!(matches!(fixture.resume(), Ok(NnsProgress::Pool(_))));
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
    let rejected = fixture.resume();
    assert!(
        matches!(rejected, Err(ApiError::Invalid(ref reason)) if reason.contains("rejected")),
        "definitive pooled refresh rejection was disguised: {rejected:?}"
    );
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
    let resumed = fixture.resume();
    assert!(
        matches!(resumed, Ok(NnsProgress::Unwind(_))),
        "ambiguous StartDissolving did not recover: {resumed:?}"
    );
    assert_eq!(fixture.governance_calls().start_dissolving, calls + 1);

    fixture.create_neuron(202, 110_000, 100);
    fixture.replace(unwind_state(
        &fixture,
        UnwindPhase::StartDissolvingSubmitted,
        202,
    ));
    fixture.control(ControlledCommand::StartDissolving, 1, 0);
    let calls = fixture.governance_calls().start_dissolving;
    let rejected = fixture.resume();
    assert!(
        matches!(rejected, Err(ApiError::Invalid(ref reason)) if reason.contains("rejected")),
        "definitive StartDissolving rejection was disguised: {rejected:?}"
    );
    assert!(matches!(fixture.resume(), Ok(NnsProgress::Unwind(_))));
    assert_eq!(fixture.governance_calls().start_dissolving, calls + 2);

    fixture.create_neuron(203, 0, 0);
    fixture.add_maturity(203, 50_000);
    fixture.add_maturity(42, 20_000);
    fixture.replace(cleanup_unwind_state(
        &fixture,
        UnwindPhase::DelayIncreaseSubmitted,
        203,
    ));
    fixture.control(ControlledCommand::IncreaseDissolveDelay, 1, 0);
    let calls = fixture.governance_calls().increase_dissolve_delay;
    fixture.upgrade();
    let rejected = fixture.resume();
    assert!(
        matches!(rejected, Err(ApiError::Invalid(ref reason)) if reason.contains("rejected")),
        "definitive zero-principal delay rejection was disguised: {rejected:?}"
    );
    let resumed = fixture.resume();
    assert!(
        matches!(resumed, Ok(NnsProgress::Unwind(_))),
        "zero-principal cleanup did not resume: {resumed:?}"
    );
    assert_eq!(
        fixture.governance_calls().increase_dissolve_delay,
        calls + 2
    );

    fixture.create_neuron(204, 0, 1);
    fixture.add_maturity(204, 50_000);
    let mut merge_state = cleanup_unwind_state(&fixture, UnwindPhase::MergeSubmitted, 204);
    let Some(NnsOperation::Unwind(operation)) = merge_state.active_operation.as_mut() else {
        unreachable!()
    };
    operation.parent_maturity_e8s = 70_000;
    fixture.replace(merge_state);
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
            phase: MaturityCommandPhase::Delivery(MaturityDeliveryOperation {
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
    let governance_after_replay = fixture.governance_calls();
    assert_eq!(
        governance_after_replay.split,
        governance_before_replay.split
    );
    assert_eq!(
        governance_after_replay.claim_or_refresh,
        governance_before_replay.claim_or_refresh
    );
    assert_eq!(
        governance_after_replay.increase_dissolve_delay,
        governance_before_replay.increase_dissolve_delay
    );
    assert_eq!(
        governance_after_replay.set_following,
        governance_before_replay.set_following
    );
    assert_eq!(
        governance_after_replay.start_dissolving,
        governance_before_replay.start_dissolving
    );
    assert_eq!(
        governance_after_replay.merge,
        governance_before_replay.merge
    );
    assert_eq!(
        governance_after_replay.disburse,
        governance_before_replay.disburse
    );
    assert_eq!(
        governance_after_replay.disburse_maturity,
        governance_before_replay.disburse_maturity
    );

    let monetary_after: LedgerCallCounters =
        query(&fixture.pic, fixture.ledger, "debug_get_call_counters");
    assert_eq!(monetary_after.transfer, monetary_before.transfer);
    assert_eq!(monetary_after.transfer_from, monetary_before.transfer_from);
}

fn ready_two_week_state(fixture: &RecoveryFixture) -> NnsStateV1 {
    let mut state = fixture.state();
    state.pooled_parent_id = Some(42);
    state.pooled_parent_staking_account = Some(Account {
        owner: fixture.governance,
        subaccount: Some(parent_staking_subaccount(fixture.manager, 42)),
    });
    state.latest_pooled_target = Some(PooledTarget {
        target_e8s: 1_000_000,
        status: PooledTargetStatus::AtTarget,
    });
    state
}

fn child_disbursement_state(fixture: &RecoveryFixture, child_neuron_id: u64) -> NnsStateV1 {
    let mut state = unwind_state(fixture, UnwindPhase::DisbursementPrepared, child_neuron_id);
    state.live_cohorts = vec![PassiveCohort {
        generation: 1,
        reconciliation_request_fingerprint: vec![3; 32],
        child_neuron_id,
        principal_e8s: 110_000,
        committed_fee_e8s: 10_000,
        child_staking_subaccount: vec![4; 32],
        ready_at_seconds: 1,
        proof: CohortProofState::Dissolving,
        disbursement_block: None,
    }];
    state
}

#[test]
fn child_disburse_decoded_rejection_retries_and_proves_exact_block() {
    let _serial = lock_recovery_test();
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        return;
    }
    let fixture = RecoveryFixture::new();
    fixture.create_controlled_neuron(501, 110_000, 0);
    fixture.replace(child_disbursement_state(&fixture, 501));
    fixture.control(ControlledCommand::Disburse, 1, 0);
    assert_eq!(
        fixture.resume(),
        Ok(NnsProgress::Unwind(UnwindProgress::Pending))
    );
    assert_eq!(
        unwind_phase(&fixture.state_from_canister()),
        &UnwindPhase::DisbursementPrepared
    );
    assert_eq!(fixture.governance_calls().disburse, 1);

    let block = fixture.record_child_disbursement(vec![4; 32], 100_000);
    assert_eq!(
        fixture.resume(),
        Ok(NnsProgress::Unwind(UnwindProgress::AwaitingTransferProof))
    );
    assert_eq!(fixture.governance_calls().disburse, 2);
    assert!(matches!(
        update::<_, Result<NnsProgress, ApiError>>(
            &fixture.pic,
            fixture.manager,
            Principal::anonymous(),
            "prove_active_transfer",
            block,
        ),
        Ok(NnsProgress::Unwind(UnwindProgress::Completed))
    ));
}

#[test]
fn child_disburse_malformed_after_effect_never_resubmits() {
    let _serial = lock_recovery_test();
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        return;
    }
    let fixture = RecoveryFixture::new();
    fixture.create_controlled_neuron(502, 110_000, 0);
    fixture.replace(child_disbursement_state(&fixture, 502));
    let block = fixture.record_child_disbursement(vec![4; 32], 100_000);
    fixture.control(ControlledCommand::Disburse, 0, 1);
    assert_eq!(
        fixture.resume(),
        Ok(NnsProgress::Unwind(UnwindProgress::AwaitingTransferProof))
    );
    assert_eq!(fixture.governance_calls().disburse, 1);
    fixture.upgrade();
    assert!(matches!(
        update::<_, Result<NnsProgress, ApiError>>(
            &fixture.pic,
            fixture.manager,
            Principal::anonymous(),
            "prove_active_transfer",
            block,
        ),
        Ok(NnsProgress::Unwind(UnwindProgress::Completed))
    ));
    assert_eq!(fixture.governance_calls().disburse, 1);
}

#[test]
fn child_disburse_checks_governance_and_ledger_fees_before_effect() {
    let _serial = lock_recovery_test();
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        return;
    }
    let fixture = RecoveryFixture::new();
    fixture.create_controlled_neuron(503, 110_000, 0);
    fixture.replace(child_disbursement_state(&fixture, 503));
    fixture.set_governance_fee(20_000);
    assert!(matches!(fixture.resume(), Err(ApiError::Stuck(_))));
    assert_eq!(fixture.governance_calls().disburse, 0);
    assert_eq!(
        unwind_phase(&fixture.state_from_canister()),
        &UnwindPhase::DisbursementPrepared
    );
    fixture.set_governance_fee(10_000);
    fixture.record_child_disbursement(vec![4; 32], 100_000);
    assert_eq!(
        fixture.resume(),
        Ok(NnsProgress::Unwind(UnwindProgress::AwaitingTransferProof))
    );
    assert_eq!(fixture.governance_calls().disburse, 1);
}

#[test]
fn two_week_maturity_captures_canonical_amount_after_intervening_reward_events() {
    let _serial = lock_recovery_test();
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        return;
    }
    for reward_events in [1_u64, 3] {
        let fixture = RecoveryFixture::new();
        fixture.replace(ready_two_week_state(&fixture));
        fixture.add_maturity(42, 200_000_000);
        assert_eq!(
            update::<_, Result<(), ApiError>>(
                &fixture.pic,
                fixture.manager,
                fixture.stream,
                "prepare_two_week_maturity",
                PrepareTwoWeekMaturityArgs {
                    entitlement_batch_generation: 1,
                    target_e8s: 1_000_000,
                },
            ),
            Ok(())
        );
        for _ in 0..reward_events {
            fixture.add_maturity(42, 10_000_000);
        }
        if reward_events > 1 {
            fixture.upgrade();
        }
        assert_eq!(
            fixture.resume(),
            Ok(NnsProgress::Maturity(MaturityProgress::Pending))
        );
        let state = fixture.state_from_canister();
        let pending = state
            .pending_two_week_maturity
            .expect("canonical two-week maturity must become passive");
        assert_eq!(pending.nominal_disbursed_e8s, 200_000_000);
        assert_eq!(pending.entitlement_batch_generation, Some(1));
        assert_eq!(fixture.governance_calls().disburse_maturity, 1);
    }
}

#[test]
fn two_year_maturity_uses_realised_disbursement_across_reward_accrual() {
    let _serial = lock_recovery_test();
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        return;
    }
    let fixture = RecoveryFixture::new();
    fixture.replace(fixture.state());
    fixture.add_maturity(41, 200_000_000);
    assert_eq!(
        update::<_, Result<MaturityProgress, ApiError>>(
            &fixture.pic,
            fixture.manager,
            fixture.sns_governance,
            "start_maturity",
            MaturityKind::TwoYear,
        ),
        Ok(MaturityProgress::Pending)
    );
    fixture.add_maturity(41, 25_000_000);
    assert_eq!(
        fixture.resume(),
        Ok(NnsProgress::Maturity(MaturityProgress::Pending))
    );
    assert_eq!(
        fixture.resume(),
        Ok(NnsProgress::Maturity(MaturityProgress::Pending))
    );
    let pending = fixture
        .state_from_canister()
        .pending_two_year_maturity
        .expect("two-year maturity must become passive");
    assert_eq!(pending.nominal_disbursed_e8s, 225_000_000);
    assert_eq!(fixture.governance_calls().disburse_maturity, 1);
}

#[test]
fn disburse_maturity_decoded_rejection_retries_but_ambiguity_never_resubmits() {
    let _serial = lock_recovery_test();
    if std::env::var_os("POCKET_IC_BIN").is_none() {
        return;
    }
    let fixture = RecoveryFixture::new();
    fixture.replace(ready_two_week_state(&fixture));
    fixture.add_maturity(42, 200_000_000);
    fixture.control(ControlledCommand::DisburseMaturity, 1, 0);
    let args = PrepareTwoWeekMaturityArgs {
        entitlement_batch_generation: 1,
        target_e8s: 1_000_000,
    };
    assert!(matches!(
        update::<_, Result<(), ApiError>>(
            &fixture.pic,
            fixture.manager,
            fixture.stream,
            "prepare_two_week_maturity",
            args.clone(),
        ),
        Err(ApiError::Pending(_))
    ));
    assert!(matches!(
        fixture.state_from_canister().active_operation,
        Some(NnsOperation::Maturity(operation))
            if matches!(operation.phase, MaturityCommandPhase::Observed(_))
    ));
    assert_eq!(
        update::<_, Result<(), ApiError>>(
            &fixture.pic,
            fixture.manager,
            fixture.stream,
            "prepare_two_week_maturity",
            args,
        ),
        Ok(())
    );
    assert_eq!(fixture.governance_calls().disburse_maturity, 2);

    let ambiguous = RecoveryFixture::new();
    ambiguous.replace(ambiguous.state());
    ambiguous.add_maturity(41, 200_000_000);
    let _: Result<MaturityProgress, ApiError> = update(
        &ambiguous.pic,
        ambiguous.manager,
        ambiguous.sns_governance,
        "start_maturity",
        MaturityKind::TwoYear,
    );
    ambiguous.control(ControlledCommand::DisburseMaturity, 0, 1);
    assert!(matches!(ambiguous.resume(), Err(ApiError::Pending(_))));
    ambiguous.upgrade();
    assert!(matches!(
        ambiguous.resume(),
        Ok(NnsProgress::Maturity(MaturityProgress::Pending))
    ));
    assert_eq!(ambiguous.governance_calls().disburse_maturity, 1);
}
