use candid::{CandidType, Nat, Principal};
use ic_cdk::call::Call;
use serde::Deserialize;

use crate::{
    canonical,
    receipt::{
        receipt_memo, CompleteLiquidReceiptArgs, LiquidReceiptOperation, LiquidReceiptPermit,
        PrepareLiquidReceiptArgs, ReceiptKind, ReceiptPhase,
    },
    redemption::{self, RedeemArgs, RedemptionOperation, RedemptionPhase},
    state::{
        self, Account, DispatchEpoch, Lifecycle, OperationSequence, RedemptionResult,
        StreamOperation, StreamStateV1,
    },
    transfer::{
        classify_result, ClassifiedResult, IcrcTransferArg, IcrcTransferFromArg, OwnTransferIntent,
        TransferResult, TransferState,
    },
};

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ApiError {
    Anonymous,
    Unauthorized,
    Paused,
    Busy,
    WrongNonce { expected: u64 },
    NonceAlreadyUsed,
    Invalid(String),
    Ledger(String),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RedemptionProgress {
    IoPullSubmitted,
    IoInReserve,
    PayoutSubmitted,
    PayoutSucceeded,
    Completed(RedemptionResult),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct Status {
    pub lifecycle: Lifecycle,
    pub operation_kind: Option<String>,
    pub operation_phase: Option<String>,
    pub next_nns_receipt_sequence: u64,
    pub next_cohort_timestamp_seconds: u64,
}

pub fn get_status() -> Status {
    let state = state::read();
    let (operation_kind, operation_phase) = match state.active_operation {
        Some(StreamOperation::Redemption(operation)) => (
            Some("Redemption".into()),
            Some(format!("{:?}", operation.phase)),
        ),
        Some(StreamOperation::LiquidReceipt(operation)) => (
            Some("LiquidReceipt".into()),
            Some(format!("{:?}", operation.phase)),
        ),
        None => (None, None),
    };
    Status {
        lifecycle: state.lifecycle,
        operation_kind,
        operation_phase,
        next_nns_receipt_sequence: state.next_nns_receipt_sequence,
        next_cohort_timestamp_seconds: state.next_cohort_timestamp_seconds,
    }
}

fn require_ready(state: &StreamStateV1) -> Result<(), ApiError> {
    match state.lifecycle {
        Lifecycle::Ready => Ok(()),
        Lifecycle::Paused => Err(ApiError::Paused),
    }
}

async fn submit(intent: &OwnTransferIntent) -> Result<TransferResult, String> {
    match intent {
        OwnTransferIntent::Icrc1 {
            ledger,
            from_subaccount,
            to,
            amount,
            fee,
            memo,
            created_at_time,
        } => Call::bounded_wait(*ledger, "icrc1_transfer")
            .with_arg(IcrcTransferArg {
                from_subaccount: (*from_subaccount != [0; 32]).then(|| from_subaccount.to_vec()),
                to: to.clone(),
                amount: Nat::from(*amount),
                fee: Some(Nat::from(*fee)),
                memo: Some(memo.clone()),
                created_at_time: Some(*created_at_time),
            })
            .await
            .map_err(|error| format!("icrc1_transfer call failed: {error:?}"))?
            .candid()
            .map_err(|error| format!("icrc1_transfer decode failed: {error:?}")),
        OwnTransferIntent::Icrc2TransferFrom {
            ledger,
            spender_subaccount,
            from,
            to,
            amount,
            fee,
            memo,
            created_at_time,
        } => Call::bounded_wait(*ledger, "icrc2_transfer_from")
            .with_arg(IcrcTransferFromArg {
                spender_subaccount: (*spender_subaccount != [0; 32])
                    .then(|| spender_subaccount.to_vec()),
                from: from.clone(),
                to: to.clone(),
                amount: Nat::from(*amount),
                fee: Some(Nat::from(*fee)),
                memo: Some(memo.clone()),
                created_at_time: Some(*created_at_time),
            })
            .await
            .map_err(|error| format!("icrc2_transfer_from call failed: {error:?}"))?
            .candid()
            .map_err(|error| format!("icrc2_transfer_from decode failed: {error:?}")),
    }
}

pub async fn redeem(
    caller: Principal,
    args: RedeemArgs,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    if caller == Principal::anonymous() {
        return Err(ApiError::Anonymous);
    }
    let initial = state::read();
    require_ready(&initial)?;
    let account = Account {
        owner: caller,
        subaccount: args.from_subaccount.clone(),
    };
    account.validate().map_err(ApiError::Invalid)?;
    let request_fingerprint = redemption::request_fingerprint(caller, &args, &account);
    if let Some(StreamOperation::Redemption(active)) = &initial.active_operation {
        if active.caller == caller && active.nonce == args.nonce {
            if active.request_fingerprint != request_fingerprint {
                return Err(ApiError::NonceAlreadyUsed);
            }
            return Ok(progress_for(active));
        }
    }
    if initial.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    if args.io_amount_e8s < initial.config.minimum_redemption_io_e8s {
        return Err(ApiError::Invalid(
            "redemption below configured minimum".into(),
        ));
    }
    let caller_state = state::caller_state(caller);
    if args.nonce != caller_state.next_nonce {
        if args.nonce.checked_add(1) == Some(caller_state.next_nonce)
            && caller_state.last_request_fingerprint.as_deref()
                == Some(request_fingerprint.as_slice())
        {
            return caller_state
                .last_result
                .map(RedemptionProgress::Completed)
                .ok_or(ApiError::Busy);
        }
        if args.nonce < caller_state.next_nonce {
            return Err(ApiError::NonceAlreadyUsed);
        }
        return Err(ApiError::WrongNonce {
            expected: caller_state.next_nonce,
        });
    }
    if args.expires_at_nanos < now {
        return Err(ApiError::Invalid("redemption expired".into()));
    }
    let snapshot = canonical::redemption_snapshot(&initial.config)
        .await
        .map_err(ApiError::Ledger)?;
    if snapshot.io_fee_e8s != initial.config.expected_io_fee_e8s
        || snapshot.icp_fee_e8s != initial.config.expected_icp_fee_e8s
    {
        return Err(ApiError::Invalid(
            "canonical fee differs from approved config".into(),
        ));
    }
    let required_io = args
        .io_amount_e8s
        .checked_add(snapshot.io_fee_e8s)
        .ok_or_else(|| ApiError::Invalid("redemption amount overflow".into()))?;
    let source_balance = canonical::balance(initial.config.io_ledger, account.clone())
        .await
        .map_err(ApiError::Ledger)?;
    if source_balance < required_io {
        return Err(ApiError::Invalid("source balance is insufficient".into()));
    }
    let spender = Account {
        owner: ic_cdk::api::canister_self(),
        subaccount: None,
    };
    let (allowance, allowance_expiry) =
        canonical::allowance(initial.config.io_ledger, account, spender)
            .await
            .map_err(ApiError::Ledger)?;
    if allowance < required_io || allowance_expiry.is_some_and(|expiry| expiry < now) {
        return Err(ApiError::Invalid(
            "allowance is insufficient or expired".into(),
        ));
    }

    let mut latest = state::read();
    require_ready(&latest)?;
    if latest.active_operation.is_some() || args.expires_at_nanos < ic_cdk::api::time() {
        return Err(ApiError::Busy);
    }
    let sequence = latest.next_operation_sequence;
    latest.next_operation_sequence.0 = latest
        .next_operation_sequence
        .0
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("operation sequence overflow".into()))?;
    let operation = redemption::calculate(caller, &args, snapshot, &latest.config, now, sequence)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(StreamOperation::Redemption(Box::new(operation)));
    state::write(latest);
    dispatch_redemption_transfer(sequence, true, now).await
}

fn progress_for(operation: &RedemptionOperation) -> RedemptionProgress {
    match operation.phase {
        RedemptionPhase::Prepared | RedemptionPhase::PullSubmitted => {
            RedemptionProgress::IoPullSubmitted
        }
        RedemptionPhase::IoInReserve => RedemptionProgress::IoInReserve,
        RedemptionPhase::PayoutSubmitted => RedemptionProgress::PayoutSubmitted,
        RedemptionPhase::PayoutSucceeded | RedemptionPhase::Completed => {
            RedemptionProgress::PayoutSucceeded
        }
        RedemptionPhase::Stuck => RedemptionProgress::PayoutSubmitted,
    }
}

fn active_redemption() -> Result<RedemptionOperation, ApiError> {
    match state::read().active_operation {
        Some(StreamOperation::Redemption(operation)) => Ok(*operation),
        _ => Err(ApiError::Invalid("no active redemption".into())),
    }
}

fn persist_redemption(operation: RedemptionOperation) {
    let mut state = state::read();
    state.active_operation = Some(StreamOperation::Redemption(Box::new(operation)));
    state::write(state);
}

async fn dispatch_redemption_transfer(
    sequence: OperationSequence,
    io_pull: bool,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    let mut operation = active_redemption()?;
    if operation.sequence != sequence {
        return Err(ApiError::Busy);
    }
    let expected_phase = if io_pull {
        RedemptionPhase::Prepared
    } else {
        RedemptionPhase::IoInReserve
    };
    if operation.phase != expected_phase {
        return Err(ApiError::Busy);
    }
    let attempt = if io_pull {
        &mut operation.io_pull
    } else {
        &mut operation.icp_payout
    };
    let epoch = match attempt.state {
        TransferState::Submitted {
            epoch,
            first_submitted_at,
            last_submitted_at,
        } => {
            let config = &state::read().config;
            if now.saturating_sub(last_submitted_at) < config.retry_delay_nanos {
                return Err(ApiError::Busy);
            }
            if now.saturating_sub(first_submitted_at) >= config.ledger_deduplication_window_nanos {
                attempt.state = TransferState::Stuck {
                    reason: "deduplication window expired".into(),
                };
                operation.phase = RedemptionPhase::Stuck;
                persist_redemption(operation);
                pause();
                return Err(ApiError::Stuck("deduplication window expired".into()));
            }
            DispatchEpoch(
                epoch
                    .0
                    .checked_add(1)
                    .ok_or_else(|| ApiError::Invalid("dispatch epoch overflow".into()))?,
            )
        }
        TransferState::Prepared => DispatchEpoch(1),
        _ => return Err(ApiError::Busy),
    };
    let first_submitted_at = match attempt.state {
        TransferState::Submitted {
            first_submitted_at, ..
        } => first_submitted_at,
        _ => now,
    };
    attempt.state = TransferState::Submitted {
        epoch,
        first_submitted_at,
        last_submitted_at: now,
    };
    let fingerprint = attempt.fingerprint.clone();
    let intent = attempt.intent.clone();
    operation.phase = if io_pull {
        RedemptionPhase::PullSubmitted
    } else {
        RedemptionPhase::PayoutSubmitted
    };
    persist_redemption(operation);

    let response = submit(&intent).await;
    apply_transfer_callback(sequence, io_pull, fingerprint, epoch, response)
}

fn apply_transfer_callback(
    sequence: OperationSequence,
    io_pull: bool,
    fingerprint: Vec<u8>,
    epoch: DispatchEpoch,
    response: Result<TransferResult, String>,
) -> Result<RedemptionProgress, ApiError> {
    let mut operation = active_redemption()?;
    let expected_phase = if io_pull {
        RedemptionPhase::PullSubmitted
    } else {
        RedemptionPhase::PayoutSubmitted
    };
    if operation.sequence != sequence || operation.phase != expected_phase {
        return Err(ApiError::Busy);
    }
    let attempt = if io_pull {
        &mut operation.io_pull
    } else {
        &mut operation.icp_payout
    };
    if attempt.fingerprint != fingerprint
        || !matches!(attempt.state, TransferState::Submitted { epoch: current, .. } if current == epoch)
    {
        return Err(ApiError::Busy);
    }
    let classified = match response {
        Ok(result) => classify_result(result).map_err(ApiError::Ledger)?,
        Err(error) => {
            persist_redemption(operation);
            return Err(ApiError::Stuck(error));
        }
    };
    match classified {
        ClassifiedResult::Succeeded(block) => {
            attempt.state = TransferState::Succeeded { block };
            operation.phase = if io_pull {
                RedemptionPhase::IoInReserve
            } else {
                RedemptionPhase::PayoutSucceeded
            };
            persist_redemption(operation);
            Ok(if io_pull {
                RedemptionProgress::IoInReserve
            } else {
                RedemptionProgress::PayoutSucceeded
            })
        }
        ClassifiedResult::NoEffect(error) if io_pull => {
            let mut state = state::read();
            state.active_operation = None;
            state::write(state);
            Err(ApiError::Ledger(error))
        }
        ClassifiedResult::NoEffect(error) => {
            attempt.state = TransferState::Stuck {
                reason: error.clone(),
            };
            operation.phase = RedemptionPhase::Stuck;
            persist_redemption(operation);
            pause();
            Err(ApiError::Stuck(error))
        }
        ClassifiedResult::Ambiguous(error) => {
            persist_redemption(operation);
            Err(ApiError::Stuck(error))
        }
    }
}

pub async fn resume(now: u64) -> Result<RedemptionProgress, ApiError> {
    let operation = active_redemption()?;
    match operation.phase {
        RedemptionPhase::IoInReserve => {
            dispatch_redemption_transfer(operation.sequence, false, now).await
        }
        RedemptionPhase::PayoutSucceeded => commit_redemption(operation, now).await,
        RedemptionPhase::PullSubmitted => retry_submitted(operation, true, now).await,
        RedemptionPhase::PayoutSubmitted => retry_submitted(operation, false, now).await,
        RedemptionPhase::Stuck => Err(ApiError::Stuck(
            "exact block proof or governance upgrade required".into(),
        )),
        RedemptionPhase::Completed => Err(ApiError::Invalid("redemption already completed".into())),
        RedemptionPhase::Prepared => {
            dispatch_redemption_transfer(operation.sequence, true, now).await
        }
    }
}

async fn retry_submitted(
    mut operation: RedemptionOperation,
    io_pull: bool,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    operation.phase = if io_pull {
        RedemptionPhase::Prepared
    } else {
        RedemptionPhase::IoInReserve
    };
    let sequence = operation.sequence;
    persist_redemption(operation);
    dispatch_redemption_transfer(sequence, io_pull, now).await
}

async fn commit_redemption(
    mut operation: RedemptionOperation,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    let post = canonical::redemption_snapshot(&state::read().config)
        .await
        .map_err(ApiError::Ledger)?;
    if let Err(error) = redemption::verify_postconditions(&operation, &post) {
        operation.phase = RedemptionPhase::Stuck;
        persist_redemption(operation);
        pause();
        return Err(ApiError::Stuck(error));
    }
    let result = RedemptionResult {
        request_fingerprint: operation.request_fingerprint.clone(),
        nonce: operation.nonce,
        io_block: operation
            .io_pull
            .succeeded_block()
            .map_err(ApiError::Invalid)?,
        icp_block: operation
            .icp_payout
            .succeeded_block()
            .map_err(ApiError::Invalid)?,
        gross_icp_e8s: operation.gross_icp_e8s,
        net_icp_e8s: operation.net_icp_e8s,
        io_fee_e8s: operation.snapshot.io_fee_e8s,
        icp_fee_e8s: operation.snapshot.icp_fee_e8s,
        completed_at_nanos: now,
    };
    let mut caller_state = state::caller_state(operation.caller);
    caller_state.next_nonce = caller_state
        .next_nonce
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("caller nonce overflow".into()))?;
    caller_state.last_request_fingerprint = Some(operation.request_fingerprint.clone());
    caller_state.last_result = Some(result.clone());
    state::set_caller_state(operation.caller, caller_state);
    let mut state = state::read();
    if !matches!(&state.active_operation, Some(StreamOperation::Redemption(current)) if current.sequence == operation.sequence)
    {
        return Err(ApiError::Busy);
    }
    state.active_operation = None;
    state::write(state);
    Ok(RedemptionProgress::Completed(result))
}

