use candid::CandidType;
use serde::Deserialize;

use crate::{
    api::ApiError,
    backing_registry,
    daily_stake::DailyStakeObservation,
    reward_evidence::{classify_sequence, event_credits_for, event_id},
    state::{
        self, BackingRewardRecord, BackingRewardStatus, Lifecycle, PendingEntitlementBatch,
        RewardEventClassification, RewardEventId, RewardEventObservation, SkippedRewardEvent,
        StructuralStakeState,
    },
};
use io_nns_types::reward_boundary::{self as reward_nns, CallError};

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RewardBackingProgress {
    Pending {
        reason: reward_nns::BackingNotReadyReason,
    },
    MaturityPrepared {
        generation: u64,
    },
}

pub async fn observe(now_nanos: u64) -> Result<RewardEventObservation, ApiError> {
    let mut initial = state::read();
    if initial.lifecycle != Lifecycle::Ready {
        return Err(ApiError::Paused);
    }
    if initial.reward_checkpoint.reward_processing_paused {
        return Err(ApiError::Invalid("reward processing is paused".into()));
    }
    if initial.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    if !initial.reward_checkpoint.reward_work_due && !initial.stake_observation_due {
        return Err(ApiError::Pending(
            "daily stake observation is not due".into(),
        ));
    }
    initial.reward_checkpoint.reward_work_due = false;
    initial.stake_observation_due = false;
    state::write(initial.clone());
    let result = observe_due(&initial, now_nanos).await;
    if let Err(error) = &result {
        handle_error(&initial, error);
    }
    result
}

async fn observe_due(
    expected: &state::StreamStateV1,
    now_nanos: u64,
) -> Result<RewardEventObservation, ApiError> {
    let daily = crate::daily_stake::observe(&expected.config).await?;
    let event = event_id(&daily.reward_event)?;
    let sequence = classify_sequence(
        expected.reward_checkpoint.last_processed_event,
        &daily.reward_event,
    )?;
    let mut records = backing_registry::reconcile(
        &expected.neuron_registry,
        &daily,
        event.round,
        &expected.config,
    )
    .map_err(ApiError::Invalid)?;
    let active_reward = active_reward_total(&records, &daily)?;
    validate_coverage(&daily, active_reward)?;
    let proposal_count = daily
        .reward_event
        .settled_proposal_count()
        .map_err(|error| {
            ApiError::Invalid(format!("SNS reward event proposal count failed: {error:?}"))
        })?;
    let (classification, policy_credit, eligible_credit, skipped) = match sequence {
        io_sns_reward_boundary::EventSequence::Same => {
            (RewardEventClassification::StructuralOnly, 0, 0, None)
        }
        io_sns_reward_boundary::EventSequence::First
        | io_sns_reward_boundary::EventSequence::Next => {
            let eligible = backing_registry::reward_eligible_ids(&records, event.round);
            let (classification, credits) = event_credits_for(
                expected.config.sns_governance,
                &expected.config.nonredeemable_governance_io_accounts,
                &daily.reward_event,
                &daily.neurons,
                Some(&eligible),
            )?;
            let total = credits
                .iter()
                .try_fold(0u128, |sum, credit| sum.checked_add(credit.event_credit))
                .ok_or_else(|| ApiError::Invalid("event credit total overflow".into()))?;
            backing_registry::apply_credits(&mut records, &credits)?;
            (
                classification,
                io_reward_policy::DAILY_EVENT_CREDIT,
                total,
                None,
            )
        }
        io_sns_reward_boundary::EventSequence::Skipped {
            previous,
            ambiguous_event_count,
            rounds_since_last_distribution,
            ..
        } => (
            RewardEventClassification::MissedSkipped,
            0,
            0,
            Some(SkippedRewardEvent {
                previous_event: previous.map(reward_event_id),
                observed_event: event,
                ambiguous_event_count,
                rounds_since_last_distribution,
                observed_at_nanos: now_nanos,
            }),
        ),
    };
    backing_registry::promote_pending(
        &mut records,
        event.round,
        daily.claim.pooled_principal_e8s,
        daily.claim.total_claim_backing_e8s,
        daily.claim.claim_supply_e8s,
        daily.active_backing_io_e8s,
    )
    .map_err(ApiError::Invalid)?;
    let observation = RewardEventObservation {
        event,
        proposal_count,
        classification,
        policy_credit,
        eligible_credit_total: eligible_credit,
        observed_at_nanos: now_nanos,
    };
    commit(
        expected,
        daily,
        records,
        observation.clone(),
        skipped,
        sequence,
    )?;
    crate::reward_timer::install_after(event);
    Ok(observation)
}

