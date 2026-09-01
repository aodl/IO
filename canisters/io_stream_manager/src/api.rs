use candid::{CandidType, Nat, Principal};
use ic_cdk::call::Call;
pub use io_receipt_types::ClaimBackingReceiptProgress;
use serde::Deserialize;

use crate::{
    canonical, receipt,
    redemption::{
        self, CanonicalRedeemRequestV1, RedeemArgs, RedemptionOperation, RedemptionPhase,
    },
    state::{
        self, Account, DispatchEpoch, Lifecycle, OperationSequence, RedemptionResult,
        RedemptionStreamOperation, StreamOperation, StreamStateV1,
    },
    transfer::{
        classify_result, ClassifiedResult, IcrcTransferArg, OwnTransferIntent, TransferResult,
        TransferState,
    },
};

#[cfg(debug_assertions)]
thread_local! {
    static TRAP_AFTER_CALLER_RESULT_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(debug_assertions)]
pub fn debug_trap_after_caller_result_write(enabled: bool) {
    TRAP_AFTER_CALLER_RESULT_WRITE.with(|value| value.set(enabled));
}

fn maybe_trap_after_caller_result_write() {
    #[cfg(debug_assertions)]
    TRAP_AFTER_CALLER_RESULT_WRITE.with(|value| {
        if value.get() {
            ic_cdk::trap("debug trap after caller redemption result write");
        }
    });
}

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
    Pending,
    Completed(RedemptionResult),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum StreamProgress {
    Redemption(RedemptionProgress),
    ClaimReceipt(ClaimBackingReceiptProgress),
    BackingReconciliation,
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct Status {
    pub lifecycle: Lifecycle,
    pub operation_kind: Option<String>,
    pub operation_phase: Option<String>,
    pub next_operation_sequence: u64,
    pub latest_entitlement_batch_generation: u64,
    pub latest_processed_reward_event: Option<crate::state::RewardEventId>,
    pub latest_reward_event_classification: Option<crate::state::RewardEventClassification>,
    pub accumulated_entitlements: Vec<crate::state::FrozenEntitlement>,
    pub accumulated_eligible_credit: u128,
    pub accumulated_policy_credit: u128,
    pub processed_reward_event_count: u64,
    pub missed_reward_event_count: u64,
    pub reward_work_due: bool,
    pub reward_processing_paused: bool,
    pub governance_parameters_fresh: bool,
    pub pending_entitlement_batch_eligible_credit: Option<u128>,
    pub pending_entitlement_batch_policy_credit: Option<u128>,
    pub latest_reconciliation_checkpoint: Option<crate::state::ReconciliationCheckpoint>,
    pub prepared_exit_generation: Option<u64>,
    pub prepared_exit_member_count: u32,
    pub committed_exit_member_count: u32,
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
    }
}

pub async fn prepare_redemption(
    caller: Principal,
    args: RedeemArgs,
    now: u64,
) -> Result<redemption::PreparedRedemption, ApiError> {
    if caller == Principal::anonymous() {
        return Err(ApiError::Anonymous);
    }
    let initial = state::read();
    require_ready(&initial)?;
    let request = CanonicalRedeemRequestV1::from_args(&args).map_err(ApiError::Invalid)?;
    let fingerprint = redemption::request_fingerprint(caller, &request);
    let caller_before = state::caller_state(caller);
    if args.nonce != caller_before.next_nonce {
        return Err(if args.nonce < caller_before.next_nonce {
            ApiError::NonceAlreadyUsed
        } else {
            ApiError::WrongNonce {
                expected: caller_before.next_nonce,
            }
        });
    }
    if let Some(state::CallerRedemptionPending::Prepared(prepared)) = &caller_before.pending {
        if now <= prepared.request.expires_at_nanos && prepared.request_fingerprint == fingerprint {
            return Ok((**prepared).clone());
        }
        if now <= prepared.request.expires_at_nanos {
            return Err(ApiError::NonceAlreadyUsed);
        }
    }
    if matches!(
        caller_before.pending,
        Some(state::CallerRedemptionPending::Pushed(_))
    ) {
        return Err(ApiError::Busy);
    }
    let snapshot = canonical::claim_snapshot(&initial.config)
        .await
        .map_err(ApiError::Ledger)?;
    let prepared = redemption::prepare(caller, request, snapshot, &initial.config, now)
        .map_err(ApiError::Invalid)?;
    if state::read() != initial || state::caller_state(caller) != caller_before {
        return Err(ApiError::Busy);
    }
    let mut next = caller_before;
    next.pending = Some(state::CallerRedemptionPending::Prepared(Box::new(
        prepared.clone(),
    )));
    state::set_caller_state(caller, next);
    Ok(prepared)
}

