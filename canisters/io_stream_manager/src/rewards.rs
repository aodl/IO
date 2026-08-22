use candid::CandidType;
use serde::Deserialize;

use crate::{
    api::ApiError,
    canonical,
    reward_evidence::{
        classify_sequence, event_credits_for, event_id, installed_governance, latest_reward_event,
        list_all_neurons, merge_event_credits, require_consistent_event,
    },
    state::{
        self, Lifecycle, PendingEntitlementBatch, RewardEventClassification, RewardEventId,
        RewardEventObservation, SkippedRewardEvent,
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

fn ensure_reward_observation_due(
    lifecycle: Lifecycle,
    processing_paused: bool,
    work_due: bool,
) -> Result<(), ApiError> {
    if lifecycle != Lifecycle::Ready {
        return Err(ApiError::Paused);
    }
    if processing_paused {
        return Err(ApiError::Invalid(
            "reward-event processing is paused pending reviewed Governance readiness".into(),
        ));
    }
    work_due
        .then_some(())
        .ok_or_else(|| ApiError::Pending("reward work is not due".into()))
}

pub async fn observe(now_nanos: u64) -> Result<RewardEventObservation, ApiError> {
    let mut snapshot = state::read();
    ensure_reward_observation_due(
        snapshot.lifecycle,
        snapshot.reward_entitlements.reward_processing_paused,
        snapshot.reward_entitlements.reward_work_due,
    )?;
    snapshot.reward_entitlements.reward_work_due = false;
    state::write(snapshot.clone());
    let result = observe_due(&snapshot, now_nanos).await;
    if let Err(error) = &result {
        handle_observation_error(&snapshot, error);
    }
    result
}

async fn observe_due(
    snapshot: &crate::state::StreamStateV1,
    now_nanos: u64,
) -> Result<RewardEventObservation, ApiError> {
    verify_governance(snapshot).await?;
    let before = latest_reward_event(snapshot.config.sns_governance).await?;
    let sequence = classify_sequence(snapshot.reward_entitlements.last_processed_event, &before)?;
    if matches!(sequence, io_sns_reward_boundary::EventSequence::Same) {
        return Err(ApiError::Pending(
            "SNS reward event has not advanced".into(),
        ));
    }
    let neurons = list_all_neurons(snapshot.config.sns_governance).await?;
    let after = latest_reward_event(snapshot.config.sns_governance).await?;
    require_consistent_event(&before, &after)?;
    verify_governance(snapshot).await?;
    let event = event_id(&before)?;
    let canonical = canonical::redemption_snapshot(&snapshot.config)
        .await
        .map_err(ApiError::Ledger)?;
    let eligible_ids =
        crate::backing_registry::reward_eligible_ids(&snapshot.backing_registry, event.round);
    let proposal_count = before.settled_proposal_count().map_err(|error| {
        ApiError::Invalid(format!("SNS reward event proposal count failed: {error:?}"))
    })?;
    let (classification, credits, skipped) = match sequence {
        io_sns_reward_boundary::EventSequence::First
        | io_sns_reward_boundary::EventSequence::Next => {
            let (classification, credits) = event_credits_for(
                snapshot.config.sns_governance,
                &snapshot.config.nonredeemable_governance_io_accounts,
                &before,
                &neurons,
                Some(&eligible_ids),
            )?;
            (classification, credits, None)
        }
        io_sns_reward_boundary::EventSequence::Skipped {
            previous,
            ambiguous_event_count,
            rounds_since_last_distribution,
            ..
        } => (
            RewardEventClassification::MissedSkipped,
            Vec::new(),
            Some(SkippedRewardEvent {
                previous_event: previous.map(reward_event_id),
                observed_event: event,
                ambiguous_event_count,
                rounds_since_last_distribution,
                observed_at_nanos: now_nanos,
            }),
        ),
        io_sns_reward_boundary::EventSequence::Same => unreachable!(),
    };
    let policy_credit = if skipped.is_some() {
        0
    } else {
        io_reward_policy::DAILY_EVENT_CREDIT
    };
    let eligible_credit_total = credits
        .iter()
        .try_fold(0u128, |sum, credit| sum.checked_add(credit.event_credit))
        .ok_or_else(|| ApiError::Invalid("event eligible-credit total overflow".into()))?;
    let observation = RewardEventObservation {
        event,
        proposal_count,
        classification,
        credits,
        policy_credit,
        eligible_credit_total,
        observed_at_nanos: now_nanos,
    };
    commit_observation(snapshot, observation.clone(), skipped, canonical)?;
    crate::reward_timer::install_after(event);
    Ok(observation)
}

fn handle_observation_error(expected: &crate::state::StreamStateV1, error: &ApiError) {
    match error {
        ApiError::Pending(_) | ApiError::Ledger(_) | ApiError::Busy => {
            crate::reward_timer::install_retry();
        }
        ApiError::Invalid(_)
        | ApiError::Stuck(_)
        | ApiError::Paused
        | ApiError::Anonymous
        | ApiError::Unauthorized
        | ApiError::WrongNonce { .. }
        | ApiError::NonceAlreadyUsed => {
            pause_reward_processing(expected);
            if !state::read().reward_entitlements.reward_processing_paused {
                // A concurrent state change made the stale snapshot unsafe to
                // pause. Re-observe through the existing timer instead.
                crate::reward_timer::install_retry();
            }
        }
        ApiError::LiquidityShortfall { .. } => crate::reward_timer::install_retry(),
    }
}

async fn verify_governance(snapshot: &crate::state::StreamStateV1) -> Result<(), ApiError> {
    let installed = match installed_governance(
        snapshot.config.sns_root,
        snapshot.config.sns_governance,
    )
    .await
    {
        Ok(installed) => installed,
        Err(error @ ApiError::Invalid(_)) => {
            pause_reward_processing(snapshot);
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    if let Err(error) =
        crate::lifecycle::validate_installed_governance(&snapshot.config, &installed)
    {
        pause_reward_processing(snapshot);
        return Err(error);
    }
    Ok(())
}

fn reward_event_id(event: io_sns_reward_boundary::EventId) -> RewardEventId {
    RewardEventId {
        end_timestamp_seconds: event.end_timestamp_seconds,
        round: event.round,
    }
}

fn pause_reward_processing(expected: &crate::state::StreamStateV1) {
    let mut latest = state::read();
    if latest.config == expected.config
        && latest.control_epoch == expected.control_epoch
        && latest.reward_entitlements.last_processed_event
            == expected.reward_entitlements.last_processed_event
        && !latest.reward_entitlements.reward_work_due
    {
        latest.reward_entitlements.reward_processing_paused = true;
        latest.reward_entitlements.reward_work_due = true;
        latest.reward_entitlements.governance_parameters_fresh = false;
        state::write(latest);
        crate::reward_timer::install(None);
    }
}

fn commit_observation(
    expected: &crate::state::StreamStateV1,
    observation: RewardEventObservation,
    skipped: Option<SkippedRewardEvent>,
    canonical: crate::redemption::CanonicalRedemptionSnapshot,
) -> Result<(), ApiError> {
    let observed_at_nanos = observation.observed_at_nanos;
    let mut latest = state::read();
    if latest.config != expected.config
        || latest.control_epoch != expected.control_epoch
        || latest.lifecycle != Lifecycle::Ready
        || latest.reward_entitlements.reward_processing_paused
        || latest.reward_entitlements.last_processed_event
            != expected.reward_entitlements.last_processed_event
        || latest.reward_entitlements.reward_work_due
    {
        return Err(ApiError::Busy);
    }
    if let Some(skipped) = skipped {
        latest.reward_entitlements.missed_event_count = latest
            .reward_entitlements
            .missed_event_count
            .checked_add(skipped.ambiguous_event_count)
            .ok_or_else(|| ApiError::Invalid("missed reward-event count overflow".into()))?;
        latest.reward_entitlements.latest_skipped_event = Some(skipped);
    } else {
        latest.reward_entitlements.entries =
            merge_event_credits(&latest.reward_entitlements.entries, &observation.credits)?;
        latest.reward_entitlements.accumulated_policy_credit = latest
            .reward_entitlements
            .accumulated_policy_credit
            .checked_add(io_reward_policy::DAILY_EVENT_CREDIT)
            .ok_or_else(|| ApiError::Invalid("accumulated policy credit overflow".into()))?;
        latest.reward_entitlements.processed_event_count = latest
            .reward_entitlements
            .processed_event_count
            .checked_add(1)
            .ok_or_else(|| ApiError::Invalid("processed reward-event count overflow".into()))?;
    }
    let event_marker = observation.event.round;
    latest.reward_entitlements.last_processed_event = Some(observation.event);
    latest.reward_entitlements.latest_observation = Some(observation);
    latest.reward_entitlements.reward_work_due = false;
    latest.reward_entitlements.governance_parameters_fresh = true;
    latest.backing_registry = crate::backing_registry::reconcile(
        &expected.backing_registry,
        &canonical,
        event_marker,
        &expected.config,
    )
    .map_err(ApiError::Invalid)?;
    let excluded = canonical
        .excluded_io_balances
        .iter()
        .try_fold(0u128, |total, (_, balance)| total.checked_add(*balance))
        .ok_or_else(|| ApiError::Invalid("nonredeemable balance overflow".into()))?;
    let claims = io_core_model::claim_supply(
        canonical.total_supply_e8s,
        canonical.reserve_io_e8s,
        &[excluded],
    )
    .map_err(|error| ApiError::Invalid(format!("claim supply failed: {error:?}")))?;
    let target = io_core_model::target(
        canonical.active_backing_io_e8s,
        canonical.total_claim_backing_e8s,
        claims,
    )
    .map_err(|error| ApiError::Invalid(format!("pooled target failed: {error:?}")))?;
    let generation = latest
        .latest_reconciliation_generation
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("reconciliation generation overflow".into()))?;
    latest.latest_reconciliation_generation = generation;
    latest.latest_reconciliation_checkpoint = Some(crate::state::ReconciliationCheckpoint {
        generation,
        event_marker,
        observed_at_nanos,
        claim_supply_e8s: claims,
        liquid_backing_e8s: canonical.liquid_icp_e8s,
        pooled_backing_e8s: canonical.pooled_principal_e8s,
        unwinding_backing_e8s: canonical.unwinding_principal_e8s,
        transit_backing_e8s: canonical.transit_backing_e8s,
        total_claim_backing_e8s: canonical.total_claim_backing_e8s,
        active_backing_io_e8s: canonical.active_backing_io_e8s,
        active_reward_io_e8s: canonical.active_reward_io_e8s,
        live_cohort_count: u32::try_from(canonical.live_cohort_generations.len())
            .map_err(|_| ApiError::Invalid("live cohort count overflow".into()))?,
        oldest_ready_at_seconds: canonical.oldest_ready_at_seconds,
        pooled_target_e8s: target,
        observed_pooled_e8s: canonical.pooled_principal_e8s,
        snapshot_fingerprint: canonical.observation_fingerprint,
    });
    latest
        .reward_entitlements
        .validate(&latest.config)
        .map_err(ApiError::Invalid)?;
    state::write(latest);
    Ok(())
}

pub async fn resume_backing(now_nanos: u64) -> Result<RewardBackingProgress, ApiError> {
    let snapshot = state::read();
    if snapshot.lifecycle != Lifecycle::Ready {
        return Err(ApiError::Paused);
    }
    if snapshot.pending_entitlement_batch.is_none()
        && (snapshot.reward_entitlements.reward_processing_paused
            || !snapshot.reward_entitlements.governance_parameters_fresh)
    {
        return Err(ApiError::Invalid(
            "reward backing cannot freeze without fresh Governance readiness".into(),
        ));
    }
    if snapshot.pending_entitlement_batch.is_none()
        && snapshot.reward_entitlements.accumulated_policy_credit == 0
    {
        return Err(ApiError::Pending(
            "no new entitlement event is available to freeze".into(),
        ));
    }
    match snapshot.pending_entitlement_batch.clone() {
        Some(batch) => submit_maturity(snapshot, batch).await,
        None => freeze_and_prepare(snapshot, now_nanos).await,
    }
}

async fn freeze_and_prepare(
    snapshot: crate::state::StreamStateV1,
    now_nanos: u64,
) -> Result<RewardBackingProgress, ApiError> {
    if !crate::pool_reconciliation::ensure_latest(now_nanos).await? {
        return Ok(RewardBackingProgress::Pending {
            reason: reward_nns::BackingNotReadyReason::ReconciliationPending,
        });
    }
    let through_event = snapshot
        .reward_entitlements
        .last_processed_event
        .ok_or_else(|| ApiError::Invalid("entitlement checkpoint is missing".into()))?;
    verify_governance(&snapshot).await?;
    let canonical = canonical::redemption_snapshot(&snapshot.config)
        .await
        .map_err(ApiError::Ledger)?;
    let excluded = canonical
        .excluded_io_balances
        .iter()
        .try_fold(0u128, |sum, (_, value)| sum.checked_add(*value))
        .ok_or_else(|| ApiError::Invalid("excluded IO balance overflow".into()))?;
    let redeemable = canonical
        .total_supply_e8s
        .checked_sub(canonical.reserve_io_e8s)
        .and_then(|value| value.checked_sub(excluded))
        .ok_or_else(|| ApiError::Invalid("invalid redeemable IO supply".into()))?;
    let target = io_core_model::target(
        canonical.active_backing_io_e8s,
        canonical.total_claim_backing_e8s,
        redeemable,
    )
    .map_err(|error| ApiError::Invalid(format!("two-week target failed: {error:?}")))?;
    verify_governance(&snapshot).await?;
    let generation = snapshot
        .latest_entitlement_batch_generation
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("entitlement batch generation exhausted".into()))?;
    let eligible_credit_total = snapshot
        .reward_entitlements
        .entries
        .iter()
        .try_fold(0u128, |sum, entry| {
            sum.checked_add(entry.accumulated_eligible_credit)
        })
        .ok_or_else(|| ApiError::Invalid("pending entitlement total overflow".into()))?;
    let batch = PendingEntitlementBatch {
        generation,
        frozen_at_timestamp_seconds: now_nanos / 1_000_000_000,
        through_event,
        target_icp_e8s: target,
        entries: snapshot.reward_entitlements.entries.clone(),
        eligible_credit_total,
        policy_credit_total: snapshot.reward_entitlements.accumulated_policy_credit,
        processed_event_count: snapshot.reward_entitlements.processed_event_count,
    };
    batch
        .validate(&snapshot.config)
        .map_err(ApiError::Invalid)?;
    let mut latest = state::read();
    if latest.config != snapshot.config
        || latest.control_epoch != snapshot.control_epoch
        || latest.lifecycle != Lifecycle::Ready
        || latest.pending_entitlement_batch.is_some()
        || latest.reward_entitlements != snapshot.reward_entitlements
    {
        return Err(ApiError::Busy);
    }
    latest.reward_entitlements.entries.clear();
    latest.reward_entitlements.accumulated_policy_credit = 0;
    latest.pending_entitlement_batch = Some(batch.clone());
    latest.latest_entitlement_batch_generation = generation;
    state::write(latest.clone());
    submit_maturity(latest, batch).await
}

async fn submit_maturity(
    snapshot: crate::state::StreamStateV1,
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

#[cfg(test)]
fn bounded_refresh_diagnostic(mut diagnostic: String) -> String {
    while diagnostic.len() > 256 {
        diagnostic.pop();
    }
    diagnostic
}

#[cfg(test)]
mod composition_tests {
    use super::*;

    #[test]
    fn premature_reward_observation_is_rejected_by_the_local_gate() {
        assert_eq!(
            ensure_reward_observation_due(Lifecycle::Ready, false, false),
            Err(ApiError::Pending("reward work is not due".into()))
        );
        assert_eq!(
            ensure_reward_observation_due(Lifecycle::Paused, false, true),
            Err(ApiError::Paused)
        );
        assert!(ensure_reward_observation_due(Lifecycle::Ready, false, true).is_ok());
    }

    #[test]
    fn refresh_diagnostics_are_bounded_without_splitting_utf8() {
        let diagnostic = bounded_refresh_diagnostic("é".repeat(200));
        assert!(diagnostic.len() <= 256);
        assert!(diagnostic.is_char_boundary(diagnostic.len()));
    }
}
