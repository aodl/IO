use io_ledger_boundary::{exact_icp_transfer, icp_account_identifier};
use io_nns_types::backing::{
    CompletedPoolCommand, FollowPolicy, PoolCommand, PoolCommandKind, PoolCommandPhase,
    PoolTargetResult, NNS_DYNAMIC_DISSOLVE_DELAY_SECONDS,
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
        || transfer.created_at_time < operation.permit.prepared_at_nanos
        || transfer.spender.is_some()
    {
        return Err(ApiError::Invalid(
            "exact pool transfer does not match its permit".into(),
        ));
    }
    operation.phase = PoolCommandPhase::TransferProved { block_index };
    operation.transfer_block_index = Some(block_index);
    replace(operation.clone())?;
    Box::pin(resume(operation)).await
}

pub async fn resume(mut operation: PoolCommand) -> Result<PoolProgress, ApiError> {
    match operation.phase.clone() {
        PoolCommandPhase::SeedObserved => {
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
            operation.phase = PoolCommandPhase::ClaimSubmitted;
            replace(operation.clone())?;
            prove_parent(operation).await
        }
        PoolCommandPhase::AwaitingTransfer => Ok(progress(&operation)),
        PoolCommandPhase::TransferProved { .. } => submit_refresh(operation).await,
        PoolCommandPhase::ClaimSubmitted => prove_parent(operation).await,
        PoolCommandPhase::ParentIdentified => configure_delay(operation).await,
        PoolCommandPhase::DelaySubmitted {
            expected_delay_seconds,
        } => prove_delay(operation, expected_delay_seconds, true).await,
        PoolCommandPhase::FollowingSubmitted => prove_following(operation, true).await,
        PoolCommandPhase::RefreshSubmitted => complete_refresh(operation, true).await,
    }
}

async fn prove_parent(mut operation: PoolCommand) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    let observed = execution::query_neuron_observation(&state::read().config, parent_id).await?;
    ensure(&operation)?;
    if observed.snapshot.cached_stake_e8s < operation.permit.expected_parent_physical_e8s
        || execution::staking_account(&state::read().config, &observed.snapshot)
            != operation.permit.destination
    {
        return Err(ApiError::Pending(
            "pooled parent claim is not proved".into(),
        ));
    }
    operation.phase = PoolCommandPhase::ParentIdentified;
    replace(operation.clone())?;
    configure_delay(operation).await
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
    let additional = NNS_DYNAMIC_DISSOLVE_DELAY_SECONDS
        .checked_sub(delay)
        .ok_or_else(|| ApiError::Invalid("new pooled parent delay exceeds 14 days".into()))?;
    let additional = u32::try_from(additional)
        .map_err(|_| ApiError::Invalid("pooled parent delay increase does not fit u32".into()))?;
    operation.phase = PoolCommandPhase::DelaySubmitted {
        expected_delay_seconds: NNS_DYNAMIC_DISSOLVE_DELAY_SECONDS,
    };
    replace(operation.clone())?;
    let result = if additional == 0 {
        Ok(())
    } else {
        execution::increase_delay(&current.config, parent_id, additional).await
    };
    ensure(&operation)?;
    result?;
    prove_delay(operation, NNS_DYNAMIC_DISSOLVE_DELAY_SECONDS, false).await
}

async fn prove_delay(
    operation: PoolCommand,
    expected_delay_seconds: u64,
    retry_missing: bool,
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
        if !retry_missing {
            return Err(ApiError::Pending(
                "pooled parent delay command awaits canonical reflection".into(),
            ));
        }
        let remaining = u32::try_from(expected_delay_seconds - delay)
            .map_err(|_| ApiError::Invalid("pooled parent delay retry does not fit u32".into()))?;
        let result = execution::increase_delay(&current.config, parent_id, remaining).await;
        ensure(&operation)?;
        result?;
        return Box::pin(prove_delay(operation, expected_delay_seconds, false)).await;
    }
    submit_following(operation, follow_policy).await
}

async fn submit_following(
    mut operation: PoolCommand,
    follow_policy: FollowPolicy,
) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    let current = state::read();
    operation.phase = PoolCommandPhase::FollowingSubmitted;
    replace(operation.clone())?;
    let result = execution::set_following(&current.config, parent_id, follow_policy).await;
    ensure(&operation)?;
    result?;
    prove_following(operation, false).await
}

