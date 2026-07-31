use candid::{CandidType, Nat, Principal};
use ic_cdk::call::Call;
use serde::Deserialize;

use crate::{
    canonical, receipt,
    redemption::{
        self, CanonicalRedeemRequestV1, RedeemArgs, RedemptionOperation, RedemptionPhase,
        RedemptionPreparation,
    },
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
    Pending(String),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RedemptionProgress {
    Preparing,
    IoPullSubmitted,
    IoInReserve,
    PayoutSubmitted,
    PayoutSucceeded,
    Completing,
    Completed(RedemptionResult),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum LiquidReceiptProgress {
    AwaitingReceipt,
    ReceiptProved,
    Settling,
    Completed(Vec<u8>),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum StreamProgress {
    Redemption(RedemptionProgress),
    LiquidReceipt(LiquidReceiptProgress),
    Idle,
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
        Some(StreamOperation::RedemptionPreparation(_)) => (
            Some("RedemptionPreparation".into()),
            Some("Preparing".into()),
        ),
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

pub(crate) fn require_ready(state: &StreamStateV1) -> Result<(), ApiError> {
    match state.lifecycle {
        Lifecycle::Ready => Ok(()),
        Lifecycle::Paused => Err(ApiError::Paused),
    }
}

pub(crate) async fn submit(intent: &OwnTransferIntent) -> Result<TransferResult, String> {
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
    let request = CanonicalRedeemRequestV1::from_args(&args).map_err(ApiError::Invalid)?;
    let account = request.account(caller);
    account.validate().map_err(ApiError::Invalid)?;
    let request_fingerprint = redemption::request_fingerprint(caller, &request);
    if let Some(StreamOperation::RedemptionPreparation(active)) = &initial.active_operation {
        if active.caller == caller && active.request.nonce == args.nonce {
            if active.request_fingerprint != request_fingerprint {
                return Err(ApiError::NonceAlreadyUsed);
            }
            return Ok(RedemptionProgress::Preparing);
        }
    }
    if let Some(StreamOperation::Redemption(active)) = &initial.active_operation {
        if active.caller == caller && active.nonce == args.nonce {
            if active.request_fingerprint != request_fingerprint {
                return Err(ApiError::NonceAlreadyUsed);
            }
            return Ok(progress_for(active));
        }
    }
    let replay_state = state::caller_state(caller);
    if args.nonce.checked_add(1) == Some(replay_state.next_nonce)
        && replay_state.last_request_fingerprint.as_deref() == Some(request_fingerprint.as_slice())
    {
        return replay_state
            .last_result
            .map(RedemptionProgress::Completed)
            .ok_or(ApiError::Busy);
    }
    if args.nonce < replay_state.next_nonce {
        return Err(ApiError::NonceAlreadyUsed);
    }
    require_ready(&initial)?;
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
    let latest_allowed_expiry = now
        .checked_add(initial.config.maximum_request_lifetime_nanos)
        .ok_or_else(|| ApiError::Invalid("redemption lifetime overflow".into()))?;
    if args.expires_at_nanos > latest_allowed_expiry {
        return Err(ApiError::Invalid(
            "redemption expiry exceeds launch lifetime bound".into(),
        ));
    }
    if args.max_io_fee_e8s < initial.config.expected_io_fee_e8s
        || args.max_icp_fee_e8s < initial.config.expected_icp_fee_e8s
    {
        return Err(ApiError::Invalid(
            "caller fee maximum is below approved config".into(),
        ));
    }
    if account
        .effective_eq(&initial.config.io_reserve)
        .map_err(ApiError::Invalid)?
    {
        return Err(ApiError::Invalid("reserve account cannot redeem".into()));
    }
    if initial
        .config
        .excluded_io_accounts
        .iter()
        .try_fold(false, |matched, excluded| {
            account.effective_eq(excluded).map(|same| matched || same)
        })
        .map_err(ApiError::Invalid)?
    {
        return Err(ApiError::Invalid("excluded account cannot redeem".into()));
    }
    let required_io = args
        .io_amount_e8s
        .checked_add(initial.config.expected_io_fee_e8s)
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
        canonical::allowance(initial.config.io_ledger, account.clone(), spender)
            .await
            .map_err(ApiError::Ledger)?;
    if allowance < required_io || allowance_expiry.is_some_and(|expiry| expiry < now) {
        return Err(ApiError::Invalid(
            "allowance is insufficient or expired".into(),
        ));
    }

    let mut latest = state::read();
    require_ready(&latest)?;
    let latest_caller = state::caller_state(caller);
    if latest.active_operation.is_some()
        || latest_caller.next_nonce != args.nonce
        || args.expires_at_nanos < ic_cdk::api::time()
    {
        return Err(ApiError::Busy);
    }
    let sequence = latest.next_operation_sequence;
    latest.next_operation_sequence.0 = latest
        .next_operation_sequence
        .0
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("operation sequence overflow".into()))?;
    let preparation = RedemptionPreparation {
        sequence,
        request_fingerprint: request_fingerprint.clone(),
        request,
        caller,
        account,
        prepared_at_nanos: now,
    };
    preparation.validate().map_err(ApiError::Invalid)?;
    latest.active_operation = Some(StreamOperation::RedemptionPreparation(Box::new(
        preparation.clone(),
    )));
    state::write(latest);

    let snapshot = match canonical::redemption_snapshot(&initial.config).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            clear_matching_preparation(&preparation);
            return Err(ApiError::Ledger(error));
        }
    };
    if snapshot.io_fee_e8s != initial.config.expected_io_fee_e8s
        || snapshot.icp_fee_e8s != initial.config.expected_icp_fee_e8s
    {
        clear_matching_preparation(&preparation);
        return Err(ApiError::Invalid(
            "canonical fee differs from approved config".into(),
        ));
    }
    let operation = match redemption::calculate(&preparation, snapshot, &initial.config) {
        Ok(operation) => operation,
        Err(error) => {
            clear_matching_preparation(&preparation);
            return Err(ApiError::Invalid(error));
        }
    };
    let mut latest = state::read();
    if !matches!(
        &latest.active_operation,
        Some(StreamOperation::RedemptionPreparation(current))
            if **current == preparation
    ) {
        return Err(ApiError::Busy);
    }
    latest.active_operation = Some(StreamOperation::Redemption(Box::new(operation)));
    state::write(latest);
    dispatch_redemption_transfer(sequence, true, now).await
}

fn clear_matching_preparation(expected: &RedemptionPreparation) {
    let mut current = state::read();
    if matches!(
        &current.active_operation,
        Some(StreamOperation::RedemptionPreparation(value)) if **value == *expected
    ) {
        current.active_operation = None;
        state::write(current);
    }
}

fn progress_for(operation: &RedemptionOperation) -> RedemptionProgress {
    match operation.phase {
        RedemptionPhase::Prepared | RedemptionPhase::PullSubmitted => {
            RedemptionProgress::IoPullSubmitted
        }
        RedemptionPhase::IoInReserve => RedemptionProgress::IoInReserve,
        RedemptionPhase::PayoutSubmitted => RedemptionProgress::PayoutSubmitted,
        RedemptionPhase::PayoutSucceeded => RedemptionProgress::PayoutSucceeded,
        RedemptionPhase::CompletionPrepared | RedemptionPhase::CallerResultApplied => {
            RedemptionProgress::Completing
        }
        RedemptionPhase::Stuck => {
            RedemptionProgress::Stuck("exact block proof or governance upgrade required".into())
        }
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
    if !io_pull && operation.icp_payout.is_none() {
        let config = state::read().config;
        let current_fee = canonical::fee(config.icp_ledger)
            .await
            .map_err(ApiError::Ledger)?;
        let latest = active_redemption()?;
        if latest.sequence != sequence
            || latest.phase != RedemptionPhase::IoInReserve
            || latest.request_fingerprint != operation.request_fingerprint
            || latest.icp_payout.is_some()
        {
            return Err(ApiError::Busy);
        }
        if current_fee != latest.snapshot.icp_fee_e8s || current_fee > config.expected_icp_fee_e8s {
            pause();
            return Err(ApiError::Invalid(
                "current ICP fee differs from approved redemption fee".into(),
            ));
        }
        now.checked_add(config.ledger_deduplication_window_nanos)
            .ok_or_else(|| ApiError::Invalid("payout deduplication deadline overflow".into()))?;
        let intent = OwnTransferIntent::Icrc1 {
            ledger: config.icp_ledger,
            from_subaccount: config
                .liquid_icp
                .canonical()
                .map_err(ApiError::Invalid)?
                .subaccount,
            to: latest.account.clone(),
            amount: latest.net_icp_e8s,
            fee: current_fee,
            memo: crate::transfer::deterministic_memo(
                b"io-redemption-pay-v1",
                latest.caller,
                latest.nonce,
            ),
            created_at_time: now,
        };
        operation = latest;
        operation.icp_payout =
            Some(crate::transfer::TransferAttempt::prepared(intent).map_err(ApiError::Invalid)?);
        persist_redemption(operation.clone());
    }
    let attempt = if io_pull {
        &mut operation.io_pull
    } else {
        operation
            .icp_payout
            .as_mut()
            .ok_or_else(|| ApiError::Invalid("payout intent is missing".into()))?
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
    let request_fingerprint = operation.request_fingerprint.clone();
    let fingerprint = attempt.fingerprint.clone();
    let intent = attempt.intent.clone();
    operation.phase = if io_pull {
        RedemptionPhase::PullSubmitted
    } else {
        RedemptionPhase::PayoutSubmitted
    };
    persist_redemption(operation);

    let response = submit(&intent).await;
    apply_transfer_callback(
        sequence,
        request_fingerprint,
        io_pull,
        fingerprint,
        epoch,
        response,
    )
}

fn apply_transfer_callback(
    sequence: OperationSequence,
    request_fingerprint: Vec<u8>,
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
    if operation.sequence != sequence
        || operation.request_fingerprint != request_fingerprint
        || operation.phase != expected_phase
    {
        return Err(ApiError::Busy);
    }
    let attempt = if io_pull {
        &mut operation.io_pull
    } else {
        operation
            .icp_payout
            .as_mut()
            .ok_or_else(|| ApiError::Invalid("payout intent is missing".into()))?
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
            return Err(ApiError::Pending(error));
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
            Err(ApiError::Pending(error))
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
        RedemptionPhase::CompletionPrepared | RedemptionPhase::CallerResultApplied => {
            finish_redemption(operation)
        }
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
        if !active_matches(&operation, RedemptionPhase::PayoutSucceeded) {
            return Err(ApiError::Busy);
        }
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
            .as_ref()
            .ok_or_else(|| ApiError::Invalid("payout intent is missing".into()))?
            .succeeded_block()
            .map_err(ApiError::Invalid)?,
        gross_icp_e8s: operation.gross_icp_e8s,
        net_icp_e8s: operation.net_icp_e8s,
        io_fee_e8s: operation.snapshot.io_fee_e8s,
        icp_fee_e8s: operation.snapshot.icp_fee_e8s,
        completed_at_nanos: now,
    };
    if !active_matches(&operation, RedemptionPhase::PayoutSucceeded) {
        return Err(ApiError::Busy);
    }
    operation.completion_result = Some(result.clone());
    operation.phase = RedemptionPhase::CompletionPrepared;
    persist_redemption(operation.clone());
    finish_redemption(operation)
}

fn active_matches(operation: &RedemptionOperation, phase: RedemptionPhase) -> bool {
    matches!(
        state::read().active_operation,
        Some(StreamOperation::Redemption(current))
            if *current == *operation
                && current.phase == phase
    )
}

fn finish_redemption(mut operation: RedemptionOperation) -> Result<RedemptionProgress, ApiError> {
    let result = operation
        .completion_result
        .clone()
        .ok_or_else(|| ApiError::Invalid("completion result is missing".into()))?;
    let mut caller_state = state::caller_state(operation.caller);
    match caller_state.next_nonce {
        nonce if nonce == operation.nonce => {
            caller_state.next_nonce = nonce
                .checked_add(1)
                .ok_or_else(|| ApiError::Invalid("caller nonce overflow".into()))?;
            caller_state.last_request_fingerprint = Some(operation.request_fingerprint.clone());
            caller_state.last_result = Some(result.clone());
            state::set_caller_state(operation.caller, caller_state);
        }
        nonce
            if Some(nonce) == operation.nonce.checked_add(1)
                && caller_state.last_request_fingerprint.as_ref()
                    == Some(&operation.request_fingerprint)
                && caller_state.last_result.as_ref() == Some(&result) => {}
        _ => {
            pause();
            return Err(ApiError::Stuck(
                "caller redemption state conflicts with completion".into(),
            ));
        }
    }
    let mut state = state::read();
    match &state.active_operation {
        Some(StreamOperation::Redemption(current))
            if current.sequence == operation.sequence
                && current.request_fingerprint == operation.request_fingerprint
                && current.phase == RedemptionPhase::CompletionPrepared =>
        {
            operation.phase = RedemptionPhase::CallerResultApplied;
            state.active_operation = Some(StreamOperation::Redemption(Box::new(operation.clone())));
            state::write(state);
        }
        Some(StreamOperation::Redemption(current))
            if current.sequence == operation.sequence
                && current.request_fingerprint == operation.request_fingerprint
                && current.phase == RedemptionPhase::CallerResultApplied => {}
        _ => {
            pause();
            return Err(ApiError::Stuck(
                "active operation conflicts with caller result".into(),
            ));
        }
    }
    let mut state = state::read();
    if !matches!(&state.active_operation, Some(StreamOperation::Redemption(current))
        if current.sequence == operation.sequence
            && current.request_fingerprint == operation.request_fingerprint
            && current.phase == RedemptionPhase::CallerResultApplied
            && current.completion_result.as_ref() == Some(&result))
    {
        return Err(ApiError::Busy);
    }
    state.active_operation = None;
    state::write(state);
    Ok(RedemptionProgress::Completed(result))
}

pub(crate) fn pause() {
    let mut state = state::read();
    state.lifecycle = Lifecycle::Paused;
    state::write(state);
}

pub async fn resume_stream(now: u64) -> Result<StreamProgress, ApiError> {
    match state::read().active_operation {
        Some(StreamOperation::Redemption(_)) => resume(now).await.map(StreamProgress::Redemption),
        Some(StreamOperation::RedemptionPreparation(_)) => {
            Ok(StreamProgress::Redemption(RedemptionProgress::Preparing))
        }
        Some(StreamOperation::LiquidReceipt(operation)) => {
            receipt::resume_liquid_receipt(*operation, now)
                .await
                .map(StreamProgress::LiquidReceipt)
        }
        None => Ok(StreamProgress::Idle),
    }
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
        operation
            .icp_payout
            .as_ref()
            .ok_or_else(|| ApiError::Invalid("payout intent is missing".into()))?
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
        latest
            .icp_payout
            .as_mut()
            .ok_or_else(|| ApiError::Invalid("payout intent is missing".into()))?
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