fn pause() {
    let mut state = state::read();
    state.lifecycle = Lifecycle::Paused;
    state::write(state);
}

pub fn prepare_liquid_receipt(
    caller: Principal,
    args: PrepareLiquidReceiptArgs,
) -> Result<LiquidReceiptPermit, ApiError> {
    let mut state = state::read();
    require_ready(&state)?;
    if caller != state.config.nns_manager {
        return Err(ApiError::Unauthorized);
    }
    if let Some(StreamOperation::LiquidReceipt(existing)) = &state.active_operation {
        if existing.sequence == args.receipt_sequence
            && existing.kind == args.receipt_kind
            && existing.source_operation_id == args.source_operation_id
            && existing.liquid_amount_e8s == args.liquid_amount_e8s
            && existing.cohort_generation == args.cohort_generation
        {
            return Ok(LiquidReceiptPermit {
                sequence: existing.sequence,
                destination: existing.destination.clone(),
                memo: existing.memo.clone(),
            });
        }
    }
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
    state.active_operation = Some(StreamOperation::LiquidReceipt(Box::new(
        LiquidReceiptOperation {
            sequence: args.receipt_sequence,
            kind: args.receipt_kind,
            source_operation_id: args.source_operation_id,
            liquid_amount_e8s: args.liquid_amount_e8s,
            cohort_generation: args.cohort_generation,
            source: state.config.nns_receipt_source.clone(),
            destination: permit.destination.clone(),
            memo,
            phase: ReceiptPhase::AwaitingReceipt,
            proved_block: None,
            active_transfer: None,
            recipient_index: 0,
        },
    )));
    state::write(state);
    Ok(permit)
}

