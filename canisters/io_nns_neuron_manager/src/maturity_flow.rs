use candid::Principal;
use io_ledger_boundary::{exact_icp_transfer, icp_account_identifier, ExpectedQueryBlockTransfer};
use io_receipt_types::{
    ClaimBackingReceiptKind, ClaimBackingReceiptProgress, PrepareClaimBackingReceiptArgs,
    ProveClaimBackingReceiptArgs,
};

use crate::{
    api::{ApiError, MaturityProgress, PrepareTwoWeekMaturityArgs},
    execution,
    maturity::{
        ClaimReceiptDeliveryOperation, DisburseMaturitySubmission, DisburseMaturitySucceeded,
        MaturityCommandOperation, MaturityCommandPhase, MaturityEvidenceSource, MaturityKind,
        MaturityPlan, MintProofState, PendingMaturityDisbursement, StakeMaturitySucceeded,
        MINIMUM_DISBURSEMENT_E8S,
    },
    state::{self, Lifecycle, NnsOperation},
    transfer::{
        NnsTransferAttempt, NnsTransferIntent, TransferOutcomeClassification, TransferState,
    },
};

pub async fn start(caller: Principal, kind: MaturityKind) -> Result<MaturityProgress, ApiError> {
    let snapshot = crate::api::ready()?;
    if caller != snapshot.config.sns_governance {
        return Err(ApiError::Unauthorized);
    }
    if kind == MaturityKind::TwoWeek {
        return Err(ApiError::Invalid(
            "two-week maturity must be prepared by the stream manager for a frozen entitlement batch".into(),
        ));
    }
    if !snapshot.two_year_maturity_baseline_reconciled {
        return Err(ApiError::Pending(
            "two-year protected NNS neuron launch baseline is unreconciled".into(),
        ));
    }
    start_observed(snapshot, kind, None).await
}

pub(crate) async fn start_observed(
    snapshot: crate::state::NnsStateV1,
    kind: MaturityKind,
    entitlement_batch: Option<PrepareTwoWeekMaturityArgs>,
) -> Result<MaturityProgress, ApiError> {
    if snapshot.active_operation.is_some() || pending_from(&snapshot, kind).is_some() {
        return Err(ApiError::Busy);
    }
    let (neuron_id, destination) = identity(&snapshot.config, kind)?;
    let observation = execution::query_neuron_observation(&snapshot.config, neuron_id).await?;
    if state::read() != snapshot {
        return Err(ApiError::Busy);
    }
    let configuration = match kind {
        MaturityKind::TwoYear => execution::validate_permanent_configuration(&observation),
        MaturityKind::TwoWeek => execution::validate_parent_configuration(
            &observation,
            io_nns_types::backing::FollowPolicy {
                followee_neuron_id: snapshot.config.pooled_parent_followee_id,
            },
        ),
    };
    if let Err(reason) = configuration {
        let mut latest = snapshot;
        latest.lifecycle = Lifecycle::Paused;
        state::write(latest);
        return Err(ApiError::Invalid(reason));
    }
    if let Some(batch) = &entitlement_batch {
        let tolerance = snapshot
            .config
            .expected_icp_fee_e8s
            .checked_mul(2)
            .ok_or_else(|| ApiError::Invalid("unwind tolerance overflow".into()))?;
        if !matches!(
            state::target_status(
                observation.snapshot.cached_stake_e8s,
                batch.target_e8s,
                tolerance,
            ),
            state::PooledTargetStatus::AtTarget
                | state::PooledTargetStatus::AtTargetWithinUnwindTolerance
        ) {
            return Err(ApiError::Pending(
                "protected two-week principal moved away from the reconciled target".into(),
            ));
        }
    }
    let (stake_maturity_e8s, remaining_maturity_e8s) = match kind {
        MaturityKind::TwoYear => crate::maturity::split_maturity(observation.maturity_e8s)
            .ok_or_else(|| ApiError::Invalid("maturity split overflow".into()))?,
        MaturityKind::TwoWeek => (0, observation.maturity_e8s),
    };
    if remaining_maturity_e8s < MINIMUM_DISBURSEMENT_E8S {
        return Err(ApiError::BelowMaturityThreshold {
            remaining_e8s: remaining_maturity_e8s,
            minimum_e8s: MINIMUM_DISBURSEMENT_E8S,
        });
    }
    let entitlement_batch_generation = entitlement_batch
        .as_ref()
        .map(|value| value.entitlement_batch_generation);

    let mut latest = state::read();
    if latest != snapshot
        || latest.lifecycle != Lifecycle::Ready
        || (kind == MaturityKind::TwoWeek
            && latest.latest_started_two_week_generation.checked_add(1)
                != entitlement_batch_generation)
    {
        return Err(ApiError::Busy);
    }
    let operation_sequence = latest.next_operation_sequence;
    latest.next_operation_sequence = operation_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("operation sequence exhausted".into()))?;
    let operation = MaturityCommandOperation {
        operation_sequence,
        dispatch_epoch: 0,
        kind,
        phase: MaturityCommandPhase::Observed(MaturityPlan {
            neuron: observation.snapshot,
            original_maturity_e8s: observation.maturity_e8s,
            original_staked_maturity_e8s: observation.staked_maturity_e8s,
            stake_maturity_e8s,
            remaining_maturity_e8s,
            destination,
            requested_at_seconds: now_seconds()?,
            entitlement_batch_generation,
        }),
    };
    operation
        .validate(
            latest.next_operation_sequence,
            neuron_id,
            &operation.plan().destination,
        )
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(NnsOperation::Maturity(Box::new(operation)));
    if let Some(generation) = entitlement_batch_generation {
        latest.latest_started_two_week_generation = generation;
    }
    state::write(latest);
    Ok(MaturityProgress::Observed)
}

