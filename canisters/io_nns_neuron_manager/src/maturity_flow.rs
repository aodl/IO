use candid::Principal;
use io_ledger_boundary::{exact_icp_transfer, icp_account_identifier, ExpectedQueryBlockTransfer};
use io_receipt_types::{
    ClaimBackingReceiptProgress, PrepareClaimBackingReceiptArgs, ProveClaimBackingReceiptArgs,
};

use crate::{
    api::{ApiError, MaturityProgress, PrepareTwoWeekMaturityArgs},
    execution,
    maturity::{
        MaturityCommandOperation, MaturityCommandPhase, MaturityDeliveryOperation, MaturityIntent,
        MaturityKind, PendingMaturityDisbursement, PermanentCreditState, MINIMUM_DISBURSEMENT_E8S,
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
    let neuron_id = neuron_id(&snapshot, kind)?;
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
        let tolerance = crate::api::hold_excess_tolerance(snapshot.config.expected_icp_fee_e8s)?;
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
    if observation.maturity_e8s < MINIMUM_DISBURSEMENT_E8S {
        return Err(ApiError::BelowMaturityThreshold {
            remaining_e8s: observation.maturity_e8s,
            minimum_e8s: MINIMUM_DISBURSEMENT_E8S,
        });
    }
    let entitlement_batch_generation = entitlement_batch
        .as_ref()
        .map(|value| value.entitlement_batch_generation);
    let two_week_target_e8s = entitlement_batch.as_ref().map(|value| value.target_e8s);

    let mut latest = state::read();
    if latest != snapshot
        || latest.lifecycle != Lifecycle::Ready
        || (kind == MaturityKind::TwoWeek
            && latest.latest_two_week_generation().checked_add(1) != entitlement_batch_generation)
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
        phase: MaturityCommandPhase::Observed(MaturityIntent {
            entitlement_batch_generation,
            two_week_target_e8s,
        }),
    };
    operation
        .validate(latest.next_operation_sequence)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(NnsOperation::Maturity(Box::new(operation.clone())));
    state::write(latest);
    resume_active(operation).await
}

