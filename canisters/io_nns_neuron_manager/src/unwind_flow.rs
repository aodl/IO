use io_ledger_boundary::{exact_icp_transfer, icp_account_identifier};

use crate::{
    api::{ApiError, UnwindProgress},
    execution::{self, DissolveState},
    pool::{UnwindOperation, UnwindPhase},
    state::{self, Lifecycle, NnsConfig, NnsOperation},
};

pub async fn resume(operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    match operation.phase.clone() {
        UnwindPhase::SplitPrepared => split(operation).await,
        UnwindPhase::SplitSubmitted => Ok(UnwindProgress::Stuck(
            "Split outcome is ambiguous; governance review is required".into(),
        )),
        UnwindPhase::ChildCreated => observe_child(operation).await,
        UnwindPhase::StartDissolvingSubmitted => recover_dissolving(operation, true).await,
        UnwindPhase::Dissolving => {
            let expected = operation.clone();
            submit_dissolving(&expected, operation, false).await
        }
        UnwindPhase::StopDissolvingSubmitted => recover_dissolving(operation, false).await,
        UnwindPhase::MergePrepared => merge(operation).await,
        UnwindPhase::MergeSubmitted => Ok(UnwindProgress::Stuck(
            "Merge outcome is ambiguous; governance review is required".into(),
        )),
        UnwindPhase::ReadyToDisburse => disburse(operation).await,
        UnwindPhase::DisburseSubmitted => Ok(UnwindProgress::Stuck(
            "Disburse outcome is ambiguous; provide its exact ICP block".into(),
        )),
        UnwindPhase::AwaitingTransferProof { block_index, .. } => match block_index {
            Some(block_index) => prove(operation, block_index).await,
            None => Ok(UnwindProgress::AwaitingTransferProof),
        },
        UnwindPhase::Stuck(reason) => Ok(UnwindProgress::Stuck(reason)),
    }
}

pub async fn resume_passive(operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let current = state::read();
    let observation =
        execution::query_neuron_observation(&current.config, operation.child_neuron_id).await?;
    ensure_passive(&operation)?;
    let latest = state::read();
    let Some(DissolveState::WhenDissolvedTimestampSeconds(timestamp)) = observation.dissolve_state
    else {
        let mut latest = state::read();
        latest.lifecycle = Lifecycle::Paused;
        state::write(latest);
        return Ok(UnwindProgress::Stuck(
            "passive child is not canonically dissolving".into(),
        ));
    };
    if timestamp <= ic_cdk::api::time() / 1_000_000_000 {
        promote(&operation, UnwindPhase::ReadyToDisburse)?;
        let mut active = operation;
        active.phase = UnwindPhase::ReadyToDisburse;
        return disburse(active).await;
    }
    if latest
        .latest_two_week_target
        .as_ref()
        .is_some_and(|target| target.status == state::TwoWeekTargetStatus::UnderTarget)
        && latest.active_operation.is_none()
    {
        promote(&operation, UnwindPhase::Dissolving)?;
        let expected = operation.clone();
        return submit_dissolving(&expected, operation, false).await;
    }
    Ok(UnwindProgress::Waiting)
}

async fn split(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    operation.phase = UnwindPhase::SplitSubmitted;
    replace(&expected, operation.clone())?;
    let config = state::read().config;
    let result = execution::split_neuron(
        &config,
        config.two_week_neuron_id,
        operation.excess_e8s,
        operation.operation_sequence,
    )
    .await;
    ensure(&operation)?;
    let child_neuron_id = match result {
        Ok(child) => child,
        Err(error) => {
            return pause(
                operation,
                format!("Split requires reviewed recovery: {error:?}"),
            )
        }
    };
    let principal_e8s = operation
        .excess_e8s
        .checked_sub(config.expected_icp_fee_e8s)
        .ok_or_else(|| ApiError::Invalid("unwind excess cannot cover the Split fee".into()))?;
    let submitted = operation.clone();
    operation.phase = UnwindPhase::ChildCreated;
    operation.child_neuron_id = child_neuron_id;
    operation.principal_e8s = principal_e8s;
    replace(&submitted, operation.clone())?;
    Ok(UnwindProgress::Waiting)
}

async fn observe_child(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    let current = state::read();
    let observation =
        execution::query_neuron_observation(&current.config, operation.child_neuron_id).await?;
    ensure(&expected)?;
    if observation.snapshot.cached_stake_e8s != operation.principal_e8s {
        return pause(
            expected,
            "Split child principal differs from the exact fee-adjusted excess".into(),
        );
    }
    operation.child_staking_subaccount = observation.snapshot.staking_subaccount.to_vec();
    submit_dissolving(&expected, operation, true).await
}

