use candid::{CandidType, Nat, Principal};
use ic_cdk::call::Call;
use serde::Deserialize;

use crate::{
    canonical,
    receipt::{
        receipt_memo, CompleteLiquidReceiptArgs, LiquidReceiptOperation, LiquidReceiptPermit,
        PrepareLiquidReceiptArgs, ReceiptKind,
    },
    redemption::{self, RedeemArgs, RedemptionPhase},
    state::{self, Account, Lifecycle, RedemptionResult, StreamOperation, StreamStateV1},
    transfer::{
        classify_result, IcrcTransferArg, IcrcTransferFromArg, TransferResult, TransferState,
    },
};

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ApiError {
    Anonymous,
    Unauthorized,
    Inert,
    Paused,
    Busy,
    WrongNonce { expected: u64 },
    Invalid(String),
    Ledger(String),
    Stuck(String),
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
            Some(if operation.proved_block.is_some() {
                "Proved".into()
            } else {
                "AwaitingProof".into()
            }),
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
        Lifecycle::Inert => Err(ApiError::Inert),
        Lifecycle::Paused => Err(ApiError::Paused),
    }
}

async fn ledger_transfer_from(
    ledger: Principal,
    args: IcrcTransferFromArg,
) -> Result<TransferResult, String> {
    Call::bounded_wait(ledger, "icrc2_transfer_from")
        .with_arg(args)
        .await
        .map_err(|error| format!("transfer_from call failed: {error:?}"))?
        .candid()
        .map_err(|error| format!("transfer_from decode failed: {error:?}"))
}

async fn ledger_transfer(
    ledger: Principal,
    method: &str,
    args: IcrcTransferArg,
) -> Result<TransferResult, String> {
    Call::bounded_wait(ledger, method)
        .with_arg(args)
        .await
        .map_err(|error| format!("{method} call failed: {error:?}"))?
        .candid()
        .map_err(|error| format!("{method} decode failed: {error:?}"))
}

pub async fn redeem(
    caller: Principal,
    args: RedeemArgs,
    now: u64,
) -> Result<RedemptionResult, ApiError> {
    if caller == Principal::anonymous() {
        return Err(ApiError::Anonymous);
    }
    let state = state::read();
    require_ready(&state)?;
    if state.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    if args.io_amount_e8s < state.config.minimum_redemption_io_e8s {
        return Err(ApiError::Invalid(
            "redemption below configured minimum".into(),
        ));
    }
    let caller_state = state::caller_state(caller);
    if args.nonce != caller_state.next_nonce {
        if caller_state
            .last_result
            .as_ref()
            .is_some_and(|result| result.nonce == args.nonce)
        {
            return Ok(caller_state.last_result.expect("checked above"));
        }
        return Err(ApiError::WrongNonce {
            expected: caller_state.next_nonce,
        });
    }
    let snapshot = canonical::redemption_snapshot(&state.config)
        .await
        .map_err(ApiError::Ledger)?;
    if snapshot.io_fee_e8s != state.config.expected_io_fee_e8s
        || snapshot.icp_fee_e8s != state.config.expected_icp_fee_e8s
    {
        return Err(ApiError::Invalid(
            "configured fee differs from canonical ledger fee; pause and upgrade config".into(),
        ));
    }
    if state::read().active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    let operation = redemption::calculate(caller, &args, snapshot, &state.config, now)
        .map_err(ApiError::Invalid)?;
    let mut latest = state::read();
    require_ready(&latest)?;
    if latest.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    latest.active_operation = Some(StreamOperation::Redemption(Box::new(operation)));
    state::write(latest);
    progress_redemption().await
}

