use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::{
    state::Account,
    transfer::{OwnTransferIntent, TransferAttempt, TransferState},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ReceiptKind {
    Jupiter,
    TwoWeekMaturity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ReceiptPhase {
    Prepared,
    AwaitingReceipt,
    ReceiptProved,
    Settling,
    Completed,
    Stuck,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PrepareLiquidReceiptArgs {
    pub receipt_sequence: u64,
    pub receipt_kind: ReceiptKind,
    pub source_operation_id: Vec<u8>,
    pub liquid_amount_e8s: u128,
    pub cohort_generation: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LiquidReceiptPermit {
    pub sequence: u64,
    pub destination: Account,
    pub memo: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CompleteLiquidReceiptArgs {
    pub receipt_sequence: u64,
    pub block_index: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LiquidReceiptOperation {
    pub request_fingerprint: Vec<u8>,
    pub sequence: u64,
    pub kind: ReceiptKind,
    pub source_operation_id: Vec<u8>,
    pub liquid_amount_e8s: u128,
    pub cohort_generation: Option<u64>,
    pub source: Account,
    pub destination: Account,
    pub memo: Vec<u8>,
    pub phase: ReceiptPhase,
    pub proved_block: Option<u128>,
    pub active_transfer: Option<TransferAttempt>,
    pub recipient_index: u32,
    pub settlement_result: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LastCompletedReceipt {
    pub request_fingerprint: Vec<u8>,
    pub permit: LiquidReceiptPermit,
    pub receipt_block: u128,
    pub settlement_result: Vec<u8>,
    pub completed_at_nanos: u64,
}

impl LiquidReceiptOperation {
    pub fn validate(&self, config: &crate::state::StreamConfig) -> Result<(), String> {
        if self.source_operation_id.is_empty() || self.source_operation_id.len() > 64 {
            return Err("invalid receipt source operation id".into());
        }
        if self.request_fingerprint.len() != 32 {
            return Err("invalid receipt request fingerprint".into());
        }
        let request = PrepareLiquidReceiptArgs {
            receipt_sequence: self.sequence,
            receipt_kind: self.kind,
            source_operation_id: self.source_operation_id.clone(),
            liquid_amount_e8s: self.liquid_amount_e8s,
            cohort_generation: self.cohort_generation,
        };
        if self.request_fingerprint != request_fingerprint(&request) {
            return Err("receipt request fingerprint does not match operation".into());
        }
        if self.liquid_amount_e8s == 0 || self.memo.is_empty() || self.memo.len() > 32 {
            return Err("invalid receipt amount or memo".into());
        }
        let expected = match self.kind {
            ReceiptKind::Jupiter => &config.jupiter_receipt_source,
            ReceiptKind::TwoWeekMaturity => &config.two_week_receipt_source,
        };
        if !self.source.effective_eq(expected)?
            || !self.destination.effective_eq(&config.liquid_icp)?
        {
            return Err("receipt accounts do not match kind configuration".into());
        }
        if let Some(attempt) = &self.active_transfer {
            attempt.validate()?;
            if attempt.intent.ledger() != config.io_ledger || self.kind != ReceiptKind::Jupiter {
                return Err("receipt settlement transfer is incompatible with kind".into());
            }
            let backed_io: u128 = candid::decode_one(
                self.settlement_result
                    .as_deref()
                    .ok_or("Jupiter settlement result is missing")?,
            )
            .map_err(|error| format!("Jupiter settlement result is invalid: {error}"))?;
            match &attempt.intent {
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
                    && *amount == backed_io
                    && *fee == config.expected_io_fee_e8s
                    && *memo == self.memo => {}
                _ => return Err("Jupiter settlement intent does not match receipt".into()),
            }
        }
        if self.proved_block.is_some()
            != matches!(
                self.phase,
                ReceiptPhase::ReceiptProved
                    | ReceiptPhase::Settling
                    | ReceiptPhase::Completed
                    | ReceiptPhase::Stuck
            )
        {
            return Err("receipt phase/proof mismatch".into());
        }
        match self.phase {
            ReceiptPhase::Prepared | ReceiptPhase::AwaitingReceipt
                if self.active_transfer.is_some()
                    || self.settlement_result.is_some()
                    || self.recipient_index != 0 =>
            {
                return Err("awaiting receipt contains settlement state".into())
            }
            ReceiptPhase::ReceiptProved
                if self.active_transfer.is_some()
                    || self.settlement_result.is_some()
                    || self.recipient_index != 0 =>
            {
                return Err("proved receipt contains premature settlement state".into())
            }
            ReceiptPhase::Settling
                if self.kind == ReceiptKind::Jupiter
                    && !matches!(
                        self.active_transfer.as_ref().map(|attempt| &attempt.state),
                        Some(TransferState::Submitted { .. })
                            | Some(TransferState::Succeeded { .. })
                    ) =>
            {
                return Err("settling Jupiter receipt has incompatible transfer state".into())
            }
            ReceiptPhase::Completed if self.settlement_result.is_none() => {
                return Err("completed receipt lacks its exact result".into())
            }
            ReceiptPhase::Stuck
                if self.active_transfer.as_ref().is_some_and(|attempt| {
                    !matches!(attempt.state, TransferState::Stuck { .. })
                }) =>
            {
                return Err("Stuck receipt has a non-Stuck transfer".into())
            }
            _ => {}
        }
        Ok(())
    }
}

pub fn request_fingerprint(args: &PrepareLiquidReceiptArgs) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"io-liquid-receipt-request-v1\0");
    hasher.update(candid::encode_one(args).expect("receipt request must encode"));
    hasher.finalize().to_vec()
}

pub fn receipt_memo(manager: Principal, sequence: u64) -> Vec<u8> {
    crate::transfer::deterministic_memo(b"io-liquid-receipt-v1", manager, sequence)
}

pub fn prepare_liquid_receipt(
    caller: Principal,
    args: PrepareLiquidReceiptArgs,
) -> Result<LiquidReceiptPermit, crate::api::ApiError> {
    use crate::{api::ApiError, state};
    let mut state = state::read();
    if caller != state.config.nns_manager {
        return Err(ApiError::Unauthorized);
    }
    let request_fingerprint = request_fingerprint(&args);
    if let Some(completed) = &state.last_completed_receipt {
        if completed.request_fingerprint == request_fingerprint {
            return Ok(completed.permit.clone());
        }
        if completed.permit.sequence == args.receipt_sequence {
            return Err(ApiError::NonceAlreadyUsed);
        }
    }
    if let Some(state::StreamOperation::LiquidReceipt(existing)) = &state.active_operation {
        if existing.request_fingerprint == request_fingerprint {
            return Ok(LiquidReceiptPermit {
                sequence: existing.sequence,
                destination: existing.destination.clone(),
                memo: existing.memo.clone(),
            });
        }
    }
    crate::api::require_ready(&state)?;
    if state.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    if args.receipt_sequence != state.next_nns_receipt_sequence
        || args.liquid_amount_e8s == 0
        || args.source_operation_id.is_empty()
        || args.source_operation_id.len() > 64
    {
        return Err(ApiError::Invalid(
            "invalid receipt sequence or bounded intent".into(),
        ));
    }
    if args.receipt_kind == ReceiptKind::TwoWeekMaturity {
        if args.cohort_generation != state.pending_reward_cohort.as_ref().map(|c| c.generation) {
            return Err(ApiError::Invalid(
                "receipt does not match pending cohort".into(),
            ));
        }
    } else if args.cohort_generation.is_some() {
        return Err(ApiError::Invalid(
            "only two-week maturity names a cohort".into(),
        ));
    }
    let memo = receipt_memo(caller, args.receipt_sequence);
    let permit = LiquidReceiptPermit {
        sequence: args.receipt_sequence,
        destination: state.config.liquid_icp.clone(),
        memo: memo.clone(),
    };
    state.active_operation = Some(state::StreamOperation::LiquidReceipt(Box::new(
        LiquidReceiptOperation {
            request_fingerprint,
            sequence: args.receipt_sequence,
            kind: args.receipt_kind,
            source_operation_id: args.source_operation_id,
            liquid_amount_e8s: args.liquid_amount_e8s,
            cohort_generation: args.cohort_generation,
            source: match args.receipt_kind {
                ReceiptKind::Jupiter => state.config.jupiter_receipt_source.clone(),
                ReceiptKind::TwoWeekMaturity => state.config.two_week_receipt_source.clone(),
            },
            destination: permit.destination.clone(),
            memo,
            phase: ReceiptPhase::AwaitingReceipt,
            proved_block: None,
            active_transfer: None,
            recipient_index: 0,
            settlement_result: None,
        },
    )));
    state::write(state);
    Ok(permit)
}

fn progress(operation: &LiquidReceiptOperation) -> crate::api::LiquidReceiptProgress {
    use crate::api::LiquidReceiptProgress;
    match operation.phase {
        ReceiptPhase::Prepared | ReceiptPhase::AwaitingReceipt => {
            LiquidReceiptProgress::AwaitingReceipt
        }
        ReceiptPhase::ReceiptProved => LiquidReceiptProgress::ReceiptProved,
        ReceiptPhase::Settling => LiquidReceiptProgress::Settling,
        ReceiptPhase::Completed => LiquidReceiptProgress::Completed(
            operation.settlement_result.clone().unwrap_or_default(),
        ),
        ReceiptPhase::Stuck => LiquidReceiptProgress::Stuck(
            "exact receipt settlement proof or governance upgrade required".into(),
        ),
    }
}

pub(crate) async fn resume_liquid_receipt(
    mut operation: LiquidReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, crate::api::ApiError> {
    use crate::{
        api::{ApiError, LiquidReceiptProgress},
        canonical,
        state::{self, DispatchEpoch},
        transfer::{OwnTransferIntent, TransferState},
    };
    if operation.kind == ReceiptKind::TwoWeekMaturity {
        return Err(ApiError::Stuck(
            "two-week reward fan-out is not yet executable".into(),
        ));
    }
    match operation.phase {
        ReceiptPhase::AwaitingReceipt | ReceiptPhase::Prepared => Ok(progress(&operation)),
        ReceiptPhase::ReceiptProved => {
            let config = state::read().config;
            let snapshot = canonical::redemption_snapshot(&config)
                .await
                .map_err(ApiError::Ledger)?;
            let pre_liquid = snapshot
                .liquid_icp_e8s
                .checked_sub(operation.liquid_amount_e8s)
                .ok_or_else(|| ApiError::Invalid("proved receipt exceeds liquid balance".into()))?;
            let excluded = snapshot
                .excluded_io_balances
                .iter()
                .try_fold(0u128, |sum, (_, value)| sum.checked_add(*value))
                .ok_or_else(|| ApiError::Invalid("excluded balance overflow".into()))?;
            let redeemable = snapshot
                .total_supply_e8s
                .checked_sub(snapshot.reserve_io_e8s)
                .and_then(|value| value.checked_sub(excluded))
                .ok_or_else(|| ApiError::Invalid("invalid redeemable supply".into()))?;
            let backed_io =
                io_core_model::backed_io(operation.liquid_amount_e8s, pre_liquid, redeemable)
                    .map_err(|error| {
                        ApiError::Invalid(format!("backed IO calculation failed: {error:?}"))
                    })?;
            let intent = OwnTransferIntent::Icrc1 {
                ledger: config.io_ledger,
                from_subaccount: config
                    .io_reserve
                    .canonical()
                    .map_err(ApiError::Invalid)?
                    .subaccount,
                to: config.jupiter_io_account,
                amount: backed_io,
                fee: config.expected_io_fee_e8s,
                memo: operation.memo.clone(),
                created_at_time: now,
            };
            let mut attempt = TransferAttempt::prepared(intent).map_err(ApiError::Invalid)?;
            attempt.state = TransferState::Submitted {
                epoch: DispatchEpoch(1),
                first_submitted_at: now,
                last_submitted_at: now,
            };
            let fingerprint = attempt.fingerprint.clone();
            let intent = attempt.intent.clone();
            operation.active_transfer = Some(attempt);
            operation.settlement_result = Some(candid::encode_one(backed_io).map_err(|error| {
                ApiError::Invalid(format!("settlement encode failed: {error}"))
            })?);
            operation.phase = ReceiptPhase::Settling;
            persist(&operation)?;
            let response = crate::api::submit(&intent).await;
            apply_settlement_callback(&operation, fingerprint, DispatchEpoch(1), response)
        }
        ReceiptPhase::Settling => {
            let attempt = operation
                .active_transfer
                .as_ref()
                .ok_or_else(|| ApiError::Invalid("settlement transfer is missing".into()))?;
            if matches!(attempt.state, TransferState::Succeeded { .. }) {
                return complete_jupiter(operation, now);
            }
            Ok(LiquidReceiptProgress::Settling)
        }
        ReceiptPhase::Completed | ReceiptPhase::Stuck => Ok(progress(&operation)),
    }
}

fn persist(operation: &LiquidReceiptOperation) -> Result<(), crate::api::ApiError> {
    use crate::{api::ApiError, state};
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(state::StreamOperation::LiquidReceipt(current))
        if current.sequence == operation.sequence
            && current.request_fingerprint == operation.request_fingerprint)
    {
        return Err(ApiError::Busy);
    }
    latest.active_operation = Some(state::StreamOperation::LiquidReceipt(Box::new(
        operation.clone(),
    )));
    state::write(latest);
    Ok(())
}

fn apply_settlement_callback(
    expected: &LiquidReceiptOperation,
    fingerprint: Vec<u8>,
    epoch: crate::state::DispatchEpoch,
    response: Result<crate::transfer::TransferResult, String>,
) -> Result<crate::api::LiquidReceiptProgress, crate::api::ApiError> {
    use crate::{
        api::{ApiError, LiquidReceiptProgress},
        state::{self, StreamOperation},
        transfer::{classify_result, ClassifiedResult, TransferState},
    };
    let mut latest = match state::read().active_operation {
        Some(StreamOperation::LiquidReceipt(current)) => *current,
        _ => return Err(ApiError::Busy),
    };
    if latest.sequence != expected.sequence
        || latest.request_fingerprint != expected.request_fingerprint
        || latest.phase != ReceiptPhase::Settling
    {
        return Err(ApiError::Busy);
    }
    let attempt = latest
        .active_transfer
        .as_mut()
        .ok_or_else(|| ApiError::Invalid("settlement transfer is missing".into()))?;
    if attempt.fingerprint != fingerprint
        || !matches!(attempt.state, TransferState::Submitted { epoch: current, .. } if current == epoch)
    {
        return Err(ApiError::Busy);
    }
    let classified = match response {
        Ok(value) => classify_result(value).map_err(ApiError::Ledger)?,
        Err(error) => return Err(ApiError::Pending(error)),
    };
    match classified {
        ClassifiedResult::Succeeded(block) => {
            attempt.state = TransferState::Succeeded { block };
            persist(&latest)?;
            Ok(LiquidReceiptProgress::Settling)
        }
        ClassifiedResult::NoEffect(error) => {
            latest.phase = ReceiptPhase::Stuck;
            attempt.state = TransferState::Stuck {
                reason: error.clone(),
            };
            persist(&latest)?;
            crate::api::pause();
            Err(ApiError::Stuck(error))
        }
        ClassifiedResult::Ambiguous(error) => Err(ApiError::Pending(error)),
    }
}

fn complete_jupiter(
    mut operation: LiquidReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, crate::api::ApiError> {
    use crate::{api::ApiError, state};
    let settlement = operation
        .settlement_result
        .clone()
        .ok_or_else(|| ApiError::Invalid("settlement result is missing".into()))?;
    operation.phase = ReceiptPhase::Completed;
    let permit = LiquidReceiptPermit {
        sequence: operation.sequence,
        destination: operation.destination.clone(),
        memo: operation.memo.clone(),
    };
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(state::StreamOperation::LiquidReceipt(current))
        if current.sequence == operation.sequence
            && current.request_fingerprint == operation.request_fingerprint
            && current.phase == ReceiptPhase::Settling)
    {
        return Err(ApiError::Busy);
    }
    latest.last_completed_receipt = Some(LastCompletedReceipt {
        request_fingerprint: operation.request_fingerprint,
        permit,
        receipt_block: operation
            .proved_block
            .ok_or_else(|| ApiError::Invalid("receipt proof is missing".into()))?,
        settlement_result: settlement.clone(),
        completed_at_nanos: now,
    });
    latest.next_nns_receipt_sequence = latest
        .next_nns_receipt_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("receipt sequence overflow".into()))?;
    latest.active_operation = None;
    state::write(latest);
    Ok(crate::api::LiquidReceiptProgress::Completed(settlement))
}

pub async fn complete_liquid_receipt(
    caller: Principal,
    args: CompleteLiquidReceiptArgs,
) -> Result<(), crate::api::ApiError> {
    use crate::{api::ApiError, canonical, state};
    let snapshot = state::read();
    if caller != snapshot.config.nns_manager {
        return Err(ApiError::Unauthorized);
    }
    let operation = match snapshot.active_operation {
        Some(state::StreamOperation::LiquidReceipt(operation))
            if operation.sequence == args.receipt_sequence =>
        {
            operation
        }
        _ => return Err(ApiError::Invalid("no matching liquid receipt".into())),
    };
    if operation.proved_block == Some(args.block_index) {
        return Ok(());
    }
    if operation.proved_block.is_some() {
        return Err(ApiError::Invalid("conflicting receipt block".into()));
    }
    let transfer = canonical::exact_icp_transfer(snapshot.config.icp_ledger, args.block_index)
        .await
        .map_err(ApiError::Ledger)?;
    let accounts_match = transfer.from
        == canonical::icp_account_identifier(&operation.source).map_err(ApiError::Invalid)?
        && transfer.to
            == canonical::icp_account_identifier(&operation.destination)
                .map_err(ApiError::Invalid)?;
    if !accounts_match
        || transfer.amount_e8s != operation.liquid_amount_e8s
        || transfer.fee_e8s != snapshot.config.expected_icp_fee_e8s
        || transfer.memo.as_deref() != Some(operation.memo.as_slice())
        || transfer.created_at_time == 0
        || transfer.spender.is_some()
    {
        return Err(ApiError::Invalid(
            "canonical block does not match receipt intent".into(),
        ));
    }
    let mut state = state::read();
    match &mut state.active_operation {
        Some(state::StreamOperation::LiquidReceipt(current))
            if **current == *operation
                && matches!(
                    current.phase,
                    ReceiptPhase::Prepared | ReceiptPhase::AwaitingReceipt
                )
                && current.proved_block.is_none() =>
        {
            current.proved_block = Some(args.block_index);
            current.phase = ReceiptPhase::ReceiptProved;
        }
        Some(state::StreamOperation::LiquidReceipt(current))
            if current.sequence == operation.sequence
                && current.proved_block == Some(args.block_index) =>
        {
            return Ok(())
        }
        _ => return Err(ApiError::Busy),
    }
    state::write(state);
    Ok(())
}