async fn submit_dissolving(
    expected: &UnwindOperation,
    mut operation: UnwindOperation,
    start: bool,
) -> Result<UnwindProgress, ApiError> {
    let current = state::read();
    operation.phase = if start {
        UnwindPhase::StartDissolvingSubmitted
    } else {
        UnwindPhase::StopDissolvingSubmitted
    };
    replace(expected, operation.clone())?;
    let result = execution::set_dissolving(&current.config, operation.child_neuron_id, start).await;
    ensure(&operation)?;
    if let Err(error) = result {
        let command = if start {
            "StartDissolving"
        } else {
            "StopDissolving"
        };
        return pause(
            operation,
            format!("{command} requires canonical review: {error:?}"),
        );
    }
    finish_dissolving(operation, start, &current.config).await
}

async fn recover_dissolving(
    operation: UnwindOperation,
    start: bool,
) -> Result<UnwindProgress, ApiError> {
    let current = state::read();
    let observed =
        execution::query_neuron_observation(&current.config, operation.child_neuron_id).await?;
    ensure(&operation)?;
    let succeeded = matches!(
        observed.dissolve_state,
        Some(DissolveState::WhenDissolvedTimestampSeconds(_))
    ) == start;
    if !succeeded {
        return pause(operation, "dissolve command remains ambiguous".into());
    }
    finish_dissolving(operation, start, &current.config).await
}

async fn finish_dissolving(
    operation: UnwindOperation,
    start: bool,
    config: &NnsConfig,
) -> Result<UnwindProgress, ApiError> {
    if !start {
        let expected = operation.clone();
        advance(&expected, operation, UnwindPhase::MergePrepared)?;
        return Ok(UnwindProgress::Waiting);
    }
    let parent = execution::query_neuron_observation(config, config.two_week_neuron_id).await?;
    ensure(&operation)?;
    if parent.snapshot.cached_stake_e8s != operation.target_e8s {
        return pause(
            operation,
            "parent principal does not equal the exact post-Split target".into(),
        );
    }
    move_to_passive(&operation)?;
    Ok(UnwindProgress::Waiting)
}

async fn merge(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    operation.phase = UnwindPhase::MergeSubmitted;
    replace(&expected, operation.clone())?;
    let current = state::read();
    let result = execution::merge_neuron(
        &current.config,
        current.config.two_week_neuron_id,
        operation.child_neuron_id,
    )
    .await;
    ensure(&operation)?;
    if let Err(error) = result {
        return pause(
            operation,
            format!("Merge requires canonical review: {error:?}"),
        );
    }
    let observation =
        execution::query_neuron_observation(&current.config, current.config.two_week_neuron_id)
            .await?;
    ensure(&operation)?;
    let minimum_parent = operation
        .target_e8s
        .checked_add(operation.principal_e8s)
        .ok_or_else(|| ApiError::Invalid("merged parent expectation overflow".into()))?;
    if observation.snapshot.cached_stake_e8s < minimum_parent {
        return pause(
            operation,
            "merged child principal is not canonically observable in the parent".into(),
        );
    }
    clear(&operation)?;
    Ok(UnwindProgress::Waiting)
}

async fn disburse(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    let current = state::read();
    let submitted_at_seconds = ic_cdk::api::time() / 1_000_000_000;
    operation.phase = UnwindPhase::DisburseSubmitted;
    replace(&expected, operation.clone())?;
    let submitted = operation.clone();
    let result = execution::disburse_neuron(
        &current.config,
        operation.child_neuron_id,
        &current.config.stream_liquid_account,
    )
    .await;
    ensure(&operation)?;
    operation.phase = UnwindPhase::AwaitingTransferProof {
        block_index: result.as_ref().ok().copied(),
        submitted_at_seconds,
    };
    replace(&submitted, operation.clone())?;
    match result {
        Ok(_) => Ok(UnwindProgress::AwaitingTransferProof),
        Err(_) => {
            let mut latest = state::read();
            latest.lifecycle = Lifecycle::Paused;
            state::write(latest);
            Ok(UnwindProgress::AwaitingTransferProof)
        }
    }
}