pub async fn resume_active(
    operation: MaturityCommandOperation,
) -> Result<MaturityProgress, ApiError> {
    match operation.phase.clone() {
        MaturityCommandPhase::Observed(plan) if operation.kind == MaturityKind::TwoWeek => {
            prove_unstaked_maturity(operation, plan).await
        }
        MaturityCommandPhase::Observed(plan) => submit_stake(operation, plan).await,
        MaturityCommandPhase::StakeMaturitySubmitted(plan) => recover_stake(operation, plan).await,
        MaturityCommandPhase::StakeMaturitySucceeded(stake) => {
            check_pre_disburse_drift(operation, stake).await
        }
        MaturityCommandPhase::ReadyToDisburse(submission) => {
            submit_disburse(operation, submission).await
        }
        MaturityCommandPhase::DisburseMaturitySubmitted(submission) => {
            recover_disburse(operation, submission).await
        }
        MaturityCommandPhase::DisburseMaturitySucceeded(disbursement) => {
            canonicalize_disbursement(operation, disbursement).await
        }
        MaturityCommandPhase::ClaimReceiptDelivery(delivery) => {
            resume_claim_receipt_delivery(operation, delivery).await
        }
        MaturityCommandPhase::MaturityDrift { reason, .. }
        | MaturityCommandPhase::DisburseMaturityMismatch { reason, .. } => {
            Ok(MaturityProgress::Stuck(reason))
        }
    }
}

pub async fn resume_kind(kind: MaturityKind) -> Result<MaturityProgress, ApiError> {
    let snapshot = state::read();
    match snapshot.active_operation {
        Some(NnsOperation::Maturity(operation)) if operation.kind == kind => {
            return resume_active(*operation).await
        }
        Some(_) => return Err(ApiError::Busy),
        None => {}
    }
    let pending = pending_from(&snapshot, kind)
        .ok_or_else(|| ApiError::Invalid("no maturity work exists for this kind".into()))?;
    match &pending.mint_proof {
        MintProofState::Awaiting => Ok(MaturityProgress::AwaitingMintProof),
        MintProofState::Proved(_) => start_claim_receipt_delivery(pending),
        MintProofState::Delivering(_) => Ok(MaturityProgress::DeliveringClaimReceipt),
    }
}

async fn submit_stake(
    mut operation: MaturityCommandOperation,
    plan: MaturityPlan,
) -> Result<MaturityProgress, ApiError> {
    let expected = operation.clone();
    operation.dispatch_epoch = next_epoch(operation.dispatch_epoch)?;
    operation.phase = MaturityCommandPhase::StakeMaturitySubmitted(plan.clone());
    write_exact(&expected, operation.clone(), false)?;
    let submitted = operation.clone();
    let result = execution::stake_maturity(&state::read().config, plan.neuron.neuron_id).await;
    ensure_exact(&submitted)?;
    let (remaining_maturity_e8s, staked_maturity_e8s) = match result {
        Ok(value) => value,
        Err(error) => {
            write_exact(&submitted, submitted.clone(), true)?;
            return Err(ApiError::Pending(format!(
                "StakeMaturity outcome requires canonical observation: {error:?}"
            )));
        }
    };
    apply_stake_result(
        submitted,
        plan,
        remaining_maturity_e8s,
        staked_maturity_e8s,
        MaturityEvidenceSource::CommandResponse,
    )
}