fn active_reward_total(
    records: &[BackingRewardRecord],
    daily: &DailyStakeObservation,
) -> Result<u128, ApiError> {
    daily.stakes.iter().try_fold(0u128, |sum, stake| {
        let eligible = stake.state == StructuralStakeState::Active
            && records
                .binary_search_by(|record| record.sns_neuron_id.cmp(&stake.sns_neuron_id))
                .ok()
                .is_some_and(|index| {
                    matches!(
                        records[index].status,
                        BackingRewardStatus::ActiveEligible { .. }
                    )
                });
        if eligible {
            sum.checked_add(stake.ledger_balance_e8s)
                .ok_or_else(|| ApiError::Invalid("active reward stake overflow".into()))
        } else {
            Ok(sum)
        }
    })
}

fn validate_coverage(daily: &DailyStakeObservation, active_reward: u128) -> Result<(), ApiError> {
    if daily.claim.pooled_principal_e8s > daily.claim.total_claim_backing_e8s {
        return Err(ApiError::Invalid(
            "pooled principal exceeds claim backing".into(),
        ));
    }
    let economics = io_core_model::EconomicState {
        backing: io_core_model::Backing {
            liquid: daily.claim.liquid_icp_e8s,
            pooled: daily.claim.pooled_principal_e8s,
            unwinding: daily.claim.unwinding_principal_e8s,
            transit: daily.claim.transit_backing_e8s,
        },
        claims: daily.claim.claim_supply_e8s,
        active_backing: daily.active_backing_io_e8s,
        active_reward,
    };
    io_core_model::claim_rate(economics)
        .and_then(|_| io_core_model::rewards_covered(economics))
        .map_err(|error| ApiError::Invalid(format!("reward coverage failed: {error:?}")))
}

fn commit(
    expected: &state::StreamStateV1,
    daily: DailyStakeObservation,
    records: Vec<BackingRewardRecord>,
    observation: RewardEventObservation,
    skipped: Option<SkippedRewardEvent>,
    sequence: io_sns_reward_boundary::EventSequence,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    if latest.config != expected.config
        || latest.control_epoch != expected.control_epoch
        || latest.lifecycle != Lifecycle::Ready
        || latest.active_operation.is_some()
        || latest.reward_checkpoint.last_processed_event
            != expected.reward_checkpoint.last_processed_event
        || latest.reward_checkpoint.reward_work_due
        || latest.stake_observation_due
    {
        return Err(ApiError::Busy);
    }
    if let Some(skipped) = skipped {
        latest.reward_checkpoint.missed_event_count = latest
            .reward_checkpoint
            .missed_event_count
            .checked_add(skipped.ambiguous_event_count)
            .ok_or_else(|| ApiError::Invalid("missed reward count overflow".into()))?;
        latest.reward_checkpoint.latest_skipped_event = Some(skipped);
    }
    if !matches!(sequence, io_sns_reward_boundary::EventSequence::Same) {
        latest.reward_checkpoint.last_processed_event = Some(observation.event);
        if observation.policy_credit > 0 {
            latest.reward_checkpoint.accumulated_policy_credit = latest
                .reward_checkpoint
                .accumulated_policy_credit
                .checked_add(observation.policy_credit)
                .ok_or_else(|| ApiError::Invalid("policy credit overflow".into()))?;
            latest.reward_checkpoint.processed_event_count = latest
                .reward_checkpoint
                .processed_event_count
                .checked_add(1)
                .ok_or_else(|| ApiError::Invalid("processed reward count overflow".into()))?;
        }
    }
    latest.reward_checkpoint.latest_observation = Some(observation.clone());
    latest.reward_checkpoint.governance_parameters_fresh = true;
    latest.neuron_registry = records;
    let target = io_core_model::target(
        daily.active_backing_io_e8s,
        daily.claim.total_claim_backing_e8s,
        daily.claim.claim_supply_e8s,
    )
    .map_err(|error| ApiError::Invalid(format!("pooled target failed: {error:?}")))?;
    let active_reward = active_reward_total(&latest.neuron_registry, &daily)?;
    let generation = latest
        .latest_reconciliation_generation
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("reconciliation generation overflow".into()))?;
    latest.latest_reconciliation_generation = generation;
    latest.latest_reconciliation_checkpoint = Some(state::ReconciliationCheckpoint {
        generation,
        event_marker: observation.event.round,
        observed_at_nanos: observation.observed_at_nanos,
        claim_supply_e8s: daily.claim.claim_supply_e8s,
        liquid_backing_e8s: daily.claim.liquid_icp_e8s,
        pooled_backing_e8s: daily.claim.pooled_principal_e8s,
        unwinding_backing_e8s: daily.claim.unwinding_principal_e8s,
        transit_backing_e8s: daily.claim.transit_backing_e8s,
        total_claim_backing_e8s: daily.claim.total_claim_backing_e8s,
        active_backing_io_e8s: daily.active_backing_io_e8s,
        active_reward_io_e8s: active_reward,
        live_cohort_count: u32::try_from(daily.nns.live_cohorts.len())
            .map_err(|_| ApiError::Invalid("live cohort count overflow".into()))?,
        oldest_ready_at_seconds: daily.nns.oldest_ready_at_seconds,
        pooled_target_e8s: target,
        observed_pooled_e8s: daily.claim.pooled_principal_e8s,
        snapshot_fingerprint: daily.claim.observation_fingerprint,
    });
    latest
        .reward_checkpoint
        .validate(&latest.config)
        .map_err(ApiError::Invalid)?;
    state::write(latest);
    Ok(())
}