async fn progress_redemption() -> Result<RedemptionResult, ApiError> {
    let mut operation = match state::read().active_operation {
        Some(StreamOperation::Redemption(operation)) => *operation,
        _ => return Err(ApiError::Invalid("no active redemption".into())),
    };
    if matches!(
        operation.phase,
        RedemptionPhase::Prepared | RedemptionPhase::PullingIo
    ) {
        operation.phase = RedemptionPhase::PullingIo;
        operation.io_pull.state = TransferState::Submitted;
        persist_redemption(operation.clone());
        let result = ledger_transfer_from(
            operation.io_pull.ledger,
            IcrcTransferFromArg {
                spender_subaccount: operation.io_pull.source_subaccount.clone(),
                from: operation.account.clone(),
                to: operation.io_pull.destination.clone(),
                amount: Nat::from(operation.io_pull.amount_e8s),
                fee: Some(Nat::from(operation.io_pull.fee_e8s)),
                memo: Some(operation.io_pull.memo.clone()),
                created_at_time: Some(operation.io_pull.created_at_time_nanos),
            },
        )
        .await;
        match result {
            Ok(result) => match classify_result(result, &mut operation.io_pull) {
                Ok(_) => operation.phase = RedemptionPhase::IoInReserve,
                Err(error) if !matches!(operation.io_pull.state, TransferState::Stuck { .. }) => {
                    let mut state = state::read();
                    state.active_operation = None;
                    state::write(state);
                    return Err(ApiError::Ledger(error));
                }
                Err(error) => {
                    operation.phase = RedemptionPhase::Stuck;
                    persist_redemption(operation);
                    pause();
                    return Err(ApiError::Stuck(error));
                }
            },
            Err(error) => {
                operation.io_pull.state = TransferState::Stuck {
                    reason: error.clone(),
                };
                operation.phase = RedemptionPhase::Stuck;
                persist_redemption(operation);
                pause();
                return Err(ApiError::Stuck(error));
            }
        }
        persist_redemption(operation.clone());
    }
    if matches!(
        operation.phase,
        RedemptionPhase::IoInReserve | RedemptionPhase::PayingIcp
    ) {
        operation.phase = RedemptionPhase::PayingIcp;
        operation.icp_payout.state = TransferState::Submitted;
        persist_redemption(operation.clone());
        let result = ledger_transfer(
            operation.icp_payout.ledger,
            "icrc1_transfer",
            IcrcTransferArg {
                from_subaccount: operation.icp_payout.source_subaccount.clone(),
                to: operation.account.clone(),
                amount: Nat::from(operation.net_icp_e8s),
                fee: Some(Nat::from(operation.icp_payout.fee_e8s)),
                memo: Some(operation.icp_payout.memo.clone()),
                created_at_time: Some(operation.icp_payout.created_at_time_nanos),
            },
        )
        .await;
        let block = match result {
            Ok(result) => classify_result(result, &mut operation.icp_payout),
            Err(error) => {
                operation.icp_payout.state = TransferState::Stuck {
                    reason: error.clone(),
                };
                Err(error)
            }
        };
        let icp_block = match block {
            Ok(block) => block,
            Err(error) => {
                operation.phase = RedemptionPhase::Stuck;
                persist_redemption(operation);
                pause();
                return Err(ApiError::Stuck(error));
            }
        };
        operation.phase = RedemptionPhase::Completed;
        let io_block = succeeded_block(&operation.io_pull)?;
        let result = RedemptionResult {
            nonce: operation.nonce,
            io_block,
            icp_block,
            net_icp_e8s: operation.net_icp_e8s,
        };
        let post = canonical::redemption_snapshot(&state::read().config)
            .await
            .map_err(ApiError::Ledger)?;
        if let Err(error) = redemption::verify_postconditions(&operation, &post) {
            operation.phase = RedemptionPhase::Stuck;
            persist_redemption(operation);
            pause();
            return Err(ApiError::Stuck(error));
        }
        let mut caller_state = state::caller_state(operation.caller);
        caller_state.next_nonce = caller_state
            .next_nonce
            .checked_add(1)
            .ok_or_else(|| ApiError::Invalid("caller nonce overflow".into()))?;
        caller_state.last_result = Some(result.clone());
        state::set_caller_state(operation.caller, caller_state);
        let mut state = state::read();
        state.active_operation = None;
        state::write(state);
        return Ok(result);
    }
    Err(ApiError::Stuck("redemption cannot progress".into()))
}

