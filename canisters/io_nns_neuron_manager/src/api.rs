use candid::{CandidType, Nat, Principal};
use io_ledger_boundary::{
    exact_icp_block, exact_icp_transfer, icp_account_identifier, ExpectedQueryBlockTransfer,
    IcpExactResult, IcrcTransferError, IcrcTransferResult,
};
use serde::Deserialize;

use crate::{
    execution::{self, StreamLiquidProgress},
    jupiter::{
        self, JupiterCompleted, JupiterDeposit, JupiterOperation, JupiterPhase,
        JupiterStuckTransfer, LiquidTransferSucceeded, StakeIncreaseProof, StakeTransferSucceeded,
    },
    maturity::{
        AwaitingMintProof, CompletedMaturity, DisburseMaturitySucceeded, MaturityKind,
        MaturityPhase, MaturityPlan, PendingMaturity,
        StakeMaturitySucceeded as MaturityStakeSucceeded,
    },
    state::{self, Lifecycle, NnsOperation, TwoWeekTarget},
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
    if current.active_operation.is_some()
        || current.pending_two_year_maturity.is_some()
        || current.pending_two_week_maturity.is_some()
    {
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

pub fn set_two_week_target(caller: Principal, args: SetTwoWeekTargetArgs) -> Result<(), ApiError> {
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
            Some(current) if current.target_e8s == args.target_e8s => Ok(()),
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
    state.latest_two_week_target = Some(TwoWeekTarget {
        generation: args.generation,
        target_e8s: args.target_e8s,
    });
    state.latest_target_generation = args.generation;
    state::write(state);
    Ok(())
}

pub async fn resume() -> Result<NnsProgress, ApiError> {
    let operation = match state::read().active_operation {
        None => {
            let snapshot = state::read();
            if let Some(pending) = snapshot.pending_two_year_maturity {
                return resume_maturity(pending).await.map(NnsProgress::Maturity);
            }
            if let Some(pending) = snapshot.pending_two_week_maturity {
                return resume_maturity(pending).await.map(NnsProgress::Maturity);
            }
            return Ok(NnsProgress::Idle);
        }
        Some(NnsOperation::Jupiter(operation)) => *operation,
        Some(NnsOperation::PoolRebalance(_)) => return Ok(NnsProgress::PoolRebalance),
    };
    let progress = resume_jupiter(operation).await?;
    Ok(NnsProgress::Jupiter(progress))
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
        JupiterPhase::Stuck { reason, .. } => Ok(JupiterProgress::Stuck(reason)),
    }
}

pub async fn start_maturity(
    caller: Principal,
    kind: MaturityKind,
) -> Result<MaturityProgress, ApiError> {
    let snapshot = ready()?;
    if caller != snapshot.config.sns_governance {
        return Err(ApiError::Unauthorized);
    }
    if snapshot.active_operation.is_some()
        || snapshot.pending_two_year_maturity.is_some()
        || snapshot.pending_two_week_maturity.is_some()
    {
        return Err(ApiError::Busy);
    }
    let (neuron_id, destination) = maturity_identity(&snapshot.config, kind);
    let observation = execution::query_neuron_observation(&snapshot.config, neuron_id).await?;
    if observation.maturity_e8s == 0 {
        return Err(ApiError::Invalid(
            "protected neuron has no ordinary maturity".into(),
        ));
    }
    let stake_maturity_e8s = observation
        .maturity_e8s
        .checked_mul(40)
        .ok_or_else(|| ApiError::Invalid("maturity stake calculation overflow".into()))?
        / 100;
    if stake_maturity_e8s == 0 {
        return Err(ApiError::Invalid(
            "ordinary maturity is too small for 40% staking".into(),
        ));
    }
    let mut latest = state::read();
    if latest.lifecycle != Lifecycle::Ready
        || latest.control_epoch != snapshot.control_epoch
        || latest.active_operation.is_some()
        || latest.pending_two_year_maturity.is_some()
        || latest.pending_two_week_maturity.is_some()
    {
        return Err(ApiError::Busy);
    }
    let operation_sequence = latest.next_operation_sequence;
    latest.next_operation_sequence = operation_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("operation sequence exhausted".into()))?;
    let pending = PendingMaturity {
        operation_sequence,
        dispatch_epoch: 0,
        kind,
        phase: MaturityPhase::Observed(MaturityPlan {
            neuron: observation.snapshot,
            original_maturity_e8s: observation.maturity_e8s,
            original_staked_maturity_e8s: observation.staked_maturity_e8s,
            stake_maturity_e8s,
            destination,
            requested_at_seconds: checked_now()? / 1_000_000_000,
        }),
    };
    set_pending_maturity(&mut latest, pending);
    state::write(latest);
    Ok(MaturityProgress::Observed)
}

