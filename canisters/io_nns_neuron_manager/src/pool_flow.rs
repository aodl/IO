use io_ledger_boundary::{exact_icp_transfer, icp_account_identifier};
use io_nns_types::backing::{
    CompletedPoolCommand, FollowPolicy, PoolCommand, PoolCommandKind, PoolCommandPhase,
    PoolTargetResult, POOLED_PARENT_DELAY_SECONDS,
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
        if operation.transfer_block_index == Some(block_index) {
            return Ok(progress(&operation));
        }
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
    operation.transfer_block_index = Some(block_index);
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
                if validate_follow_target(parent_id, state::read().config.pooled_parent_followee_id)
                    .is_err()
                {
                    return fail_self_follow(operation, parent_id);
                }
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
    if observed.snapshot.cached_stake_e8s < operation.permit.expected_credit_e8s
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
    if validate_follow_target(parent_id, current.config.pooled_parent_followee_id).is_err() {
        return fail_self_follow(operation, parent_id);
    }
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
    let result = if additional == 0 {
        Ok(())
    } else {
        execution::increase_delay(&current.config, parent_id, additional).await
    };
    ensure(&operation)?;
    Err(execution::command_pending(
        "pooled parent delay submission",
        result,
    ))
}

async fn prove_delay(
    mut operation: PoolCommand,
    expected_delay_seconds: u64,
) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    let current = state::read();
    let follow_policy =
        match guarded_follow_policy(parent_id, current.config.pooled_parent_followee_id) {
            Ok(policy) => policy,
            Err(_) => return fail_self_follow(operation, parent_id),
        };
    let observed = execution::query_neuron_observation(&current.config, parent_id).await?;
    ensure(&operation)?;
    let Some(DissolveState::DissolveDelaySeconds(delay)) = observed.dissolve_state else {
        return stuck(
            &operation,
            "pooled parent is not canonically non-dissolving",
        );
    };
    if delay > expected_delay_seconds {
        return stuck(
            &operation,
            "pooled parent delay exceeded the immutable target",
        );
    }
    if delay < expected_delay_seconds {
        let remaining = u32::try_from(expected_delay_seconds - delay)
            .map_err(|_| ApiError::Invalid("pooled parent delay retry does not fit u32".into()))?;
        let result = execution::increase_delay(&current.config, parent_id, remaining).await;
        ensure(&operation)?;
        return Err(execution::command_pending(
            "pooled parent delay retry",
            result,
        ));
    }
    operation.phase = PoolCommandPhase::FollowingSubmitted;
    replace(operation.clone())?;
    let result = execution::set_following(&current.config, parent_id, follow_policy).await;
    ensure(&operation)?;
    Err(execution::command_pending(
        "pooled parent following submission",
        result,
    ))
}

async fn prove_following(operation: PoolCommand) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    let current = state::read();
    let observed = execution::query_neuron_observation(&current.config, parent_id).await?;
    ensure(&operation)?;
    let policy = guarded_follow_policy(parent_id, current.config.pooled_parent_followee_id)
        .map_err(ApiError::Stuck)?;
    if execution::has_follow_policy(&observed, policy) {
        return submit_refresh(operation).await;
    }
    let result = execution::set_following(&current.config, parent_id, policy).await;
    ensure(&operation)?;
    Err(execution::command_pending(
        "pooled parent following retry",
        result,
    ))
}