fn handle_error(expected: &state::StreamStateV1, error: &ApiError) {
    let mut latest = state::read();
    if latest.config != expected.config || latest.control_epoch != expected.control_epoch {
        crate::reward_timer::install_retry();
        return;
    }
    latest.reward_checkpoint.reward_work_due = false;
    latest.stake_observation_due = false;
    if matches!(error, ApiError::Invalid(_) | ApiError::Stuck(_)) {
        latest.reward_checkpoint.reward_processing_paused = true;
        latest.reward_checkpoint.governance_parameters_fresh = false;
    }
    state::write(latest);
    crate::reward_timer::install_retry();
}

pub async fn resume_backing(now_nanos: u64) -> Result<RewardBackingProgress, ApiError> {
    let snapshot = state::read();
    if snapshot.lifecycle != Lifecycle::Ready {
        return Err(ApiError::Paused);
    }
    if let Some(batch) = snapshot.pending_entitlement_batch.clone() {
        return submit_maturity(snapshot, batch).await;
    }
    if snapshot.reward_checkpoint.reward_processing_paused
        || !snapshot.reward_checkpoint.governance_parameters_fresh
    {
        return Err(ApiError::Pending(
            "daily stake observation is not fresh".into(),
        ));
    }
    if !crate::pool_reconciliation::ensure_latest(now_nanos).await? {
        return Ok(RewardBackingProgress::Pending {
            reason: reward_nns::BackingNotReadyReason::ReconciliationPending,
        });
    }
    let snapshot = state::read();
    if snapshot.reward_checkpoint.accumulated_policy_credit == 0 {
        return Err(ApiError::Pending(
            "no fresh entitlement batch can be frozen".into(),
        ));
    }
    freeze_and_prepare(snapshot, now_nanos).await
}

async fn freeze_and_prepare(
    snapshot: state::StreamStateV1,
    now_nanos: u64,
) -> Result<RewardBackingProgress, ApiError> {
    let through_event = snapshot
        .reward_checkpoint
        .last_processed_event
        .ok_or_else(|| ApiError::Invalid("reward checkpoint is missing".into()))?;
    let generation = snapshot
        .latest_entitlement_batch_generation
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("entitlement generation exhausted".into()))?;
    let mut latest = state::read();
    if latest != snapshot || latest.pending_entitlement_batch.is_some() {
        return Err(ApiError::Busy);
    }
    let entries = backing_registry::freeze(&mut latest.neuron_registry);
    let eligible_credit_total = entries
        .iter()
        .try_fold(0u128, |sum, entry| {
            sum.checked_add(entry.accumulated_eligible_credit)
        })
        .ok_or_else(|| ApiError::Invalid("entitlement total overflow".into()))?;
    let target = latest
        .latest_reconciliation_checkpoint
        .as_ref()
        .ok_or_else(|| ApiError::Pending("reconciliation checkpoint is missing".into()))?
        .pooled_target_e8s;
    let batch = PendingEntitlementBatch {
        generation,
        frozen_at_timestamp_seconds: now_nanos / 1_000_000_000,
        through_event,
        target_icp_e8s: target,
        entries,
        eligible_credit_total,
        policy_credit_total: latest.reward_checkpoint.accumulated_policy_credit,
        processed_event_count: latest.reward_checkpoint.processed_event_count,
    };
    batch.validate(&latest.config).map_err(ApiError::Invalid)?;
    latest.reward_checkpoint.accumulated_policy_credit = 0;
    latest.pending_entitlement_batch = Some(batch.clone());
    latest.latest_entitlement_batch_generation = generation;
    state::write(latest.clone());
    submit_maturity(latest, batch).await
}

async fn submit_maturity(
    snapshot: state::StreamStateV1,
    batch: PendingEntitlementBatch,
) -> Result<RewardBackingProgress, ApiError> {
    reward_nns::prepare_maturity(
        snapshot.config.nns_manager,
        batch.generation,
        batch.target_icp_e8s,
    )
    .await
    .map_err(nns_call_error)?;
    if state::read() != snapshot {
        return Err(ApiError::Busy);
    }
    Ok(RewardBackingProgress::MaturityPrepared {
        generation: batch.generation,
    })
}

fn nns_call_error(error: CallError) -> ApiError {
    match error {
        CallError::Pending(message) | CallError::Waiting(message) => ApiError::Pending(message),
        CallError::Paused => ApiError::Paused,
        CallError::Invalid(message) => ApiError::Invalid(message),
    }
}

fn reward_event_id(event: io_sns_reward_boundary::EventId) -> RewardEventId {
    RewardEventId {
        end_timestamp_seconds: event.end_timestamp_seconds,
        round: event.round,
    }
}