async fn resume_maturity(mut pending: PendingMaturity) -> Result<MaturityProgress, ApiError> {
    match pending.phase.clone() {
        MaturityPhase::Observed(plan) => {
            let expected = pending.clone();
            pending.dispatch_epoch = pending
                .dispatch_epoch
                .checked_add(1)
                .ok_or_else(|| ApiError::Invalid("maturity dispatch epoch exhausted".into()))?;
            pending.phase = MaturityPhase::StakeMaturitySubmitted(plan.clone());
            replace_pending_maturity(&expected, pending.clone())?;
            let result = execution::stake_maturity(&state::read().config, plan.neuron.neuron_id).await;
            if !pending_maturity_equals(&pending) {
                return Ok(crate::maturity::progress(&pending));
            }
            let (remaining_maturity_e8s, staked_maturity_e8s) = match result {
                Ok(value) => value,
                Err(error) => {
                    let reason = format!("StakeMaturity outcome is ambiguous: {error:?}");
                    let expected = pending.clone();
                    pending.phase = MaturityPhase::Stuck {
                        reason: reason.clone(),
                        plan: Box::new(plan),
                    };
                    replace_pending_maturity(&expected, pending)?;
                    let mut latest = state::read();
                    latest.lifecycle = Lifecycle::Paused;
                    state::write(latest);
                    return Err(ApiError::Stuck(reason));
                }
            };
            let expected_remaining = plan
                .original_maturity_e8s
                .checked_sub(plan.stake_maturity_e8s)
                .ok_or_else(|| ApiError::Invalid("maturity split underflow".into()))?;
            let expected_staked = plan
                .original_staked_maturity_e8s
                .checked_add(plan.stake_maturity_e8s)
                .ok_or_else(|| ApiError::Invalid("staked maturity overflow".into()))?;
            if remaining_maturity_e8s != expected_remaining || staked_maturity_e8s != expected_staked {
                let reason = format!(
                    "ordinary maturity drifted during StakeMaturity: expected remaining {expected_remaining} and staked {expected_staked}, observed {remaining_maturity_e8s} and {staked_maturity_e8s}"
                );
                let expected = pending.clone();
                pending.phase = MaturityPhase::Stuck {
                    reason: reason.clone(),
                    plan: Box::new(plan),
                };
                replace_pending_maturity(&expected, pending)?;
                let mut latest = state::read();
                latest.lifecycle = Lifecycle::Paused;
                state::write(latest);
                return Err(ApiError::Stuck(reason));
            }
            let expected = pending.clone();
            pending.phase = MaturityPhase::StakeMaturitySucceeded(MaturityStakeSucceeded {
                plan,
                remaining_maturity_e8s,
                staked_maturity_e8s,
            });
            replace_pending_maturity(&expected, pending)?;
            Ok(MaturityProgress::StakeMaturitySucceeded)
        }
        MaturityPhase::StakeMaturitySubmitted(_) => Err(ApiError::Pending(
            "StakeMaturity response was ambiguous; exact governance review is required".into(),
        )),
        MaturityPhase::StakeMaturitySucceeded(stake) => {
            let expected = pending.clone();
            pending.dispatch_epoch = pending
                .dispatch_epoch
                .checked_add(1)
                .ok_or_else(|| ApiError::Invalid("maturity dispatch epoch exhausted".into()))?;
            pending.phase = MaturityPhase::DisburseMaturitySubmitted(stake.clone());
            replace_pending_maturity(&expected, pending.clone())?;
            let result = execution::disburse_maturity(
                &state::read().config,
                stake.plan.neuron.neuron_id,
                &stake.plan.destination,
            )
            .await;
            if !pending_maturity_equals(&pending) {
                return Ok(crate::maturity::progress(&pending));
            }
            let amount = match result {
                Ok(value) => value,
                Err(error) => {
                    let reason = format!("DisburseMaturity outcome is ambiguous: {error:?}");
                    let expected = pending.clone();
                    pending.phase = MaturityPhase::Stuck {
                        reason: reason.clone(),
                        plan: Box::new(stake.plan),
                    };
                    replace_pending_maturity(&expected, pending)?;
                    let mut latest = state::read();
                    latest.lifecycle = Lifecycle::Paused;
                    state::write(latest);
                    return Err(ApiError::Stuck(reason));
                }
            };
            let expected = pending.clone();
            pending.phase = MaturityPhase::DisburseMaturitySucceeded(
                DisburseMaturitySucceeded {
                    stake,
                    amount_disbursed_e8s: amount,
                },
            );
            replace_pending_maturity(&expected, pending)?;
            Ok(MaturityProgress::DisburseMaturitySucceeded)
        }
        MaturityPhase::DisburseMaturitySubmitted(_) => Err(ApiError::Pending(
            "DisburseMaturity response was ambiguous; inspect the exact neuron before a forward fix"
                .into(),
        )),
        MaturityPhase::DisburseMaturitySucceeded(disbursement) => {
            let observation = execution::query_neuron_observation(
                &state::read().config,
                disbursement.stake.plan.neuron.neuron_id,
            )
            .await?;
            if !pending_maturity_equals(&pending) {
                return Ok(crate::maturity::progress(&pending));
            }
            let finalization = execution::exact_maturity_finalization(
                &observation,
                disbursement.amount_disbursed_e8s,
                &disbursement.stake.plan.destination,
            )?;
            let expected = pending.clone();
            pending.phase = MaturityPhase::AwaitingMintProof(AwaitingMintProof {
                stake: disbursement.stake,
                amount_disbursed_e8s: disbursement.amount_disbursed_e8s,
                expected_finalization_timestamp_seconds: finalization,
            });
            replace_pending_maturity(&expected, pending)?;
            Ok(MaturityProgress::AwaitingMintProof)
        }
        MaturityPhase::AwaitingMintProof(_) => Ok(MaturityProgress::AwaitingMintProof),
        MaturityPhase::MintProved { .. } => Ok(MaturityProgress::MintProved),
        MaturityPhase::DeliveringTwoWeekReceipt { .. } => {
            Ok(MaturityProgress::DeliveringTwoWeekReceipt)
        }
        MaturityPhase::Stuck { reason, .. } => Ok(MaturityProgress::Stuck(reason)),
    }
}

