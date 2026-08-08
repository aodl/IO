use candid::{CandidType, Principal};
pub use io_receipt_types::{
    CompleteLiquidReceiptArgs, CompletedReceiptResult, JupiterReceiptResult, LiquidReceiptPermit,
    PrepareLiquidReceiptArgs, ReceiptKind, TwoWeekReceiptResult,
};
use serde::Deserialize;

pub use crate::receipt_preparation::{
    receipt_memo, request_fingerprint, BackingSnapshot, ReceiptPreparation,
};

use crate::{
    state::{Account, DispatchEpoch, StreamConfig},
    transfer::{OwnTransferIntent, TransferAttempt, TransferState},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ReceiptPhase {
    AwaitingReceipt,
    ReceiptProved,
    Settling,
    Completed,
    Stuck,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ReceiptContext {
    pub request: PrepareLiquidReceiptArgs,
    pub request_fingerprint: Vec<u8>,
    pub source: Account,
    pub permit: LiquidReceiptPermit,
    pub backing_snapshot: BackingSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct JupiterSettlement {
    pub backed_io_e8s: u128,
    pub transfer: TransferAttempt,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TwoWeekSettlement {
    pub backed_io_pool_e8s: u128,
    pub recipients: Vec<RewardRecipient>,
    pub recipient_index: u32,
    pub distributed_io_e8s: u128,
    pub forfeited_io_e8s: u128,
    pub rounding_dust_io_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardRecipient {
    pub sns_neuron_id: Vec<u8>,
    pub destination: Account,
    pub io_e8s: u128,
    pub transfer: Option<TransferAttempt>,
    pub refresh_attempted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct JupiterReceiptOperation {
    pub context: ReceiptContext,
    pub phase: ReceiptPhase,
    pub receipt_block: Option<u128>,
    pub settlement: Option<JupiterSettlement>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TwoWeekReceiptOperation {
    pub context: ReceiptContext,
    pub phase: ReceiptPhase,
    pub receipt_block: Option<u128>,
    pub settlement: Option<TwoWeekSettlement>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum LiquidReceiptOperation {
    Jupiter(Box<JupiterReceiptOperation>),
    TwoWeek(Box<TwoWeekReceiptOperation>),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LastCompletedReceipt {
    pub request: PrepareLiquidReceiptArgs,
    pub request_fingerprint: Vec<u8>,
    pub permit: LiquidReceiptPermit,
    pub backing_snapshot: BackingSnapshot,
    pub receipt_block: u128,
    pub result: CompletedReceiptResult,
}

impl ReceiptContext {
    pub(crate) fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        if self.request.source_operation_id.is_empty()
            || self.request.source_operation_id.len() > 64
            || self.request.liquid_amount_e8s == 0
            || self.request_fingerprint.len() != 32
            || self.request_fingerprint != request_fingerprint(&self.request)
        {
            return Err("invalid canonical receipt request".into());
        }
        self.backing_snapshot.validate(config)?;
        let expected_source = match self.request.receipt_kind {
            ReceiptKind::Jupiter => &config.jupiter_receipt_source,
            ReceiptKind::TwoWeekMaturity => &config.two_week_receipt_source,
        };
        if !self.source.effective_eq(expected_source)?
            || self.permit.sequence != self.request.receipt_sequence
            || !self.permit.destination.effective_eq(&config.liquid_icp)?
            || self.permit.memo != receipt_memo(config.nns_manager, self.request.receipt_sequence)
        {
            return Err("receipt context does not match immutable configuration".into());
        }
        match self.request.receipt_kind {
            ReceiptKind::Jupiter if self.request.entitlement_batch_generation.is_some() => {
                Err("Jupiter receipt cannot name an entitlement batch".into())
            }
            ReceiptKind::TwoWeekMaturity if self.request.entitlement_batch_generation.is_none() => {
                Err("two-week receipt must name its entitlement batch".into())
            }
            _ => Ok(()),
        }
    }
}

impl LiquidReceiptOperation {
    pub fn context(&self) -> &ReceiptContext {
        match self {
            Self::Jupiter(operation) => &operation.context,
            Self::TwoWeek(operation) => &operation.context,
        }
    }

    pub fn phase(&self) -> ReceiptPhase {
        match self {
            Self::Jupiter(operation) => operation.phase,
            Self::TwoWeek(operation) => operation.phase,
        }
    }

    pub fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        match self {
            Self::Jupiter(operation) => operation.validate(config),
            Self::TwoWeek(operation) => operation.validate(config),
        }
    }
}

impl JupiterReceiptOperation {
    fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        self.context.validate(config)?;
        if self.context.request.receipt_kind != ReceiptKind::Jupiter {
            return Err("Jupiter operation has the wrong receipt kind".into());
        }
        validate_phase_proof(self.phase, self.receipt_block)?;
        match (&self.phase, &self.settlement) {
            (ReceiptPhase::AwaitingReceipt | ReceiptPhase::ReceiptProved, None) => Ok(()),
            (ReceiptPhase::Settling, Some(settlement)) => {
                settlement.validate(&self.context, config)?;
                if matches!(
                    settlement.transfer.state,
                    TransferState::Submitted { .. } | TransferState::Succeeded { .. }
                ) {
                    Ok(())
                } else {
                    Err("settling Jupiter transfer has the wrong state".into())
                }
            }
            (ReceiptPhase::Stuck, Some(settlement)) => {
                settlement.validate(&self.context, config)?;
                if matches!(settlement.transfer.state, TransferState::Stuck { .. }) {
                    Ok(())
                } else {
                    Err("Stuck Jupiter operation lacks a Stuck transfer".into())
                }
            }
            _ => Err("Jupiter receipt phase and settlement disagree".into()),
        }
    }
}

impl JupiterSettlement {
    fn validate(&self, context: &ReceiptContext, config: &StreamConfig) -> Result<(), String> {
        if self.backed_io_e8s == 0 {
            return Err("Jupiter settlement amount must be positive".into());
        }
        self.transfer.validate()?;
        match &self.transfer.intent {
            OwnTransferIntent::Icrc1 {
                ledger,
                from_subaccount,
                to,
                amount,
                fee,
                memo,
                ..
            } if *ledger == config.io_ledger
                && *from_subaccount == config.io_reserve.canonical()?.subaccount
                && to.effective_eq(&config.jupiter_io_account)?
                && *amount == self.backed_io_e8s
                && *fee == config.expected_io_fee_e8s
                && *memo == context.permit.memo =>
            {
                Ok(())
            }
            _ => Err("Jupiter settlement intent does not match receipt".into()),
        }
    }
}

impl TwoWeekReceiptOperation {
    fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        self.context.validate(config)?;
        if self.context.request.receipt_kind != ReceiptKind::TwoWeekMaturity {
            return Err("two-week operation has the wrong receipt kind".into());
        }
        validate_phase_proof(self.phase, self.receipt_block)?;
        match (&self.phase, &self.settlement) {
            (ReceiptPhase::AwaitingReceipt | ReceiptPhase::ReceiptProved, None) => Ok(()),
            (ReceiptPhase::Settling | ReceiptPhase::Completed, Some(settlement)) => {
                crate::reward_settlement::validate(settlement, config)
            }
            _ => Err("two-week receipt phase and settlement disagree".into()),
        }
    }
}

fn validate_phase_proof(phase: ReceiptPhase, block: Option<u128>) -> Result<(), String> {
    let requires_proof = matches!(
        phase,
        ReceiptPhase::ReceiptProved
            | ReceiptPhase::Settling
            | ReceiptPhase::Completed
            | ReceiptPhase::Stuck
    );
    if block.is_some() == requires_proof {
        Ok(())
    } else {
        Err("receipt phase and exact block proof disagree".into())
    }
}

pub async fn prepare_liquid_receipt(
    caller: Principal,
    args: PrepareLiquidReceiptArgs,
    now: u64,
) -> Result<LiquidReceiptPermit, crate::api::ApiError> {
    use crate::{api::ApiError, canonical, state};
    let mut current = state::read();
    if caller != current.config.nns_manager {
        return Err(ApiError::Unauthorized);
    }
    let fingerprint = request_fingerprint(&args);
    if let Some(completed) = &current.last_completed_receipt {
        if completed.request_fingerprint == fingerprint {
            return Ok(completed.permit.clone());
        }
        if completed.request.receipt_sequence == args.receipt_sequence {
            return Err(ApiError::NonceAlreadyUsed);
        }
    }
    if let Some(state::StreamOperation::LiquidReceipt(operation)) = &current.active_operation {
        match operation.as_ref() {
            state::LiquidReceiptStreamOperation::Active(existing) => {
                if existing.context().request_fingerprint == fingerprint {
                    return Ok(existing.context().permit.clone());
                }
                if existing.context().request.receipt_sequence == args.receipt_sequence {
                    return Err(ApiError::NonceAlreadyUsed);
                }
            }
            state::LiquidReceiptStreamOperation::Preparing(existing) => {
                if existing.request.receipt_sequence == args.receipt_sequence
                    && existing.request_fingerprint != fingerprint
                {
                    return Err(ApiError::NonceAlreadyUsed);
                }
                if existing.request_fingerprint != fingerprint || existing.authority != caller {
                    return Err(ApiError::Busy);
                }
            }
        }
    }
    crate::api::require_ready(&current)?;
    if current.active_operation.is_some()
        && !matches!(
            &current.active_operation,
            Some(state::StreamOperation::LiquidReceipt(operation))
                if matches!(operation.as_ref(), state::LiquidReceiptStreamOperation::Preparing(_))
        )
    {
        return Err(ApiError::Busy);
    }
    if args.receipt_sequence != current.next_nns_receipt_sequence
        || args.liquid_amount_e8s == 0
        || args.source_operation_id.is_empty()
        || args.source_operation_id.len() > 64
    {
        return Err(ApiError::Invalid(
            "invalid receipt sequence or bounded intent".into(),
        ));
    }
    match args.receipt_kind {
        ReceiptKind::Jupiter if args.entitlement_batch_generation.is_some() => {
            return Err(ApiError::Invalid(
                "only two-week maturity names an entitlement batch".into(),
            ))
        }
        ReceiptKind::TwoWeekMaturity
            if current.pending_entitlement_status
                != state::PendingEntitlementStatus::MaturityPrepared
                || args.entitlement_batch_generation
                    != current
                        .pending_entitlement_batch
                        .as_ref()
                        .map(|batch| batch.generation) =>
        {
            return Err(ApiError::Invalid(
                "receipt does not match pending entitlement batch".into(),
            ))
        }
        _ => {}
    }
    let preparation = ReceiptPreparation {
        request: args.clone(),
        request_fingerprint: fingerprint.clone(),
        authority: caller,
        captured_control_epoch: current.control_epoch,
        prepared_at_nanos: now,
    };
    preparation
        .validate(&current.config)
        .map_err(ApiError::Invalid)?;
    current.active_operation = Some(state::StreamOperation::LiquidReceipt(Box::new(
        state::LiquidReceiptStreamOperation::Preparing(Box::new(preparation.clone())),
    )));
    state::write(current.clone());

    let canonical = match canonical::redemption_snapshot(&current.config).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            clear_matching_preparation(&preparation);
            return Err(ApiError::Ledger(error));
        }
    };
    if canonical.io_fee_e8s != current.config.expected_io_fee_e8s
        || canonical.icp_fee_e8s != current.config.expected_icp_fee_e8s
    {
        clear_matching_preparation(&preparation);
        return Err(ApiError::Invalid(
            "canonical fee differs from approved receipt configuration".into(),
        ));
    }
    let observed_at_nanos = ic_cdk::api::time().max(now);
    let backing_snapshot = BackingSnapshot {
        total_io_supply_e8s: canonical.total_supply_e8s,
        reserve_io_e8s: canonical.reserve_io_e8s,
        excluded_io_balances: canonical.excluded_io_balances,
        liquid_icp_e8s: canonical.liquid_icp_e8s,
        io_fee_e8s: canonical.io_fee_e8s,
        observed_at_nanos,
    };
    backing_snapshot
        .validate(&current.config)
        .map_err(ApiError::Invalid)?;

    let mut latest = state::read();
    if latest.config != current.config
        || latest.control_epoch != preparation.captured_control_epoch
        || !matches!(
            &latest.active_operation,
            Some(state::StreamOperation::LiquidReceipt(active))
                if matches!(active.as_ref(), state::LiquidReceiptStreamOperation::Preparing(value) if **value == preparation)
        )
    {
        return Err(ApiError::Busy);
    }
    let permit = LiquidReceiptPermit {
        sequence: args.receipt_sequence,
        destination: current.config.liquid_icp.clone(),
        memo: receipt_memo(caller, args.receipt_sequence),
    };
    let context = ReceiptContext {
        source: match args.receipt_kind {
            ReceiptKind::Jupiter => current.config.jupiter_receipt_source.clone(),
            ReceiptKind::TwoWeekMaturity => current.config.two_week_receipt_source.clone(),
        },
        request: args,
        request_fingerprint: fingerprint,
        permit: permit.clone(),
        backing_snapshot,
    };
    let operation = match context.request.receipt_kind {
        ReceiptKind::Jupiter => {
            LiquidReceiptOperation::Jupiter(Box::new(JupiterReceiptOperation {
                context,
                phase: ReceiptPhase::AwaitingReceipt,
                receipt_block: None,
                settlement: None,
            }))
        }
        ReceiptKind::TwoWeekMaturity => {
            LiquidReceiptOperation::TwoWeek(Box::new(TwoWeekReceiptOperation {
                context,
                phase: ReceiptPhase::AwaitingReceipt,
                receipt_block: None,
                settlement: None,
            }))
        }
    };
    operation
        .validate(&latest.config)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(state::StreamOperation::LiquidReceipt(Box::new(
        state::LiquidReceiptStreamOperation::Active(Box::new(operation)),
    )));
    state::write(latest);
    Ok(permit)
}

fn clear_matching_preparation(expected: &ReceiptPreparation) {
    use crate::state::{self, LiquidReceiptStreamOperation, StreamOperation};
    let mut latest = state::read();
    if matches!(
        &latest.active_operation,
        Some(StreamOperation::LiquidReceipt(active))
            if matches!(active.as_ref(), LiquidReceiptStreamOperation::Preparing(value) if **value == *expected)
    ) {
        latest.active_operation = None;
        state::write(latest);
    }
}

pub(crate) async fn resume_liquid_receipt(
    operation: LiquidReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, crate::api::ApiError> {
    match operation {
        LiquidReceiptOperation::Jupiter(operation) => resume_jupiter(*operation, now).await,
        LiquidReceiptOperation::TwoWeek(operation) => {
            crate::rewards::resume_two_week(*operation, now).await
        }
    }
}

async fn resume_jupiter(
    operation: JupiterReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, crate::api::ApiError> {
    use crate::api::{ApiError, LiquidReceiptProgress};
    match operation.phase {
        ReceiptPhase::AwaitingReceipt => Ok(LiquidReceiptProgress::AwaitingReceipt),
        ReceiptPhase::ReceiptProved => prepare_jupiter_settlement(operation, now).await,
        ReceiptPhase::Settling => {
            let settlement = operation
                .settlement
                .as_ref()
                .ok_or_else(|| ApiError::Invalid("Jupiter settlement is missing".into()))?;
            match settlement.transfer.state {
                TransferState::Succeeded { .. } => complete_jupiter(operation, now),
                TransferState::Submitted { .. } => retry_jupiter_settlement(operation, now).await,
                _ => Err(ApiError::Invalid(
                    "Jupiter settling transfer has invalid state".into(),
                )),
            }
        }
        ReceiptPhase::Stuck => Err(ApiError::Stuck(
            "exact Jupiter IO transfer proof is required".into(),
        )),
        ReceiptPhase::Completed => Err(ApiError::Invalid(
            "completed receipt should be available through replay".into(),
        )),
    }
}

async fn prepare_jupiter_settlement(
    operation: JupiterReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, crate::api::ApiError> {
    use crate::{api::ApiError, canonical, state};
    let config = state::read().config;
    let snapshot = canonical::redemption_snapshot(&config)
        .await
        .map_err(ApiError::Ledger)?;
    crate::receipt_preparation::validate_post_receipt_snapshot(
        &operation.context.backing_snapshot,
        &snapshot,
        operation.context.request.liquid_amount_e8s,
        0,
    )?;
    let excluded = operation
        .context
        .backing_snapshot
        .excluded_io_balances
        .iter()
        .try_fold(0u128, |sum, (_, value)| sum.checked_add(*value))
        .ok_or_else(|| ApiError::Invalid("excluded balance overflow".into()))?;
    let redeemable = operation
        .context
        .backing_snapshot
        .total_io_supply_e8s
        .checked_sub(operation.context.backing_snapshot.reserve_io_e8s)
        .and_then(|value| value.checked_sub(excluded))
        .ok_or_else(|| ApiError::Invalid("invalid redeemable supply".into()))?;
    let backed_io = io_core_model::backed_io(
        operation.context.request.liquid_amount_e8s,
        operation.context.backing_snapshot.liquid_icp_e8s,
        redeemable,
    )
    .map_err(|error| ApiError::Invalid(format!("backed IO calculation failed: {error:?}")))?;
    let intent = OwnTransferIntent::Icrc1 {
        ledger: config.io_ledger,
        from_subaccount: config
            .io_reserve
            .canonical()
            .map_err(ApiError::Invalid)?
            .subaccount,
        to: config.jupiter_io_account,
        amount: backed_io,
        fee: operation.context.backing_snapshot.io_fee_e8s,
        memo: operation.context.permit.memo.clone(),
        created_at_time: now,
    };
    now.checked_add(config.ledger_deduplication_window_nanos)
        .ok_or_else(|| ApiError::Invalid("settlement deduplication deadline overflow".into()))?;
    let reserve_required = backed_io
        .checked_add(operation.context.backing_snapshot.io_fee_e8s)
        .ok_or_else(|| ApiError::Invalid("Jupiter reserve requirement overflow".into()))?;
    if snapshot.reserve_io_e8s < reserve_required {
        return Err(ApiError::Invalid(
            "IO reserve does not cover Jupiter backing plus fee".into(),
        ));
    }
    let mut transfer = TransferAttempt::prepared(intent).map_err(ApiError::Invalid)?;
    transfer.state = TransferState::Submitted {
        epoch: DispatchEpoch(1),
        first_submitted_at: now,
        last_submitted_at: now,
    };
    let fingerprint = transfer.fingerprint.clone();
    let intent = transfer.intent.clone();
    let mut submitted = operation.clone();
    submitted.phase = ReceiptPhase::Settling;
    submitted.settlement = Some(JupiterSettlement {
        backed_io_e8s: backed_io,
        transfer,
    });
    persist_exact(
        &LiquidReceiptOperation::Jupiter(Box::new(operation)),
        LiquidReceiptOperation::Jupiter(Box::new(submitted.clone())),
    )?;
    let response = crate::api::submit(&intent).await;
    apply_jupiter_callback(
        submitted.context.request.receipt_sequence,
        submitted.context.request_fingerprint,
        fingerprint,
        DispatchEpoch(1),
        response,
    )
}

async fn retry_jupiter_settlement(
    operation: JupiterReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, crate::api::ApiError> {
    use crate::api::ApiError;
    let config = crate::state::read().config;
    let settlement = operation
        .settlement
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("Jupiter settlement is missing".into()))?;
    let next_epoch = match retry_decision(
        &settlement.transfer,
        now,
        config.retry_delay_nanos,
        config.ledger_deduplication_window_nanos,
    )
    .map_err(ApiError::Invalid)?
    {
        RetryDecision::Wait => return Ok(crate::api::LiquidReceiptProgress::Settling),
        RetryDecision::Expired => {
            let mut stuck = operation.clone();
            stuck.phase = ReceiptPhase::Stuck;
            stuck
                .settlement
                .as_mut()
                .expect("validated Jupiter settlement")
                .transfer
                .state = TransferState::Stuck {
                reason: "Jupiter IO settlement deduplication window expired".into(),
            };
            persist_exact(
                &LiquidReceiptOperation::Jupiter(Box::new(operation)),
                LiquidReceiptOperation::Jupiter(Box::new(stuck)),
            )?;
            crate::api::pause();
            return Err(ApiError::Stuck(
                "Jupiter IO settlement deduplication window expired".into(),
            ));
        }
        RetryDecision::Dispatch(epoch) => epoch,
    };
    let mut submitted = operation.clone();
    let transfer = &mut submitted
        .settlement
        .as_mut()
        .expect("validated Jupiter settlement")
        .transfer;
    let first_submitted_at = match transfer.state {
        TransferState::Submitted {
            first_submitted_at, ..
        } => first_submitted_at,
        _ => return Err(ApiError::Busy),
    };
    transfer.state = TransferState::Submitted {
        epoch: next_epoch,
        first_submitted_at,
        last_submitted_at: now,
    };
    let fingerprint = transfer.fingerprint.clone();
    let intent = transfer.intent.clone();
    persist_exact(
        &LiquidReceiptOperation::Jupiter(Box::new(operation)),
        LiquidReceiptOperation::Jupiter(Box::new(submitted.clone())),
    )?;
    let response = crate::api::submit(&intent).await;
    apply_jupiter_callback(
        submitted.context.request.receipt_sequence,
        submitted.context.request_fingerprint,
        fingerprint,
        next_epoch,
        response,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RetryDecision {
    Wait,
    Dispatch(DispatchEpoch),
    Expired,
}

fn retry_decision(
    transfer: &TransferAttempt,
    now: u64,
    retry_delay_nanos: u64,
    deduplication_window_nanos: u64,
) -> Result<RetryDecision, String> {
    let (epoch, last_submitted_at) = match transfer.state {
        TransferState::Submitted {
            epoch,
            last_submitted_at,
            ..
        } => (epoch, last_submitted_at),
        _ => return Err("retry requires a submitted transfer".into()),
    };
    if now
        .checked_sub(last_submitted_at)
        .ok_or("receipt retry clock regressed")?
        < retry_delay_nanos
    {
        return Ok(RetryDecision::Wait);
    }
    let deadline = transfer
        .intent
        .created_at_time()
        .checked_add(deduplication_window_nanos)
        .ok_or("settlement deduplication deadline overflow")?;
    if now >= deadline {
        return Ok(RetryDecision::Expired);
    }
    Ok(RetryDecision::Dispatch(DispatchEpoch(
        epoch.0.checked_add(1).ok_or("dispatch epoch overflow")?,
    )))
}

fn apply_jupiter_callback(
    sequence: u64,
    request_fingerprint: Vec<u8>,
    transfer_fingerprint: Vec<u8>,
    epoch: DispatchEpoch,
    response: Result<crate::transfer::TransferResult, String>,
) -> Result<crate::api::LiquidReceiptProgress, crate::api::ApiError> {
    use crate::{
        api::{ApiError, LiquidReceiptProgress},
        transfer::{classify_result, ClassifiedResult},
    };
    let current = active_jupiter()?;
    if current.context.request.receipt_sequence != sequence
        || current.context.request_fingerprint != request_fingerprint
        || current.phase != ReceiptPhase::Settling
    {
        return Err(ApiError::Busy);
    }
    let transfer = &current
        .settlement
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("Jupiter settlement is missing".into()))?
        .transfer;
    if transfer.fingerprint != transfer_fingerprint
        || !matches!(transfer.state, TransferState::Submitted { epoch: current, .. } if current == epoch)
    {
        return Err(ApiError::Busy);
    }
    let classified = match response {
        Ok(value) => classify_result(value).map_err(ApiError::Ledger)?,
        Err(error) => return Err(ApiError::Pending(error)),
    };
    let mut updated = current.clone();
    let transfer = &mut updated
        .settlement
        .as_mut()
        .expect("validated Jupiter settlement")
        .transfer;
    match classified {
        ClassifiedResult::Succeeded(block) => {
            transfer.state = TransferState::Succeeded { block };
            persist_exact(
                &LiquidReceiptOperation::Jupiter(Box::new(current)),
                LiquidReceiptOperation::Jupiter(Box::new(updated)),
            )?;
            Ok(LiquidReceiptProgress::Settling)
        }
        ClassifiedResult::NoEffect(error) => {
            transfer.state = TransferState::Stuck {
                reason: error.clone(),
            };
            updated.phase = ReceiptPhase::Stuck;
            persist_exact(
                &LiquidReceiptOperation::Jupiter(Box::new(current)),
                LiquidReceiptOperation::Jupiter(Box::new(updated)),
            )?;
            crate::api::pause();
            Err(ApiError::Stuck(error))
        }
        ClassifiedResult::Ambiguous(error) => Err(ApiError::Pending(error)),
    }
}

fn complete_jupiter(
    operation: JupiterReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, crate::api::ApiError> {
    use crate::{api::ApiError, state};
    let settlement = operation
        .settlement
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("Jupiter settlement is missing".into()))?;
    let io_transfer_block = settlement
        .transfer
        .succeeded_block()
        .map_err(ApiError::Invalid)?;
    let receipt_block = operation
        .receipt_block
        .ok_or_else(|| ApiError::Invalid("receipt proof is missing".into()))?;
    let result = CompletedReceiptResult::Jupiter(JupiterReceiptResult {
        request_fingerprint: operation.context.request_fingerprint.clone(),
        receipt_block,
        backed_io_e8s: settlement.backed_io_e8s,
        io_transfer_block,
        io_fee_e8s: state::read().config.expected_io_fee_e8s,
        completed_at_nanos: now,
    });
    let expected = LiquidReceiptOperation::Jupiter(Box::new(operation.clone()));
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(state::StreamOperation::LiquidReceipt(current))
        if matches!(current.as_ref(), state::LiquidReceiptStreamOperation::Active(value) if **value == expected))
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
    state::write(latest);
    Ok(crate::api::LiquidReceiptProgress::Completed(result))
}

fn active_jupiter() -> Result<JupiterReceiptOperation, crate::api::ApiError> {
    match crate::state::read().active_operation {
        Some(crate::state::StreamOperation::LiquidReceipt(operation)) => match *operation {
            crate::state::LiquidReceiptStreamOperation::Active(operation) => match *operation {
                LiquidReceiptOperation::Jupiter(operation) => Ok(*operation),
                LiquidReceiptOperation::TwoWeek(_) => Err(crate::api::ApiError::Busy),
            },
            crate::state::LiquidReceiptStreamOperation::Preparing(_) => {
                Err(crate::api::ApiError::Busy)
            }
        },
        _ => Err(crate::api::ApiError::Busy),
    }
}

pub(crate) fn persist_exact(
    expected: &LiquidReceiptOperation,
    replacement: LiquidReceiptOperation,
) -> Result<(), crate::api::ApiError> {
    use crate::{api::ApiError, state};
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(state::StreamOperation::LiquidReceipt(current))
        if matches!(current.as_ref(), state::LiquidReceiptStreamOperation::Active(value) if **value == *expected))
    {
        return Err(ApiError::Busy);
    }
    replacement
        .validate(&latest.config)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(state::StreamOperation::LiquidReceipt(Box::new(
        state::LiquidReceiptStreamOperation::Active(Box::new(replacement)),
    )));
    state::write(latest);
    Ok(())
}

pub async fn complete_liquid_receipt(
    caller: Principal,
    args: CompleteLiquidReceiptArgs,
) -> Result<crate::api::LiquidReceiptProgress, crate::api::ApiError> {
    use crate::{
        api::{ApiError, LiquidReceiptProgress},
        canonical, state,
    };
    let snapshot = state::read();
    if caller != snapshot.config.nns_manager {
        return Err(ApiError::Unauthorized);
    }
    if let Some(completed) = &snapshot.last_completed_receipt {
        if completed.request.receipt_sequence == args.receipt_sequence {
            if completed.receipt_block == args.block_index {
                return Ok(LiquidReceiptProgress::Completed(completed.result.clone()));
            }
            return Err(ApiError::Invalid(
                "conflicting completed receipt block".into(),
            ));
        }
    }
    let operation = match snapshot.active_operation {
        Some(state::StreamOperation::LiquidReceipt(operation)) => match *operation {
            state::LiquidReceiptStreamOperation::Active(operation)
                if operation.context().request.receipt_sequence == args.receipt_sequence =>
            {
                *operation
            }
            _ => return Err(ApiError::Invalid("no matching liquid receipt".into())),
        },
        _ => return Err(ApiError::Invalid("no matching liquid receipt".into())),
    };
    let context = operation.context();
    let existing_block = match &operation {
        LiquidReceiptOperation::Jupiter(value) => value.receipt_block,
        LiquidReceiptOperation::TwoWeek(value) => value.receipt_block,
    };
    if existing_block == Some(args.block_index) {
        return Ok(crate::reward_settlement::receipt_progress(&operation));
    }
    if existing_block.is_some() {
        return Err(ApiError::Invalid("conflicting receipt block".into()));
    }
    let transfer = canonical::exact_icp_transfer(snapshot.config.icp_ledger, args.block_index)
        .await
        .map_err(ApiError::Ledger)?;
    if transfer.from
        != canonical::icp_account_identifier(&context.source).map_err(ApiError::Invalid)?
        || transfer.to
            != canonical::icp_account_identifier(&context.permit.destination)
                .map_err(ApiError::Invalid)?
        || transfer.amount_e8s != context.request.liquid_amount_e8s
        || transfer.fee_e8s != snapshot.config.expected_icp_fee_e8s
        || transfer.native_memo_u64 != 0
        || transfer.icrc1_memo.as_deref() != Some(context.permit.memo.as_slice())
        || transfer.created_at_time == 0
        || transfer.spender.is_some()
    {
        return Err(ApiError::Invalid(
            "canonical block does not match receipt intent".into(),
        ));
    }
    let replacement = match &operation {
        LiquidReceiptOperation::Jupiter(value) if value.phase == ReceiptPhase::AwaitingReceipt => {
            let mut value = value.clone();
            value.receipt_block = Some(args.block_index);
            value.phase = ReceiptPhase::ReceiptProved;
            LiquidReceiptOperation::Jupiter(value)
        }
        LiquidReceiptOperation::TwoWeek(value) if value.phase == ReceiptPhase::AwaitingReceipt => {
            let mut value = value.clone();
            value.receipt_block = Some(args.block_index);
            value.phase = ReceiptPhase::ReceiptProved;
            LiquidReceiptOperation::TwoWeek(value)
        }
        _ => return Err(ApiError::Busy),
    };
    persist_exact(&operation, replacement)?;
    Ok(LiquidReceiptProgress::ReceiptProved)
}

pub async fn prove_jupiter_settlement(block_index: u128) -> Result<(), crate::api::ApiError> {
    use crate::{api::ApiError, canonical};
    let operation = active_jupiter()?;
    if operation.phase != ReceiptPhase::Stuck {
        return Err(ApiError::Invalid(
            "only a Stuck Jupiter settlement accepts proof".into(),
        ));
    }
    let settlement = operation
        .settlement
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("Jupiter settlement is missing".into()))?;
    if !matches!(settlement.transfer.state, TransferState::Stuck { .. }) {
        return Err(ApiError::Invalid(
            "Jupiter settlement transfer is not Stuck".into(),
        ));
    }
    let exact = canonical::exact_icrc_transfer(settlement.transfer.intent.ledger(), block_index)
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
    } = &settlement.transfer.intent
    else {
        return Err(ApiError::Invalid(
            "Jupiter settlement has wrong intent kind".into(),
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
            "exact block does not match Stuck Jupiter settlement".into(),
        ));
    }
    let mut succeeded = operation.clone();
    succeeded.phase = ReceiptPhase::Settling;
    succeeded
        .settlement
        .as_mut()
        .expect("validated Jupiter settlement")
        .transfer
        .state = TransferState::Succeeded { block: block_index };
    persist_exact(
        &LiquidReceiptOperation::Jupiter(Box::new(operation)),
        LiquidReceiptOperation::Jupiter(Box::new(succeeded)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn submitted_transfer(
        created_at_time: u64,
        first_submitted_at: u64,
        last_submitted_at: u64,
        epoch: u64,
    ) -> TransferAttempt {
        let mut attempt = TransferAttempt::prepared(OwnTransferIntent::Icrc1 {
            ledger: Principal::from_slice(&[1]),
            from_subaccount: [0; 32],
            to: Account {
                owner: Principal::from_slice(&[2]),
                subaccount: None,
            },
            amount: 10,
            fee: 1,
            memo: vec![3],
            created_at_time,
        })
        .unwrap();
        attempt.state = TransferState::Submitted {
            epoch: DispatchEpoch(epoch),
            first_submitted_at,
            last_submitted_at,
        };
        attempt
    }

    #[test]
    fn typed_completed_result_rejects_kind_mismatch() {
        let request = PrepareLiquidReceiptArgs {
            receipt_sequence: 4,
            receipt_kind: ReceiptKind::Jupiter,
            source_operation_id: vec![1],
            liquid_amount_e8s: 10,
            entitlement_batch_generation: None,
        };
        assert_ne!(
            std::mem::discriminant(&CompletedReceiptResult::Jupiter(JupiterReceiptResult {
                request_fingerprint: request_fingerprint(&request),
                receipt_block: 1,
                backed_io_e8s: 2,
                io_transfer_block: 3,
                io_fee_e8s: 1,
                completed_at_nanos: 4,
            })),
            std::mem::discriminant(&CompletedReceiptResult::TwoWeek(TwoWeekReceiptResult {
                request_fingerprint: request_fingerprint(&request),
                receipt_block: 1,
                backed_io_pool_e8s: 2,
                distributed_io_e8s: 2,
                forfeited_io_e8s: 0,
                rounding_dust_io_e8s: 0,
                completed_at_nanos: 4,
            }))
        );
    }

    #[test]
    fn jupiter_retry_reuses_identity_and_expires_from_intent_creation() {
        let transfer = submitted_transfer(100, 140, 145, 1);
        assert_eq!(
            retry_decision(&transfer, 149, 5, 50),
            Ok(RetryDecision::Wait)
        );
        assert_eq!(
            retry_decision(&transfer, 149, 4, 50),
            Ok(RetryDecision::Dispatch(DispatchEpoch(2)))
        );
        assert_eq!(
            retry_decision(&transfer, 150, 4, 50),
            Ok(RetryDecision::Expired)
        );
        let retry = transfer.clone();
        assert_eq!(retry.intent, transfer.intent);
        assert_eq!(retry.fingerprint, transfer.fingerprint);
    }
}