fn succeeded_block(attempt: &crate::transfer::OwnTransferAttempt) -> Result<u128, ApiError> {
    match attempt.state {
        TransferState::Succeeded { block } => Ok(block),
        _ => Err(ApiError::Invalid("transfer lacks success evidence".into())),
    }
}

fn persist_redemption(operation: crate::redemption::RedemptionOperation) {
    let mut state = state::read();
    state.active_operation = Some(StreamOperation::Redemption(Box::new(operation)));
    state::write(state);
}

fn pause() {
    let mut state = state::read();
    state.lifecycle = Lifecycle::Paused;
    state::write(state);
}

pub async fn resume() -> Result<(), ApiError> {
    match state::read().active_operation {
        Some(StreamOperation::Redemption(operation)) => {
            if operation.phase != RedemptionPhase::Stuck {
                progress_redemption().await.map(|_| ())
            } else {
                Err(ApiError::Stuck(
                    "exact block proof or governance upgrade required".into(),
                ))
            }
        }
        Some(StreamOperation::LiquidReceipt(_)) => Err(ApiError::Invalid(
            "receipt awaits exact completion proof".into(),
        )),
        None => Ok(()),
    }
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
    if state.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    if args.receipt_sequence != state.next_nns_receipt_sequence
        || args.liquid_amount_e8s == 0
        || args.source_operation_id.is_empty()
    {
        return Err(ApiError::Invalid(
            "invalid receipt sequence or intent".into(),
        ));
    }
    if args.receipt_kind == ReceiptKind::TwoWeekMaturity {
        let pending_generation = state.pending_reward_cohort.as_ref().map(|c| c.generation);
        if args.cohort_generation != pending_generation {
            return Err(ApiError::Invalid(
                "receipt does not match pending cohort".into(),
            ));
        }
    } else if args.cohort_generation.is_some() {
        return Err(ApiError::Invalid(
            "Jupiter receipt cannot name a cohort".into(),
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
            source: Account {
                owner: state.config.nns_receipt_source.owner,
                subaccount: state.config.nns_receipt_source.subaccount.clone(),
            },
            destination: permit.destination.clone(),
            memo,
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
    let state = state::read();
    if caller != state.config.nns_manager {
        return Err(ApiError::Unauthorized);
    }
    let operation = match state.active_operation {
        Some(StreamOperation::LiquidReceipt(operation))
            if operation.sequence == args.receipt_sequence =>
        {
            operation
        }
        _ => return Err(ApiError::Invalid("no matching liquid receipt".into())),
    };
    let transfer = canonical::exact_icrc_transfer(state.config.icp_ledger, args.block_index)
        .await
        .map_err(ApiError::Ledger)?;
    if transfer.from != operation.source
        || transfer.to != operation.destination
        || transfer.amount_e8s != operation.liquid_amount_e8s
        || transfer.fee_e8s != Some(state.config.expected_icp_fee_e8s)
        || transfer.memo.as_deref() != Some(operation.memo.as_slice())
        || transfer.created_at_time.is_none()
        || transfer.spender.is_some()
    {
        return Err(ApiError::Invalid(
            "canonical block does not match persisted liquid receipt intent".into(),
        ));
    }
    let mut state = state::read();
    let still_matches = matches!(
        &state.active_operation,
        Some(StreamOperation::LiquidReceipt(current))
            if current.sequence == operation.sequence && current.proved_block.is_none()
    );
    if !still_matches {
        return Err(ApiError::Busy);
    }
    state.next_nns_receipt_sequence = state
        .next_nns_receipt_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("receipt sequence overflow".into()))?;
    state.active_operation = None;
    state::write(state);
    Ok(())
}

pub async fn prove_active_transfer(_block_index: u128) -> Result<(), ApiError> {
    Err(ApiError::Invalid(
        "exact current/archive decoder not yet connected".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_does_not_claim_canonical_balances() {
        let fields = format!("{:?}", std::any::type_name::<Status>());
        assert!(!fields.contains("balance"));
    }
}