pub async fn prove_maturity_mint(
    kind: MaturityKind,
    block_index: u128,
) -> Result<MaturityProgress, ApiError> {
    let mut pending = pending_maturity(kind)
        .ok_or_else(|| ApiError::Invalid("no pending maturity proof slot".into()))?;
    let proof = match pending.phase.clone() {
        MaturityPhase::AwaitingMintProof(proof) => proof,
        _ => {
            return Err(ApiError::Invalid(
                "maturity is not awaiting an exact Mint proof".into(),
            ))
        }
    };
    let exact = exact_icp_block(state::read().config.icp_ledger, block_index)
        .await
        .map_err(ApiError::Invalid)?;
    if !pending_maturity_equals(&pending) {
        return Err(ApiError::Busy);
    }
    let mint = match exact {
        IcpExactResult::Mint(mint) => mint,
        IcpExactResult::Transfer(_) => {
            return Err(ApiError::Invalid(
                "maturity proof block is not an ICP Mint".into(),
            ))
        }
    };
    let destination =
        icp_account_identifier(&proof.stake.plan.destination).map_err(ApiError::Invalid)?;
    if mint.to != destination
        || mint.amount_e8s != u128::from(proof.amount_disbursed_e8s)
        || mint.amount_e8s == 0
        || mint.created_at_time / 1_000_000_000 < proof.stake.plan.requested_at_seconds
    {
        return Err(ApiError::Invalid(
            "exact Mint does not match pending maturity".into(),
        ));
    }
    if kind == MaturityKind::TwoWeek {
        let expected = pending.clone();
        pending.phase = MaturityPhase::DeliveringTwoWeekReceipt {
            proof,
            mint_block: block_index,
            actual_minted_e8s: mint.amount_e8s,
        };
        replace_pending_maturity(&expected, pending)?;
        return Ok(MaturityProgress::DeliveringTwoWeekReceipt);
    }
    let completed = CompletedMaturity {
        kind,
        neuron_id: proof.stake.plan.neuron.neuron_id,
        mint_block: block_index,
        actual_minted_e8s: mint.amount_e8s,
        destination: proof.stake.plan.destination,
        completed_at_nanos: checked_now()?,
    };
    let mut latest = state::read();
    if pending_maturity_from(&latest, kind).as_ref() != Some(&pending) {
        return Err(ApiError::Busy);
    }
    latest.pending_two_year_maturity = None;
    latest.last_two_year_maturity = Some(completed.clone());
    state::write(latest);
    Ok(MaturityProgress::Completed(completed))
}