pub async fn settle_redemption(
    caller: Principal,
    block_index: u128,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    if caller == Principal::anonymous() {
        return Err(ApiError::Anonymous);
    }
    let config = state::read().config;
    let caller_before = state::caller_state(caller);
    let pushed = match caller_before.pending.clone() {
        Some(state::CallerRedemptionPending::Pushed(value)) => {
            if value.io_block != block_index {
                return Err(ApiError::NonceAlreadyUsed);
            }
            *value
        }
        Some(state::CallerRedemptionPending::Prepared(prepared)) => {
            let exact = canonical::exact_icrc_transfer(config.io_ledger, block_index)
                .await
                .map_err(ApiError::Ledger)?;
            let created_at = exact.created_at_time.ok_or_else(|| {
                ApiError::Invalid("redemption push lacks canonical created_at_time".into())
            })?;
            if !exact
                .from
                .effective_eq(&prepared.account)
                .map_err(ApiError::Invalid)?
                || !exact
                    .to
                    .effective_eq(&prepared.reserve)
                    .map_err(ApiError::Invalid)?
                || exact.amount_e8s != prepared.request.io_amount_e8s
                || exact.fee_e8s != Some(prepared.snapshot.io_fee_e8s)
                || exact.memo.as_deref() != Some(prepared.push_memo.as_slice())
                || exact.spender.is_some()
                || created_at < prepared.prepared_at_nanos
                || created_at > prepared.request.expires_at_nanos
            {
                return Err(ApiError::Invalid(
                    "exact IO block does not match the prepared push".into(),
                ));
            }
            if state::caller_state(caller) != caller_before {
                return Err(ApiError::Busy);
            }
            let pushed = redemption::PushedRedemption {
                prepared: *prepared,
                io_block: block_index,
                transfer_created_at_nanos: created_at,
            };
            pushed.validate(&config).map_err(ApiError::Invalid)?;
            let mut next = caller_before;
            next.pending = Some(state::CallerRedemptionPending::Pushed(Box::new(
                pushed.clone(),
            )));
            state::set_caller_state(caller, next);
            pushed
        }
        None => {
            return state::caller_state(caller)
                .last_result
                .filter(|result| result.io_block == block_index)
                .map(RedemptionProgress::Completed)
                .ok_or_else(|| {
                    ApiError::Invalid("caller has no matching prepared redemption".into())
                })
        }
    };
    activate_pushed(pushed, now).await
}

pub async fn resume_redemption(
    caller: Principal,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    if let Ok(active) = active_redemption() {
        if active.pushed.prepared.caller == caller {
            return resume(now).await;
        }
        return Ok(RedemptionProgress::Pending);
    }
    let pending = state::caller_state(caller).pending;
    match pending {
        Some(state::CallerRedemptionPending::Pushed(value)) => activate_pushed(*value, now).await,
        Some(state::CallerRedemptionPending::Prepared(_)) => Err(ApiError::Invalid(
            "prepared redemption has no proved IO push".into(),
        )),
        None => state::caller_state(caller)
            .last_result
            .map(RedemptionProgress::Completed)
            .ok_or_else(|| ApiError::Invalid("caller has no redemption to resume".into())),
    }
}

