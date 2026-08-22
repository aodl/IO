use candid::CandidType;
use serde::Deserialize;

use crate::{
    api::ApiError,
    canonical,
    receipt::{
        CompletedReceiptResult, LastCompletedReceipt, LiquidReceiptOperation, ReceiptPhase,
        RewardRecipient, TwoWeekReceiptOperation, TwoWeekReceiptResult, TwoWeekSettlement,
    },
    reward_evidence::{
        classify_sequence, event_credits_for, event_id, installed_governance, latest_reward_event,
        list_all_neurons, merge_event_credits, require_consistent_event,
    },
    state::{
        self, Account, Lifecycle, LiquidReceiptStreamOperation, PendingEntitlementBatch,
        RewardEventClassification, RewardEventId, RewardEventObservation, SkippedRewardEvent,
        StreamOperation,
    },
    transfer::{
        classify_result, deterministic_memo, ClassifiedResult, OwnTransferIntent, TransferAttempt,
        TransferState,
    },
};
use io_nns_types::reward_boundary::{self as reward_nns, CallError};
use io_sns_reward_boundary::claim_or_refresh;

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
    let readiness = reward_nns::reconcile_readiness(snapshot.config.nns_manager, target)
        .await
        .map_err(nns_call_error)?;
    match readiness {
        io_receipt_types::TwoWeekBackingReadiness::NotReady(reason) => {
            return Ok(RewardBackingProgress::Pending { reason });
        }
        io_receipt_types::TwoWeekBackingReadiness::Ready { .. } => {}
    }
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
    if let io_receipt_types::TwoWeekBackingReadiness::NotReady(reason) =
        reward_nns::reconcile_readiness(snapshot.config.nns_manager, batch.target_icp_e8s)
            .await
            .map_err(nns_call_error)?
    {
        return Ok(RewardBackingProgress::Pending { reason });
    }
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

pub(crate) async fn resume_two_week(
    operation: TwoWeekReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    match operation.phase {
        ReceiptPhase::AwaitingReceipt => Ok(crate::api::LiquidReceiptProgress::AwaitingReceipt),
        ReceiptPhase::ReceiptProved => prepare_settlement(operation).await,
        ReceiptPhase::Settling => resume_recipient(operation, now).await,
        ReceiptPhase::Stuck => Err(ApiError::Stuck(
            "two-week recipient transfer requires exact proof or reviewed recovery".into(),
        )),
        ReceiptPhase::Completed => Err(ApiError::Invalid(
            "completed two-week receipt must be available through replay".into(),
        )),
    }
}

