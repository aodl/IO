use candid::Principal;
use io_ledger_boundary::{exact_icp_transfer, icp_account_identifier, ExpectedQueryBlockTransfer};
use io_receipt_types::{
    ClaimBackingReceiptProgress, PrepareClaimBackingReceiptArgs, ProveClaimBackingReceiptArgs,
};

use crate::{
    api::{ApiError, MaturityProgress, PrepareTwoWeekMaturityArgs},
    execution,
    maturity::{
        ClaimReceiptDeliveryOperation, DisburseMaturitySubmission, DisburseMaturitySucceeded,
        MaturityCommandOperation, MaturityCommandPhase, MaturityEvidenceSource, MaturityKind,
        MaturityPlan, MintProofState, PendingMaturityDisbursement, PermanentCreditState,
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
            observed_maturity_e8s: observation.maturity_e8s,
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
        MaturityCommandPhase::Observed(plan) => submit_disburse(operation, plan).await,
        MaturityCommandPhase::DisburseMaturitySubmitted(submission) => {
            recover_disburse(operation, submission).await
        }
        MaturityCommandPhase::DisburseMaturitySucceeded(disbursement) => {
            canonicalize_disbursement(operation, disbursement).await
        }
        MaturityCommandPhase::ClaimReceiptDelivery(delivery) => {
            resume_claim_receipt_delivery(operation, delivery).await
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

async fn submit_disburse(
    mut operation: MaturityCommandOperation,
    plan: MaturityPlan,
) -> Result<MaturityProgress, ApiError> {
    let expected = operation.clone();
    let submission = DisburseMaturitySubmission {
        plan,
        submitted_at_seconds: now_seconds()?,
    };
    operation.dispatch_epoch = next_epoch(operation.dispatch_epoch)?;
    operation.phase = MaturityCommandPhase::DisburseMaturitySubmitted(submission.clone());
    write_exact(&expected, operation.clone(), false)?;
    let submitted = operation;
    let result = execution::disburse_maturity(
        &state::read().config,
        submission.plan.neuron.neuron_id,
        &submission.plan.destination,
    )
    .await;
    ensure_exact(&submitted)?;
    match result {
        execution::GovernanceCallOutcome::Succeeded(amount) => {
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
        execution::GovernanceCallOutcome::RejectedNoEffect(reason) => {
            let mut replacement = submitted.clone();
            replacement.phase = MaturityCommandPhase::Observed(submission.plan);
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
    submission: DisburseMaturitySubmission,
) -> Result<MaturityProgress, ApiError> {
    let observation = execution::query_neuron_observation(
        &state::read().config,
        submission.plan.neuron.neuron_id,
    )
    .await?;
    ensure_exact(&operation)?;
    let canonical = match execution::exact_maturity_disbursement(
        &observation,
        &submission.plan.destination,
        submission.submitted_at_seconds,
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
    replacement.phase =
        MaturityCommandPhase::DisburseMaturitySucceeded(DisburseMaturitySucceeded {
            amount_disbursed_e8s: canonical.amount_disbursed_e8s,
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
    let plan = &disbursement.submission.plan;
    let observation =
        execution::query_neuron_observation(&state::read().config, plan.neuron.neuron_id).await?;
    ensure_exact(&operation)?;
    let canonical = execution::exact_maturity_disbursement(
        &observation,
        &plan.destination,
        disbursement.submission.submitted_at_seconds,
    )?
    .ok_or_else(|| {
        ApiError::Pending("canonical maturity disbursement is not observable yet".into())
    })?;
    if canonical.amount_disbursed_e8s != disbursement.amount_disbursed_e8s {
        let reason = format!(
            "DisburseMaturity response amount {} conflicts with canonical amount {}",
            disbursement.amount_disbursed_e8s, canonical.amount_disbursed_e8s
        );
        write_exact(&operation, operation.clone(), true)?;
        return Err(ApiError::Stuck(reason));
    }
    let passive = PendingMaturityDisbursement {
        kind: operation.kind,
        neuron_id: plan.neuron.neuron_id,
        nominal_disbursed_maturity_e8s: canonical.amount_disbursed_e8s,
        destination: plan.destination.clone(),
        initiation_timestamp_seconds: canonical.initiated_at_seconds,
        scheduled_finalization_timestamp_seconds: canonical
            .scheduled_finalization_timestamp_seconds,
        disburse_evidence: disbursement,
        committed_claim_transfer_fee_e8s: 0,
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
            permanent_credit: None,
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
    if crate::claim_assets::maturity_delivery_has_unpaid_fee(&delivery) {
        let canonical_fee = io_ledger_boundary::icp_fee(config.icp_ledger)
            .await
            .map_err(ApiError::Pending)?;
        ensure_exact(&operation)?;
        if canonical_fee != delivery.pending.committed_claim_transfer_fee_e8s
            || canonical_fee != config.expected_icp_fee_e8s
        {
            let mut latest = state::read();
            latest.lifecycle = Lifecycle::Paused;
            state::write(latest);
            return Err(ApiError::Stuck(format!(
                "maturity transit fee drift: frozen {}, canonical {canonical_fee}",
                delivery.pending.committed_claim_transfer_fee_e8s
            )));
        }
    }
    let split = io_core_model::split_40_60(mint.actual_minted_icp_e8s)
        .map_err(|error| ApiError::Invalid(format!("maturity split failed: {error:?}")))?;
    let credit = split
        .permanent
        .checked_sub(delivery.pending.committed_claim_transfer_fee_e8s)
        .filter(|credit| *credit > 0)
        .ok_or_else(|| ApiError::Invalid("permanent maturity leg cannot pay its fee".into()))?;
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
                credit,
                delivery.pending.committed_claim_transfer_fee_e8s,
            );
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
            write_exact(&operation, replacement, false)?;
            return crate::permanent_credit::refresh(before.neuron_id).await;
        }
        Some(PermanentCreditState::RefreshSubmitted {
            before,
            transfer_block,
        }) => {
            return crate::permanent_credit::prove_or_refresh(
                operation,
                before.clone(),
                *transfer_block,
                credit,
            )
            .await;
        }
        Some(PermanentCreditState::Proved(_)) => {}
    }
    let Some(permit) = delivery.permit.clone() else {
        let observation = crate::api::claim_asset_observation().await?;
        ensure_exact(&operation)?;
        let (kind, claim_credit) = crate::claim_assets::claim_receipt_economics(
            &delivery.pending,
            mint.actual_minted_icp_e8s,
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
            return prepare_claim_transfer(operation, permit.destination.clone(), permit.amount_e8s)
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

fn prepare_claim_transfer(
    operation: MaturityCommandOperation,
    destination: crate::state::Account,
    amount: u128,
) -> Result<MaturityProgress, ApiError> {
    let config = state::read().config;
    let memo = delivery_ref(&operation)
        .permit
        .as_ref()
        .ok_or(ApiError::Busy)?
        .memo
        .clone();
    let attempt = NnsTransferAttempt::prepared(NnsTransferIntent {
        ledger: config.icp_ledger,
        source_subaccount: config
            .maturity_staging
            .canonical()
            .map_err(ApiError::Invalid)?
            .subaccount,
        destination,
        amount_e8s: amount,
        fee_e8s: delivery_ref(&operation)
            .pending
            .committed_claim_transfer_fee_e8s,
        memo,
        created_at_time_nanos: now_nanos()?,
    })
    .map_err(ApiError::Invalid)?;
    let mut replacement = operation.clone();
    delivery_mut(&mut replacement).claim_transfer = Some(attempt);
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

pub(crate) fn maturity_transfer_memo(domain: &[u8], sequence: u64) -> Vec<u8> {
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
        match delivery.permanent_credit.as_mut() {
            Some(PermanentCreditState::Prepared { transfer, .. }) => Some(transfer.as_mut()),
            _ => None,
        }
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