async fn prove_unstaked_maturity(
    operation: MaturityCommandOperation,
    plan: MaturityPlan,
) -> Result<MaturityProgress, ApiError> {
    let observation =
        execution::query_neuron_observation(&state::read().config, plan.neuron.neuron_id).await?;
    ensure_exact(&operation)?;
    if observation.maturity_e8s != plan.original_maturity_e8s
        || observation.staked_maturity_e8s != plan.original_staked_maturity_e8s
        || plan.stake_maturity_e8s != 0
        || plan.remaining_maturity_e8s != plan.original_maturity_e8s
    {
        return Err(ApiError::Pending(
            "pooled-parent maturity changed before its no-stake disbursement proof".into(),
        ));
    }
    let mut replacement = operation.clone();
    replacement.phase = MaturityCommandPhase::StakeMaturitySucceeded(StakeMaturitySucceeded {
        plan,
        remaining_maturity_e8s: observation.maturity_e8s,
        staked_maturity_e8s: observation.staked_maturity_e8s,
        evidence_source: MaturityEvidenceSource::CanonicalNeuronObservation,
    });
    write_exact(&operation, replacement, false)?;
    Ok(MaturityProgress::StakeMaturitySucceeded)
}

async fn recover_stake(
    operation: MaturityCommandOperation,
    plan: MaturityPlan,
) -> Result<MaturityProgress, ApiError> {
    let observation =
        execution::query_neuron_observation(&state::read().config, plan.neuron.neuron_id).await?;
    ensure_exact(&operation)?;
    let expected_staked = expected_staked(&plan)?;
    if observation.maturity_e8s == plan.remaining_maturity_e8s
        && observation.staked_maturity_e8s == expected_staked
    {
        return apply_stake_result(
            operation,
            plan.clone(),
            observation.maturity_e8s,
            observation.staked_maturity_e8s,
            MaturityEvidenceSource::CanonicalNeuronObservation,
        );
    }
    write_exact(&operation, operation.clone(), true)?;
    Err(ApiError::Pending(format!(
        "StakeMaturity remains ambiguous: expected ordinary {} and staked {}, observed ordinary {} and staked {}",
        plan.remaining_maturity_e8s,
        expected_staked,
        observation.maturity_e8s,
        observation.staked_maturity_e8s
    )))
}

fn apply_stake_result(
    operation: MaturityCommandOperation,
    plan: MaturityPlan,
    remaining_maturity_e8s: u64,
    staked_maturity_e8s: u64,
    evidence_source: MaturityEvidenceSource,
) -> Result<MaturityProgress, ApiError> {
    let expected_staked = expected_staked(&plan)?;
    if remaining_maturity_e8s != plan.remaining_maturity_e8s
        || staked_maturity_e8s != expected_staked
    {
        write_exact(&operation, operation.clone(), true)?;
        return Err(ApiError::Stuck(format!(
            "StakeMaturity response drift: expected ordinary {} and staked {}, observed ordinary {} and staked {}",
            plan.remaining_maturity_e8s,
            expected_staked,
            remaining_maturity_e8s,
            staked_maturity_e8s
        )));
    }
    let mut replacement = operation.clone();
    replacement.phase = MaturityCommandPhase::StakeMaturitySucceeded(StakeMaturitySucceeded {
        plan,
        remaining_maturity_e8s,
        staked_maturity_e8s,
        evidence_source,
    });
    write_exact(&operation, replacement, false)?;
    Ok(MaturityProgress::StakeMaturitySucceeded)
}

async fn check_pre_disburse_drift(
    operation: MaturityCommandOperation,
    stake: StakeMaturitySucceeded,
) -> Result<MaturityProgress, ApiError> {
    let observation =
        execution::query_neuron_observation(&state::read().config, stake.plan.neuron.neuron_id)
            .await?;
    ensure_exact(&operation)?;
    if observation.maturity_e8s != stake.remaining_maturity_e8s
        || observation.staked_maturity_e8s != stake.staked_maturity_e8s
    {
        let reason = format!(
            "MaturityDrift before DisburseMaturity: expected ordinary {} and staked {}, observed ordinary {} and staked {}",
            stake.remaining_maturity_e8s,
            stake.staked_maturity_e8s,
            observation.maturity_e8s,
            observation.staked_maturity_e8s
        );
        let mut replacement = operation.clone();
        replacement.phase = MaturityCommandPhase::MaturityDrift {
            reason: reason.clone(),
            stake,
        };
        write_exact(&operation, replacement, true)?;
        return Err(ApiError::Stuck(reason));
    }
    let mut replacement = operation.clone();
    replacement.phase = MaturityCommandPhase::ReadyToDisburse(DisburseMaturitySubmission {
        stake,
        submitted_at_seconds: now_seconds()?,
    });
    write_exact(&operation, replacement, false)?;
    Ok(MaturityProgress::StakeMaturitySucceeded)
}

