use io_nns_types::inflow::{BackingEffect, ProveBackingEffectArgs};
use io_reward_policy::ClaimRoute;

use crate::{
    api::{ApiError, MaturityProgress},
    execution,
    maturity::{MaturityCommandOperation, ParentCreditPhase},
    maturity_flow,
    state::{self, NnsOperation},
};

pub(crate) async fn advance(
    operation: MaturityCommandOperation,
    credit: u128,
) -> Result<MaturityProgress, ApiError> {
    let delivery = maturity_flow::delivery_ref(&operation).clone();
    let permit = delivery.permit.as_ref().ok_or(ApiError::Busy)?;
    let config = state::read().config;
    match delivery.parent_credit_phase {
        ParentCreditPhase::NotRequired | ParentCreditPhase::Required => {
            if let Some(parent_id) = state::read().pooled_parent_id {
                let mut replacement = operation.clone();
                maturity_flow::delivery_mut(&mut replacement).parent_credit_phase =
                    ParentCreditPhase::RefreshSubmitted {
                        parent_neuron_id: parent_id,
                    };
                maturity_flow::write_exact(&operation, replacement, false)?;
                execution::refresh_neuron(&config, parent_id).await?;
            } else {
                let parent_id = execution::claim_parent(&config, config.pooled_parent_memo).await?;
                maturity_flow::ensure_exact(&operation)?;
                let mut replacement = operation.clone();
                maturity_flow::delivery_mut(&mut replacement).parent_credit_phase =
                    ParentCreditPhase::ClaimSubmitted {
                        parent_neuron_id: parent_id,
                    };
                maturity_flow::write_exact(&operation, replacement, false)?;
            }
        }
        ParentCreditPhase::ClaimSubmitted { parent_neuron_id } => {
            let observed = execution::query_neuron_observation(&config, parent_neuron_id).await?;
            maturity_flow::ensure_exact(&operation)?;
            if execution::staking_account(&config, &observed.snapshot) != permit.pool_destination
                || observed.snapshot.cached_stake_e8s != credit
            {
                return Err(ApiError::Pending(
                    "new pooled parent credit is not proved".into(),
                ));
            }
            let Some(execution::DissolveState::DissolveDelaySeconds(delay)) =
                observed.dissolve_state
            else {
                return Err(ApiError::Invalid("new pooled parent is dissolving".into()));
            };
            let additional = io_nns_types::backing::POOLED_PARENT_DELAY_SECONDS
                .checked_sub(delay)
                .ok_or_else(|| ApiError::Invalid("new parent delay exceeds 14 days".into()))?;
            let mut replacement = operation.clone();
            maturity_flow::delivery_mut(&mut replacement).parent_credit_phase =
                ParentCreditPhase::DelaySubmitted { parent_neuron_id };
            maturity_flow::write_exact(&operation, replacement, false)?;
            execution::increase_delay(
                &config,
                parent_neuron_id,
                u32::try_from(additional)
                    .map_err(|_| ApiError::Invalid("delay increase does not fit u32".into()))?,
            )
            .await?;
        }
        ParentCreditPhase::DelaySubmitted { parent_neuron_id } => {
            let observed = execution::query_neuron_observation(&config, parent_neuron_id).await?;
            maturity_flow::ensure_exact(&operation)?;
            if observed.dissolve_state
                != Some(execution::DissolveState::DissolveDelaySeconds(
                    io_nns_types::backing::POOLED_PARENT_DELAY_SECONDS,
                ))
            {
                return Err(ApiError::Pending(
                    "pooled parent delay is not proved".into(),
                ));
            }
            let mut replacement = operation.clone();
            maturity_flow::delivery_mut(&mut replacement).parent_credit_phase =
                ParentCreditPhase::FollowingSubmitted { parent_neuron_id };
            maturity_flow::write_exact(&operation, replacement, false)?;
            execution::set_following(
                &config,
                parent_neuron_id,
                io_nns_types::backing::FollowPolicy {
                    followee_neuron_id: config.pooled_parent_followee_id,
                },
            )
            .await?;
        }
        ParentCreditPhase::FollowingSubmitted { parent_neuron_id } => {
            let mut replacement = operation.clone();
            maturity_flow::delivery_mut(&mut replacement).parent_credit_phase =
                ParentCreditPhase::VotingPowerRefreshSubmitted { parent_neuron_id };
            maturity_flow::write_exact(&operation, replacement, false)?;
            execution::refresh_voting_power(&config, parent_neuron_id).await?;
        }
        ParentCreditPhase::VotingPowerRefreshSubmitted { parent_neuron_id } => {
            let observed = execution::query_neuron_observation(&config, parent_neuron_id).await?;
            maturity_flow::ensure_exact(&operation)?;
            execution::validate_parent_configuration(
                &observed,
                io_nns_types::backing::FollowPolicy {
                    followee_neuron_id: config.pooled_parent_followee_id,
                },
            )
            .map_err(ApiError::Pending)?;
            prove_parent(operation, parent_neuron_id)?;
        }
        ParentCreditPhase::RefreshSubmitted { parent_neuron_id } => {
            let observed = execution::query_neuron_observation(&config, parent_neuron_id).await?;
            maturity_flow::ensure_exact(&operation)?;
            let expected = permit
                .expected_parent_before_e8s
                .checked_add(credit)
                .ok_or_else(|| ApiError::Invalid("parent credit proof overflow".into()))?;
            if observed.snapshot.cached_stake_e8s != expected {
                return Err(ApiError::Pending(
                    "pooled parent credit is not proved".into(),
                ));
            }
            prove_parent(operation, parent_neuron_id)?;
        }
        ParentCreditPhase::Proved { .. } => return prove_to_stream(operation).await,
    }
    Ok(MaturityProgress::DeliveringBackingInflow)
}

fn prove_parent(
    operation: MaturityCommandOperation,
    parent_neuron_id: u64,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(NnsOperation::Maturity(active)) if **active == operation)
    {
        return Err(ApiError::Busy);
    }
    let mut replacement = operation;
    let permit = maturity_flow::delivery_ref(&replacement)
        .permit
        .as_ref()
        .ok_or(ApiError::Busy)?
        .clone();
    maturity_flow::delivery_mut(&mut replacement).parent_credit_phase =
        ParentCreditPhase::Proved { parent_neuron_id };
    latest.pooled_parent_id = Some(parent_neuron_id);
    latest.pooled_parent_staking_account = Some(permit.pool_destination);
    latest.active_operation = Some(NnsOperation::Maturity(Box::new(replacement)));
    state::write(latest);
    Ok(())
}

async fn prove_to_stream(
    operation: MaturityCommandOperation,
) -> Result<MaturityProgress, ApiError> {
    let delivery = maturity_flow::delivery_ref(&operation);
    let permit = delivery.permit.as_ref().ok_or(ApiError::Busy)?;
    let (effect, block) = if permit.route().route == ClaimRoute::Mixed {
        (
            BackingEffect::PooledCredit,
            delivery.stream_pooled_block.ok_or(ApiError::Busy)?,
        )
    } else {
        (
            BackingEffect::FirstClaimCredit,
            delivery
                .claim_transfer
                .as_ref()
                .ok_or(ApiError::Busy)?
                .succeeded_block()
                .map_err(ApiError::Invalid)?,
        )
    };
    let progress = execution::prove_backing_effect(
        &state::read().config,
        ProveBackingEffectArgs {
            stream_operation_sequence: permit.stream_operation_sequence,
            effect,
            block_index: block,
        },
    )
    .await?;
    maturity_flow::ensure_exact(&operation)?;
    maturity_flow::resume_stream_backing(operation, progress).await
}