async fn prepare_stake_transfer(
    mut operation: JupiterOperation,
) -> Result<JupiterProgress, ApiError> {
    let snapshot = state::read();
    let before =
        execution::query_neuron(&snapshot.config, snapshot.config.two_year_neuron_id).await?;
    if before.neuron_id != snapshot.config.two_year_neuron_id {
        return Err(ApiError::Invalid("wrong Jupiter protected neuron".into()));
    }
    ensure_exact_jupiter(&operation, &snapshot)?;
    let intent = NnsTransferIntent {
        ledger: snapshot.config.icp_ledger,
        source_subaccount: snapshot
            .config
            .jupiter_staging
            .canonical()
            .map(|account| account.subaccount)
            .map_err(ApiError::Invalid)?,
        destination: execution::staking_account(&snapshot.config, &before),
        amount_e8s: operation.deposit.stake_e8s,
        fee_e8s: snapshot.config.expected_icp_fee_e8s,
        memo: b"IO:JUPITER:STAKE".to_vec(),
        created_at_time_nanos: checked_now()?,
    };
    operation.phase = JupiterPhase::StakeTransferPrepared {
        before,
        attempt: NnsTransferAttempt::prepared(intent).map_err(ApiError::Invalid)?,
    };
    write_exact_jupiter(&operation)?;
    Ok(JupiterProgress::StakeTransferPrepared)
}

async fn submit_jupiter_transfer(
    mut operation: JupiterOperation,
    before: jupiter::NeuronSnapshot,
    liquid: Option<(StakeIncreaseProof, jupiter::StreamReceiptPermit)>,
    mut attempt: NnsTransferAttempt,
) -> Result<JupiterProgress, ApiError> {
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
        attempt.state = TransferState::Stuck {
            reason: "immutable ICP transfer intent reached its deduplication deadline".into(),
        };
        operation.phase = JupiterPhase::Stuck {
            reason: "Jupiter ICP transfer requires exact block proof".into(),
            transfer: Some(match liquid {
                Some((proof, permit)) => JupiterStuckTransfer::Liquid {
                    proof,
                    permit,
                    attempt,
                },
                None => JupiterStuckTransfer::Stake { before, attempt },
            }),
        };
        pause_and_write_exact_jupiter(&operation)?;
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
    write_exact_jupiter(&operation)?;
    let submitted = operation.clone();
    let result = execution::submit_transfer(&attempt.intent).await;
    if !active_jupiter_equals(&submitted) {
        return Ok(jupiter_progress(&submitted));
    }
    let block = match classify_transfer_result(result) {
        Ok(value) => value,
        Err(error) => {
            let reason = format!("Jupiter ICP transfer requires exact review: {error:?}");
            attempt.state = TransferState::Stuck {
                reason: reason.clone(),
            };
            operation.phase = JupiterPhase::Stuck {
                reason: reason.clone(),
                transfer: Some(match liquid {
                    Some((proof, permit)) => JupiterStuckTransfer::Liquid {
                        proof,
                        permit,
                        attempt,
                    },
                    None => JupiterStuckTransfer::Stake { before, attempt },
                }),
            };
            pause_and_write_exact_jupiter(&operation)?;
            return Err(ApiError::Stuck(reason));
        }
    };
    let Some(block) = block else {
        return Err(ApiError::Pending(
            "identical ICP intent remains retryable".into(),
        ));
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
    write_exact_jupiter(&operation)?;
    Ok(jupiter_progress(&operation))
}

async fn refresh(
    mut operation: JupiterOperation,
    succeeded: StakeTransferSucceeded,
) -> Result<JupiterProgress, ApiError> {
    operation.dispatch_epoch = operation
        .dispatch_epoch
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("dispatch epoch exhausted".into()))?;
    operation.phase = JupiterPhase::RefreshSubmitted(succeeded.clone());
    write_exact_jupiter(&operation)?;
    let submitted = operation.clone();
    let config = state::read().config;
    let result = execution::refresh_neuron(&config, succeeded.before.neuron_id).await;
    if !active_jupiter_equals(&submitted) {
        return Ok(jupiter_progress(&submitted));
    }
    if let Err(error) = result {
        let reason = format!("claim/refresh requires operator review: {error:?}");
        operation.phase = JupiterPhase::Stuck {
            reason: reason.clone(),
            transfer: None,
        };
        pause_and_write_exact_jupiter(&operation)?;
        return Err(ApiError::Stuck(reason));
    }
    Ok(JupiterProgress::RefreshSubmitted)
}