async fn submit_disburse(
    mut operation: MaturityCommandOperation,
    mut submission: DisburseMaturitySubmission,
) -> Result<MaturityProgress, ApiError> {
    let expected = operation.clone();
    submission.submitted_at_seconds = now_seconds()?;
    operation.dispatch_epoch = next_epoch(operation.dispatch_epoch)?;
    operation.phase = MaturityCommandPhase::DisburseMaturitySubmitted(submission.clone());
    write_exact(&expected, operation.clone(), false)?;
    let submitted = operation.clone();
    let result = execution::disburse_maturity(
        &state::read().config,
        submission.stake.plan.neuron.neuron_id,
        &submission.stake.plan.destination,
    )
    .await;
    ensure_exact(&submitted)?;
    let amount = match result {
        Ok(amount) => amount,
        Err(error) => {
            write_exact(&submitted, submitted.clone(), true)?;
            return Err(ApiError::Pending(format!(
                "DisburseMaturity outcome requires canonical observation: {error:?}"
            )));
        }
    };
    if amount != submission.stake.remaining_maturity_e8s {
        let reason = format!(
            "DisburseMaturity returned {amount}, expected {}",
            submission.stake.remaining_maturity_e8s
        );
        let mut replacement = submitted.clone();
        replacement.phase = MaturityCommandPhase::DisburseMaturityMismatch {
            reason: reason.clone(),
            submission,
            observed_amount_e8s: amount,
        };
        write_exact(&submitted, replacement, true)?;
        return Err(ApiError::Stuck(reason));
    }
    let mut replacement = submitted.clone();
    replacement.phase =
        MaturityCommandPhase::DisburseMaturitySucceeded(DisburseMaturitySucceeded {
            submission,
            amount_disbursed_e8s: amount,
            evidence_source: MaturityEvidenceSource::CommandResponse,
        });
    write_exact(&submitted, replacement, false)?;
    Ok(MaturityProgress::DisburseMaturitySucceeded)
}

async fn recover_disburse(
    operation: MaturityCommandOperation,
    submission: DisburseMaturitySubmission,
) -> Result<MaturityProgress, ApiError> {
    let observation = execution::query_neuron_observation(
        &state::read().config,
        submission.stake.plan.neuron.neuron_id,
    )
    .await?;
    ensure_exact(&operation)?;
    let canonical = execution::exact_maturity_disbursement(
        &observation,
        submission.stake.remaining_maturity_e8s,
        &submission.stake.plan.destination,
        submission.submitted_at_seconds,
    );
    if canonical.is_err() {
        write_exact(&operation, operation.clone(), true)?;
        return Err(ApiError::Pending(
            "DisburseMaturity remains ambiguous; no exact canonical pending record exists".into(),
        ));
    }
    let mut replacement = operation.clone();
    replacement.phase =
        MaturityCommandPhase::DisburseMaturitySucceeded(DisburseMaturitySucceeded {
            amount_disbursed_e8s: submission.stake.remaining_maturity_e8s,
            submission,
            evidence_source: MaturityEvidenceSource::CanonicalNeuronObservation,
        });
    write_exact(&operation, replacement, false)?;
    Ok(MaturityProgress::DisburseMaturitySucceeded)
}

async fn canonicalize_disbursement(
    operation: MaturityCommandOperation,
    disbursement: DisburseMaturitySucceeded,
) -> Result<MaturityProgress, ApiError> {
    let plan = &disbursement.submission.stake.plan;
    let observation =
        execution::query_neuron_observation(&state::read().config, plan.neuron.neuron_id).await?;
    ensure_exact(&operation)?;
    let canonical = execution::exact_maturity_disbursement(
        &observation,
        disbursement.amount_disbursed_e8s,
        &plan.destination,
        disbursement.submission.submitted_at_seconds,
    )?;
    let passive = PendingMaturityDisbursement {
        kind: operation.kind,
        neuron_id: plan.neuron.neuron_id,
        nominal_disbursed_maturity_e8s: disbursement.amount_disbursed_e8s,
        destination: plan.destination.clone(),
        initiation_timestamp_seconds: canonical.initiated_at_seconds,
        scheduled_finalization_timestamp_seconds: canonical
            .scheduled_finalization_timestamp_seconds,
        stake_evidence: disbursement.submission.stake.clone(),
        disburse_evidence: disbursement,
        mint_proof: MintProofState::Awaiting,
    };
    move_to_passive(&operation, passive)?;
    Ok(MaturityProgress::AwaitingMintProof)
}

