use crate::{
    api::{ApiError, JupiterProgress, NotifyJupiterDepositArgs},
    execution::{self, ExactTransferOutcome, StreamLiquidProgress},
    jupiter::{
        self, JupiterCompleted, JupiterDeposit, JupiterOperation, JupiterPauseReason, JupiterPhase,
        JupiterStuckTransfer, LiquidTransferSucceeded, StakeIncreaseProof, StakeTransferSucceeded,
    },
    state::{self, Lifecycle, NnsOperation},
    transfer::{
        NnsTransferAttempt, NnsTransferIntent, TransferOutcomeClassification, TransferState,
    },
};
use io_ledger_boundary::{exact_icp_transfer, icp_account_identifier, ExpectedQueryBlockTransfer};

fn enforce_activation_floor(
    current: &state::NnsStateV1,
    block_index: u128,
) -> Result<(), ApiError> {
    if block_index < current.config.jupiter_activation_block_floor {
        return Err(ApiError::Invalid(format!(
            "Jupiter block {block_index} predates activation floor {}",
            current.config.jupiter_activation_block_floor
        )));
    }
    Ok(())
}

pub async fn notify_jupiter_deposit(
    args: NotifyJupiterDepositArgs,
) -> Result<JupiterProgress, ApiError> {
    if let Some(completed) =
        state::processed_jupiter(args.block_index).map_err(ApiError::Invalid)?
    {
        return Ok(JupiterProgress::Completed(completed));
    }
    let current = crate::api::ready()?;
    enforce_activation_floor(&current, args.block_index)?;
    if let Some(NnsOperation::Jupiter(operation)) = &current.active_operation {
        if operation.deposit.block_index == args.block_index {
            return Ok(jupiter_progress(operation));
        }
        return Err(ApiError::Busy);
    }
    if current.active_operation.is_some() {
        return Err(ApiError::Busy);
    }

    lookup_and_begin(current, args.block_index).await
}

async fn lookup_and_begin(
    current: state::NnsStateV1,
    block_index: u128,
) -> Result<JupiterProgress, ApiError> {
    let transfer = exact_icp_transfer(current.config.icp_ledger, block_index)
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
        jupiter::fee_reduced_split(transfer.amount_e8s, current.config.expected_icp_fee_e8s)
            .map_err(ApiError::Invalid)?;
    let staging_balance =
        execution::icp_balance(&current.config, &current.config.jupiter_staging).await?;
    if staging_balance < transfer.amount_e8s {
        return Err(ApiError::Invalid(format!(
            "Jupiter staging balance {staging_balance} is below the exact deposit {}",
            transfer.amount_e8s
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
            block_index,
            gross_e8s: transfer.amount_e8s,
            stake_e8s,
            liquid_e8s,
            fee_e8s: current.config.expected_icp_fee_e8s,
            created_at_time_nanos: transfer.created_at_time,
        },
        phase: JupiterPhase::DepositProved,
    })));
    state::write(latest);
    Ok(JupiterProgress::DepositProved)
}