async fn activate_pushed(
    pushed: redemption::PushedRedemption,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    let caller = pushed.prepared.caller;
    let mut latest = state::read();
    if let Some(StreamOperation::Redemption(active)) = &latest.active_operation {
        let RedemptionStreamOperation::Active(active) = active.as_ref();
        if active.pushed == pushed {
            return resume(now).await;
        }
        return Ok(RedemptionProgress::Pending);
    }
    if latest.active_operation.is_some() {
        return Ok(RedemptionProgress::Pending);
    }
    if !matches!(
        state::caller_state(caller).pending,
        Some(state::CallerRedemptionPending::Pushed(ref value)) if **value == pushed
    ) {
        return Err(ApiError::Busy);
    }
    let sequence = latest.next_operation_sequence;
    latest.next_operation_sequence.0 = sequence
        .0
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("operation sequence overflow".into()))?;
    latest.active_operation = Some(StreamOperation::Redemption(Box::new(
        RedemptionStreamOperation::Active(Box::new(RedemptionOperation {
            sequence,
            pushed,
            icp_payout: None,
            phase: RedemptionPhase::PayoutOwed,
        })),
    )));
    state::write(latest);
    dispatch_payout_and_complete(sequence, now).await
}

fn active_redemption() -> Result<RedemptionOperation, ApiError> {
    match state::read().active_operation {
        Some(StreamOperation::Redemption(operation)) => match *operation {
            RedemptionStreamOperation::Active(operation) => Ok(*operation),
        },
        _ => Err(ApiError::Invalid("no active redemption payout".into())),
    }
}

fn persist_redemption(operation: RedemptionOperation) {
    let mut latest = state::read();
    latest.active_operation = Some(StreamOperation::Redemption(Box::new(
        RedemptionStreamOperation::Active(Box::new(operation)),
    )));
    state::write(latest);
}