pub(crate) fn start_claim_receipt_delivery(
    pending: PendingMaturityDisbursement,
) -> Result<MaturityProgress, ApiError> {
    let MintProofState::Proved(mint) = pending.mint_proof.clone() else {
        return Err(ApiError::Busy);
    };
    let mut delivering = pending.clone();
    delivering.mint_proof = MintProofState::Delivering(mint);
    let mut latest = state::read();
    if latest.active_operation.is_some() || pending_from(&latest, pending.kind) != Some(pending) {
        return Err(ApiError::Busy);
    }
    let operation_sequence = latest.next_operation_sequence;
    latest.next_operation_sequence = operation_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("maturity operation sequence exhausted".into()))?;
    let operation = MaturityCommandOperation {
        operation_sequence,
        dispatch_epoch: 0,
        kind: delivering.kind,
        phase: MaturityCommandPhase::ClaimReceiptDelivery(ClaimReceiptDeliveryOperation {
            pending: delivering.clone(),
            permit: None,
            permanent_transfer: None,
            claim_transfer: None,
        }),
    };
    set_pending(&mut latest, delivering);
    latest.active_operation = Some(NnsOperation::Maturity(Box::new(operation)));
    state::write(latest);
    Ok(MaturityProgress::DeliveringClaimReceipt)
}

async fn resume_claim_receipt_delivery(
    operation: MaturityCommandOperation,
    delivery: ClaimReceiptDeliveryOperation,
) -> Result<MaturityProgress, ApiError> {
    let mint = match &delivery.pending.mint_proof {
        MintProofState::Delivering(mint) => mint.clone(),
        _ => {
            return Err(ApiError::Invalid(
                "claim receipt lost exact Mint evidence".into(),
            ))
        }
    };
    let config = state::read().config;
    if operation.kind == MaturityKind::TwoWeek {
        match delivery.permanent_transfer.as_ref() {
            None => {
                let split =
                    io_core_model::split_40_60(mint.actual_minted_icp_e8s).map_err(|error| {
                        ApiError::Invalid(format!("maturity split failed: {error:?}"))
                    })?;
                let credit = split
                    .permanent
                    .checked_sub(config.expected_icp_fee_e8s)
                    .filter(|credit| *credit > 0)
                    .ok_or_else(|| {
                        ApiError::Invalid("permanent maturity leg cannot pay its fee".into())
                    })?;
                let observation = crate::api::observe_claim_backing().await?;
                ensure_exact(&operation)?;
                return prepare_maturity_transfer(
                    operation,
                    true,
                    observation.permanent_staking_account,
                    credit,
                );
            }
            Some(attempt) if !matches!(attempt.state, TransferState::Succeeded { .. }) => {
                return submit_maturity_transfer(operation, true).await;
            }
            Some(_) => {}
        }
    }
    let Some(permit) = delivery.permit.clone() else {
        let observation = crate::api::observe_claim_backing().await?;
        ensure_exact(&operation)?;
        let (kind, claim_credit) = claim_receipt_economics(
            &delivery.pending,
            mint.actual_minted_icp_e8s,
            config.expected_icp_fee_e8s,
        )?;
        let permit = execution::prepare_claim_receipt(
            &config,
            PrepareClaimBackingReceiptArgs {
                source_operation_id: maturity_source_operation_id(&delivery.pending),
                kind,
                source_account: config.maturity_staging.clone(),
                source_block: mint.mint_block,
                net_liquid_credit_e8s: claim_credit,
                nns_fingerprint: observation.fingerprint,
            },
        )
        .await?;
        ensure_exact(&operation)?;
        if permit.amount_e8s != claim_credit
            || !permit
                .destination
                .effective_eq(&config.stream_liquid_account)
                .map_err(ApiError::Invalid)?
        {
            return Err(ApiError::Invalid(
                "Stream claim permit differs from maturity economics".into(),
            ));
        }
        let mut replacement = operation.clone();
        delivery_mut(&mut replacement).permit = Some(permit);
        write_exact(&operation, replacement, false)?;
        return Ok(MaturityProgress::DeliveringClaimReceipt);
    };
    match delivery.claim_transfer.as_ref() {
        None => {
            return prepare_maturity_transfer(
                operation,
                false,
                permit.destination.clone(),
                permit.amount_e8s,
            )
        }
        Some(attempt) if !matches!(attempt.state, TransferState::Succeeded { .. }) => {
            return submit_maturity_transfer(operation, false).await;
        }
        Some(_) => {}
    }
    let block = delivery
        .claim_transfer
        .as_ref()
        .ok_or(ApiError::Busy)?
        .succeeded_block()
        .map_err(ApiError::Invalid)?;
    let progress = execution::prove_claim_receipt(
        &config,
        ProveClaimBackingReceiptArgs {
            stream_operation_sequence: permit.stream_operation_sequence,
            block_index: block,
        },
    )
    .await?;
    ensure_exact(&operation)?;
    resume_stream_receipt(operation, progress).await
}

