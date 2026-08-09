use candid::Principal;
use io_ledger_boundary::{
    exact_icp_block, exact_icp_transfer, icp_account_identifier, ExpectedQueryBlockTransfer,
    IcpExactResult,
};

use crate::{
    api::{ApiError, MaturityProgress, PrepareTwoWeekMaturityArgs},
    execution,
    maturity::{
        CompletedMaturity, DisburseMaturitySubmission, DisburseMaturitySucceeded,
        MaturityCommandOperation, MaturityCommandPhase, MaturityEvidenceSource, MaturityKind,
        MaturityPlan, MintEvidence, MintProofState, PendingMaturityDisbursement,
        StakeMaturitySucceeded, TwoWeekDeliveryOperation, MINIMUM_DISBURSEMENT_E8S,
    },
    state::{self, Lifecycle, NnsOperation},
    transfer::{
        NnsTransferAttempt, NnsTransferIntent, TransferOutcomeClassification, TransferState,
    },
};

pub async fn start(caller: Principal, kind: MaturityKind) -> Result<MaturityProgress, ApiError> {
    let snapshot = ready()?;
    if caller != snapshot.config.sns_governance {
        return Err(ApiError::Unauthorized);
    }
    if kind == MaturityKind::TwoWeek {
        return Err(ApiError::Invalid(
            "two-week maturity must be prepared by the stream manager for a frozen entitlement batch".into(),
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
    let (neuron_id, destination) = identity(&snapshot.config, kind);
    let observation = execution::query_neuron_observation(&snapshot.config, neuron_id).await?;
    if state::read() != snapshot {
        return Err(ApiError::Busy);
    }
    if let Err(reason) = execution::validate_maturity_configuration(&observation) {
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
            state::TwoWeekTargetStatus::AtTarget
                | state::TwoWeekTargetStatus::AtTargetWithinUnwindTolerance
        ) {
            return Err(ApiError::Pending(
                "protected two-week principal moved away from the reconciled target".into(),
            ));
        }
    }
    let (stake_maturity_e8s, remaining_maturity_e8s) =
        crate::maturity::split_maturity(observation.maturity_e8s)
            .ok_or_else(|| ApiError::Invalid("maturity split overflow".into()))?;
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
        MaturityCommandPhase::TwoWeekDelivery(delivery) => {
            resume_two_week_delivery(operation, delivery).await
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
        MintProofState::Proved(_) if kind == MaturityKind::TwoWeek => {
            crate::two_week_binding::start_delivery(pending).await
        }
        MintProofState::Delivering(_) => Ok(MaturityProgress::DeliveringTwoWeekReceipt),
        MintProofState::Proved(_) => Err(ApiError::Invalid(
            "two-year maturity cannot retain a proved passive slot".into(),
        )),
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

pub async fn prove_mint(
    kind: MaturityKind,
    block_index: u128,
) -> Result<MaturityProgress, ApiError> {
    let expected = pending_from(&state::read(), kind)
        .ok_or_else(|| ApiError::Invalid("no pending maturity proof slot".into()))?;
    if !matches!(expected.mint_proof, MintProofState::Awaiting) {
        return replay_proved(&expected, block_index);
    }
    let exact = exact_icp_block(state::read().config.icp_ledger, block_index)
        .await
        .map_err(ApiError::Invalid)?;
    ensure_pending(&expected)?;
    let mint = match exact {
        IcpExactResult::Mint(mint) => mint,
        IcpExactResult::Transfer(_) => {
            return Err(ApiError::Invalid(
                "maturity proof block is not an ICP Mint".into(),
            ))
        }
    };
    let destination = icp_account_identifier(&expected.destination).map_err(ApiError::Invalid)?;
    if mint.to != destination
        || mint.amount_e8s == 0
        || mint.icrc1_memo.is_some()
        || mint.native_memo_u64 < expected.scheduled_finalization_timestamp_seconds
        || mint.created_at_time / 1_000_000_000 < mint.native_memo_u64
    {
        return Err(ApiError::Invalid(
            "exact Mint does not match pinned NNS maturity finalization behavior".into(),
        ));
    }
    let observation =
        execution::query_neuron_observation(&state::read().config, expected.neuron_id).await?;
    ensure_pending(&expected)?;
    if execution::has_exact_maturity_disbursement(
        &observation,
        expected.nominal_disbursed_maturity_e8s,
        &expected.destination,
        expected.initiation_timestamp_seconds,
        expected.scheduled_finalization_timestamp_seconds,
    ) {
        return Err(ApiError::Pending(
            "canonical neuron still contains the pending maturity disbursement".into(),
        ));
    }
    let evidence = MintEvidence {
        mint_block: block_index,
        actual_minted_icp_e8s: mint.amount_e8s,
        native_memo_u64: mint.native_memo_u64,
        created_at_time_nanos: mint.created_at_time,
    };
    match kind {
        MaturityKind::TwoYear => complete_two_year(&expected, evidence),
        MaturityKind::TwoWeek => {
            let mut replacement = expected.clone();
            replacement.mint_proof = MintProofState::Proved(evidence);
            replace_pending(&expected, replacement)?;
            Ok(MaturityProgress::MintProved)
        }
    }
}

async fn resume_two_week_delivery(
    operation: MaturityCommandOperation,
    delivery: TwoWeekDeliveryOperation,
) -> Result<MaturityProgress, ApiError> {
    let mint = match &delivery.pending.mint_proof {
        MintProofState::Delivering(mint) => mint.clone(),
        _ => {
            return Err(ApiError::Invalid(
                "two-week delivery lacks exact Mint evidence".into(),
            ))
        }
    };
    let config = state::read().config;
    let Some(permit) = delivery.permit.clone() else {
        let permit = execution::prepare_two_week_receipt(
            &config,
            &delivery.pending,
            mint.actual_minted_icp_e8s,
        )
        .await?;
        ensure_exact(&operation)?;
        if !permit
            .destination
            .effective_eq(&config.stream_liquid_account)
            .map_err(ApiError::Invalid)?
        {
            return Err(ApiError::Invalid(
                "stream returned the wrong two-week liquid destination".into(),
            ));
        }
        let mut replacement = operation.clone();
        let MaturityCommandPhase::TwoWeekDelivery(next) = &mut replacement.phase else {
            unreachable!()
        };
        next.permit = Some(permit);
        write_exact(&operation, replacement, false)?;
        return Ok(MaturityProgress::DeliveringTwoWeekReceipt);
    };
    let Some(attempt) = delivery.transfer.clone() else {
        let intent = NnsTransferIntent {
            ledger: config.icp_ledger,
            source_subaccount: config
                .two_week_maturity_staging
                .canonical()
                .map_err(ApiError::Invalid)?
                .subaccount,
            destination: permit.destination,
            amount_e8s: mint.actual_minted_icp_e8s,
            fee_e8s: config.expected_icp_fee_e8s,
            memo: permit.memo,
            created_at_time_nanos: now_nanos()?,
        };
        let mut replacement = operation.clone();
        let MaturityCommandPhase::TwoWeekDelivery(next) = &mut replacement.phase else {
            unreachable!()
        };
        next.transfer = Some(NnsTransferAttempt::prepared(intent).map_err(ApiError::Invalid)?);
        write_exact(&operation, replacement, false)?;
        return Ok(MaturityProgress::DeliveringTwoWeekReceipt);
    };
    match attempt.state {
        TransferState::Prepared | TransferState::Submitted { .. } => {
            submit_two_week_transfer(operation, attempt).await
        }
        TransferState::Paused {
            classification:
                TransferOutcomeClassification::AmbiguousPossibleEffect
                | TransferOutcomeClassification::InsufficientFunds,
            ..
        } => submit_two_week_transfer(operation, attempt).await,
        TransferState::Paused { reason, .. } => Ok(MaturityProgress::Stuck(reason)),
        TransferState::Succeeded { block } if !delivery.receipt_completed => {
            complete_two_week_receipt(operation, permit, block).await
        }
        TransferState::Succeeded { block } => observe_two_week_settlement(operation, block).await,
        TransferState::Stuck { reason } => Ok(MaturityProgress::Stuck(reason)),
    }
}

async fn submit_two_week_transfer(
    mut operation: MaturityCommandOperation,
    attempt: NnsTransferAttempt,
) -> Result<MaturityProgress, ApiError> {
    let now = now_nanos()?;
    let (epoch, first_submitted_at_nanos) = match attempt.state {
        TransferState::Prepared => (1, now),
        TransferState::Submitted {
            epoch,
            first_submitted_at_nanos,
            last_submitted_at_nanos,
        }
        | TransferState::Paused {
            epoch,
            first_submitted_at_nanos,
            last_submitted_at_nanos,
            classification:
                TransferOutcomeClassification::AmbiguousPossibleEffect
                | TransferOutcomeClassification::InsufficientFunds,
            ..
        } => {
            let config = &state::read().config;
            if now
                .checked_sub(last_submitted_at_nanos)
                .ok_or_else(|| ApiError::Invalid("two-week retry clock regressed".into()))?
                < config.transfer_retry_delay_nanos
            {
                return Ok(MaturityProgress::DeliveringTwoWeekReceipt);
            }
            let deadline = attempt
                .intent
                .created_at_time_nanos
                .checked_add(config.ledger_deduplication_window_nanos)
                .ok_or_else(|| ApiError::Invalid("two-week retry deadline overflow".into()))?;
            if now >= deadline {
                let reason =
                    "two-week transfer retry window expired; exact block proof is required";
                let mut replacement = operation.clone();
                let MaturityCommandPhase::TwoWeekDelivery(next) = &mut replacement.phase else {
                    unreachable!()
                };
                next.transfer.as_mut().expect("delivery transfer").state = TransferState::Paused {
                    epoch,
                    first_submitted_at_nanos,
                    last_submitted_at_nanos,
                    classification: TransferOutcomeClassification::AmbiguousPossibleEffect,
                    reason: reason.into(),
                };
                write_exact(&operation, replacement, true)?;
                return Err(ApiError::Stuck(reason.into()));
            }
            (
                epoch
                    .checked_add(1)
                    .ok_or_else(|| ApiError::Invalid("two-week dispatch epoch exhausted".into()))?,
                first_submitted_at_nanos,
            )
        }
        _ => return Err(ApiError::Busy),
    };
    let expected = operation.clone();
    operation.dispatch_epoch = next_epoch(operation.dispatch_epoch)?;
    let MaturityCommandPhase::TwoWeekDelivery(delivery) = &mut operation.phase else {
        unreachable!()
    };
    let transfer = delivery.transfer.as_mut().expect("delivery transfer");
    transfer.state = TransferState::Submitted {
        epoch,
        first_submitted_at_nanos,
        last_submitted_at_nanos: now,
    };
    let intent = transfer.intent.clone();
    write_exact(&expected, operation.clone(), false)?;
    let submitted = operation.clone();
    let result = execution::submit_transfer(&intent).await;
    ensure_exact(&submitted)?;
    match execution::classify_transfer(result)? {
        execution::ExactTransferOutcome::Succeeded(block) => {
            let mut replacement = submitted.clone();
            let MaturityCommandPhase::TwoWeekDelivery(next) = &mut replacement.phase else {
                unreachable!()
            };
            next.transfer.as_mut().expect("delivery transfer").state =
                TransferState::Succeeded { block };
            write_exact(&submitted, replacement, false)?;
            Ok(MaturityProgress::DeliveringTwoWeekReceipt)
        }
        execution::ExactTransferOutcome::Paused(classification, reason) => {
            let mut replacement = submitted.clone();
            let MaturityCommandPhase::TwoWeekDelivery(next) = &mut replacement.phase else {
                unreachable!()
            };
            let TransferState::Submitted {
                epoch,
                first_submitted_at_nanos,
                last_submitted_at_nanos,
            } = next.transfer.as_ref().expect("delivery transfer").state
            else {
                unreachable!()
            };
            next.transfer.as_mut().expect("delivery transfer").state = TransferState::Paused {
                epoch,
                first_submitted_at_nanos,
                last_submitted_at_nanos,
                classification,
                reason: reason.clone(),
            };
            write_exact(&submitted, replacement, true)?;
            Err(ApiError::Stuck(reason))
        }
    }
}

pub async fn prove_active_transfer(
    operation: MaturityCommandOperation,
    block_index: u128,
) -> Result<MaturityProgress, ApiError> {
    let delivery = match &operation.phase {
        MaturityCommandPhase::TwoWeekDelivery(delivery) => delivery,
        _ => {
            return Err(ApiError::Invalid(
                "active maturity operation has no two-week transfer proof slot".into(),
            ))
        }
    };
    let attempt = delivery
        .transfer
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("two-week transfer is not prepared".into()))?;
    if !matches!(
        attempt.state,
        TransferState::Paused {
            classification: TransferOutcomeClassification::AmbiguousPossibleEffect,
            ..
        }
    ) {
        return Err(ApiError::Invalid(
            "only an ambiguous possible-effect transfer accepts exact proof".into(),
        ));
    }
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
            "exact ICP block does not match the two-week delivery intent".into(),
        ));
    }
    let mut replacement = operation.clone();
    let MaturityCommandPhase::TwoWeekDelivery(delivery) = &mut replacement.phase else {
        unreachable!()
    };
    delivery.transfer.as_mut().expect("delivery transfer").state =
        TransferState::Succeeded { block: block_index };
    write_exact(&operation, replacement, false)?;
    Ok(MaturityProgress::DeliveringTwoWeekReceipt)
}