async fn dispatch_payout_and_complete(
    sequence: OperationSequence,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    let mut operation = active_redemption()?;
    if operation.sequence != sequence {
        return Err(ApiError::Busy);
    }
    if operation.phase == RedemptionPhase::PayoutOwed {
        let config = state::read().config;
        let fresh = canonical::claim_snapshot(&config)
            .await
            .map_err(ApiError::Ledger)?;
        if fresh.icp_fee_e8s != operation.pushed.prepared.snapshot.icp_fee_e8s {
            operation.phase = RedemptionPhase::Stuck;
            persist_redemption(operation);
            pause();
            return Err(ApiError::Stuck(
                "ICP payout fee changed after proved IO push".into(),
            ));
        }
        if fresh.liquid_icp_e8s < operation.pushed.prepared.gross_icp_e8s {
            pause();
            return Err(ApiError::Pending(
                "proved IO push has a durable payout obligation awaiting liquid ICP".into(),
            ));
        }
        let prepared = &operation.pushed.prepared;
        operation.icp_payout = Some(
            crate::transfer::TransferAttempt::prepared(OwnTransferIntent::Icrc1 {
                ledger: config.icp_ledger,
                from_subaccount: config
                    .liquid_icp
                    .canonical()
                    .map_err(ApiError::Invalid)?
                    .subaccount,
                to: prepared.account.clone(),
                amount: prepared.net_icp_e8s,
                fee: prepared.snapshot.icp_fee_e8s,
                memo: crate::transfer::deterministic_memo(
                    b"io-redemption-pay-v1",
                    prepared.caller,
                    prepared.request.nonce,
                ),
                created_at_time: now,
            })
            .map_err(ApiError::Invalid)?,
        );
        persist_redemption(operation.clone());
    }
    if operation.phase == RedemptionPhase::PayoutSucceeded {
        return commit_redemption(operation, now).await;
    }
    let attempt = operation
        .icp_payout
        .as_mut()
        .ok_or_else(|| ApiError::Invalid("payout intent is missing".into()))?;
    let epoch = match attempt.state {
        TransferState::Prepared => DispatchEpoch(1),
        TransferState::Submitted {
            epoch,
            last_submitted_at,
            ..
        } => {
            let config = &state::read().config;
            if now.saturating_sub(last_submitted_at) < config.retry_delay_nanos {
                return Err(ApiError::Busy);
            }
            DispatchEpoch(
                epoch
                    .0
                    .checked_add(1)
                    .ok_or_else(|| ApiError::Invalid("payout dispatch epoch overflow".into()))?,
            )
        }
        TransferState::Succeeded { .. } => {
            operation.phase = RedemptionPhase::PayoutSucceeded;
            persist_redemption(operation.clone());
            return commit_redemption(operation, now).await;
        }
        TransferState::Stuck { ref reason } => return Err(ApiError::Stuck(reason.clone())),
    };
    let first = match attempt.state {
        TransferState::Submitted {
            first_submitted_at, ..
        } => first_submitted_at,
        _ => now,
    };
    attempt.state = TransferState::Submitted {
        epoch,
        first_submitted_at: first,
        last_submitted_at: now,
    };
    let intent = attempt.intent.clone();
    operation.phase = RedemptionPhase::PayoutSubmitted;
    persist_redemption(operation.clone());
    let response = submit(&intent).await;
    let mut latest = active_redemption()?;
    if latest.sequence != sequence || latest.phase != RedemptionPhase::PayoutSubmitted {
        return Err(ApiError::Busy);
    }
    let attempt = latest
        .icp_payout
        .as_mut()
        .ok_or_else(|| ApiError::Invalid("payout intent disappeared".into()))?;
    match response {
        Ok(result) => match classify_result(result).map_err(ApiError::Ledger)? {
            ClassifiedResult::Succeeded(block) => {
                attempt.state = TransferState::Succeeded { block };
                latest.phase = RedemptionPhase::PayoutSucceeded;
                persist_redemption(latest.clone());
                commit_redemption(latest, ic_cdk::api::time()).await
            }
            ClassifiedResult::NoEffect(reason) => {
                attempt.state = TransferState::Stuck {
                    reason: reason.clone(),
                };
                latest.phase = RedemptionPhase::Stuck;
                persist_redemption(latest);
                pause();
                Err(ApiError::Stuck(reason))
            }
            ClassifiedResult::Ambiguous(reason) => {
                persist_redemption(latest);
                Err(ApiError::Pending(reason))
            }
        },
        Err(reason) => {
            persist_redemption(latest);
            Err(ApiError::Pending(reason))
        }
    }
}

pub async fn resume(now: u64) -> Result<RedemptionProgress, ApiError> {
    let operation = active_redemption()?;
    match operation.phase {
        RedemptionPhase::PayoutOwed
        | RedemptionPhase::PayoutSubmitted
        | RedemptionPhase::PayoutSucceeded => {
            dispatch_payout_and_complete(operation.sequence, now).await
        }
        RedemptionPhase::Stuck => Err(ApiError::Stuck(
            "exact payout proof or reviewed recovery is required".into(),
        )),
    }
}