fn claim_receipt_economics(
    pending: &PendingMaturityDisbursement,
    mint: u128,
    fee: u128,
) -> Result<(ClaimBackingReceiptKind, u128), ApiError> {
    Ok(match pending.kind {
        MaturityKind::TwoYear => (
            ClaimBackingReceiptKind::PermanentMaturity {
                maturity_generation: pending.initiation_timestamp_seconds,
            },
            io_reward_policy::permanent_maturity_credit(mint, fee).map_err(|error| {
                ApiError::Invalid(format!("permanent maturity credit failed: {error:?}"))
            })?,
        ),
        MaturityKind::TwoWeek => {
            let split = io_core_model::split_40_60(mint).map_err(|error| {
                ApiError::Invalid(format!("pooled maturity split failed: {error:?}"))
            })?;
            let credit = split
                .claim
                .checked_sub(fee)
                .filter(|credit| *credit > 0)
                .ok_or_else(|| ApiError::Invalid("pooled claim leg cannot pay its fee".into()))?;
            let generation = pending
                .stake_evidence
                .plan
                .entitlement_batch_generation
                .ok_or_else(|| {
                    ApiError::Invalid("pooled maturity lost its entitlement generation".into())
                })?;
            (
                ClaimBackingReceiptKind::PooledMaturity {
                    entitlement_batch_generation: generation,
                },
                credit,
            )
        }
    })
}

fn prepare_maturity_transfer(
    operation: MaturityCommandOperation,
    permanent: bool,
    destination: crate::state::Account,
    amount: u128,
) -> Result<MaturityProgress, ApiError> {
    let config = state::read().config;
    let memo = if permanent {
        maturity_transfer_memo(
            b"io-pooled-maturity-permanent-v1",
            operation.operation_sequence,
        )
    } else {
        delivery_ref(&operation)
            .permit
            .as_ref()
            .ok_or(ApiError::Busy)?
            .memo
            .clone()
    };
    let attempt = NnsTransferAttempt::prepared(NnsTransferIntent {
        ledger: config.icp_ledger,
        source_subaccount: config
            .maturity_staging
            .canonical()
            .map_err(ApiError::Invalid)?
            .subaccount,
        destination,
        amount_e8s: amount,
        fee_e8s: config.expected_icp_fee_e8s,
        memo,
        created_at_time_nanos: now_nanos()?,
    })
    .map_err(ApiError::Invalid)?;
    let mut replacement = operation.clone();
    if permanent {
        delivery_mut(&mut replacement).permanent_transfer = Some(attempt);
    } else {
        delivery_mut(&mut replacement).claim_transfer = Some(attempt);
    }
    write_exact(&operation, replacement, false)?;
    Ok(MaturityProgress::DeliveringClaimReceipt)
}

async fn submit_maturity_transfer(
    mut operation: MaturityCommandOperation,
    permanent: bool,
) -> Result<MaturityProgress, ApiError> {
    let now = now_nanos()?;
    let expected = operation.clone();
    operation.dispatch_epoch = next_epoch(operation.dispatch_epoch)?;
    let attempt = transfer_mut(delivery_mut(&mut operation), permanent)?;
    let (epoch, first) = match attempt.state {
        TransferState::Prepared => (1, now),
        TransferState::Submitted {
            epoch,
            first_submitted_at_nanos,
            ..
        }
        | TransferState::Paused {
            epoch,
            first_submitted_at_nanos,
            ..
        } => (
            epoch
                .checked_add(1)
                .ok_or_else(|| ApiError::Invalid("inflow retry epoch exhausted".into()))?,
            first_submitted_at_nanos,
        ),
        _ => return Err(ApiError::Busy),
    };
    attempt.state = TransferState::Submitted {
        epoch,
        first_submitted_at_nanos: first,
        last_submitted_at_nanos: now,
    };
    let intent = attempt.intent.clone();
    write_exact(&expected, operation.clone(), false)?;
    let result = execution::submit_transfer(&intent).await;
    ensure_exact(&operation)?;
    let mut replacement = operation.clone();
    let attempt = transfer_mut(delivery_mut(&mut replacement), permanent)?;
    match execution::classify_transfer(result)? {
        execution::ExactTransferOutcome::Succeeded(block) => {
            attempt.state = TransferState::Succeeded { block };
            write_exact(&operation, replacement, false)?;
            Ok(MaturityProgress::DeliveringClaimReceipt)
        }
        execution::ExactTransferOutcome::Paused(classification, reason) => {
            attempt.state = TransferState::Paused {
                epoch,
                first_submitted_at_nanos: first,
                last_submitted_at_nanos: now,
                classification,
                reason: reason.clone(),
            };
            write_exact(&operation, replacement, true)?;
            Err(ApiError::Stuck(reason))
        }
    }
}