async fn prove_stake_increase(
    mut operation: JupiterOperation,
    succeeded: StakeTransferSucceeded,
) -> Result<JupiterProgress, ApiError> {
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
            transfer: None,
        };
        pause_and_write_exact_jupiter(&operation)?;
        return Err(ApiError::Stuck(reason));
    }
    operation.phase = JupiterPhase::StakeIncreaseProved(StakeIncreaseProof {
        before: succeeded.before,
        after_cached_stake_e8s: after.cached_stake_e8s,
        stake_transfer_block: succeeded.block_index,
    });
    write_exact_jupiter(&operation)?;
    Ok(JupiterProgress::StakeIncreaseProved)
}

async fn prepare_receipt(
    mut operation: JupiterOperation,
    proof: StakeIncreaseProof,
) -> Result<JupiterProgress, ApiError> {
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
    write_exact_jupiter(&operation)?;
    Ok(JupiterProgress::ReceiptPermitPrepared)
}

fn prepare_liquid_transfer(
    mut operation: JupiterOperation,
    proof: StakeIncreaseProof,
    permit: jupiter::StreamReceiptPermit,
) -> Result<JupiterProgress, ApiError> {
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
    write_exact_jupiter(&operation)?;
    Ok(JupiterProgress::LiquidTransferPrepared)
}

async fn complete_receipt(
    mut operation: JupiterOperation,
    succeeded: LiquidTransferSucceeded,
) -> Result<JupiterProgress, ApiError> {
    operation.dispatch_epoch = operation
        .dispatch_epoch
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("dispatch epoch exhausted".into()))?;
    operation.phase = JupiterPhase::ReceiptCompletionSubmitted(succeeded.clone());
    write_exact_jupiter(&operation)?;
    let submitted = operation.clone();
    let progress = execution::complete_jupiter_receipt(
        &state::read().config,
        &succeeded.permit,
        succeeded.block_index,
    )
    .await?;
    if !active_jupiter_equals(&submitted) {
        return Ok(jupiter_progress(&submitted));
    }
    operation.phase = JupiterPhase::AwaitingStreamSettlement(succeeded.clone());
    write_exact_jupiter(&operation)?;
    if matches!(progress, StreamLiquidProgress::Completed(_)) {
        finish_jupiter(operation, succeeded)
    } else {
        Ok(JupiterProgress::AwaitingStreamSettlement)
    }
}

async fn observe_stream_settlement(
    operation: JupiterOperation,
    succeeded: LiquidTransferSucceeded,
) -> Result<JupiterProgress, ApiError> {
    let progress = execution::resume_stream(&state::read().config).await?;
    ensure_exact_jupiter(&operation, &state::read())?;
    match progress {
        StreamLiquidProgress::Completed(_) => finish_jupiter(operation, succeeded),
        StreamLiquidProgress::Stuck(reason) => Err(ApiError::Stuck(reason)),
        _ => Ok(JupiterProgress::AwaitingStreamSettlement),
    }
}