async fn complete_two_week_receipt(
    operation: MaturityCommandOperation,
    permit: crate::jupiter::StreamReceiptPermit,
    block: u128,
) -> Result<MaturityProgress, ApiError> {
    let progress =
        execution::complete_jupiter_receipt(&state::read().config, &permit, block).await?;
    ensure_exact(&operation)?;
    let mut replacement = operation.clone();
    let MaturityCommandPhase::TwoWeekDelivery(delivery) = &mut replacement.phase else {
        unreachable!()
    };
    delivery.receipt_completed = true;
    write_exact(&operation, replacement.clone(), false)?;
    match progress {
        execution::StreamLiquidProgress::Completed(result) => {
            finish_two_week(replacement, block, result)
        }
        _ => Ok(MaturityProgress::DeliveringTwoWeekReceipt),
    }
}

async fn observe_two_week_settlement(
    operation: MaturityCommandOperation,
    block: u128,
) -> Result<MaturityProgress, ApiError> {
    let progress = execution::resume_stream(&state::read().config).await?;
    ensure_exact(&operation)?;
    match progress {
        execution::StreamLiquidProgress::Completed(result) => {
            finish_two_week(operation, block, result)
        }
        execution::StreamLiquidProgress::Stuck(reason) => Err(ApiError::Stuck(reason)),
        _ => Ok(MaturityProgress::DeliveringTwoWeekReceipt),
    }
}