pub async fn prove(
    operation: UnwindOperation,
    block_index: u128,
) -> Result<UnwindProgress, ApiError> {
    let (expected_block, submitted_at_seconds) = match operation.phase {
        UnwindPhase::AwaitingTransferProof {
            block_index,
            submitted_at_seconds,
        } => (block_index, submitted_at_seconds),
        UnwindPhase::DisburseSubmitted => (None, 0),
        _ => {
            return Err(ApiError::Invalid(
                "unwind is not awaiting an exact transfer proof".into(),
            ))
        }
    };
    if expected_block.is_some_and(|expected| expected != block_index) {
        return Err(ApiError::Invalid(
            "proof block differs from the canonical Disburse response".into(),
        ));
    }
    let current = state::read();
    let exact = exact_icp_transfer(current.config.icp_ledger, block_index)
        .await
        .map_err(ApiError::Invalid)?;
    ensure(&operation)?;
    let from = icp_account_identifier(&crate::state::Account {
        owner: current.config.nns_governance,
        subaccount: Some(operation.child_staking_subaccount.clone()),
    })
    .map_err(ApiError::Invalid)?;
    let to =
        icp_account_identifier(&current.config.stream_liquid_account).map_err(ApiError::Invalid)?;
    let amount = operation
        .principal_e8s
        .checked_sub(current.config.expected_icp_fee_e8s)
        .ok_or_else(|| ApiError::Invalid("child principal cannot cover Disburse fee".into()))?;
    if exact.from != from
        || exact.to != to
        || exact.amount_e8s != amount
        || exact.fee_e8s != current.config.expected_icp_fee_e8s
        || exact.icrc1_memo.is_some()
        || exact.spender.is_some()
        || exact.native_memo_u64 < submitted_at_seconds
        || exact.created_at_time / 1_000_000_000 < exact.native_memo_u64
    {
        return Err(ApiError::Invalid(
            "exact ICP block does not match direct child disbursement".into(),
        ));
    }
    clear(&operation)?;
    Ok(UnwindProgress::Completed {
        block_index,
        liquid_e8s: amount,
    })
}

fn ensure(expected: &UnwindOperation) -> Result<(), ApiError> {
    matches!(state::read().active_operation, Some(NnsOperation::Unwind(active)) if active == *expected)
        .then_some(())
        .ok_or(ApiError::Busy)
}

fn ensure_passive(expected: &UnwindOperation) -> Result<(), ApiError> {
    (state::read().pending_unwind.as_ref() == Some(expected))
        .then_some(())
        .ok_or(ApiError::Busy)
}

fn replace(expected: &UnwindOperation, replacement: UnwindOperation) -> Result<(), ApiError> {
    let mut latest = state::read();
    clear_active(&mut latest, expected)?;
    replacement
        .validate(latest.next_operation_sequence)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(NnsOperation::Unwind(replacement));
    state::write(latest);
    Ok(())
}

fn advance(
    expected: &UnwindOperation,
    mut operation: UnwindOperation,
    phase: UnwindPhase,
) -> Result<(), ApiError> {
    operation.phase = phase;
    replace(expected, operation)
}

fn move_to_passive(expected: &UnwindOperation) -> Result<(), ApiError> {
    let mut latest = state::read();
    if latest.pending_unwind.is_some() {
        return Err(ApiError::Busy);
    }
    clear_active(&mut latest, expected)?;
    let mut passive = expected.clone();
    passive.phase = UnwindPhase::Dissolving;
    latest.pending_unwind = Some(passive);
    state::write(latest);
    Ok(())
}

fn promote(expected: &UnwindOperation, phase: UnwindPhase) -> Result<(), ApiError> {
    let mut latest = state::read();
    if latest.active_operation.is_some() || latest.pending_unwind.as_ref() != Some(expected) {
        return Err(ApiError::Busy);
    }
    let mut active = expected.clone();
    active.phase = phase;
    latest.pending_unwind = None;
    latest.active_operation = Some(NnsOperation::Unwind(active));
    state::write(latest);
    Ok(())
}

fn pause(mut operation: UnwindOperation, reason: String) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    operation.phase = UnwindPhase::Stuck(reason.clone());
    replace(&expected, operation)?;
    let mut latest = state::read();
    latest.lifecycle = Lifecycle::Paused;
    state::write(latest);
    Ok(UnwindProgress::Stuck(reason))
}

fn clear(expected: &UnwindOperation) -> Result<(), ApiError> {
    let mut latest = state::read();
    clear_active(&mut latest, expected)?;
    state::write(latest);
    Ok(())
}

fn clear_active(
    state: &mut crate::state::NnsStateV1,
    expected: &UnwindOperation,
) -> Result<(), ApiError> {
    if !matches!(&state.active_operation, Some(NnsOperation::Unwind(active)) if active == expected)
    {
        return Err(ApiError::Busy);
    }
    state.active_operation = None;
    Ok(())
}