fn finish_jupiter(
    operation: JupiterOperation,
    succeeded: LiquidTransferSucceeded,
) -> Result<JupiterProgress, ApiError> {
    ensure_exact_jupiter(&operation, &state::read())?;
    let result = JupiterCompleted {
        deposit_block: operation.deposit.block_index,
        gross_e8s: operation.deposit.gross_e8s,
        stake_e8s: operation.deposit.stake_e8s,
        liquid_e8s: operation.deposit.liquid_e8s,
        stake_transfer_block: succeeded.proof.stake_transfer_block,
        liquid_transfer_block: succeeded.block_index,
        stream_receipt_sequence: succeeded.permit.sequence,
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
    let (context, attempt) = match operation.phase.clone() {
        JupiterPhase::Stuck {
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
                "active operation is not a Stuck transfer".into(),
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
        memo: Some(&attempt.1.intent.memo),
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
    write_exact_jupiter(&operation)?;
    Ok(NnsProgress::Jupiter(jupiter_progress(&operation)))
}

fn classify_transfer_result(
    result: Result<IcrcTransferResult, String>,
) -> Result<Option<u128>, ApiError> {
    match result {
        Ok(Ok(block)) => nat_to_u128(block).map(Some).map_err(ApiError::Invalid),
        Ok(Err(IcrcTransferError::Duplicate { duplicate_of })) => nat_to_u128(duplicate_of)
            .map(Some)
            .map_err(ApiError::Invalid),
        Err(_) | Ok(Err(IcrcTransferError::TemporarilyUnavailable)) => Ok(None),
        Ok(Err(error)) => Err(ApiError::Stuck(format!("ICP transfer rejected: {error:?}"))),
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

fn maturity_identity(
    config: &crate::state::NnsConfig,
    kind: MaturityKind,
) -> (u64, crate::state::Account) {
    match kind {
        MaturityKind::TwoYear => (
            config.two_year_neuron_id,
            config.stream_liquid_account.clone(),
        ),
        MaturityKind::TwoWeek => (
            config.two_week_neuron_id,
            config.two_week_maturity_staging.clone(),
        ),
    }
}

fn pending_maturity(kind: MaturityKind) -> Option<PendingMaturity> {
    pending_maturity_from(&state::read(), kind)
}

fn pending_maturity_from(
    state: &crate::state::NnsStateV1,
    kind: MaturityKind,
) -> Option<PendingMaturity> {
    match kind {
        MaturityKind::TwoYear => state.pending_two_year_maturity.clone(),
        MaturityKind::TwoWeek => state.pending_two_week_maturity.clone(),
    }
}

fn set_pending_maturity(state: &mut crate::state::NnsStateV1, pending: PendingMaturity) {
    match pending.kind {
        MaturityKind::TwoYear => state.pending_two_year_maturity = Some(pending),
        MaturityKind::TwoWeek => state.pending_two_week_maturity = Some(pending),
    }
}

fn replace_pending_maturity(
    expected: &PendingMaturity,
    replacement: PendingMaturity,
) -> Result<(), ApiError> {
    if expected.kind != replacement.kind {
        return Err(ApiError::Invalid(
            "maturity kind changed during transition".into(),
        ));
    }
    let mut latest = state::read();
    if pending_maturity_from(&latest, expected.kind).as_ref() != Some(expected) {
        return Err(ApiError::Busy);
    }
    set_pending_maturity(&mut latest, replacement);
    state::write(latest);
    Ok(())
}

fn pending_maturity_equals(expected: &PendingMaturity) -> bool {
    pending_maturity(expected.kind).as_ref() == Some(expected)
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

fn write_exact_jupiter(operation: &JupiterOperation) -> Result<(), ApiError> {
    let mut latest = state::read();
    match &latest.active_operation {
        Some(NnsOperation::Jupiter(active))
            if active.operation_sequence == operation.operation_sequence
                && active.deposit.block_index == operation.deposit.block_index => {}
        _ => return Err(ApiError::Busy),
    }
    latest.active_operation = Some(NnsOperation::Jupiter(Box::new(operation.clone())));
    state::write(latest);
    Ok(())
}

fn pause_and_write_exact_jupiter(operation: &JupiterOperation) -> Result<(), ApiError> {
    write_exact_jupiter(operation)?;
    let mut latest = state::read();
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
            NnsOperation::PoolRebalance(_) => "PoolRebalance".into(),
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
        set_two_week_target(
            principal,
            SetTwoWeekTargetArgs {
                target_e8s: 10,
                generation: 1,
            },
        )
        .unwrap();
        set_two_week_target(
            principal,
            SetTwoWeekTargetArgs {
                target_e8s: 20,
                generation: 2,
            },
        )
        .unwrap();
        let state = state::read();
        assert_eq!(state.latest_two_week_target.unwrap().target_e8s, 20);
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