pub async fn resume_active(
    operation: MaturityCommandOperation,
) -> Result<MaturityProgress, ApiError> {
    match operation.phase.clone() {
        MaturityCommandPhase::Observed(plan) => submit_disburse(operation, plan).await,
        MaturityCommandPhase::DisburseMaturitySubmitted {
            intent,
            submitted_at_seconds,
        } => recover_disburse(operation, intent, submitted_at_seconds).await,
        MaturityCommandPhase::DisburseMaturitySucceeded {
            intent,
            submitted_at_seconds,
            amount_disbursed_e8s,
        } => {
            canonicalize_disbursement(
                operation,
                intent,
                submitted_at_seconds,
                amount_disbursed_e8s,
            )
            .await
        }
        MaturityCommandPhase::Delivery(delivery) => resume_delivery(operation, delivery).await,
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
    match pending.captured_e8s {
        None => capture_staging_balance(kind, pending).await,
        Some(_) => start_delivery(kind, pending).await,
    }
}

async fn submit_disburse(
    mut operation: MaturityCommandOperation,
    intent: MaturityIntent,
) -> Result<MaturityProgress, ApiError> {
    let expected = operation.clone();
    let submitted_at_seconds = now_seconds()?;
    operation.dispatch_epoch = next_epoch(operation.dispatch_epoch)?;
    operation.phase = MaturityCommandPhase::DisburseMaturitySubmitted {
        intent,
        submitted_at_seconds,
    };
    write_exact(&expected, operation.clone(), false)?;
    let submitted = operation.clone();
    let result = execution::disburse_maturity(
        &state::read().config,
        neuron_id(&state::read(), operation.kind)?,
        &staging_account(ic_cdk::api::canister_self(), operation.kind),
    )
    .await;
    ensure_exact(&submitted)?;
    match result {
        execution::GovernanceCallOutcome::Succeeded(amount) => {
            let mut replacement = submitted.clone();
            replacement.phase = MaturityCommandPhase::DisburseMaturitySucceeded {
                intent,
                submitted_at_seconds,
                amount_disbursed_e8s: amount,
            };
            write_exact(&submitted, replacement.clone(), false)?;
            canonicalize_disbursement(replacement, intent, submitted_at_seconds, amount).await
        }
        execution::GovernanceCallOutcome::RejectedNoEffect(reason) => {
            let mut replacement = submitted.clone();
            replacement.phase = MaturityCommandPhase::Observed(intent);
            write_exact(&submitted, replacement, false)?;
            Err(ApiError::Pending(format!(
                "{reason}; DisburseMaturity is prepared for deterministic retry"
            )))
        }
        execution::GovernanceCallOutcome::Ambiguous(reason) => {
            write_exact(&submitted, submitted.clone(), true)?;
            Err(ApiError::Pending(format!(
                "{reason}; canonical pending-disbursement proof is required"
            )))
        }
    }
}

async fn recover_disburse(
    operation: MaturityCommandOperation,
    intent: MaturityIntent,
    submitted_at_seconds: u64,
) -> Result<MaturityProgress, ApiError> {
    let current = state::read();
    let observation =
        execution::query_neuron_observation(&current.config, neuron_id(&current, operation.kind)?)
            .await?;
    ensure_exact(&operation)?;
    let canonical = match execution::exact_maturity_disbursement(
        &observation,
        &staging_account(ic_cdk::api::canister_self(), operation.kind),
        submitted_at_seconds,
    ) {
        Ok(Some(canonical)) => canonical,
        Ok(None) => {
            return Err(ApiError::Pending(
                "DisburseMaturity remains ambiguous; no canonical pending record exists".into(),
            ))
        }
        Err(ApiError::Invalid(reason)) => {
            write_exact(&operation, operation.clone(), true)?;
            return Err(ApiError::Stuck(reason));
        }
        Err(error) => return Err(error),
    };
    let mut replacement = operation.clone();
    replacement.phase = MaturityCommandPhase::DisburseMaturitySucceeded {
        intent,
        submitted_at_seconds,
        amount_disbursed_e8s: canonical.amount_disbursed_e8s,
    };
    write_exact(&operation, replacement.clone(), false)?;
    finish_disbursement(replacement, intent, canonical)
}

async fn canonicalize_disbursement(
    operation: MaturityCommandOperation,
    intent: MaturityIntent,
    submitted_at_seconds: u64,
    amount_disbursed_e8s: u64,
) -> Result<MaturityProgress, ApiError> {
    let current = state::read();
    let observation =
        execution::query_neuron_observation(&current.config, neuron_id(&current, operation.kind)?)
            .await?;
    ensure_exact(&operation)?;
    let canonical = execution::exact_maturity_disbursement(
        &observation,
        &staging_account(ic_cdk::api::canister_self(), operation.kind),
        submitted_at_seconds,
    )?
    .ok_or_else(|| {
        ApiError::Pending("canonical maturity disbursement is not observable yet".into())
    })?;
    if canonical.amount_disbursed_e8s != amount_disbursed_e8s {
        write_exact(&operation, operation.clone(), true)?;
        return Err(ApiError::Stuck(format!(
            "DisburseMaturity response amount {} conflicts with canonical amount {}",
            amount_disbursed_e8s, canonical.amount_disbursed_e8s
        )));
    }
    finish_disbursement(operation, intent, canonical)
}

fn finish_disbursement(
    operation: MaturityCommandOperation,
    intent: MaturityIntent,
    canonical: io_nns_types::maturity::CanonicalDisbursementEvidence,
) -> Result<MaturityProgress, ApiError> {
    let passive = PendingMaturityDisbursement {
        nominal_disbursed_e8s: canonical.amount_disbursed_e8s,
        initiated_at_seconds: canonical.initiated_at_seconds,
        scheduled_finalization_timestamp_seconds: canonical
            .scheduled_finalization_timestamp_seconds,
        entitlement_batch_generation: intent.entitlement_batch_generation,
        two_week_target_e8s: intent.two_week_target_e8s,
        captured_e8s: None,
    };
    move_to_passive(&operation, passive)?;
    Ok(MaturityProgress::Pending)
}

async fn capture_staging_balance(
    kind: MaturityKind,
    pending: PendingMaturityDisbursement,
) -> Result<MaturityProgress, ApiError> {
    if now_seconds()? < pending.scheduled_finalization_timestamp_seconds {
        return Err(ApiError::Pending(
            "maturity finalization boundary has not elapsed".into(),
        ));
    }
    let config = state::read().config;
    let current = state::read();
    let observation =
        execution::query_neuron_observation(&config, neuron_id(&current, kind)?).await?;
    ensure_pending(kind, &pending)?;
    let destination = staging_account(ic_cdk::api::canister_self(), kind);
    if execution::has_exact_maturity_disbursement(
        &observation,
        pending.nominal_disbursed_e8s,
        &destination,
        pending.initiated_at_seconds,
        pending.scheduled_finalization_timestamp_seconds,
    ) {
        return Err(ApiError::Pending(
            "canonical neuron still contains the pending maturity disbursement".into(),
        ));
    }
    let balance = execution::icp_balance(&config, &destination).await?;
    ensure_pending(kind, &pending)?;
    if balance == 0 {
        return Err(ApiError::Pending(
            "maturity staging Account has no unprocessed semantic ICP".into(),
        ));
    }
    let captured_e8s = balance;
    let mut replacement = pending.clone();
    replacement.captured_e8s = Some(captured_e8s);
    replace_pending(kind, &pending, replacement.clone())?;
    start_delivery(kind, replacement).await
}

pub(crate) async fn start_delivery(
    kind: MaturityKind,
    pending: PendingMaturityDisbursement,
) -> Result<MaturityProgress, ApiError> {
    if pending.captured_e8s.is_none() {
        return Err(ApiError::Busy);
    }
    let mut latest = state::read();
    if latest.active_operation.is_some() || pending_from(&latest, kind) != Some(pending.clone()) {
        return Err(ApiError::Busy);
    }
    let operation_sequence = latest.next_operation_sequence;
    latest.next_operation_sequence = operation_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("maturity operation sequence exhausted".into()))?;
    let operation = MaturityCommandOperation {
        operation_sequence,
        dispatch_epoch: 0,
        kind,
        phase: MaturityCommandPhase::Delivery(MaturityDeliveryOperation {
            pending: pending.clone(),
            permit: None,
            permanent_credit: None,
            claim_transfer: None,
        }),
    };
    latest.active_operation = Some(NnsOperation::Maturity(Box::new(operation.clone())));
    state::write(latest);
    Box::pin(resume_active(operation)).await
}