pub async fn prove_active_transfer(
    operation: MaturityCommandOperation,
    block_index: u128,
) -> Result<MaturityProgress, ApiError> {
    let delivery = delivery_ref(&operation);
    let (permanent, attempt) = [
        (true, delivery.permanent_transfer.as_ref()),
        (false, delivery.claim_transfer.as_ref()),
    ]
    .into_iter()
    .find_map(|(effect, attempt)| {
        attempt
            .filter(|attempt| {
                matches!(
                    attempt.state,
                    TransferState::Paused {
                        classification: TransferOutcomeClassification::AmbiguousPossibleEffect,
                        ..
                    }
                )
            })
            .map(|attempt| (effect, attempt))
    })
    .ok_or_else(|| {
        ApiError::Invalid("active maturity operation has no ambiguous transfer".into())
    })?;
    let exact = exact_icp_transfer(attempt.intent.ledger, block_index)
        .await
        .map_err(ApiError::Invalid)?;
    ensure_exact(&operation)?;
    let from = icp_account_identifier(&crate::state::Account {
        owner: ic_cdk::api::canister_self(),
        subaccount: Some(attempt.intent.source_subaccount.to_vec()),
    })
    .map_err(ApiError::Invalid)?;
    let to = icp_account_identifier(&attempt.intent.destination).map_err(ApiError::Invalid)?;
    if !exact.matches(&ExpectedQueryBlockTransfer {
        from: &from,
        to: &to,
        amount_e8s: attempt.intent.amount_e8s,
        fee_e8s: attempt.intent.fee_e8s,
        native_memo_u64: 0,
        icrc1_memo: Some(&attempt.intent.memo),
        created_at_time: attempt.intent.created_at_time_nanos,
        spender: None,
    }) {
        return Err(ApiError::Invalid(
            "exact ICP block does not match the claim-receipt intent".into(),
        ));
    }
    let mut replacement = operation.clone();
    transfer_mut(delivery_mut(&mut replacement), permanent)?.state =
        TransferState::Succeeded { block: block_index };
    write_exact(&operation, replacement, false)?;
    Ok(MaturityProgress::DeliveringClaimReceipt)
}

pub(crate) async fn resume_stream_receipt(
    operation: MaturityCommandOperation,
    progress: ClaimBackingReceiptProgress,
) -> Result<MaturityProgress, ApiError> {
    match progress {
        ClaimBackingReceiptProgress::AwaitingLiquidProof(_) => {
            Ok(MaturityProgress::DeliveringClaimReceipt)
        }
        ClaimBackingReceiptProgress::SettlingRecipients => {
            let progress = execution::resume_claim_receipt(&state::read().config).await?;
            ensure_exact(&operation)?;
            match progress {
                ClaimBackingReceiptProgress::Completed(_) => finish_inflow(operation),
                ClaimBackingReceiptProgress::Stuck(reason) => Err(ApiError::Stuck(reason)),
                _ => Ok(MaturityProgress::DeliveringClaimReceipt),
            }
        }
        ClaimBackingReceiptProgress::Completed(_) => finish_inflow(operation),
        ClaimBackingReceiptProgress::Stuck(reason) => Err(ApiError::Stuck(reason)),
    }
}

fn finish_inflow(operation: MaturityCommandOperation) -> Result<MaturityProgress, ApiError> {
    crate::maturity_mint::finish(operation)
}

pub(crate) fn maturity_source_operation_id(pending: &PendingMaturityDisbursement) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(candid::encode_one(pending).expect("maturity evidence must encode")).to_vec()
}

fn maturity_transfer_memo(domain: &[u8], sequence: u64) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(sequence.to_be_bytes());
    hasher.finalize().to_vec()
}

pub(crate) fn delivery_ref(operation: &MaturityCommandOperation) -> &ClaimReceiptDeliveryOperation {
    match &operation.phase {
        MaturityCommandPhase::ClaimReceiptDelivery(delivery) => delivery,
        _ => panic!("validated claim-receipt phase"),
    }
}

pub(crate) fn delivery_mut(
    operation: &mut MaturityCommandOperation,
) -> &mut ClaimReceiptDeliveryOperation {
    match &mut operation.phase {
        MaturityCommandPhase::ClaimReceiptDelivery(delivery) => delivery,
        _ => panic!("validated claim-receipt phase"),
    }
}

fn transfer_mut(
    delivery: &mut ClaimReceiptDeliveryOperation,
    permanent: bool,
) -> Result<&mut NnsTransferAttempt, ApiError> {
    if permanent {
        delivery.permanent_transfer.as_mut()
    } else {
        delivery.claim_transfer.as_mut()
    }
    .ok_or_else(|| ApiError::Invalid("claim-receipt transfer is not prepared".into()))
}

