use io_ledger_boundary::{exact_icp_transfer, icp_account_identifier};
use io_nns_types::backing::{
    FollowPolicy, PoolCommand, PoolCommandKind, PoolCommandPhase, POOLED_PARENT_DELAY_SECONDS,
};

use crate::{
    api::{ApiError, PoolProgress},
    execution::{self, DissolveState},
    state::{self, NnsOperation},
};

pub async fn prove_transfer(
    mut operation: PoolCommand,
    block_index: u128,
) -> Result<PoolProgress, ApiError> {
    if operation.phase != PoolCommandPhase::AwaitingTransfer {
        return Err(ApiError::Invalid(
            "pool command is not awaiting a transfer".into(),
        ));
    }
    let current = state::read();
    let transfer = exact_icp_transfer(current.config.icp_ledger, block_index)
        .await
        .map_err(ApiError::Invalid)?;
    ensure(&operation)?;
    let from =
        icp_account_identifier(&current.config.stream_liquid_account).map_err(ApiError::Invalid)?;
    let to = icp_account_identifier(&operation.permit.destination).map_err(ApiError::Invalid)?;
    if transfer.from != from
        || transfer.to != to
        || transfer.amount_e8s != operation.permit.expected_credit_e8s
        || transfer.fee_e8s != operation.permit.fee_e8s
        || transfer.icrc1_memo.as_deref() != Some(operation.permit.memo.as_slice())
        || transfer.created_at_time != operation.permit.prepared_at_nanos
        || transfer.spender.is_some()
    {
        return Err(ApiError::Invalid(
            "exact pool transfer does not match its permit".into(),
        ));
    }
    operation.phase = PoolCommandPhase::TransferProved { block_index };
    replace(operation.clone())?;
    Ok(progress(&operation))
}

pub async fn resume(mut operation: PoolCommand) -> Result<PoolProgress, ApiError> {
    match operation.phase.clone() {
        PoolCommandPhase::AwaitingTransfer => Ok(progress(&operation)),
        PoolCommandPhase::TransferProved { block_index } => match operation.kind {
            PoolCommandKind::Bootstrap => {
                let parent_id = execution::claim_parent(
                    &state::read().config,
                    state::read().config.pooled_parent_memo,
                )
                .await?;
                ensure(&operation)?;
                operation.parent_neuron_id = Some(parent_id);
                operation.phase = PoolCommandPhase::ClaimSubmitted { block_index };
                replace(operation.clone())?;
                Ok(progress(&operation))
            }
            PoolCommandKind::TopUp => submit_refresh(operation).await,
        },
        PoolCommandPhase::ClaimSubmitted { .. } => prove_parent(operation).await,
        PoolCommandPhase::ParentIdentified => configure_delay(operation).await,
        PoolCommandPhase::DelaySubmitted {
            expected_delay_seconds,
        } => prove_delay(operation, expected_delay_seconds).await,
        PoolCommandPhase::FollowingSubmitted => prove_following(operation).await,
        PoolCommandPhase::RefreshSubmitted => complete_refresh(operation).await,
    }
}

async fn prove_parent(mut operation: PoolCommand) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    let observed = execution::query_neuron_observation(&state::read().config, parent_id).await?;
    ensure(&operation)?;
    if observed.snapshot.cached_stake_e8s != operation.permit.expected_credit_e8s
        || execution::staking_account(&state::read().config, &observed.snapshot)
            != operation.permit.destination
    {
        return Err(ApiError::Pending(
            "pooled parent claim is not proved".into(),
        ));
    }
    operation.phase = PoolCommandPhase::ParentIdentified;
    replace(operation.clone())?;
    Ok(progress(&operation))
}

async fn configure_delay(mut operation: PoolCommand) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    let current = state::read();
    let observed = execution::query_neuron_observation(&current.config, parent_id).await?;
    ensure(&operation)?;
    let Some(DissolveState::DissolveDelaySeconds(delay)) = observed.dissolve_state else {
        return Err(ApiError::Invalid(
            "new pooled parent is not non-dissolving".into(),
        ));
    };
    let additional = POOLED_PARENT_DELAY_SECONDS
        .checked_sub(delay)
        .ok_or_else(|| ApiError::Invalid("new pooled parent delay exceeds 14 days".into()))?;
    let additional = u32::try_from(additional)
        .map_err(|_| ApiError::Invalid("pooled parent delay increase does not fit u32".into()))?;
    operation.phase = PoolCommandPhase::DelaySubmitted {
        expected_delay_seconds: POOLED_PARENT_DELAY_SECONDS,
    };
    replace(operation.clone())?;
    execution::increase_delay(&current.config, parent_id, additional).await?;
    Ok(progress(&operation))
}