fn finish_two_week(
    operation: MaturityCommandOperation,
    receipt_block: u128,
    result: execution::CompletedReceiptResult,
) -> Result<MaturityProgress, ApiError> {
    let MaturityCommandPhase::TwoWeekDelivery(delivery) = &operation.phase else {
        return Err(ApiError::Busy);
    };
    let mint = match &delivery.pending.mint_proof {
        MintProofState::Delivering(mint) => mint,
        _ => {
            return Err(ApiError::Invalid(
                "two-week delivery lost Mint evidence".into(),
            ))
        }
    };
    let permit = delivery
        .permit
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("two-week delivery lacks receipt permit".into()))?;
    let stream = match result {
        execution::CompletedReceiptResult::TwoWeek(result) => result,
        execution::CompletedReceiptResult::Jupiter(_) => {
            return Err(ApiError::Invalid(
                "stream completed the wrong receipt kind".into(),
            ))
        }
    };
    let fingerprint = execution::two_week_receipt_fingerprint(
        permit.sequence,
        &delivery.pending,
        mint.actual_minted_icp_e8s,
    )?;
    if stream.request_fingerprint != fingerprint
        || stream.receipt_block != receipt_block
        || stream.backed_io_pool_e8s
            != stream
                .distributed_io_e8s
                .checked_add(stream.forfeited_io_e8s)
                .and_then(|total| total.checked_add(stream.rounding_dust_io_e8s))
                .ok_or_else(|| ApiError::Invalid("two-week receipt total overflow".into()))?
        || stream.completed_at_nanos == 0
    {
        return Err(ApiError::Invalid(
            "stream two-week completion evidence does not match the exact receipt".into(),
        ));
    }
    let completed = CompletedMaturity {
        kind: MaturityKind::TwoWeek,
        neuron_id: delivery.pending.neuron_id,
        mint_block: mint.mint_block,
        nominal_disbursed_maturity_e8s: delivery.pending.nominal_disbursed_maturity_e8s,
        actual_minted_icp_e8s: mint.actual_minted_icp_e8s,
        destination: delivery.pending.destination.clone(),
        completed_at_nanos: now_nanos()?,
    };
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(NnsOperation::Maturity(active)) if **active == operation)
        || latest.pending_two_week_maturity.as_ref() != Some(&delivery.pending)
    {
        return Err(ApiError::Busy);
    }
    latest.active_operation = None;
    latest.pending_two_week_maturity = None;
    latest.last_two_week_maturity = Some(completed.clone());
    let generation = delivery
        .pending
        .stake_evidence
        .plan
        .entitlement_batch_generation
        .ok_or_else(|| ApiError::Invalid("two-week completion lacks generation".into()))?;
    if generation != latest.latest_started_two_week_generation
        || generation <= latest.latest_completed_two_week_generation
    {
        return Err(ApiError::Busy);
    }
    latest.latest_completed_two_week_generation = generation;
    state::write(latest);
    Ok(MaturityProgress::Completed(completed))
}