pub(crate) fn replay_proved(
    pending: &PendingMaturityDisbursement,
    block_index: u128,
) -> Result<MaturityProgress, ApiError> {
    let mint = match &pending.mint_proof {
        MintProofState::Proved(mint) | MintProofState::Delivering(mint) => mint,
        MintProofState::Awaiting => return Err(ApiError::Busy),
    };
    if mint.mint_block != block_index {
        return Err(ApiError::Invalid("conflicting maturity Mint block".into()));
    }
    Ok(match pending.mint_proof {
        MintProofState::Proved(_) => MaturityProgress::MintProved,
        MintProofState::Delivering(_) => MaturityProgress::DeliveringClaimReceipt,
        MintProofState::Awaiting => unreachable!(),
    })
}

fn expected_staked(plan: &MaturityPlan) -> Result<u64, ApiError> {
    plan.original_staked_maturity_e8s
        .checked_add(plan.stake_maturity_e8s)
        .ok_or_else(|| ApiError::Invalid("staked maturity overflow".into()))
}

fn next_epoch(epoch: u64) -> Result<u64, ApiError> {
    epoch
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("maturity dispatch epoch exhausted".into()))
}

fn identity(
    config: &crate::state::NnsConfig,
    kind: MaturityKind,
) -> Result<(u64, crate::state::Account), ApiError> {
    Ok(match kind {
        MaturityKind::TwoYear => (config.two_year_neuron_id, config.maturity_staging.clone()),
        MaturityKind::TwoWeek => (
            state::read()
                .pooled_parent_id
                .ok_or_else(|| ApiError::Pending("pooled parent is absent".into()))?,
            config.maturity_staging.clone(),
        ),
    })
}

pub(crate) fn write_exact(
    expected: &MaturityCommandOperation,
    replacement: MaturityCommandOperation,
    pause: bool,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(NnsOperation::Maturity(active)) if **active == *expected)
    {
        return Err(ApiError::Busy);
    }
    let (neuron_id, destination) = identity(&latest.config, replacement.kind)?;
    replacement
        .validate(latest.next_operation_sequence, neuron_id, &destination)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(NnsOperation::Maturity(Box::new(replacement)));
    if pause {
        latest.lifecycle = Lifecycle::Paused;
    }
    state::write(latest);
    Ok(())
}

pub(crate) fn ensure_exact(expected: &MaturityCommandOperation) -> Result<(), ApiError> {
    if matches!(&state::read().active_operation, Some(NnsOperation::Maturity(active)) if **active == *expected)
    {
        Ok(())
    } else {
        Err(ApiError::Busy)
    }
}

fn move_to_passive(
    expected: &MaturityCommandOperation,
    passive: PendingMaturityDisbursement,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(NnsOperation::Maturity(active)) if **active == *expected)
        || pending_from(&latest, passive.kind).is_some()
    {
        return Err(ApiError::Busy);
    }
    set_pending(&mut latest, passive);
    latest.active_operation = None;
    state::write(latest);
    Ok(())
}

pub(crate) fn pending_from(
    state: &crate::state::NnsStateV1,
    kind: MaturityKind,
) -> Option<PendingMaturityDisbursement> {
    match kind {
        MaturityKind::TwoYear => state.pending_two_year_maturity.clone(),
        MaturityKind::TwoWeek => state.pending_two_week_maturity.clone(),
    }
}

fn set_pending(state: &mut crate::state::NnsStateV1, pending: PendingMaturityDisbursement) {
    match pending.kind {
        MaturityKind::TwoYear => state.pending_two_year_maturity = Some(pending),
        MaturityKind::TwoWeek => state.pending_two_week_maturity = Some(pending),
    }
}

pub(crate) fn ensure_pending(expected: &PendingMaturityDisbursement) -> Result<(), ApiError> {
    if pending_from(&state::read(), expected.kind).as_ref() == Some(expected) {
        Ok(())
    } else {
        Err(ApiError::Busy)
    }
}

pub(crate) fn replace_pending(
    expected: &PendingMaturityDisbursement,
    replacement: PendingMaturityDisbursement,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    if pending_from(&latest, expected.kind).as_ref() != Some(expected) {
        return Err(ApiError::Busy);
    }
    set_pending(&mut latest, replacement);
    state::write(latest);
    Ok(())
}

fn now_nanos() -> Result<u64, ApiError> {
    let now = ic_cdk::api::time();
    if now == 0 {
        Err(ApiError::Invalid("canister time is zero".into()))
    } else {
        Ok(now)
    }
}

fn now_seconds() -> Result<u64, ApiError> {
    Ok(now_nanos()? / 1_000_000_000)
}