async fn commit_redemption(
    operation: RedemptionOperation,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    let payout = operation
        .icp_payout
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("payout intent is missing".into()))?;
    let prepared = &operation.pushed.prepared;
    let result = RedemptionResult {
        request_fingerprint: prepared.request_fingerprint.clone(),
        nonce: prepared.request.nonce,
        io_block: operation.pushed.io_block,
        icp_block: payout.succeeded_block().map_err(ApiError::Invalid)?,
        net_icp_e8s: prepared.net_icp_e8s,
        gross_icp_e8s: prepared.gross_icp_e8s,
        io_fee_e8s: prepared.snapshot.io_fee_e8s,
        icp_fee_e8s: prepared.snapshot.icp_fee_e8s,
        completed_at_nanos: now,
    };
    let mut caller_state = state::caller_state(prepared.caller);
    if caller_state.next_nonce != prepared.request.nonce {
        return caller_state
            .last_result
            .map(RedemptionProgress::Completed)
            .ok_or(ApiError::Busy);
    }
    caller_state.next_nonce = caller_state
        .next_nonce
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("caller nonce overflow".into()))?;
    caller_state.pending = None;
    caller_state.last_request_fingerprint = Some(prepared.request_fingerprint.clone());
    caller_state.last_result = Some(result.clone());
    state::set_caller_state(prepared.caller, caller_state);
    maybe_trap_after_caller_result_write();
    let mut latest = state::read();
    if !matches!(
        &latest.active_operation,
        Some(StreamOperation::Redemption(active))
            if matches!(active.as_ref(), RedemptionStreamOperation::Active(value) if **value == operation)
    ) {
        return Err(ApiError::Busy);
    }
    latest.active_operation = None;
    state::write(latest);
    crate::reward_timer::install_for_ready_state();
    Ok(RedemptionProgress::Completed(result))
}

pub(crate) fn pause() {
    let mut state = state::read();
    state.lifecycle = Lifecycle::Paused;
    state::write(state);
}

pub async fn resume_stream(now: u64) -> Result<StreamProgress, ApiError> {
    match state::read().active_operation {
        Some(StreamOperation::Redemption(operation)) => match *operation {
            RedemptionStreamOperation::Active(_) => {
                resume(now).await.map(StreamProgress::Redemption)
            }
        },
        Some(StreamOperation::ClaimReceipt(_)) => {
            receipt::resume(now).await.map(StreamProgress::ClaimReceipt)
        }
        Some(StreamOperation::PoolTopUp(_)) => {
            crate::pool_reconciliation::resume(now).await?;
            Ok(StreamProgress::BackingReconciliation)
        }
        None => Ok(StreamProgress::Idle),
    }
}

pub async fn prove_active_transfer(block_index: u128) -> Result<(), ApiError> {
    if matches!(
        state::read().active_operation,
        Some(StreamOperation::ClaimReceipt(_))
    ) {
        receipt::prove_recipient(block_index).await?;
        return Ok(());
    }
    if matches!(
        state::read().active_operation,
        Some(StreamOperation::PoolTopUp(_))
    ) {
        return crate::pool_reconciliation::prove_transfer(block_index).await;
    }
    let operation = active_redemption()?;
    let attempt = operation
        .icp_payout
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("payout intent is missing".into()))?;
    if operation.phase != RedemptionPhase::Stuck
        || !matches!(attempt.state, TransferState::Stuck { .. })
    {
        return Err(ApiError::Invalid(
            "only a Stuck ICP payout accepts proof".into(),
        ));
    }
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
    } = &attempt.intent;
    let source = Account {
        owner: ic_cdk::api::canister_self(),
        subaccount: (*from_subaccount != [0; 32]).then(|| from_subaccount.to_vec()),
    };
    if exact.from != canonical::icp_account_identifier(&source).map_err(ApiError::Invalid)?
        || exact.to != canonical::icp_account_identifier(to).map_err(ApiError::Invalid)?
        || exact.amount_e8s != *amount
        || exact.fee_e8s != *fee
        || exact.native_memo_u64 != 0
        || exact.icrc1_memo.as_deref() != Some(memo.as_slice())
        || exact.created_at_time != *created_at_time
        || exact.spender.is_some()
    {
        return Err(ApiError::Invalid(
            "exact block does not match stuck payout".into(),
        ));
    }
    let mut latest = active_redemption()?;
    let target = latest
        .icp_payout
        .as_mut()
        .ok_or_else(|| ApiError::Invalid("payout intent disappeared".into()))?;
    if target.intent != attempt.intent {
        return Err(ApiError::Busy);
    }
    target.state = TransferState::Succeeded { block: block_index };
    latest.phase = RedemptionPhase::PayoutSucceeded;
    persist_redemption(latest);
    Ok(())
}