pub async fn resume(operation: JupiterOperation) -> Result<JupiterProgress, ApiError> {
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
        .checked_add(snapshot.config.ledger_deduplication_window_nanos)
        .ok_or_else(|| ApiError::Invalid("transfer deduplication deadline overflow".into()))?;
    let (first, can_submit) = match attempt.state {
        TransferState::Prepared => (now, true),
        TransferState::Submitted {
            first_submitted_at_nanos,
            last_submitted_at_nanos,
            ..
        } => (
            first_submitted_at_nanos,
            now.checked_sub(last_submitted_at_nanos)
                .is_some_and(|elapsed| elapsed >= snapshot.config.transfer_retry_delay_nanos),
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
    if ensure_exact_jupiter(&submitted, &state::read()).is_err() {
        return Err(ApiError::Busy);
    }
    let block = match execution::classify_transfer(result)? {
        ExactTransferOutcome::Succeeded(block) => block,
        ExactTransferOutcome::Paused(classification, reason) => {
            attempt.state = TransferState::Stuck {
                reason: reason.clone(),
            };
            let pause_reason = match classification {
                TransferOutcomeClassification::AmbiguousPossibleEffect => {
                    JupiterPauseReason::AmbiguousPossibleEffect
                }
                TransferOutcomeClassification::BadFee => JupiterPauseReason::BadFee,
                TransferOutcomeClassification::InsufficientFunds => {
                    JupiterPauseReason::InsufficientFunds
                }
                TransferOutcomeClassification::RejectedNoEffect => JupiterPauseReason::Other,
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
            return Err(
                if classification == TransferOutcomeClassification::AmbiguousPossibleEffect {
                    ApiError::Pending(reason)
                } else {
                    ApiError::Stuck(reason)
                },
            );
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
    if ensure_exact_jupiter(&submitted, &state::read()).is_err() {
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
    if ensure_exact_jupiter(&submitted, &state::read()).is_err() {
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
    stream: io_receipt_types::JupiterReceiptResult,
) -> Result<JupiterProgress, ApiError> {
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

pub async fn prove_active_transfer(block_index: u128) -> Result<JupiterProgress, ApiError> {
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
    Ok(jupiter_progress(&operation))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PooledTargetStatus;
    use candid::Principal;

    fn valid_test_state() -> (Principal, crate::state::NnsStateV1) {
        let principal = Principal::from_slice(&[1; 29]);
        let account = |subaccount: u8| crate::state::Account {
            owner: principal,
            subaccount: Some(vec![subaccount; 32]),
        };
        (
            principal,
            crate::state::NnsStateV1 {
                launch_schema_marker: crate::state::LAUNCH_SCHEMA_MARKER,
                config: crate::state::NnsConfig {
                    sns_governance: Principal::from_slice(&[2; 29]),
                    stream_manager: Principal::from_slice(&[6; 29]),
                    jupiter: Principal::from_slice(&[3; 29]),
                    icp_ledger: Principal::from_slice(&[4; 29]),
                    nns_governance: Principal::from_slice(&[5; 29]),
                    two_year_neuron_id: 1,
                    pooled_parent_memo: 2,
                    pooled_parent_followee_id: 3,
                    minimum_parent_stake_e8s: 100_000_000,
                    jupiter_account: crate::state::Account {
                        owner: Principal::from_slice(&[3; 29]),
                        subaccount: None,
                    },
                    jupiter_staging: crate::state::Account {
                        owner: principal,
                        subaccount: None,
                    },
                    maturity_staging: account(2),
                    stream_liquid_account: crate::state::Account {
                        owner: Principal::from_slice(&[6; 29]),
                        subaccount: Some(vec![3; 32]),
                    },
                    expected_io_fee_e8s: 10_000,
                    expected_icp_fee_e8s: 10_000,
                    jupiter_activation_block_floor: 1,
                    audited_permanent_principal_e8s: 1,
                    transfer_retry_delay_nanos: 1_000_000_000,
                    ledger_deduplication_window_nanos: 86_400_000_000_000,
                },
                lifecycle: Lifecycle::Ready,
                active_operation: None,
                pooled_parent_id: None,
                pooled_parent_staking_account: None,
                live_cohorts: Vec::new(),
                last_completed_pool: None,
                last_held_reconciliation: None,
                latest_reconciliation_generation: 0,
                latest_pooled_target: None,
                two_year_maturity_baseline_reconciled: true,
                latest_started_two_week_generation: 0,
                latest_completed_two_week_generation: 0,
                pending_two_year_maturity: None,
                pending_two_week_maturity: None,
                last_two_year_maturity: None,
                last_two_week_maturity: None,
                next_operation_sequence: 1,
                control_epoch: 0,
            },
        )
    }

    #[test]
    fn target_updates_are_strict_and_coalesced() {
        let (principal, state) = valid_test_state();
        crate::state::initialize(state, principal).unwrap();
        assert_eq!(
            crate::state::target_status(9, 10, 1),
            PooledTargetStatus::UnderTarget
        );
        assert_eq!(
            crate::state::target_status(10, 10, 1),
            PooledTargetStatus::AtTarget
        );
        assert_eq!(
            crate::state::target_status(11, 10, 1),
            PooledTargetStatus::AtTargetWithinUnwindTolerance
        );
        assert_eq!(
            crate::state::target_status(12, 10, 1),
            PooledTargetStatus::OverTarget
        );
    }

    #[test]
    fn activation_floor_fails_before_ledger_work() {
        let (_, mut state) = valid_test_state();
        state.config.jupiter_activation_block_floor = 50;
        assert!(matches!(
            enforce_activation_floor(&state, 49),
            Err(ApiError::Invalid(message)) if message.contains("predates activation floor")
        ));
        enforce_activation_floor(&state, 50).unwrap();
        enforce_activation_floor(&state, 51).unwrap();

        // A later balance change cannot affect this local immutable boundary.
        state.config.minimum_parent_stake_e8s = u128::MAX;
        assert!(matches!(
            enforce_activation_floor(&state, 49),
            Err(ApiError::Invalid(_))
        ));
    }

    #[test]
    fn transfer_retry_deadline_is_immutable_intent_time() {
        let created: u64 = 10;
        let deadline = created.checked_add(100).unwrap();
        let first_submitted = 90;
        assert_eq!(deadline, 110);
        assert_ne!(first_submitted + 100, deadline);
    }

    #[test]
    fn reverse_callbacks_cannot_overwrite_any_newer_jupiter_phase() {
        let (principal, mut latest) = valid_test_state();
        latest.next_operation_sequence = 2;
        state::initialize(latest.clone(), principal).unwrap();
        let neuron = jupiter::NeuronSnapshot {
            neuron_id: 1,
            staking_subaccount: [7; 32],
            cached_stake_e8s: 1_000,
        };
        let stake_attempt = NnsTransferAttempt::prepared(NnsTransferIntent {
            ledger: latest.config.icp_ledger,
            source_subaccount: [1; 32],
            destination: crate::state::Account {
                owner: latest.config.nns_governance,
                subaccount: Some(neuron.staking_subaccount.to_vec()),
            },
            amount_e8s: 400,
            fee_e8s: 10,
            memo: vec![1],
            created_at_time_nanos: 1,
        })
        .unwrap();
        let succeeded = StakeTransferSucceeded {
            before: neuron.clone(),
            block_index: 10,
        };
        let proof = StakeIncreaseProof {
            before: neuron,
            after_cached_stake_e8s: 1_400,
            stake_transfer_block: 10,
        };
        let permit = jupiter::StreamReceiptPermit {
            sequence: 11,
            destination: latest.config.stream_liquid_account.clone(),
            memo: vec![2],
        };
        let liquid_attempt = NnsTransferAttempt::prepared(NnsTransferIntent {
            ledger: latest.config.icp_ledger,
            source_subaccount: [1; 32],
            destination: permit.destination.clone(),
            amount_e8s: 600,
            fee_e8s: 10,
            memo: permit.memo.clone(),
            created_at_time_nanos: 2,
        })
        .unwrap();
        let liquid = LiquidTransferSucceeded {
            proof: proof.clone(),
            permit: permit.clone(),
            block_index: 12,
        };
        let operation = |phase| JupiterOperation {
            operation_sequence: 1,
            dispatch_epoch: 0,
            captured_control_epoch: 0,
            deposit: JupiterDeposit {
                block_index: 1,
                gross_e8s: 1_000,
                stake_e8s: 400,
                liquid_e8s: 600,
                fee_e8s: 0,
                created_at_time_nanos: 1,
            },
            phase,
        };
        let cases = [
            (
                "neuron query during stake preparation",
                operation(JupiterPhase::DepositProved),
                operation(JupiterPhase::StakeTransferPrepared {
                    before: succeeded.before.clone(),
                    attempt: stake_attempt.clone(),
                }),
            ),
            (
                "stake transfer callback",
                operation(JupiterPhase::StakeTransferSubmitted {
                    before: succeeded.before.clone(),
                    attempt: stake_attempt,
                }),
                operation(JupiterPhase::StakeTransferSucceeded(succeeded.clone())),
            ),
            (
                "claim or refresh callback",
                operation(JupiterPhase::StakeTransferSucceeded(succeeded.clone())),
                operation(JupiterPhase::RefreshSubmitted(succeeded)),
            ),
            (
                "stake increase query",
                operation(JupiterPhase::RefreshSubmitted(StakeTransferSucceeded {
                    before: proof.before.clone(),
                    block_index: proof.stake_transfer_block,
                })),
                operation(JupiterPhase::StakeIncreaseProved(proof.clone())),
            ),
            (
                "receipt preparation",
                operation(JupiterPhase::StakeIncreaseProved(proof.clone())),
                operation(JupiterPhase::ReceiptPermitPrepared {
                    proof: proof.clone(),
                    permit: permit.clone(),
                }),
            ),
            (
                "liquid transfer callback",
                operation(JupiterPhase::LiquidTransferSubmitted {
                    proof: proof.clone(),
                    permit: permit.clone(),
                    attempt: liquid_attempt,
                }),
                operation(JupiterPhase::LiquidTransferSucceeded(liquid.clone())),
            ),
            (
                "receipt completion",
                operation(JupiterPhase::ReceiptCompletionSubmitted(liquid.clone())),
                operation(JupiterPhase::AwaitingStreamSettlement(liquid.clone())),
            ),
            (
                "stream settlement observation",
                operation(JupiterPhase::AwaitingStreamSettlement(liquid)),
                operation(JupiterPhase::Stuck {
                    reason: "newer reviewed state".into(),
                    pause_reason: JupiterPauseReason::Other,
                    transfer: None,
                }),
            ),
        ];
        for (name, expected, active) in cases {
            latest.active_operation = Some(NnsOperation::Jupiter(Box::new(active.clone())));
            state::write(latest.clone());
            let before = state::read();
            assert_eq!(
                replace_jupiter(&expected, active),
                Err(ApiError::Busy),
                "{name}"
            );
            assert_eq!(state::read(), before, "{name} mutated the newer operation");
        }
    }
}