async fn prove_delay(
    mut operation: PoolCommand,
    expected_delay_seconds: u64,
) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    let current = state::read();
    let observed = execution::query_neuron_observation(&current.config, parent_id).await?;
    ensure(&operation)?;
    if observed.dissolve_state != Some(DissolveState::DissolveDelaySeconds(expected_delay_seconds))
    {
        return Err(ApiError::Pending(
            "pooled parent delay is not proved".into(),
        ));
    }
    operation.phase = PoolCommandPhase::FollowingSubmitted;
    replace(operation.clone())?;
    execution::set_following(
        &current.config,
        parent_id,
        FollowPolicy {
            followee_neuron_id: current.config.pooled_parent_followee_id,
        },
    )
    .await?;
    Ok(progress(&operation))
}

async fn prove_following(operation: PoolCommand) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    let current = state::read();
    let observed = execution::query_neuron_observation(&current.config, parent_id).await?;
    ensure(&operation)?;
    execution::validate_parent_configuration(
        &observed,
        FollowPolicy {
            followee_neuron_id: current.config.pooled_parent_followee_id,
        },
    )
    .map_err(ApiError::Pending)?;
    submit_refresh(operation).await
}

async fn submit_refresh(mut operation: PoolCommand) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    operation.phase = PoolCommandPhase::RefreshSubmitted;
    replace(operation.clone())?;
    execution::refresh_neuron(&state::read().config, parent_id).await?;
    Ok(progress(&operation))
}

async fn complete_refresh(operation: PoolCommand) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    let current = state::read();
    let observed = execution::query_neuron_observation(&current.config, parent_id).await?;
    ensure(&operation)?;
    let expected = operation
        .permit
        .expected_parent_principal_e8s
        .checked_add(operation.permit.expected_credit_e8s)
        .ok_or_else(|| ApiError::Invalid("pooled parent proof overflow".into()))?;
    if observed.snapshot.cached_stake_e8s != expected {
        return Err(ApiError::Pending(
            "pooled parent credit is not proved".into(),
        ));
    }
    if operation.kind == PoolCommandKind::Bootstrap {
        execution::validate_parent_configuration(
            &observed,
            FollowPolicy {
                followee_neuron_id: current.config.pooled_parent_followee_id,
            },
        )
        .map_err(ApiError::Pending)?;
    }
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(NnsOperation::Pool(active)) if active == &operation)
    {
        return Err(ApiError::Busy);
    }
    latest.pooled_parent_id = Some(parent_id);
    latest.pooled_parent_staking_account = Some(operation.permit.destination.clone());
    latest.active_operation = None;
    latest.control_epoch = latest
        .control_epoch
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("control epoch overflow".into()))?;
    state::write(latest);
    Ok(PoolProgress::Completed {
        parent_neuron_id: parent_id,
        principal_e8s: expected,
    })
}

fn ensure(expected: &PoolCommand) -> Result<(), ApiError> {
    matches!(&state::read().active_operation, Some(NnsOperation::Pool(active)) if active == expected)
        .then_some(())
        .ok_or(ApiError::Busy)
}

fn replace(operation: PoolCommand) -> Result<(), ApiError> {
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(NnsOperation::Pool(active)) if active.permit.operation_sequence == operation.permit.operation_sequence)
    {
        return Err(ApiError::Busy);
    }
    operation
        .validate(latest.next_operation_sequence)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(NnsOperation::Pool(operation));
    state::write(latest);
    Ok(())
}

fn progress(operation: &PoolCommand) -> PoolProgress {
    match operation.phase {
        PoolCommandPhase::AwaitingTransfer => {
            PoolProgress::AwaitingTransfer(operation.permit.clone())
        }
        PoolCommandPhase::RefreshSubmitted => PoolProgress::AwaitingParentProof,
        _ => PoolProgress::ConfiguringParent,
    }
}