async fn submit_refresh(mut operation: PoolCommand) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    operation.phase = PoolCommandPhase::RefreshSubmitted;
    replace(operation.clone())?;
    let result = execution::refresh_neuron(&state::read().config, parent_id).await;
    ensure(&operation)?;
    Err(execution::command_pending(
        "pooled parent ClaimOrRefresh submission",
        result,
    ))
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
    if observed.snapshot.cached_stake_e8s < expected {
        let result = execution::refresh_neuron(&current.config, parent_id).await;
        ensure(&operation)?;
        return Err(execution::command_pending(
            "pooled parent ClaimOrRefresh retry",
            result,
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
    let target = latest
        .latest_pooled_target
        .as_mut()
        .ok_or_else(|| ApiError::Invalid("completed pool command lacks its target".into()))?;
    if target.target_e8s != expected {
        return Err(ApiError::Invalid(
            "completed pool command did not reach its exact target".into(),
        ));
    }
    let actual = observed.snapshot.cached_stake_e8s;
    let target_status = if actual == expected {
        target.status = crate::state::PooledTargetStatus::AtTarget;
        PoolTargetResult::AtTarget
    } else {
        target.status = crate::state::PooledTargetStatus::OverTarget;
        PoolTargetResult::OverTarget
    };
    latest.last_completed_pool = Some(CompletedPoolCommand {
        permit: operation.permit.clone(),
        transfer_block_index: operation
            .transfer_block_index
            .ok_or_else(|| ApiError::Invalid("pool transfer proof was not retained".into()))?,
        parent_neuron_id: parent_id,
        principal_e8s: actual,
    });
    latest.active_operation = None;
    latest.control_epoch = latest
        .control_epoch
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("control epoch overflow".into()))?;
    state::write(latest);
    Ok(PoolProgress::Completed {
        parent_neuron_id: parent_id,
        principal_e8s: actual,
        target_status,
    })
}

fn fail_self_follow(operation: PoolCommand, parent_id: u64) -> Result<PoolProgress, ApiError> {
    let mut latest = state::read();
    if matches!(&latest.active_operation, Some(NnsOperation::Pool(active)) if active == &operation)
    {
        latest.lifecycle = crate::state::Lifecycle::Paused;
        state::write(latest);
    }
    Err(ApiError::Invalid(format!(
        "pooled parent {parent_id} equals the configured followee; choose a different pre-launch memo or followee"
    )))
}

fn stuck(operation: &PoolCommand, reason: &str) -> Result<PoolProgress, ApiError> {
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(NnsOperation::Pool(active)) if active == operation)
    {
        return Err(ApiError::Busy);
    }
    latest.lifecycle = crate::state::Lifecycle::Paused;
    state::write(latest);
    Err(ApiError::Stuck(reason.into()))
}

fn validate_follow_target(parent_id: u64, followee_id: u64) -> Result<(), String> {
    if parent_id == followee_id {
        Err(format!(
            "pooled parent {parent_id} equals the configured followee; choose a different pre-launch memo or followee"
        ))
    } else {
        Ok(())
    }
}

fn guarded_follow_policy(parent_id: u64, followee_id: u64) -> Result<FollowPolicy, String> {
    validate_follow_target(parent_id, followee_id)?;
    Ok(FollowPolicy {
        followee_neuron_id: followee_id,
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
        _ => PoolProgress::AwaitingProof,
    }
}

#[cfg(test)]
mod tests {
    use super::{guarded_follow_policy, validate_follow_target};
    use std::cell::Cell;

    #[test]
    fn pooled_parent_self_follow_is_rejected_before_following() {
        assert!(validate_follow_target(42, 42)
            .unwrap_err()
            .contains("different pre-launch memo or followee"));
        assert_eq!(validate_follow_target(42, 43), Ok(()));
    }

    #[test]
    fn controlled_follow_submitter_is_not_called_for_self_follow() {
        let submissions = Cell::new(0);
        if let Ok(policy) = guarded_follow_policy(42, 42) {
            submissions.set(submissions.get() + 1);
            assert_eq!(policy.followee_neuron_id, 42);
        }
        assert_eq!(submissions.get(), 0);

        let policy = guarded_follow_policy(42, 43).unwrap();
        submissions.set(submissions.get() + 1);
        assert_eq!(policy.followee_neuron_id, 43);
        assert_eq!(submissions.get(), 1);
    }
}
