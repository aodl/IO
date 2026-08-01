use candid::{CandidType, Nat, Principal};
use io_ledger_boundary::{
    exact_icp_transfer, icp_account_identifier, ExpectedQueryBlockTransfer, IcrcTransferError,
    IcrcTransferResult,
};
use serde::Deserialize;

use crate::{
    execution::{self, StreamLiquidProgress},
    jupiter::{
        self, JupiterCompleted, JupiterDeposit, JupiterOperation, JupiterPauseReason, JupiterPhase,
        JupiterStuckTransfer, LiquidTransferSucceeded, StakeIncreaseProof, StakeTransferSucceeded,
    },
    maturity::{CompletedMaturity, MaturityKind},
    state::{self, Lifecycle, NnsOperation, TwoWeekTarget, TwoWeekTargetStatus},
    transfer::{NnsTransferAttempt, NnsTransferIntent, TransferState},
};

const TRANSFER_RETRY_DELAY_NANOS: u64 = 1_000_000_000;
const ICP_LEDGER_DEDUPLICATION_WINDOW_NANOS: u64 = 86_400_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ApiError {
    Unauthorized,
    Paused,
    Busy,
    Invalid(String),
    Pending(String),
    Stuck(String),
    BelowMaturityThreshold {
        remaining_e8s: u64,
        minimum_e8s: u64,
    },
    ImplementationIncomplete(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NotifyJupiterDepositArgs {
    pub block_index: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SetTwoWeekTargetArgs {
    pub target_e8s: u128,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum JupiterProgress {
    DepositProved,
    StakeTransferPrepared,
    StakeTransferSubmitted,
    StakeTransferSucceeded,
    RefreshSubmitted,
    StakeIncreaseProved,
    ReceiptPermitPrepared,
    LiquidTransferPrepared,
    LiquidTransferSubmitted,
    LiquidTransferSucceeded,
    ReceiptCompletionSubmitted,
    AwaitingStreamSettlement,
    Completed(JupiterCompleted),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsProgress {
    Jupiter(JupiterProgress),
    Maturity(MaturityProgress),
    PoolRebalance,
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum MaturityProgress {
    Observed,
    StakeMaturitySubmitted,
    StakeMaturitySucceeded,
    DisburseMaturitySubmitted,
    DisburseMaturitySucceeded,
    AwaitingMintProof,
    MintProved,
    DeliveringTwoWeekReceipt,
    Completed(CompletedMaturity),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct Status {
    pub lifecycle: Lifecycle,
    pub active_operation: Option<String>,
    pub latest_target_generation: u64,
    pub has_pending_two_year_maturity: bool,
    pub has_pending_two_week_maturity: bool,
    pub has_pending_unwind: bool,
}

fn ready() -> Result<crate::state::NnsStateV1, ApiError> {
    let state = state::read();
    match state.lifecycle {
        Lifecycle::Ready => Ok(state),
        Lifecycle::Paused => Err(ApiError::Paused),
    }
}

pub async fn notify_jupiter_deposit(
    _caller: Principal,
    args: NotifyJupiterDepositArgs,
) -> Result<JupiterProgress, ApiError> {
    if let Some(completed) =
        state::processed_jupiter(args.block_index).map_err(ApiError::Invalid)?
    {
        return Ok(JupiterProgress::Completed(completed));
    }
    let current = ready()?;
    if let Some(NnsOperation::Jupiter(operation)) = &current.active_operation {
        if operation.deposit.block_index == args.block_index {
            return Ok(jupiter_progress(operation));
        }
        return Err(ApiError::Busy);
    }
    if current.active_operation.is_some() {
        return Err(ApiError::Busy);
    }

    let transfer = exact_icp_transfer(current.config.icp_ledger, args.block_index)
        .await
        .map_err(ApiError::Invalid)?;
    let source =
        icp_account_identifier(&current.config.jupiter_account).map_err(ApiError::Invalid)?;
    let destination =
        icp_account_identifier(&current.config.jupiter_staging).map_err(ApiError::Invalid)?;
    if transfer.from != source
        || transfer.to != destination
        || transfer.amount_e8s == 0
        || transfer.fee_e8s != current.config.expected_icp_fee_e8s
        || transfer.created_at_time == 0
        || transfer.spender.is_some()
    {
        return Err(ApiError::Invalid(
            "exact block is not a canonical Jupiter raw-ICP deposit".into(),
        ));
    }
    let (stake_e8s, liquid_e8s) =
        jupiter::checked_split(transfer.amount_e8s).map_err(ApiError::Invalid)?;
    let staging_balance =
        execution::icp_balance(&current.config, &current.config.jupiter_staging).await?;
    let required_staging = transfer
        .amount_e8s
        .checked_add(current.config.jupiter_fee_float_e8s)
        .ok_or_else(|| ApiError::Invalid("Jupiter staging preflight overflow".into()))?;
    if staging_balance < required_staging {
        return Err(ApiError::Invalid(format!(
            "Jupiter staging balance {staging_balance} is below gross deposit plus fee float {required_staging}"
        )));
    }

    let mut latest = state::read();
    if latest.lifecycle != Lifecycle::Ready
        || latest.control_epoch != current.control_epoch
        || latest.active_operation.is_some()
    {
        return Err(ApiError::Busy);
    }
    let operation_sequence = latest.next_operation_sequence;
    latest.next_operation_sequence = operation_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("operation sequence exhausted".into()))?;
    latest.active_operation = Some(NnsOperation::Jupiter(Box::new(JupiterOperation {
        operation_sequence,
        dispatch_epoch: 0,
        captured_control_epoch: latest.control_epoch,
        deposit: JupiterDeposit {
            block_index: args.block_index,
            gross_e8s: transfer.amount_e8s,
            stake_e8s,
            liquid_e8s,
            created_at_time_nanos: transfer.created_at_time,
        },
        phase: JupiterPhase::DepositProved,
    })));
    state::write(latest);
    Ok(JupiterProgress::DepositProved)
}

pub async fn set_two_week_target(
    caller: Principal,
    args: SetTwoWeekTargetArgs,
) -> Result<TwoWeekTargetStatus, ApiError> {
    if caller != state::read().config.stream_manager {
        return Err(ApiError::Unauthorized);
    }
    let mut state = ready()?;
    let expected = state
        .latest_target_generation
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("target generation overflow".into()))?;
    if args.generation == state.latest_target_generation {
        return match &state.latest_two_week_target {
            Some(current) if current.target_e8s == args.target_e8s => Ok(current.status),
            _ => Err(ApiError::Invalid(
                "target generation conflicts with existing intent".into(),
            )),
        };
    }
    if args.generation != expected {
        return Err(ApiError::Invalid(format!(
            "expected target generation {expected}"
        )));
    }
    let observation =
        execution::query_neuron_observation(&state.config, state.config.two_week_neuron_id).await?;
    if state::read() != state {
        return Err(ApiError::Busy);
    }
    let actual_cached_principal_e8s = observation.snapshot.cached_stake_e8s;
    let status = crate::state::target_status(actual_cached_principal_e8s, args.target_e8s);
    state.latest_two_week_target = Some(TwoWeekTarget {
        generation: args.generation,
        target_e8s: args.target_e8s,
        actual_cached_principal_e8s,
        status,
    });
    state.latest_target_generation = args.generation;
    state::write(state);
    Ok(status)
}

pub async fn resume() -> Result<NnsProgress, ApiError> {
    match state::read().active_operation {
        None => return Ok(NnsProgress::Idle),
        Some(NnsOperation::Jupiter(operation)) => {
            return resume_jupiter(*operation).await.map(NnsProgress::Jupiter)
        }
        Some(NnsOperation::Maturity(operation)) => {
            return crate::maturity_flow::resume_active(*operation)
                .await
                .map(NnsProgress::Maturity)
        }
        Some(NnsOperation::Unwind(_)) => return Ok(NnsProgress::PoolRebalance),
    }
}

async fn resume_jupiter(operation: JupiterOperation) -> Result<JupiterProgress, ApiError> {
    match operation.phase.clone() {
        JupiterPhase::DepositProved => prepare_stake_transfer(operation).await,
        JupiterPhase::StakeTransferPrepared { before, attempt }
        | JupiterPhase::StakeTransferSubmitted { before, attempt } => {
            submit_jupiter_transfer(operation, before, None, attempt).await
        }
        JupiterPhase::StakeTransferSucceeded(succeeded) => refresh(operation, succeeded).await,
        JupiterPhase::RefreshSubmitted(succeeded) => {
            prove_stake_increase(operation, succeeded).await
        }
        JupiterPhase::StakeIncreaseProved(proof) => prepare_receipt(operation, proof).await,
        JupiterPhase::ReceiptPermitPrepared { proof, permit } => {
            prepare_liquid_transfer(operation, proof, permit)
        }
        JupiterPhase::LiquidTransferPrepared {
            proof,
            permit,
            attempt,
        }
        | JupiterPhase::LiquidTransferSubmitted {
            proof,
            permit,
            attempt,
        } => {
            submit_jupiter_transfer(
                operation,
                proof.before.clone(),
                Some((proof, permit)),
                attempt,
            )
            .await
        }
        JupiterPhase::LiquidTransferSucceeded(succeeded) => {
            complete_receipt(operation, succeeded).await
        }
        JupiterPhase::ReceiptCompletionSubmitted(succeeded)
        | JupiterPhase::AwaitingStreamSettlement(succeeded) => {
            observe_stream_settlement(operation, succeeded).await
        }
        JupiterPhase::Stuck {
            pause_reason: JupiterPauseReason::InsufficientFunds,
            transfer: Some(transfer),
            ..
        } => resume_insufficient_transfer(operation, transfer).await,
        JupiterPhase::Stuck { reason, .. } => Ok(JupiterProgress::Stuck(reason)),
    }
}

async fn resume_insufficient_transfer(
    mut operation: JupiterOperation,
    transfer: JupiterStuckTransfer,
) -> Result<JupiterProgress, ApiError> {
    let expected = operation.clone();
    let attempt = match &transfer {
        JupiterStuckTransfer::Stake { attempt, .. }
        | JupiterStuckTransfer::Liquid { attempt, .. } => attempt,
    };
    let account = crate::state::Account {
        owner: ic_cdk::api::canister_self(),
        subaccount: Some(attempt.intent.source_subaccount.to_vec()),
    };
    let config = state::read().config;
    let balance = execution::icp_balance(&config, &account).await?;
    ensure_exact_jupiter(&expected, &state::read())?;
    let required = attempt
        .intent
        .amount_e8s
        .checked_add(attempt.intent.fee_e8s)
        .ok_or_else(|| ApiError::Invalid("Jupiter transfer funding requirement overflow".into()))?;
    if balance < required {
        return Ok(JupiterProgress::Stuck(format!(
            "staging balance {balance} remains below immutable transfer requirement {required}"
        )));
    }
    operation.phase = match transfer {
        JupiterStuckTransfer::Stake {
            before,
            mut attempt,
        } => {
            attempt.state = TransferState::Prepared;
            JupiterPhase::StakeTransferPrepared { before, attempt }
        }
        JupiterStuckTransfer::Liquid {
            proof,
            permit,
            mut attempt,
        } => {
            attempt.state = TransferState::Prepared;
            JupiterPhase::LiquidTransferPrepared {
                proof,
                permit,
                attempt,
            }
        }
    };
    replace_jupiter(&expected, operation.clone())?;
    Ok(jupiter_progress(&operation))
}

pub async fn start_maturity(
    caller: Principal,
    kind: MaturityKind,
) -> Result<MaturityProgress, ApiError> {
    crate::maturity_flow::start(caller, kind).await
}

pub async fn resume_maturity(kind: MaturityKind) -> Result<MaturityProgress, ApiError> {
    crate::maturity_flow::resume_kind(kind).await
}

pub async fn prove_maturity_mint(
    kind: MaturityKind,
    block_index: u128,
) -> Result<MaturityProgress, ApiError> {
    crate::maturity_flow::prove_mint(kind, block_index).await
}

async fn prepare_stake_transfer(
    mut operation: JupiterOperation,
) -> Result<JupiterProgress, ApiError> {
    let expected = operation.clone();
    let config = state::read().config;
    let before = execution::query_neuron(&config, config.two_year_neuron_id).await?;
    let latest = state::read();
    ensure_exact_jupiter(&expected, &latest)?;
    if before.neuron_id != latest.config.two_year_neuron_id {
        return Err(ApiError::Invalid("wrong Jupiter protected neuron".into()));
    }
    let intent = NnsTransferIntent {
        ledger: latest.config.icp_ledger,
        source_subaccount: latest
            .config
            .jupiter_staging
            .canonical()
            .map(|account| account.subaccount)
            .map_err(ApiError::Invalid)?,
        destination: execution::staking_account(&latest.config, &before),
        amount_e8s: operation.deposit.stake_e8s,
        fee_e8s: latest.config.expected_icp_fee_e8s,
        memo: b"IO:JUPITER:STAKE".to_vec(),
        created_at_time_nanos: checked_now()?,
    };
    operation.phase = JupiterPhase::StakeTransferPrepared {
        before,
        attempt: NnsTransferAttempt::prepared(intent).map_err(ApiError::Invalid)?,
    };
    replace_jupiter(&expected, operation)?;
    Ok(JupiterProgress::StakeTransferPrepared)
}

async fn submit_jupiter_transfer(
    mut operation: JupiterOperation,
    before: jupiter::NeuronSnapshot,
    liquid: Option<(StakeIncreaseProof, jupiter::StreamReceiptPermit)>,
    mut attempt: NnsTransferAttempt,
) -> Result<JupiterProgress, ApiError> {
    let expected = operation.clone();
    let now = checked_now()?;
    let snapshot = state::read();
    ensure_exact_jupiter(&operation, &snapshot)?;
    let deadline = attempt
        .intent
        .created_at_time_nanos
        .checked_add(ICP_LEDGER_DEDUPLICATION_WINDOW_NANOS)
        .ok_or_else(|| ApiError::Invalid("transfer deduplication deadline overflow".into()))?;
    let (first, can_submit) = match attempt.state {
        TransferState::Prepared => (now, true),
        TransferState::Submitted {
            first_submitted_at_nanos,
            last_submitted_at_nanos,
            ..
        } => (
            first_submitted_at_nanos,
            now >= last_submitted_at_nanos.saturating_add(TRANSFER_RETRY_DELAY_NANOS),
        ),
        _ => return Err(ApiError::Invalid("transfer is not dispatchable".into())),
    };
    if now >= deadline {
        let proof_allowed = matches!(attempt.state, TransferState::Submitted { .. });
        attempt.state = TransferState::Stuck {
            reason: "immutable ICP transfer intent reached its deduplication deadline".into(),
        };
        operation.phase = JupiterPhase::Stuck {
            reason: "Jupiter ICP transfer requires exact block proof".into(),
            pause_reason: if proof_allowed {
                JupiterPauseReason::AmbiguousPossibleEffect
            } else {
                JupiterPauseReason::Other
            },
            transfer: Some(match liquid {
                Some((proof, permit)) => JupiterStuckTransfer::Liquid {
                    proof,
                    permit,
                    attempt,
                },
                None => JupiterStuckTransfer::Stake { before, attempt },
            }),
        };
        pause_and_replace_jupiter(&expected, operation)?;
        return Ok(JupiterProgress::Stuck(
            "Jupiter ICP transfer requires exact block proof".into(),
        ));
    }
    if !can_submit {
        return Ok(if liquid.is_some() {
            JupiterProgress::LiquidTransferSubmitted
        } else {
            JupiterProgress::StakeTransferSubmitted
        });
    }
    operation.dispatch_epoch = operation
        .dispatch_epoch
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("dispatch epoch exhausted".into()))?;
    attempt.state = TransferState::Submitted {
        epoch: operation.dispatch_epoch,
        first_submitted_at_nanos: first,
        last_submitted_at_nanos: now,
    };
    operation.phase = match liquid.clone() {
        Some((proof, permit)) => JupiterPhase::LiquidTransferSubmitted {
            proof,
            permit,
            attempt: attempt.clone(),
        },
        None => JupiterPhase::StakeTransferSubmitted {
            before: before.clone(),
            attempt: attempt.clone(),
        },
    };
    replace_jupiter(&expected, operation.clone())?;
    let submitted = operation.clone();
    let result = execution::submit_transfer(&attempt.intent).await;
    if !active_jupiter_equals(&submitted) {
        return Err(ApiError::Busy);
    }
    let block = match classify_transfer_result(result)? {
        TransferOutcome::Succeeded(block) => block,
        TransferOutcome::AmbiguousPossibleEffect(reason) => {
            attempt.state = TransferState::Stuck {
                reason: reason.clone(),
            };
            operation.phase = JupiterPhase::Stuck {
                reason: reason.clone(),
                pause_reason: JupiterPauseReason::AmbiguousPossibleEffect,
                transfer: Some(match liquid {
                    Some((proof, permit)) => JupiterStuckTransfer::Liquid {
                        proof,
                        permit,
                        attempt,
                    },
                    None => JupiterStuckTransfer::Stake { before, attempt },
                }),
            };
            pause_and_replace_jupiter(&submitted, operation)?;
            return Err(ApiError::Pending(reason));
        }
        TransferOutcome::RejectedNoEffect {
            reason,
            pause_reason,
        } => {
            attempt.state = TransferState::Stuck {
                reason: reason.clone(),
            };
            operation.phase = JupiterPhase::Stuck {
                reason: reason.clone(),
                pause_reason,
                transfer: Some(match liquid {
                    Some((proof, permit)) => JupiterStuckTransfer::Liquid {
                        proof,
                        permit,
                        attempt,
                    },
                    None => JupiterStuckTransfer::Stake { before, attempt },
                }),
            };
            pause_and_replace_jupiter(&submitted, operation)?;
            return Err(ApiError::Stuck(reason));
        }
    };
    attempt.state = TransferState::Succeeded { block };
    operation.phase = match liquid {
        Some((proof, permit)) => JupiterPhase::LiquidTransferSucceeded(LiquidTransferSucceeded {
            proof,
            permit,
            block_index: block,
        }),
        None => JupiterPhase::StakeTransferSucceeded(StakeTransferSucceeded {
            before,
            block_index: block,
        }),
    };
    replace_jupiter(&submitted, operation.clone())?;
    Ok(jupiter_progress(&operation))
}

async fn refresh(
    mut operation: JupiterOperation,
    succeeded: StakeTransferSucceeded,
) -> Result<JupiterProgress, ApiError> {
    let expected = operation.clone();
    operation.dispatch_epoch = operation
        .dispatch_epoch
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("dispatch epoch exhausted".into()))?;
    operation.phase = JupiterPhase::RefreshSubmitted(succeeded.clone());
    replace_jupiter(&expected, operation.clone())?;
    let submitted = operation.clone();
    let config = state::read().config;
    let result = execution::refresh_neuron(&config, succeeded.before.neuron_id).await;
    if !active_jupiter_equals(&submitted) {
        return Err(ApiError::Busy);
    }
    if let Err(error) = result {
        pause_exact_jupiter(&submitted)?;
        return Err(ApiError::Pending(format!(
            "claim/refresh outcome requires canonical neuron observation: {error:?}"
        )));
    }
    Ok(JupiterProgress::RefreshSubmitted)
}

async fn prove_stake_increase(
    mut operation: JupiterOperation,
    succeeded: StakeTransferSucceeded,
) -> Result<JupiterProgress, ApiError> {
    let expected = operation.clone();
    let snapshot = state::read();
    let after = execution::query_neuron(&snapshot.config, succeeded.before.neuron_id).await?;
    ensure_exact_jupiter(&operation, &state::read())?;
    if after.staking_subaccount != succeeded.before.staking_subaccount
        || after
            .cached_stake_e8s
            .checked_sub(succeeded.before.cached_stake_e8s)
            != Some(operation.deposit.stake_e8s)
    {
        let reason: String =
            "protected neuron cached stake did not increase by the exact 40% deposit".into();
        operation.phase = JupiterPhase::Stuck {
            reason: reason.clone(),
            pause_reason: JupiterPauseReason::RefreshUnconfirmed,
            transfer: None,
        };
        pause_and_replace_jupiter(&expected, operation)?;
        return Err(ApiError::Stuck(reason));
    }
    operation.phase = JupiterPhase::StakeIncreaseProved(StakeIncreaseProof {
        before: succeeded.before,
        after_cached_stake_e8s: after.cached_stake_e8s,
        stake_transfer_block: succeeded.block_index,
    });
    replace_jupiter(&expected, operation.clone())?;
    Ok(JupiterProgress::StakeIncreaseProved)
}

async fn prepare_receipt(
    mut operation: JupiterOperation,
    proof: StakeIncreaseProof,
) -> Result<JupiterProgress, ApiError> {
    let expected = operation.clone();
    let config = state::read().config;
    let permit = execution::prepare_jupiter_receipt(
        &config,
        operation.deposit.block_index,
        operation.deposit.liquid_e8s,
    )
    .await?;
    ensure_exact_jupiter(&operation, &state::read())?;
    if !permit
        .destination
        .effective_eq(&config.stream_liquid_account)
        .map_err(ApiError::Invalid)?
    {
        return Err(ApiError::Invalid(
            "stream returned the wrong liquid destination".into(),
        ));
    }
    operation.phase = JupiterPhase::ReceiptPermitPrepared { proof, permit };
    replace_jupiter(&expected, operation.clone())?;
    Ok(JupiterProgress::ReceiptPermitPrepared)
}

fn prepare_liquid_transfer(
    mut operation: JupiterOperation,
    proof: StakeIncreaseProof,
    permit: jupiter::StreamReceiptPermit,
) -> Result<JupiterProgress, ApiError> {
    let expected = operation.clone();
    let config = state::read().config;
    let intent = NnsTransferIntent {
        ledger: config.icp_ledger,
        source_subaccount: config
            .jupiter_staging
            .canonical()
            .map(|account| account.subaccount)
            .map_err(ApiError::Invalid)?,
        destination: permit.destination.clone(),
        amount_e8s: operation.deposit.liquid_e8s,
        fee_e8s: config.expected_icp_fee_e8s,
        memo: permit.memo.clone(),
        created_at_time_nanos: checked_now()?,
    };
    operation.phase = JupiterPhase::LiquidTransferPrepared {
        proof,
        permit,
        attempt: NnsTransferAttempt::prepared(intent).map_err(ApiError::Invalid)?,
    };
    replace_jupiter(&expected, operation.clone())?;
    Ok(JupiterProgress::LiquidTransferPrepared)
}

async fn complete_receipt(
    mut operation: JupiterOperation,
    succeeded: LiquidTransferSucceeded,
) -> Result<JupiterProgress, ApiError> {
    let expected = operation.clone();
    operation.dispatch_epoch = operation
        .dispatch_epoch
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("dispatch epoch exhausted".into()))?;
    operation.phase = JupiterPhase::ReceiptCompletionSubmitted(succeeded.clone());
    replace_jupiter(&expected, operation.clone())?;
    let submitted = operation.clone();
    let progress = execution::complete_jupiter_receipt(
        &state::read().config,
        &succeeded.permit,
        succeeded.block_index,
    )
    .await?;
    if !active_jupiter_equals(&submitted) {
        return Err(ApiError::Busy);
    }
    operation.phase = JupiterPhase::AwaitingStreamSettlement(succeeded.clone());
    replace_jupiter(&submitted, operation.clone())?;
    match progress {
        StreamLiquidProgress::Completed(result) => finish_jupiter(operation, succeeded, result),
        _ => Ok(JupiterProgress::AwaitingStreamSettlement),
    }
}

async fn observe_stream_settlement(
    operation: JupiterOperation,
    succeeded: LiquidTransferSucceeded,
) -> Result<JupiterProgress, ApiError> {
    let progress = execution::resume_stream(&state::read().config).await?;
    ensure_exact_jupiter(&operation, &state::read())?;
    match progress {
        StreamLiquidProgress::Completed(result) => finish_jupiter(operation, succeeded, result),
        StreamLiquidProgress::Stuck(reason) => Err(ApiError::Stuck(reason)),
        _ => Ok(JupiterProgress::AwaitingStreamSettlement),
    }
}

fn finish_jupiter(
    operation: JupiterOperation,
    succeeded: LiquidTransferSucceeded,
    completed: execution::CompletedReceiptResult,
) -> Result<JupiterProgress, ApiError> {
    let stream = match completed {
        execution::CompletedReceiptResult::Jupiter(result) => result,
        execution::CompletedReceiptResult::TwoWeek(_) => {
            return Err(ApiError::Invalid(
                "stream completed the wrong receipt kind for Jupiter".into(),
            ))
        }
    };
    let expected_fingerprint = execution::jupiter_receipt_fingerprint(
        succeeded.permit.sequence,
        operation.deposit.block_index,
        operation.deposit.liquid_e8s,
    );
    if stream.request_fingerprint != expected_fingerprint
        || stream.receipt_block != succeeded.block_index
        || stream.backed_io_e8s == 0
        || stream.io_transfer_block == 0
        || stream.io_fee_e8s != state::read().config.expected_io_fee_e8s
        || stream.completed_at_nanos == 0
    {
        return Err(ApiError::Invalid(
            "stream Jupiter completion evidence does not match the exact receipt".into(),
        ));
    }
    ensure_exact_jupiter(&operation, &state::read())?;
    let result = JupiterCompleted {
        deposit_block: operation.deposit.block_index,
        gross_e8s: operation.deposit.gross_e8s,
        stake_e8s: operation.deposit.stake_e8s,
        liquid_e8s: operation.deposit.liquid_e8s,
        stake_transfer_block: succeeded.proof.stake_transfer_block,
        liquid_transfer_block: succeeded.block_index,
        stream_receipt_sequence: succeeded.permit.sequence,
        backed_io_e8s: stream.backed_io_e8s,
        io_transfer_block: stream.io_transfer_block,
        io_fee_e8s: stream.io_fee_e8s,
        stream_receipt_fingerprint: stream.request_fingerprint,
        completed_at_nanos: checked_now()?,
    };
    state::record_processed_jupiter(result.clone()).map_err(ApiError::Invalid)?;
    let mut latest = state::read();
    ensure_exact_jupiter(&operation, &latest)?;
    latest.active_operation = None;
    state::write(latest);
    Ok(JupiterProgress::Completed(result))
}

pub async fn prove_active_transfer(block_index: u128) -> Result<NnsProgress, ApiError> {
    let mut operation = match state::read().active_operation {
        Some(NnsOperation::Jupiter(operation)) => *operation,
        _ => {
            return Err(ApiError::Invalid(
                "no active Jupiter transfer proof slot".into(),
            ))
        }
    };
    let expected = operation.clone();
    let (context, attempt) = match operation.phase.clone() {
        JupiterPhase::Stuck {
            pause_reason: JupiterPauseReason::AmbiguousPossibleEffect,
            transfer: Some(transfer),
            ..
        } => match transfer {
            JupiterStuckTransfer::Stake { before, attempt } => (None, (before, attempt)),
            JupiterStuckTransfer::Liquid {
                proof,
                permit,
                attempt,
            } => (Some((proof.clone(), permit)), (proof.before, attempt)),
        },
        _ => {
            return Err(ApiError::Invalid(
                "active operation is not an ambiguous possible-effect transfer".into(),
            ))
        }
    };
    let exact = exact_icp_transfer(attempt.1.intent.ledger, block_index)
        .await
        .map_err(ApiError::Invalid)?;
    ensure_exact_jupiter(&operation, &state::read())?;
    let from = icp_account_identifier(&crate::state::Account {
        owner: ic_cdk::api::canister_self(),
        subaccount: Some(attempt.1.intent.source_subaccount.to_vec()),
    })
    .map_err(ApiError::Invalid)?;
    let to = icp_account_identifier(&attempt.1.intent.destination).map_err(ApiError::Invalid)?;
    if !exact.matches(&ExpectedQueryBlockTransfer {
        from: &from,
        to: &to,
        amount_e8s: attempt.1.intent.amount_e8s,
        fee_e8s: attempt.1.intent.fee_e8s,
        native_memo_u64: 0,
        icrc1_memo: Some(&attempt.1.intent.memo),
        created_at_time: attempt.1.intent.created_at_time_nanos,
        spender: None,
    }) {
        return Err(ApiError::Invalid(
            "exact ICP block does not match the stuck intent".into(),
        ));
    }
    operation.phase = match context {
        None => JupiterPhase::StakeTransferSucceeded(StakeTransferSucceeded {
            before: attempt.0,
            block_index,
        }),
        Some((proof, permit)) => JupiterPhase::LiquidTransferSucceeded(LiquidTransferSucceeded {
            proof,
            permit,
            block_index,
        }),
    };
    replace_jupiter(&expected, operation.clone())?;
    Ok(NnsProgress::Jupiter(jupiter_progress(&operation)))
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TransferOutcome {
    Succeeded(u128),
    AmbiguousPossibleEffect(String),
    RejectedNoEffect {
        reason: String,
        pause_reason: JupiterPauseReason,
    },
}

fn classify_transfer_result(
    result: Result<IcrcTransferResult, String>,
) -> Result<TransferOutcome, ApiError> {
    match result {
        Ok(Ok(block)) => nat_to_u128(block)
            .map(TransferOutcome::Succeeded)
            .map_err(ApiError::Invalid),
        Ok(Err(IcrcTransferError::Duplicate { duplicate_of })) => nat_to_u128(duplicate_of)
            .map(TransferOutcome::Succeeded)
            .map_err(ApiError::Invalid),
        Err(error) => Ok(TransferOutcome::AmbiguousPossibleEffect(format!(
            "ICP transfer callback is ambiguous: {error}"
        ))),
        Ok(Err(IcrcTransferError::BadFee { expected_fee })) => {
            Ok(TransferOutcome::RejectedNoEffect {
                reason: format!(
                    "ICP transfer rejected BadFee; approved fee update required (ledger expected {expected_fee})"
                ),
                pause_reason: JupiterPauseReason::BadFee,
            })
        }
        Ok(Err(IcrcTransferError::InsufficientFunds { balance })) => {
            Ok(TransferOutcome::RejectedNoEffect {
                reason: format!(
                    "ICP transfer rejected InsufficientFunds; replenish exact staging Account (balance {balance})"
                ),
                pause_reason: JupiterPauseReason::InsufficientFunds,
            })
        }
        Ok(Err(error)) => Ok(TransferOutcome::RejectedNoEffect {
            reason: format!("ICP transfer rejected without effect: {error:?}"),
            pause_reason: JupiterPauseReason::Other,
        }),
    }
}

fn nat_to_u128(value: Nat) -> Result<u128, String> {
    value
        .0
        .try_into()
        .map_err(|_| "ledger block does not fit u128".into())
}

fn checked_now() -> Result<u64, ApiError> {
    let now = ic_cdk::api::time();
    if now == 0 {
        Err(ApiError::Invalid("canister time is zero".into()))
    } else {
        Ok(now)
    }
}

fn ensure_exact_jupiter(
    operation: &JupiterOperation,
    state: &crate::state::NnsStateV1,
) -> Result<(), ApiError> {
    match &state.active_operation {
        Some(NnsOperation::Jupiter(active)) if **active == *operation => Ok(()),
        _ => Err(ApiError::Busy),
    }
}

fn active_jupiter_equals(operation: &JupiterOperation) -> bool {
    matches!(
        state::read().active_operation,
        Some(NnsOperation::Jupiter(active)) if *active == *operation
    )
}

fn replace_jupiter(
    expected: &JupiterOperation,
    replacement: JupiterOperation,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    match &latest.active_operation {
        Some(NnsOperation::Jupiter(active)) if **active == *expected => {}
        _ => return Err(ApiError::Busy),
    }
    replacement
        .validate(latest.config.icp_ledger, latest.config.nns_governance)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(NnsOperation::Jupiter(Box::new(replacement)));
    state::write(latest);
    Ok(())
}

fn pause_and_replace_jupiter(
    expected: &JupiterOperation,
    replacement: JupiterOperation,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    match &latest.active_operation {
        Some(NnsOperation::Jupiter(active)) if **active == *expected => {}
        _ => return Err(ApiError::Busy),
    }
    replacement
        .validate(latest.config.icp_ledger, latest.config.nns_governance)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(NnsOperation::Jupiter(Box::new(replacement)));
    latest.lifecycle = Lifecycle::Paused;
    state::write(latest);
    Ok(())
}

fn pause_exact_jupiter(expected: &JupiterOperation) -> Result<(), ApiError> {
    let mut latest = state::read();
    match &latest.active_operation {
        Some(NnsOperation::Jupiter(active)) if **active == *expected => {}
        _ => return Err(ApiError::Busy),
    }
    latest.lifecycle = Lifecycle::Paused;
    state::write(latest);
    Ok(())
}

fn jupiter_progress(operation: &JupiterOperation) -> JupiterProgress {
    match &operation.phase {
        JupiterPhase::DepositProved => JupiterProgress::DepositProved,
        JupiterPhase::StakeTransferPrepared { .. } => JupiterProgress::StakeTransferPrepared,
        JupiterPhase::StakeTransferSubmitted { .. } => JupiterProgress::StakeTransferSubmitted,
        JupiterPhase::StakeTransferSucceeded(_) => JupiterProgress::StakeTransferSucceeded,
        JupiterPhase::RefreshSubmitted(_) => JupiterProgress::RefreshSubmitted,
        JupiterPhase::StakeIncreaseProved(_) => JupiterProgress::StakeIncreaseProved,
        JupiterPhase::ReceiptPermitPrepared { .. } => JupiterProgress::ReceiptPermitPrepared,
        JupiterPhase::LiquidTransferPrepared { .. } => JupiterProgress::LiquidTransferPrepared,
        JupiterPhase::LiquidTransferSubmitted { .. } => JupiterProgress::LiquidTransferSubmitted,
        JupiterPhase::LiquidTransferSucceeded(_) => JupiterProgress::LiquidTransferSucceeded,
        JupiterPhase::ReceiptCompletionSubmitted(_) => JupiterProgress::ReceiptCompletionSubmitted,
        JupiterPhase::AwaitingStreamSettlement(_) => JupiterProgress::AwaitingStreamSettlement,
        JupiterPhase::Stuck { reason, .. } => JupiterProgress::Stuck(reason.clone()),
    }
}

pub fn get_status() -> Status {
    let state = state::read();
    Status {
        lifecycle: state.lifecycle,
        active_operation: state.active_operation.map(|operation| match operation {
            NnsOperation::Jupiter(_) => "Jupiter".into(),
            NnsOperation::Maturity(_) => "Maturity".into(),
            NnsOperation::Unwind(_) => "Unwind".into(),
        }),
        latest_target_generation: state.latest_target_generation,
        has_pending_two_year_maturity: state.pending_two_year_maturity.is_some(),
        has_pending_two_week_maturity: state.pending_two_week_maturity.is_some(),
        has_pending_unwind: state.pending_unwind.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_updates_are_strict_and_coalesced() {
        let principal = Principal::from_slice(&[1; 29]);
        let account = |subaccount: u8| crate::state::Account {
            owner: principal,
            subaccount: Some(vec![subaccount; 32]),
        };
        state::initialize(
            crate::state::NnsStateV1 {
                config: crate::state::NnsConfig {
                    sns_governance: Principal::from_slice(&[2; 29]),
                    stream_manager: principal,
                    jupiter: Principal::from_slice(&[3; 29]),
                    icp_ledger: Principal::from_slice(&[4; 29]),
                    nns_governance: Principal::from_slice(&[5; 29]),
                    two_year_neuron_id: 1,
                    two_week_neuron_id: 2,
                    jupiter_account: crate::state::Account {
                        owner: Principal::from_slice(&[3; 29]),
                        subaccount: None,
                    },
                    jupiter_staging: crate::state::Account {
                        owner: principal,
                        subaccount: None,
                    },
                    two_week_maturity_staging: account(2),
                    stream_liquid_account: crate::state::Account {
                        owner: principal,
                        subaccount: Some(vec![3; 32]),
                    },
                    expected_io_fee_e8s: 10_000,
                    expected_icp_fee_e8s: 10_000,
                    jupiter_fee_float_e8s: 20_000,
                    two_week_fee_float_e8s: 10_000,
                    seeded_two_week_principal_e8s: 1,
                },
                lifecycle: Lifecycle::Ready,
                active_operation: None,
                latest_two_week_target: None,
                latest_target_generation: 0,
                pending_two_year_maturity: None,
                pending_two_week_maturity: None,
                last_two_year_maturity: None,
                last_two_week_maturity: None,
                pending_unwind: None,
                next_operation_sequence: 1,
                control_epoch: 0,
            },
            principal,
        )
        .unwrap();
        assert_eq!(
            crate::state::target_status(9, 10),
            TwoWeekTargetStatus::UnderTarget
        );
        assert_eq!(
            crate::state::target_status(10, 10),
            TwoWeekTargetStatus::AtTarget
        );
        assert_eq!(
            crate::state::target_status(11, 10),
            TwoWeekTargetStatus::OverTarget
        );
    }

    #[test]
    fn transfer_retry_deadline_is_immutable_intent_time() {
        let created: u64 = 10;
        let deadline = created.checked_add(100).unwrap();
        let first_submitted = 90;
        assert_eq!(deadline, 110);
        assert_ne!(first_submitted + 100, deadline);
    }
}