async fn prove_following(
    operation: PoolCommand,
    retry_missing: bool,
) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    let current = state::read();
    let observed = execution::query_neuron_observation(&current.config, parent_id).await?;
    ensure(&operation)?;
    let policy = guarded_follow_policy(parent_id, current.config.pooled_parent_followee_id)
        .map_err(ApiError::Stuck)?;
    if execution::has_follow_policy(&observed, policy) {
        return submit_refresh(operation).await;
    }
    if !retry_missing {
        return Err(ApiError::Pending(
            "pooled parent following command awaits canonical reflection".into(),
        ));
    }
    let result = execution::set_following(&current.config, parent_id, policy).await;
    ensure(&operation)?;
    result?;
    Box::pin(prove_following(operation, false)).await
}

async fn submit_refresh(mut operation: PoolCommand) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    operation.phase = PoolCommandPhase::RefreshSubmitted;
    replace(operation.clone())?;
    let result = execution::refresh_neuron(&state::read().config, parent_id).await;
    ensure(&operation)?;
    result?;
    complete_refresh(operation, false).await
}

async fn complete_refresh(
    operation: PoolCommand,
    retry_missing: bool,
) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    let current = state::read();
    let observed = execution::query_neuron_observation(&current.config, parent_id).await?;
    ensure(&operation)?;
    let expected_physical = operation
        .permit
        .expected_parent_physical_e8s
        .checked_add(operation.permit.expected_credit_e8s)
        .ok_or_else(|| ApiError::Invalid("pooled parent proof overflow".into()))?;
    if observed.snapshot.cached_stake_e8s < expected_physical {
        if !retry_missing {
            return Err(ApiError::Pending(
                "pooled parent ClaimOrRefresh awaits canonical stake reflection".into(),
            ));
        }
        let result = execution::refresh_neuron(&current.config, parent_id).await;
        ensure(&operation)?;
        result?;
        return Box::pin(complete_refresh(operation, false)).await;
    }
    finish_refresh(operation, observed, expected_physical)
}

fn finish_refresh(
    operation: PoolCommand,
    observed: execution::NeuronObservation,
    expected_physical: u128,
) -> Result<PoolProgress, ApiError> {
    let parent_id = operation.parent_neuron_id.ok_or(ApiError::Busy)?;
    let current = state::read();
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
    if operation.kind == PoolCommandKind::Bootstrap {
        if actual_bootstrap_principal(&observed) < io_nns_types::backing::DYNAMIC_ANCHOR_TARGET_E8S
        {
            return Err(ApiError::Invalid(
                "Dynamic bootstrap principal fell below the anchor target".into(),
            ));
        }
        latest.claim_bearing_dynamic_principal_e8s = 0;
        latest.anchor_available_e8s = io_nns_types::backing::DYNAMIC_ANCHOR_TARGET_E8S;
        latest.active_operation = None;
        let actual = actual_bootstrap_principal(&observed);
        state::write(latest);
        return Ok(PoolProgress::Completed {
            parent_neuron_id: parent_id,
            principal_e8s: actual,
            target_status: PoolTargetResult::AtTarget,
        });
    }
    let target = latest
        .latest_pooled_target
        .as_mut()
        .ok_or_else(|| ApiError::Invalid("completed pool command lacks its target".into()))?;
    let expected_claim = operation
        .permit
        .expected_parent_principal_e8s
        .checked_add(operation.permit.claim_credit_e8s)
        .ok_or_else(|| ApiError::Invalid("Dynamic claim principal overflow".into()))?;
    if target.target_e8s != expected_claim {
        return Err(ApiError::Invalid(
            "completed pool command did not reach its exact target".into(),
        ));
    }
    let actual = observed.snapshot.cached_stake_e8s;
    if actual < expected_physical {
        return Err(ApiError::Pending(
            "Dynamic top-up awaits canonical physical principal".into(),
        ));
    }
    latest.claim_bearing_dynamic_principal_e8s = expected_claim;
    latest.anchor_available_e8s = latest
        .anchor_available_e8s
        .checked_sub(operation.permit.fee_e8s)
        .ok_or_else(|| ApiError::Invalid("Dynamic anchor fee capacity underflow".into()))?;
    target.status = crate::state::PooledTargetStatus::AtTarget;
    let target_status = PoolTargetResult::AtTarget;
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

fn actual_bootstrap_principal(observed: &execution::NeuronObservation) -> u128 {
    observed.snapshot.cached_stake_e8s
}

fn fail_self_follow(operation: PoolCommand, parent_id: u64) -> Result<PoolProgress, ApiError> {
    let mut latest = state::read();
    if matches!(&latest.active_operation, Some(NnsOperation::Pool(active)) if active == &operation)
    {
        latest.lifecycle = crate::state::Lifecycle::Paused;
        state::write(latest);
    }
    Err(ApiError::Invalid(format!(
        "pooled parent {parent_id} collides with the fixed protected followee; production identity audit failed"
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
            "pooled parent {parent_id} collides with the fixed protected followee; production identity audit failed"
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
            .contains("production identity audit failed"));
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