fn complete_two_year(
    expected: &PendingMaturityDisbursement,
    mint: MintEvidence,
) -> Result<MaturityProgress, ApiError> {
    let completed = CompletedMaturity {
        kind: MaturityKind::TwoYear,
        neuron_id: expected.neuron_id,
        mint_block: mint.mint_block,
        nominal_disbursed_maturity_e8s: expected.nominal_disbursed_maturity_e8s,
        actual_minted_icp_e8s: mint.actual_minted_icp_e8s,
        destination: expected.destination.clone(),
        completed_at_nanos: now_nanos()?,
    };
    let mut latest = state::read();
    if latest.pending_two_year_maturity.as_ref() != Some(expected) {
        return Err(ApiError::Busy);
    }
    latest.pending_two_year_maturity = None;
    latest.last_two_year_maturity = Some(completed.clone());
    state::write(latest);
    Ok(MaturityProgress::Completed(completed))
}

fn replay_proved(
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
        MintProofState::Delivering(_) => MaturityProgress::DeliveringTwoWeekReceipt,
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

fn ready() -> Result<crate::state::NnsStateV1, ApiError> {
    let state = state::read();
    match state.lifecycle {
        Lifecycle::Ready => Ok(state),
        Lifecycle::Paused => Err(ApiError::Paused),
    }
}

fn identity(config: &crate::state::NnsConfig, kind: MaturityKind) -> (u64, crate::state::Account) {
    match kind {
        MaturityKind::TwoYear => (
            config.two_year_neuron_id,
            config.stream_liquid_account.clone(),
        ),
        MaturityKind::TwoWeek => (
            config.two_week_neuron_id,
            config.two_week_maturity_staging.clone(),
        ),
    }
}

fn write_exact(
    expected: &MaturityCommandOperation,
    replacement: MaturityCommandOperation,
    pause: bool,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(NnsOperation::Maturity(active)) if **active == *expected)
    {
        return Err(ApiError::Busy);
    }
    let (neuron_id, destination) = identity(&latest.config, replacement.kind);
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

fn ensure_exact(expected: &MaturityCommandOperation) -> Result<(), ApiError> {
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

fn replace_pending(
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