pub async fn complete_liquid_receipt(
    caller: Principal,
    args: CompleteLiquidReceiptArgs,
) -> Result<(), ApiError> {
    let snapshot = state::read();
    if caller != snapshot.config.nns_manager {
        return Err(ApiError::Unauthorized);
    }
    let operation = match snapshot.active_operation {
        Some(StreamOperation::LiquidReceipt(operation))
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
    let transfer = canonical::exact_icrc_transfer(snapshot.config.icp_ledger, args.block_index)
        .await
        .map_err(ApiError::Ledger)?;
    let accounts_match = transfer
        .from
        .effective_eq(&operation.source)
        .map_err(ApiError::Invalid)?
        && transfer
            .to
            .effective_eq(&operation.destination)
            .map_err(ApiError::Invalid)?;
    if !accounts_match
        || transfer.amount_e8s != operation.liquid_amount_e8s
        || transfer.fee_e8s != Some(snapshot.config.expected_icp_fee_e8s)
        || transfer.memo.as_deref() != Some(operation.memo.as_slice())
        || transfer.created_at_time.is_none()
        || transfer.spender.is_some()
    {
        return Err(ApiError::Invalid(
            "canonical block does not match receipt intent".into(),
        ));
    }
    let mut state = state::read();
    match &mut state.active_operation {
        Some(StreamOperation::LiquidReceipt(current))
            if current.sequence == operation.sequence && current.proved_block.is_none() =>
        {
            current.proved_block = Some(args.block_index);
            current.phase = ReceiptPhase::ReceiptProved;
        }
        Some(StreamOperation::LiquidReceipt(current))
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

pub async fn prove_active_transfer(block_index: u128) -> Result<(), ApiError> {
    let operation = active_redemption()?;
    if operation.phase != RedemptionPhase::Stuck {
        return Err(ApiError::Invalid(
            "only a Stuck transfer accepts proof".into(),
        ));
    }
    let proving_pull = !matches!(operation.io_pull.state, TransferState::Succeeded { .. });
    let attempt = if proving_pull {
        &operation.io_pull
    } else {
        &operation.icp_payout
    };
    if !matches!(attempt.state, TransferState::Stuck { .. }) {
        return Err(ApiError::Invalid("active transfer is not Stuck".into()));
    }
    if proving_pull {
        let exact = canonical::exact_icrc_transfer(attempt.intent.ledger(), block_index)
            .await
            .map_err(ApiError::Ledger)?;
        let OwnTransferIntent::Icrc2TransferFrom {
            spender_subaccount,
            from,
            to,
            amount,
            fee,
            memo,
            created_at_time,
            ..
        } = &attempt.intent
        else {
            return Err(ApiError::Invalid(
                "stuck IO pull has wrong intent kind".into(),
            ));
        };
        let spender = Account {
            owner: ic_cdk::api::canister_self(),
            subaccount: (*spender_subaccount != [0; 32]).then(|| spender_subaccount.to_vec()),
        };
        let matches = exact.from.effective_eq(from).map_err(ApiError::Invalid)?
            && exact.to.effective_eq(to).map_err(ApiError::Invalid)?
            && exact.amount_e8s == *amount
            && exact.fee_e8s == Some(*fee)
            && exact.memo.as_deref() == Some(memo.as_slice())
            && exact.created_at_time == Some(*created_at_time)
            && exact
                .spender
                .as_ref()
                .is_some_and(|value| value.effective_eq(&spender).unwrap_or(false));
        if !matches {
            return Err(ApiError::Invalid(
                "exact block does not match stuck IO pull".into(),
            ));
        }
    } else {
        let exact = canonical::exact_icp_transfer(attempt.intent.ledger(), block_index)
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
                "stuck ICP payout has wrong intent kind".into(),
            ));
        };
        let source = Account {
            owner: ic_cdk::api::canister_self(),
            subaccount: (*from_subaccount != [0; 32]).then(|| from_subaccount.to_vec()),
        };
        let matches = exact.from
            == canonical::icp_account_identifier(&source).map_err(ApiError::Invalid)?
            && exact.to == canonical::icp_account_identifier(to).map_err(ApiError::Invalid)?
            && exact.amount_e8s == *amount
            && exact.fee_e8s == *fee
            && exact.memo.as_deref() == Some(memo.as_slice())
            && exact.created_at_time == *created_at_time
            && exact.spender.is_none();
        if !matches {
            return Err(ApiError::Invalid(
                "exact block does not match stuck ICP payout".into(),
            ));
        }
    }
    let mut latest = active_redemption()?;
    if latest.sequence != operation.sequence
        || latest.request_fingerprint != operation.request_fingerprint
    {
        return Err(ApiError::Busy);
    }
    let target = if proving_pull {
        &mut latest.io_pull
    } else {
        &mut latest.icp_payout
    };
    if target.fingerprint != attempt.fingerprint
        || !matches!(target.state, TransferState::Stuck { .. })
    {
        return Err(ApiError::Busy);
    }
    target.state = TransferState::Succeeded { block: block_index };
    latest.phase = if proving_pull {
        RedemptionPhase::IoInReserve
    } else {
        RedemptionPhase::PayoutSucceeded
    };
    persist_redemption(latest);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_does_not_claim_canonical_balances() {
        assert!(!format!("{:?}", std::any::type_name::<Status>()).contains("balance"));
    }
}