async fn prepare_settlement(
    operation: TwoWeekReceiptOperation,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let snapshot = state::read();
    let generation = operation
        .context
        .request
        .entitlement_batch_generation
        .ok_or_else(|| ApiError::Invalid("two-week receipt lacks entitlement batch".into()))?;
    let batch = snapshot
        .pending_entitlement_batch
        .as_ref()
        .filter(|batch| batch.generation == generation)
        .ok_or_else(|| {
            ApiError::Invalid("two-week receipt lost pending entitlement batch".into())
        })?;
    let canonical = canonical::redemption_snapshot(&snapshot.config)
        .await
        .map_err(ApiError::Ledger)?;
    crate::receipt_preparation::validate_post_receipt_snapshot(
        &operation.context.backing_snapshot,
        &canonical,
        operation.context.request.liquid_amount_e8s,
        0,
    )?;
    let excluded = operation
        .context
        .backing_snapshot
        .excluded_io_balances
        .iter()
        .try_fold(0u128, |sum, (_, value)| sum.checked_add(*value))
        .ok_or_else(|| ApiError::Invalid("excluded IO overflow".into()))?;
    let redeemable = operation
        .context
        .backing_snapshot
        .total_io_supply_e8s
        .checked_sub(operation.context.backing_snapshot.reserve_io_e8s)
        .and_then(|value| value.checked_sub(excluded))
        .ok_or_else(|| ApiError::Invalid("invalid redeemable IO supply".into()))?;
    let pool = io_core_model::backed_io(
        operation.context.request.liquid_amount_e8s,
        operation.context.backing_snapshot.liquid_icp_e8s,
        redeemable,
    )
    .map_err(|error| ApiError::Invalid(format!("reward backing failed: {error:?}")))?;
    let participants = batch
        .entries
        .iter()
        .map(|entry| {
            io_reward_policy::entitlement_credit_from_bytes(
                entry.sns_neuron_id.clone(),
                entry.accumulated_eligible_credit,
            )
        })
        .collect::<Vec<_>>();
    let allocation =
        io_reward_policy::allocate_rewards(pool, batch.policy_credit_total, &participants)
            .map_err(|error| ApiError::Invalid(format!("reward allocation failed: {error:?}")))?;
    let recipients = allocation
        .allocations
        .iter()
        .map(|allocation| {
            let id = allocation.sns_neuron_id.clone();
            let entry = batch
                .entries
                .iter()
                .find(|entry| entry.sns_neuron_id == id)
                .ok_or_else(|| ApiError::Invalid("allocation lacks entitlement entry".into()))?;
            Ok(RewardRecipient {
                sns_neuron_id: id,
                destination: entry.destination.clone(),
                io_e8s: allocation.io_e8s,
                transfer: None,
                refresh_attempted: false,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let fees = operation
        .context
        .backing_snapshot
        .io_fee_e8s
        .checked_mul(recipients.len() as u128)
        .ok_or_else(|| ApiError::Invalid("reward fee total overflow".into()))?;
    let issued = recipients
        .iter()
        .try_fold(0u128, |sum, recipient| sum.checked_add(recipient.io_e8s))
        .ok_or_else(|| ApiError::Invalid("reward issue total overflow".into()))?;
    let required_reserve = issued
        .checked_add(fees)
        .ok_or_else(|| ApiError::Invalid("reward reserve requirement overflow".into()))?;
    if canonical.reserve_io_e8s < required_reserve {
        return Err(ApiError::Invalid(
            "IO reserve does not cover rewards plus one fee per recipient".into(),
        ));
    }
    if state::read() != snapshot {
        return Err(ApiError::Busy);
    }
    let mut replacement = operation.clone();
    replacement.phase = ReceiptPhase::Settling;
    replacement.settlement = Some(TwoWeekSettlement {
        backed_io_pool_e8s: pool,
        recipients,
        recipient_index: 0,
        distributed_io_e8s: 0,
        forfeited_io_e8s: allocation.forfeited_io_e8s,
        rounding_dust_io_e8s: allocation.rounding_dust_e8s,
    });
    crate::receipt::persist_exact(
        &LiquidReceiptOperation::TwoWeek(Box::new(operation)),
        LiquidReceiptOperation::TwoWeek(Box::new(replacement)),
    )?;
    Ok(crate::api::LiquidReceiptProgress::Settling)
}

async fn resume_recipient(
    operation: TwoWeekReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let settlement = operation
        .settlement
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("two-week settlement is missing".into()))?;
    let index = settlement.recipient_index as usize;
    if index == settlement.recipients.len() {
        return complete_settlement(operation, now);
    }
    let recipient = &settlement.recipients[index];
    match &recipient.transfer {
        None
        | Some(TransferAttempt {
            state: TransferState::Prepared | TransferState::Submitted { .. },
            ..
        }) => submit_recipient(operation, now).await,
        Some(TransferAttempt {
            state: TransferState::Succeeded { .. },
            ..
        }) if !recipient.refresh_attempted => refresh_recipient(operation).await,
        Some(TransferAttempt {
            state: TransferState::Succeeded { .. },
            ..
        }) => advance_recipient(operation),
        Some(TransferAttempt {
            state: TransferState::Stuck { reason },
            ..
        }) => Ok(crate::api::LiquidReceiptProgress::Stuck(reason.clone())),
    }
}

async fn submit_recipient(
    operation: TwoWeekReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let config = state::read().config;
    let settlement = operation.settlement.as_ref().expect("validated settlement");
    let index = settlement.recipient_index as usize;
    let recipient = settlement.recipients[index].clone();
    let mut attempt = match &recipient.transfer {
        None => TransferAttempt::prepared(OwnTransferIntent::Icrc1 {
            ledger: config.io_ledger,
            from_subaccount: config
                .io_reserve
                .canonical()
                .map_err(ApiError::Invalid)?
                .subaccount,
            to: recipient.destination.clone(),
            amount: recipient.io_e8s,
            fee: config.expected_io_fee_e8s,
            memo: deterministic_memo(
                b"io-two-week-reward-v1",
                ic_cdk::api::canister_self(),
                (operation.context.request.receipt_sequence << 32) | index as u64,
            ),
            created_at_time: now,
        })
        .map_err(ApiError::Invalid)?,
        Some(attempt) => attempt.clone(),
    };
    let (epoch, first_submitted_at) =
        match attempt.state {
            TransferState::Prepared => (crate::state::DispatchEpoch(1), now),
            TransferState::Submitted {
                epoch,
                first_submitted_at,
                last_submitted_at,
            } => {
                if now
                    .checked_sub(last_submitted_at)
                    .ok_or_else(|| ApiError::Invalid("reward retry clock regressed".into()))?
                    < config.retry_delay_nanos
                {
                    return Ok(crate::api::LiquidReceiptProgress::Settling);
                }
                let deadline = attempt
                    .intent
                    .created_at_time()
                    .checked_add(config.ledger_deduplication_window_nanos)
                    .ok_or_else(|| ApiError::Invalid("reward retry deadline overflow".into()))?;
                if now >= deadline {
                    return stick_recipient(operation, "reward transfer retry window expired");
                }
                (
                    crate::state::DispatchEpoch(epoch.0.checked_add(1).ok_or_else(|| {
                        ApiError::Invalid("reward dispatch epoch exhausted".into())
                    })?),
                    first_submitted_at,
                )
            }
            _ => return Err(ApiError::Busy),
        };
    attempt.state = TransferState::Submitted {
        epoch,
        first_submitted_at,
        last_submitted_at: now,
    };
    let intent = attempt.intent.clone();
    let mut submitted = operation.clone();
    submitted
        .settlement
        .as_mut()
        .expect("validated settlement")
        .recipients[index]
        .transfer = Some(attempt.clone());
    crate::receipt::persist_exact(
        &LiquidReceiptOperation::TwoWeek(Box::new(operation)),
        LiquidReceiptOperation::TwoWeek(Box::new(submitted.clone())),
    )?;
    let response = crate::api::submit(&intent).await;
    if active_two_week()? != submitted {
        return Err(ApiError::Busy);
    }
    match response {
        Err(error) => Err(ApiError::Pending(error)),
        Ok(result) => match classify_result(result).map_err(ApiError::Ledger)? {
            ClassifiedResult::Succeeded(block) => {
                let mut succeeded = submitted.clone();
                succeeded
                    .settlement
                    .as_mut()
                    .expect("validated settlement")
                    .recipients[index]
                    .transfer
                    .as_mut()
                    .expect("submitted transfer")
                    .state = TransferState::Succeeded { block };
                crate::receipt::persist_exact(
                    &LiquidReceiptOperation::TwoWeek(Box::new(submitted)),
                    LiquidReceiptOperation::TwoWeek(Box::new(succeeded)),
                )?;
                Ok(crate::api::LiquidReceiptProgress::Settling)
            }
            ClassifiedResult::NoEffect(error) => stick_recipient(submitted, &error),
            ClassifiedResult::Ambiguous(error) => Err(ApiError::Pending(error)),
        },
    }
}

fn stick_recipient(
    operation: TwoWeekReceiptOperation,
    reason: &str,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let mut stuck = operation.clone();
    stuck.phase = ReceiptPhase::Stuck;
    let settlement = stuck.settlement.as_mut().expect("validated settlement");
    let index = settlement.recipient_index as usize;
    let attempt = settlement.recipients[index]
        .transfer
        .as_mut()
        .ok_or_else(|| ApiError::Invalid("Stuck reward transfer is missing".into()))?;
    attempt.state = TransferState::Stuck {
        reason: reason.into(),
    };
    crate::receipt::persist_exact(
        &LiquidReceiptOperation::TwoWeek(Box::new(operation)),
        LiquidReceiptOperation::TwoWeek(Box::new(stuck)),
    )?;
    crate::api::pause();
    Err(ApiError::Stuck(reason.into()))
}

async fn refresh_recipient(
    operation: TwoWeekReceiptOperation,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let settlement = operation.settlement.as_ref().expect("validated settlement");
    let index = settlement.recipient_index as usize;
    let neuron_id = settlement.recipients[index].sns_neuron_id.clone();
    let mut submitted = operation.clone();
    submitted
        .settlement
        .as_mut()
        .expect("validated settlement")
        .recipients[index]
        .refresh_attempted = true;
    crate::receipt::persist_exact(
        &LiquidReceiptOperation::TwoWeek(Box::new(operation)),
        LiquidReceiptOperation::TwoWeek(Box::new(submitted.clone())),
    )?;
    let result = claim_or_refresh(state::read().config.sns_governance, neuron_id.clone()).await;
    if active_two_week()? != submitted {
        return Err(ApiError::Busy);
    }
    if let Err(error) = result {
        log_refresh_failure(&neuron_id, error);
    }
    advance_recipient(submitted)
}

fn log_refresh_failure(neuron_id: &[u8], error: io_sns_reward_boundary::ClaimOrRefreshError) {
    let (class, diagnostic) = match error {
        io_sns_reward_boundary::ClaimOrRefreshError::Governance(value) => ("governance", value),
        io_sns_reward_boundary::ClaimOrRefreshError::Transport(value) => ("transport", value),
        io_sns_reward_boundary::ClaimOrRefreshError::Malformed(value) => ("malformed", value),
    };
    let diagnostic = bounded_refresh_diagnostic(diagnostic);
    ic_cdk::api::debug_print(format!(
        "best-effort SNS neuron refresh failed: class={class} neuron_id={neuron_id:?} diagnostic={diagnostic}"
    ));
}

fn bounded_refresh_diagnostic(mut diagnostic: String) -> String {
    const MAX_BYTES: usize = 256;
    if diagnostic.len() > MAX_BYTES {
        let mut end = MAX_BYTES;
        while !diagnostic.is_char_boundary(end) {
            end -= 1;
        }
        diagnostic.truncate(end);
    }
    diagnostic
}

fn advance_recipient(
    operation: TwoWeekReceiptOperation,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let settlement = operation.settlement.as_ref().expect("validated settlement");
    let index = settlement.recipient_index as usize;
    let recipient = &settlement.recipients[index];
    if !recipient.refresh_attempted
        || !matches!(
            recipient.transfer.as_ref().map(|attempt| &attempt.state),
            Some(TransferState::Succeeded { .. })
        )
    {
        return Err(ApiError::Invalid(
            "reward recipient cannot advance before exact transfer and refresh attempt".into(),
        ));
    }
    let mut replacement = operation.clone();
    let settlement = replacement
        .settlement
        .as_mut()
        .expect("validated settlement");
    settlement.distributed_io_e8s = settlement
        .distributed_io_e8s
        .checked_add(settlement.recipients[index].io_e8s)
        .ok_or_else(|| ApiError::Invalid("distributed reward overflow".into()))?;
    settlement.recipient_index = settlement
        .recipient_index
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("reward recipient index overflow".into()))?;
    crate::receipt::persist_exact(
        &LiquidReceiptOperation::TwoWeek(Box::new(operation)),
        LiquidReceiptOperation::TwoWeek(Box::new(replacement)),
    )?;
    Ok(crate::api::LiquidReceiptProgress::Settling)
}

fn complete_settlement(
    operation: TwoWeekReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let settlement = operation.settlement.as_ref().expect("validated settlement");
    let receipt_block = operation
        .receipt_block
        .ok_or_else(|| ApiError::Invalid("two-week receipt block is missing".into()))?;
    if settlement
        .distributed_io_e8s
        .checked_add(settlement.forfeited_io_e8s)
        .and_then(|value| value.checked_add(settlement.rounding_dust_io_e8s))
        .ok_or_else(|| ApiError::Invalid("two-week settlement total overflow".into()))?
        != settlement.backed_io_pool_e8s
    {
        return Err(ApiError::Invalid(
            "two-week settlement does not reconcile".into(),
        ));
    }
    let result = CompletedReceiptResult::TwoWeek(TwoWeekReceiptResult {
        request_fingerprint: operation.context.request_fingerprint.clone(),
        receipt_block,
        backed_io_pool_e8s: settlement.backed_io_pool_e8s,
        distributed_io_e8s: settlement.distributed_io_e8s,
        forfeited_io_e8s: settlement.forfeited_io_e8s,
        rounding_dust_io_e8s: settlement.rounding_dust_io_e8s,
        completed_at_nanos: now,
    });
    let expected = LiquidReceiptOperation::TwoWeek(Box::new(operation.clone()));
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(StreamOperation::LiquidReceipt(active))
        if matches!(active.as_ref(), LiquidReceiptStreamOperation::Active(value) if **value == expected))
    {
        return Err(ApiError::Busy);
    }
    let generation = operation.context.request.entitlement_batch_generation;
    if latest
        .pending_entitlement_batch
        .as_ref()
        .map(|batch| batch.generation)
        != generation
    {
        return Err(ApiError::Busy);
    }
    latest.last_completed_receipt = Some(LastCompletedReceipt {
        request: operation.context.request,
        request_fingerprint: operation.context.request_fingerprint,
        permit: operation.context.permit,
        backing_snapshot: operation.context.backing_snapshot,
        receipt_block,
        result: result.clone(),
    });
    latest.next_nns_receipt_sequence = latest
        .next_nns_receipt_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("receipt sequence overflow".into()))?;
    latest.active_operation = None;
    latest.pending_entitlement_batch = None;
    state::write(latest);
    Ok(crate::api::LiquidReceiptProgress::Completed(result))
}