async fn resume_delivery(
    operation: MaturityCommandOperation,
    delivery: MaturityDeliveryOperation,
) -> Result<MaturityProgress, ApiError> {
    let captured_e8s = delivery
        .pending
        .captured_e8s
        .ok_or_else(|| ApiError::Invalid("maturity delivery lacks a frozen capture".into()))?;
    let config = state::read().config;
    if crate::claim_assets::maturity_delivery_has_unpaid_fee(&delivery) {
        let canonical_fee = io_ledger_boundary::icp_fee(config.icp_ledger)
            .await
            .map_err(ApiError::Pending)?;
        ensure_exact(&operation)?;
        if canonical_fee != config.expected_icp_fee_e8s {
            let mut latest = state::read();
            latest.lifecycle = Lifecycle::Paused;
            state::write(latest);
            return Err(ApiError::Stuck(format!(
                "maturity transit fee drift: configured {}, canonical {canonical_fee}",
                config.expected_icp_fee_e8s
            )));
        }
    }
    let split = io_nns_types::maturity::capture_40_60(
        captured_e8s,
        config.expected_icp_fee_e8s,
        config.expected_icp_fee_e8s,
    )
    .map_err(|error| ApiError::Invalid(format!("maturity capture split failed: {error:?}")))?;
    match delivery.permanent_credit.as_ref() {
        None => {
            let permanent =
                execution::query_neuron_observation(&config, config.two_year_neuron_id).await?;
            ensure_exact(&operation)?;
            if let Err(reason) = execution::validate_permanent_configuration(&permanent) {
                let mut latest = state::read();
                latest.lifecycle = Lifecycle::Paused;
                state::write(latest);
                return Err(ApiError::Invalid(reason));
            }
            return crate::permanent_credit::prepare(
                operation,
                permanent.snapshot,
                split.permanent_credit,
                config.expected_icp_fee_e8s,
            )
            .await;
        }
        Some(PermanentCreditState::Prepared { transfer, .. })
            if !matches!(transfer.state, TransferState::Succeeded { .. }) =>
        {
            return submit_maturity_transfer(operation, true).await;
        }
        Some(PermanentCreditState::Prepared { before, transfer }) => {
            let transfer_block = transfer.succeeded_block().map_err(ApiError::Invalid)?;
            let mut replacement = operation.clone();
            delivery_mut(&mut replacement).permanent_credit =
                Some(PermanentCreditState::RefreshSubmitted {
                    before: before.clone(),
                    transfer_block,
                });
            write_exact(&operation, replacement.clone(), false)?;
            return crate::permanent_credit::refresh(
                replacement,
                before.clone(),
                transfer_block,
                split.permanent_credit,
            )
            .await;
        }
        Some(PermanentCreditState::RefreshSubmitted {
            before,
            transfer_block,
        }) => {
            return crate::permanent_credit::prove_or_refresh(
                operation,
                before.clone(),
                *transfer_block,
                split.permanent_credit,
                true,
            )
            .await;
        }
        Some(PermanentCreditState::Proved(_)) => {}
    }
    let permit = match (operation.kind, delivery.permit.clone()) {
        (MaturityKind::TwoWeek, None) => {
            let generation = delivery
                .pending
                .entitlement_batch_generation
                .ok_or_else(|| {
                    ApiError::Invalid("two-week capture lost its entitlement generation".into())
                })?;
            let permit = execution::prepare_claim_receipt(
                &config,
                PrepareClaimBackingReceiptArgs {
                    nns_operation_sequence: operation.operation_sequence,
                    kind: io_receipt_types::ClaimBackingReceiptKind::TwoWeek {
                        entitlement_generation: generation,
                    },
                    net_liquid_credit_e8s: split.claim_credit,
                },
            )
            .await?;
            ensure_exact(&operation)?;
            if permit.amount_e8s != split.claim_credit
                || !permit
                    .destination
                    .effective_eq(&config.stream_liquid_account)
                    .map_err(ApiError::Invalid)?
            {
                return Err(ApiError::Invalid(
                    "Stream paired-inflow permit differs from frozen maturity economics".into(),
                ));
            }
            let mut replacement = operation.clone();
            delivery_mut(&mut replacement).permit = Some(permit);
            write_exact(&operation, replacement.clone(), false)?;
            return Box::pin(resume_active(replacement)).await;
        }
        (MaturityKind::TwoWeek, Some(permit)) => Some(permit),
        (MaturityKind::TwoYear, None) => None,
        (MaturityKind::TwoYear, Some(_)) => {
            return Err(ApiError::Invalid(
                "two-year maturity must not contain paired issuance state".into(),
            ))
        }
    };
    match delivery.claim_transfer.as_ref() {
        None => {
            let (destination, amount, memo) = match &permit {
                Some(permit) => (
                    permit.destination.clone(),
                    permit.amount_e8s,
                    permit.memo.clone(),
                ),
                None => (
                    config.stream_liquid_account.clone(),
                    split.claim_credit,
                    maturity_transfer_memo(
                        b"io-two-year-maturity-claim-v1",
                        operation.operation_sequence,
                    ),
                ),
            };
            return prepare_claim_transfer(operation, destination, amount, memo).await;
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
    let Some(permit) = permit else {
        return finish_inflow(operation, None);
    };
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

async fn prepare_claim_transfer(
    operation: MaturityCommandOperation,
    destination: crate::state::Account,
    amount: u128,
    memo: Vec<u8>,
) -> Result<MaturityProgress, ApiError> {
    let config = state::read().config;
    let attempt = NnsTransferAttempt::prepared(NnsTransferIntent {
        ledger: config.icp_ledger,
        source_subaccount: staging_account(ic_cdk::api::canister_self(), operation.kind)
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
    delivery_mut(&mut replacement).claim_transfer = Some(attempt);
    write_exact(&operation, replacement.clone(), false)?;
    Box::pin(resume_active(replacement)).await
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
            classification:
                TransferOutcomeClassification::BadFee
                | TransferOutcomeClassification::InsufficientFunds
                | TransferOutcomeClassification::RejectedNoEffect,
            ..
        } => (
            epoch
                .checked_add(1)
                .ok_or_else(|| ApiError::Invalid("inflow retry epoch exhausted".into()))?,
            first_submitted_at_nanos,
        ),
        TransferState::Paused {
            classification: TransferOutcomeClassification::AmbiguousPossibleEffect,
            ..
        } => {
            return Err(ApiError::Pending(
                "ambiguous maturity transfer requires exact block proof".into(),
            ))
        }
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
            write_exact(&operation, replacement.clone(), false)?;
            Box::pin(resume_active(replacement)).await
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
        (
            true,
            match delivery.permanent_credit.as_ref() {
                Some(PermanentCreditState::Prepared { transfer, .. }) => Some(transfer.as_ref()),
                _ => None,
            },
        ),
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
    write_exact(&operation, replacement.clone(), false)?;
    Box::pin(resume_active(replacement)).await
}

pub(crate) async fn resume_stream_receipt(
    operation: MaturityCommandOperation,
    progress: ClaimBackingReceiptProgress,
) -> Result<MaturityProgress, ApiError> {
    match progress {
        ClaimBackingReceiptProgress::AwaitingLiquidProof(_) => Ok(MaturityProgress::Pending),
        ClaimBackingReceiptProgress::Pending => {
            let progress = execution::resume_claim_receipt(&state::read().config).await?;
            ensure_exact(&operation)?;
            match progress {
                ClaimBackingReceiptProgress::Completed(result) => {
                    finish_inflow(operation, Some(&result))
                }
                ClaimBackingReceiptProgress::Stuck(reason) => Err(ApiError::Stuck(reason)),
                _ => Ok(MaturityProgress::Pending),
            }
        }
        ClaimBackingReceiptProgress::Completed(result) => finish_inflow(operation, Some(&result)),
        ClaimBackingReceiptProgress::Stuck(reason) => Err(ApiError::Stuck(reason)),
    }
}

fn finish_inflow(
    operation: MaturityCommandOperation,
    stream_result: Option<&io_receipt_types::ClaimBackingReceiptResult>,
) -> Result<MaturityProgress, ApiError> {
    let delivery = delivery_ref(&operation);
    let captured_e8s = delivery
        .pending
        .captured_e8s
        .ok_or_else(|| ApiError::Invalid("completed maturity lacks frozen capture".into()))?;
    let config = state::read().config;
    let split = io_nns_types::maturity::capture_40_60(
        captured_e8s,
        config.expected_icp_fee_e8s,
        config.expected_icp_fee_e8s,
    )
    .map_err(|error| ApiError::Invalid(format!("completed maturity split failed: {error:?}")))?;
    if !matches!(
        delivery.permanent_credit,
        Some(PermanentCreditState::Proved(_))
    ) || !crate::claim_assets::claim_transfer_succeeded(delivery.claim_transfer.as_ref())
    {
        return Err(ApiError::Invalid(
            "completed maturity lacks proved outgoing effects".into(),
        ));
    }
    let entitlement_batch_generation = delivery.pending.entitlement_batch_generation;
    match (operation.kind, stream_result) {
        (MaturityKind::TwoYear, None) => {}
        (MaturityKind::TwoWeek, Some(result))
            if result.nns_operation_sequence == operation.operation_sequence
                && result.kind
                    == io_receipt_types::ClaimBackingReceiptKind::TwoWeek {
                        entitlement_generation: entitlement_batch_generation.ok_or_else(|| {
                            ApiError::Invalid("two-week completion lost generation".into())
                        })?,
                    }
                && result.liquid_credit_e8s == split.claim_credit => {}
        (MaturityKind::TwoYear, Some(_)) => {
            return Err(ApiError::Invalid(
                "two-year maturity used paired issuance".into(),
            ))
        }
        (MaturityKind::TwoWeek, _) => {
            return Err(ApiError::Invalid(
                "two-week Stream settlement differs from frozen capture".into(),
            ))
        }
    }
    let completed = crate::maturity::CompletedMaturity {
        kind: operation.kind,
        captured_e8s,
        permanent_credit_e8s: split.permanent_credit,
        claim_credit_e8s: split.claim_credit,
        entitlement_batch_generation,
        two_week_target_e8s: delivery.pending.two_week_target_e8s,
        completed_at_nanos: now_nanos()?,
    };
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(NnsOperation::Maturity(active)) if **active == operation)
    {
        return Err(ApiError::Busy);
    }
    latest.active_operation = None;
    match operation.kind {
        MaturityKind::TwoYear => {
            latest.pending_two_year_maturity = None;
            latest.last_two_year_maturity = Some(completed.clone());
        }
        MaturityKind::TwoWeek => {
            latest.pending_two_week_maturity = None;
            latest.last_two_week_maturity = Some(completed.clone());
        }
    }
    state::write(latest);
    Ok(MaturityProgress::Completed(Box::new(completed)))
}

pub(crate) fn maturity_transfer_memo(domain: &[u8], sequence: u64) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(sequence.to_be_bytes());
    hasher.finalize().to_vec()
}

pub(crate) fn delivery_ref(operation: &MaturityCommandOperation) -> &MaturityDeliveryOperation {
    match &operation.phase {
        MaturityCommandPhase::Delivery(delivery) => delivery,
        _ => panic!("validated maturity delivery phase"),
    }
}

pub(crate) fn delivery_mut(
    operation: &mut MaturityCommandOperation,
) -> &mut MaturityDeliveryOperation {
    match &mut operation.phase {
        MaturityCommandPhase::Delivery(delivery) => delivery,
        _ => panic!("validated maturity delivery phase"),
    }
}

fn transfer_mut(
    delivery: &mut MaturityDeliveryOperation,
    permanent: bool,
) -> Result<&mut NnsTransferAttempt, ApiError> {
    if permanent {
        match delivery.permanent_credit.as_mut() {
            Some(PermanentCreditState::Prepared { transfer, .. }) => Some(transfer.as_mut()),
            _ => None,
        }
    } else {
        delivery.claim_transfer.as_mut()
    }
    .ok_or_else(|| ApiError::Invalid("claim-receipt transfer is not prepared".into()))
}

fn next_epoch(epoch: u64) -> Result<u64, ApiError> {
    epoch
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("maturity dispatch epoch exhausted".into()))
}

fn neuron_id(state: &crate::state::NnsStateV1, kind: MaturityKind) -> Result<u64, ApiError> {
    match kind {
        MaturityKind::TwoYear => Ok(state.config.two_year_neuron_id),
        MaturityKind::TwoWeek => state
            .pooled_parent_id
            .ok_or_else(|| ApiError::Pending("pooled parent is absent".into())),
    }
}

pub(crate) fn staging_account(owner: Principal, kind: MaturityKind) -> crate::state::Account {
    match kind {
        MaturityKind::TwoYear => io_accounts::two_year_maturity_staging(owner),
        MaturityKind::TwoWeek => io_accounts::two_week_maturity_staging(owner),
    }
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
    replacement
        .validate(latest.next_operation_sequence)
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
        || pending_from(&latest, expected.kind).is_some()
    {
        return Err(ApiError::Busy);
    }
    set_pending(&mut latest, expected.kind, passive);
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

fn set_pending(
    state: &mut crate::state::NnsStateV1,
    kind: MaturityKind,
    pending: PendingMaturityDisbursement,
) {
    match kind {
        MaturityKind::TwoYear => state.pending_two_year_maturity = Some(pending),
        MaturityKind::TwoWeek => state.pending_two_week_maturity = Some(pending),
    }
}

pub(crate) fn ensure_pending(
    kind: MaturityKind,
    expected: &PendingMaturityDisbursement,
) -> Result<(), ApiError> {
    if pending_from(&state::read(), kind).as_ref() == Some(expected) {
        Ok(())
    } else {
        Err(ApiError::Busy)
    }
}

pub(crate) fn replace_pending(
    kind: MaturityKind,
    expected: &PendingMaturityDisbursement,
    replacement: PendingMaturityDisbursement,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    if pending_from(&latest, kind).as_ref() != Some(expected) {
        return Err(ApiError::Busy);
    }
    set_pending(&mut latest, kind, replacement);
    state::write(latest);
    Ok(())
}

pub(crate) fn now_nanos() -> Result<u64, ApiError> {
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