fn active_two_week() -> Result<TwoWeekReceiptOperation, ApiError> {
    match state::read().active_operation {
        Some(StreamOperation::LiquidReceipt(operation)) => match *operation {
            LiquidReceiptStreamOperation::Active(operation) => match *operation {
                LiquidReceiptOperation::TwoWeek(operation) => Ok(*operation),
                LiquidReceiptOperation::Jupiter(_) => Err(ApiError::Busy),
            },
            LiquidReceiptStreamOperation::Preparing(_) => Err(ApiError::Busy),
        },
        _ => Err(ApiError::Busy),
    }
}

pub(crate) async fn prove_recipient_transfer(block_index: u128) -> Result<(), ApiError> {
    let operation = active_two_week()?;
    if operation.phase != ReceiptPhase::Stuck {
        return Err(ApiError::Invalid("two-week receipt is not Stuck".into()));
    }
    let settlement = operation.settlement.as_ref().expect("validated settlement");
    let index = settlement.recipient_index as usize;
    let attempt = settlement.recipients[index]
        .transfer
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("reward transfer proof slot is empty".into()))?;
    let exact = canonical::exact_icrc_transfer(attempt.intent.ledger(), block_index)
        .await
        .map_err(ApiError::Ledger)?;
    let OwnTransferIntent::Icrc1 {
        from_subaccount,
        to,
        amount,
        fee,
        memo,
        created_at_time,
        ..
    } = &attempt.intent
    else {
        return Err(ApiError::Invalid(
            "reward intent has wrong transfer kind".into(),
        ));
    };
    let source = Account {
        owner: ic_cdk::api::canister_self(),
        subaccount: (*from_subaccount != [0; 32]).then(|| from_subaccount.to_vec()),
    };
    if !exact
        .matches(&io_ledger_boundary::ExpectedIcrcTransfer {
            from: &source,
            to,
            amount_e8s: *amount,
            fee_e8s: Some(*fee),
            memo: Some(memo),
            created_at_time: Some(*created_at_time),
            spender: None,
        })
        .map_err(ApiError::Invalid)?
    {
        return Err(ApiError::Invalid(
            "exact block does not match reward transfer".into(),
        ));
    }
    if active_two_week()? != operation {
        return Err(ApiError::Busy);
    }
    let mut replacement = operation.clone();
    replacement.phase = ReceiptPhase::Settling;
    replacement
        .settlement
        .as_mut()
        .expect("validated settlement")
        .recipients[index]
        .transfer
        .as_mut()
        .expect("reward transfer")
        .state = TransferState::Succeeded { block: block_index };
    crate::receipt::persist_exact(
        &LiquidReceiptOperation::TwoWeek(Box::new(operation)),
        LiquidReceiptOperation::TwoWeek(Box::new(replacement)),
    )
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
