#[cfg(target_family = "wasm")]
use crate::clients::{icp_ledger, io_ledger, sns_governance};
#[cfg(target_family = "wasm")]
use crate::state::JUPITER_FAUCET_SOURCE;
use crate::DebugTickOutcome;
#[cfg(target_family = "wasm")]
use crate::{
    ApiIoRecipientPolicy, ApiStreamKind, OperationPhase, StreamManagerError, StreamOperation,
    StreamOperationKind, TransferStatus, TwoWeekRecipientTransfer, CANISTER_STATE,
};
use candid::CandidType;
#[cfg(target_family = "wasm")]
use candid::Principal;
#[cfg(any(target_family = "wasm", test))]
use io_ledger_types::AccountHistoryPageOrder;
#[cfg(any(target_family = "wasm", test))]
use io_ledger_types::AccountHistoryScanState;
#[cfg(any(target_family = "wasm", test))]
use io_ledger_types::{
    duplicate_matches_expected, LedgerBlock, LedgerTransferError, LedgerTransferRequest,
    LedgerTransferSuccess,
};
#[cfg(target_family = "wasm")]
use io_ledger_types::{Account, IndexScanRequest};
use io_ledger_types::{BlockIndex, IndexError, IndexScanResult};
#[cfg(target_family = "wasm")]
use io_ledger_types::{IcrcIndexCanisterClient, LedgerIndexClient, LedgerTransferClient};
use serde::Deserialize;

pub const STREAM_MANAGER_DEPOSIT_ACCOUNT: &str = "stream_manager_deposit";
pub const REDEMPTION_ACCOUNT: &str = "redemption";
pub const PROTOCOL_RESERVE_ACCOUNT: &str = "protocol_reserve";
pub const REDEMPTION_PAYOUT_MEMO: &str = "redemption_payout";
pub const REDEEMED_IO_MEMO: &str = "redeemed_io_to_reserve";
pub const TWO_WEEK_REWARD_ACCOUNT_PREFIX: &str = "sns_neuron_";

#[cfg(any(target_family = "wasm", test))]
fn legacy_icp_account_history_scan_state(cursor: u64) -> AccountHistoryScanState {
    AccountHistoryScanState {
        cursor: io_ledger_types::AccountHistoryCursor {
            order: Some(AccountHistoryPageOrder::Descending),
            latest_cursor: Some(BlockIndex(cursor)),
            oldest_cursor: Some(BlockIndex(cursor)),
            backfill_complete: true,
        },
        status: Default::default(),
    }
}

#[cfg(target_family = "wasm")]
fn legacy_io_account_history_scan_state(cursor: u64) -> AccountHistoryScanState {
    AccountHistoryScanState {
        cursor: io_ledger_types::AccountHistoryCursor {
            order: Some(AccountHistoryPageOrder::Ascending),
            latest_cursor: Some(BlockIndex(cursor)),
            oldest_cursor: Some(BlockIndex(cursor)),
            backfill_complete: true,
        },
        status: Default::default(),
    }
}

#[cfg(target_family = "wasm")]
fn no_new_page_errors(outcome: &DebugTickOutcome, page_error_count: usize) -> bool {
    outcome.errors.len() == page_error_count
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SchedulerTickOutcome {
    pub scanned_jupiter_faucet_deposits: u64,
    pub scanned_nns_maturity_deposits: u64,
    pub scanned_redemption_transfers: u64,
    pub processed_authorized_streams: u64,
    pub planned_steps: Vec<String>,
}

impl SchedulerTickOutcome {
    fn no_work_configured() -> Self {
        Self {
            scanned_jupiter_faucet_deposits: 0,
            scanned_nns_maturity_deposits: 0,
            scanned_redemption_transfers: 0,
            processed_authorized_streams: 0,
            planned_steps: vec![
                "scan ICP ledger/index for Jupiter Faucet deposits".to_string(),
                "scan ICP ledger/index for NNS maturity deposits".to_string(),
                "scan IO ledger/index for user redemption transfers".to_string(),
                "classify observed flows before internal processing".to_string(),
            ],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(any(target_family = "wasm", test))]
enum BoundaryTransferDecision {
    Succeeded(u64),
    Retryable(String),
}

#[cfg(any(target_family = "wasm", test))]
fn boundary_error_message(err: &LedgerTransferError) -> String {
    match err {
        LedgerTransferError::TemporarilyUnavailable => {
            "ledger transfer temporarily unavailable".to_string()
        }
        LedgerTransferError::CanisterCallFailed { method, message } => {
            format!("ledger transfer call {method} failed: {message}")
        }
        LedgerTransferError::BadFee { expected_fee_e8s } => {
            format!("ledger transfer bad fee; expected {expected_fee_e8s} e8s")
        }
        LedgerTransferError::InsufficientFunds { balance_e8s } => {
            format!("ledger transfer insufficient funds; balance {balance_e8s} e8s")
        }
        LedgerTransferError::Duplicate { duplicate_of } => {
            format!("ledger transfer duplicate at block {}", duplicate_of.0)
        }
        err => format!("ledger transfer failed: {err:?}"),
    }
}

#[cfg(any(target_family = "wasm", test))]
fn classify_boundary_transfer_result(
    expected: &LedgerTransferRequest,
    result: Result<LedgerTransferSuccess, LedgerTransferError>,
    duplicate_block: Option<&LedgerBlock>,
) -> BoundaryTransferDecision {
    match result {
        Ok(success) => BoundaryTransferDecision::Succeeded(success.block_index.0),
        Err(LedgerTransferError::Duplicate { .. }) => match duplicate_block {
            Some(block) => match duplicate_matches_expected(expected, block) {
                Ok(block) => BoundaryTransferDecision::Succeeded(block.0),
                Err(proof) => BoundaryTransferDecision::Retryable(format!(
                    "duplicate transfer did not match expected amount/account/memo: {proof:?}"
                )),
            },
            None => BoundaryTransferDecision::Retryable(
                "duplicate transfer could not be proven against expected amount/account/memo"
                    .to_string(),
            ),
        },
        Err(err) => BoundaryTransferDecision::Retryable(boundary_error_message(&err)),
    }
}

#[cfg(target_family = "wasm")]
fn principal(text: &Option<String>) -> Option<Principal> {
    text.as_deref()
        .and_then(|value| Principal::from_text(value).ok())
}

#[cfg(target_family = "wasm")]
fn kind_from_api(kind: ApiStreamKind) -> StreamOperationKind {
    match kind {
        ApiStreamKind::JupiterFaucet => StreamOperationKind::JupiterFaucetStream,
        ApiStreamKind::TwoYearMaturity => StreamOperationKind::TwoYearMaturityStream,
        ApiStreamKind::TwoWeekMaturity => StreamOperationKind::TwoWeekMaturityStream,
    }
}

pub fn scheduler_tick_plan_only() -> SchedulerTickOutcome {
    SchedulerTickOutcome::no_work_configured()
}

pub fn boundary_cursor_after_contiguous_page(
    current: Option<BlockIndex>,
    result: &IndexScanResult,
) -> Result<Option<BlockIndex>, IndexError> {
    if let (Some(requested), Some(tip)) = (current, result.index_tip) {
        if tip < requested {
            return Err(IndexError::IndexLag {
                requested,
                tip: Some(tip),
            });
        }
    }

    if result.archive_required {
        return Err(IndexError::ArchiveRequired {
            from: current.unwrap_or(BlockIndex(0)),
        });
    }

    let mut expected_next = current.map(|block| block.0.saturating_add(1));
    let mut highest = current;
    for tx in &result.transactions {
        if let Some(cursor) = current {
            if tx.block_index <= cursor {
                highest = Some(cursor);
                continue;
            }
        }
        if let Some(expected) = expected_next {
            if tx.block_index.0 != expected {
                return Err(IndexError::MissingBlock {
                    block_index: BlockIndex(expected),
                });
            }
        }
        expected_next = Some(tx.block_index.0.saturating_add(1));
        highest = Some(tx.block_index);
    }

    Ok(highest)
}

pub fn boundary_cursor_after_account_page(
    current: Option<BlockIndex>,
    result: &IndexScanResult,
) -> Result<Option<BlockIndex>, IndexError> {
    if let (Some(requested), Some(tip)) = (current, result.index_tip) {
        if tip < requested {
            return Err(IndexError::IndexLag {
                requested,
                tip: Some(tip),
            });
        }
    }

    if result.archive_required {
        return Err(IndexError::ArchiveRequired {
            from: current.unwrap_or(BlockIndex(0)),
        });
    }

    let mut last = None;
    let mut highest = current;
    for tx in &result.transactions {
        if let Some(previous) = last {
            if tx.block_index <= previous {
                return Err(IndexError::MissingBlock {
                    block_index: tx.block_index,
                });
            }
        }
        last = Some(tx.block_index);

        if current.is_some_and(|cursor| tx.block_index <= cursor) {
            continue;
        }

        highest = Some(highest.map_or(tx.block_index, |cursor| cursor.max(tx.block_index)));
    }

    Ok(highest)
}

#[cfg(target_family = "wasm")]
fn mock_transfer_request(
    from: &str,
    to: &str,
    amount_e8s: u128,
    memo: &str,
) -> LedgerTransferRequest {
    LedgerTransferRequest {
        from_subaccount: Some(icp_ledger::mock_subaccount(from)),
        to: icp_ledger::mock_account(to),
        amount_e8s,
        fee_e8s: None,
        memo: Some(io_ledger_types::Memo::from(memo)),
        created_at_time: None,
    }
}

#[cfg(target_family = "wasm")]
async fn duplicate_block(canister: Principal, block_index: BlockIndex) -> Option<LedgerBlock> {
    icp_ledger::debug_get_transactions(canister)
        .await
        .ok()?
        .into_iter()
        .find(|tx| tx.block_index == block_index.0)
        .map(|tx| tx.into_boundary_block())
}

#[cfg(target_family = "wasm")]
async fn classify_mock_transfer(
    canister: Principal,
    request: &LedgerTransferRequest,
    result: Result<LedgerTransferSuccess, LedgerTransferError>,
) -> BoundaryTransferDecision {
    let duplicate = match result {
        Err(LedgerTransferError::Duplicate { duplicate_of }) => {
            duplicate_block(canister, duplicate_of).await
        }
        _ => None,
    };
    classify_boundary_transfer_result(request, result, duplicate.as_ref())
}

#[cfg(target_family = "wasm")]
fn boundary_transaction_to_mock_transaction(
    tx: io_ledger_types::IndexTransaction,
) -> icp_ledger::LedgerTransaction {
    icp_ledger::LedgerTransaction {
        from: tx
            .transaction
            .from
            .as_ref()
            .map(icp_ledger::mock_label_from_account)
            .unwrap_or_default(),
        to: tx
            .transaction
            .to
            .as_ref()
            .map(icp_ledger::mock_label_from_account)
            .unwrap_or_default(),
        amount_e8s: tx.transaction.amount_e8s,
        memo: tx
            .transaction
            .memo
            .map(|memo| String::from_utf8_lossy(&memo.0).into_owned())
            .unwrap_or_default(),
        block_index: tx.block_index.0,
        timestamp: tx.transaction.timestamp_nanos,
    }
}

#[cfg(target_family = "wasm")]
async fn scan_account_through_index(
    index_canister: Principal,
    account: Account,
    scan_state: AccountHistoryScanState,
) -> Result<
    (
        Vec<icp_ledger::LedgerTransaction>,
        AccountHistoryScanState,
        Option<u64>,
    ),
    String,
> {
    let client = IcrcIndexCanisterClient {
        canister: index_canister,
    };
    let requested_start = scan_state.next_request_start();
    let page = client
        .get_account_transactions(IndexScanRequest {
            start: requested_start,
            limit: 100,
            account_filter: Some(account),
        })
        .await
        .map_err(|err| format!("ledger index scan failed: {err:?}"))?;
    let outcome = scan_state
        .observe_page(&page, requested_start, 100, 1, 1, Some(ic_cdk::api::time()))
        .map_err(|err| format!("ledger index cursor validation failed: {err:?}"))?;
    let latest = outcome.next_state.cursor.latest_cursor.map(|block| block.0);
    Ok((
        outcome
            .transactions_chronological
            .into_iter()
            .map(boundary_transaction_to_mock_transaction)
            .collect(),
        outcome.next_state,
        latest,
    ))
}

#[cfg(target_family = "wasm")]
async fn retry_pending_two_week_streams(
    io_canister: Principal,
    outcome: &mut DebugTickOutcome,
) -> bool {
    loop {
        let next = CANISTER_STATE.with(|cell| {
            let state = cell.borrow();
            state.operation_journal.iter().find_map(|op| {
                if op.kind != StreamOperationKind::TwoWeekMaturityStream
                    || op.phase == OperationPhase::Completed
                {
                    return None;
                }
                op.two_week_recipients
                    .iter()
                    .position(|recipient| recipient.transfer_status != TransferStatus::Succeeded)
                    .map(|index| {
                        (
                            op.operation_id.clone(),
                            index,
                            op.two_week_recipients[index].clone(),
                        )
                    })
            })
        });
        let Some((operation_id, recipient_index, recipient)) = next else {
            break;
        };

        let to = format!("{TWO_WEEK_REWARD_ACCOUNT_PREFIX}{}", recipient.neuron_id);
        let request = mock_transfer_request(
            PROTOCOL_RESERVE_ACCOUNT,
            &to,
            recipient.amount_e8s,
            &operation_id,
        );
        let client = io_ledger::MockLedgerCanisterClient {
            canister: io_canister,
            fee_e8s: 0,
        };
        match classify_mock_transfer(
            io_canister,
            &request,
            client.transfer(request.clone()).await,
        )
        .await
        {
            BoundaryTransferDecision::Succeeded(block) => CANISTER_STATE.with(|cell| {
                if let Some(op) = cell
                    .borrow_mut()
                    .operation_journal
                    .iter_mut()
                    .find(|op| op.operation_id == operation_id)
                {
                    if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                        recipient.transfer_status = TransferStatus::Succeeded;
                        recipient.transfer_block_index = Some(block);
                        recipient.last_error = None;
                    }
                    op.mark_updated(OperationPhase::PartiallyDistributed);
                }
            }),
            BoundaryTransferDecision::Retryable(err) => {
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|op| op.operation_id == operation_id)
                    {
                        if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                            recipient.transfer_status = TransferStatus::FailedRetryable;
                            recipient.last_error = Some(err.clone());
                        }
                        op.mark_retryable_error(err.clone(), OperationPhase::PartiallyDistributed);
                    }
                });
                outcome.errors.push(err);
                return false;
            }
        }
    }

    loop {
        let completed = CANISTER_STATE.with(|cell| {
            let state = cell.borrow();
            state
                .operation_journal
                .iter()
                .position(|op| {
                    op.kind == StreamOperationKind::TwoWeekMaturityStream
                        && op.phase != OperationPhase::Completed
                        && op
                            .two_week_recipients
                            .iter()
                            .all(|recipient| recipient.transfer_status == TransferStatus::Succeeded)
                })
                .map(|index| state.operation_journal[index].clone())
        });
        let Some(op) = completed else {
            break;
        };
        let committed = CANISTER_STATE.with(|cell| {
            cell.borrow_mut()
                .manager
                .commit_previewed_stream(op.operation_id.clone(), op.post_state.into())
        });
        match committed {
            Ok(()) => {
                mark_completed(&op.operation_id);
                outcome.processed_authorized_streams += 1;
                outcome.io_issued_e8s = outcome.io_issued_e8s.saturating_add(op.io_issued_e8s);
            }
            Err(StreamManagerError::DuplicateTransaction) => mark_completed(&op.operation_id),
            Err(err) => {
                outcome
                    .errors
                    .push(format!("stream {}: {err:?}", op.operation_id));
                return false;
            }
        }
    }

    CANISTER_STATE.with(|cell| {
        !cell.borrow().operation_journal.iter().any(|op| {
            op.kind == StreamOperationKind::TwoWeekMaturityStream
                && op.phase != OperationPhase::Completed
        })
    })
}

#[cfg(target_family = "wasm")]
async fn retry_pending_io_issuances(
    io_canister: Principal,
    outcome: &mut DebugTickOutcome,
) -> bool {
    loop {
        let pending = CANISTER_STATE.with(|cell| {
            cell.borrow().operation_journal.iter().find_map(|op| {
                (op.kind == StreamOperationKind::JupiterFaucetStream
                    && op.phase != OperationPhase::Completed)
                    .then(|| op.clone())
            })
        });
        let Some(op) = pending else {
            return true;
        };

        if op.downstream_io_issuance_block.is_none() {
            let request = mock_transfer_request(
                PROTOCOL_RESERVE_ACCOUNT,
                JUPITER_FAUCET_SOURCE,
                op.io_issued_e8s,
                &op.operation_id,
            );
            let client = io_ledger::MockLedgerCanisterClient {
                canister: io_canister,
                fee_e8s: 0,
            };
            match classify_mock_transfer(
                io_canister,
                &request,
                client.transfer(request.clone()).await,
            )
            .await
            {
                BoundaryTransferDecision::Succeeded(block) => {
                    mark_io_issuance(&op.operation_id, block)
                }
                BoundaryTransferDecision::Retryable(err) => {
                    mark_operation_error(
                        &op.operation_id,
                        err.clone(),
                        OperationPhase::AwaitingIoIssuance,
                    );
                    outcome.errors.push(err);
                    return false;
                }
            }
        }

        let committed = CANISTER_STATE.with(|cell| {
            cell.borrow_mut()
                .manager
                .commit_previewed_stream(op.operation_id.clone(), op.post_state.into())
        });
        match committed {
            Ok(()) => {
                mark_completed(&op.operation_id);
                outcome.processed_authorized_streams += 1;
                outcome.io_issued_e8s = outcome.io_issued_e8s.saturating_add(op.io_issued_e8s);
            }
            Err(StreamManagerError::DuplicateTransaction) => mark_completed(&op.operation_id),
            Err(err) => {
                outcome
                    .errors
                    .push(format!("stream {}: {err:?}", op.operation_id));
                return false;
            }
        }
    }
}

#[cfg(target_family = "wasm")]
async fn retry_pending_redemptions(
    icp_canister: Option<Principal>,
    io_canister: Principal,
    outcome: &mut DebugTickOutcome,
) -> bool {
    loop {
        let pending = CANISTER_STATE.with(|cell| {
            cell.borrow().operation_journal.iter().find_map(|op| {
                (op.kind == StreamOperationKind::Redemption
                    && op.phase != OperationPhase::Completed)
                    .then(|| op.clone())
            })
        });
        let Some(op) = pending else {
            return true;
        };

        if op.icp_payout_status != TransferStatus::Succeeded {
            let Some(icp_canister) = icp_canister else {
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|pending| pending.operation_id == op.operation_id)
                    {
                        op.icp_payout_status = TransferStatus::FailedRetryable;
                        op.mark_retryable_error(
                            "missing ICP payout ledger principal".to_string(),
                            OperationPhase::AwaitingIcpPayout,
                        );
                    }
                });
                outcome
                    .errors
                    .push("missing ICP payout ledger principal".to_string());
                return false;
            };

            let user_account = op.user_account.clone().unwrap_or_default();
            let request = mock_transfer_request(
                STREAM_MANAGER_DEPOSIT_ACCOUNT,
                &user_account,
                op.amount_e8s,
                REDEMPTION_PAYOUT_MEMO,
            );
            let client = icp_ledger::MockLedgerCanisterClient {
                canister: icp_canister,
                fee_e8s: 0,
            };
            match classify_mock_transfer(
                icp_canister,
                &request,
                client.transfer(request.clone()).await,
            )
            .await
            {
                BoundaryTransferDecision::Succeeded(block) => {
                    CANISTER_STATE.with(|cell| {
                        if let Some(op) = cell
                            .borrow_mut()
                            .operation_journal
                            .iter_mut()
                            .find(|pending| pending.operation_id == op.operation_id)
                        {
                            op.icp_payout_status = TransferStatus::Succeeded;
                            op.icp_payout_block = Some(block);
                            op.mark_updated(OperationPhase::AwaitingIoReturn);
                        }
                    });
                }
                BoundaryTransferDecision::Retryable(err) => {
                    CANISTER_STATE.with(|cell| {
                        if let Some(op) = cell
                            .borrow_mut()
                            .operation_journal
                            .iter_mut()
                            .find(|pending| pending.operation_id == op.operation_id)
                        {
                            op.icp_payout_status = TransferStatus::FailedRetryable;
                            op.mark_retryable_error(err.clone(), OperationPhase::AwaitingIcpPayout);
                        }
                    });
                    outcome.errors.push(err);
                    return false;
                }
            }
            continue;
        }

        if op.io_return_status != TransferStatus::Succeeded {
            let request = mock_transfer_request(
                REDEMPTION_ACCOUNT,
                PROTOCOL_RESERVE_ACCOUNT,
                op.io_amount,
                REDEEMED_IO_MEMO,
            );
            let client = io_ledger::MockLedgerCanisterClient {
                canister: io_canister,
                fee_e8s: 0,
            };
            match classify_mock_transfer(
                io_canister,
                &request,
                client.transfer(request.clone()).await,
            )
            .await
            {
                BoundaryTransferDecision::Succeeded(block) => {
                    CANISTER_STATE.with(|cell| {
                        if let Some(op) = cell
                            .borrow_mut()
                            .operation_journal
                            .iter_mut()
                            .find(|pending| pending.operation_id == op.operation_id)
                        {
                            op.io_return_status = TransferStatus::Succeeded;
                            op.io_return_block = Some(block);
                            op.mark_updated(OperationPhase::AwaitingIoReturn);
                        }
                    });
                }
                BoundaryTransferDecision::Retryable(err) => {
                    CANISTER_STATE.with(|cell| {
                        if let Some(op) = cell
                            .borrow_mut()
                            .operation_journal
                            .iter_mut()
                            .find(|pending| pending.operation_id == op.operation_id)
                        {
                            op.io_return_status = TransferStatus::FailedRetryable;
                            op.mark_retryable_error(err.clone(), OperationPhase::AwaitingIoReturn);
                        }
                    });
                    outcome.errors.push(err);
                    return false;
                }
            }
        }

        let committed = CANISTER_STATE.with(|cell| {
            cell.borrow_mut()
                .manager
                .commit_previewed_redemption(op.operation_id.clone(), op.post_state.into())
        });
        match committed {
            Ok(()) => {
                mark_completed(&op.operation_id);
                outcome.processed_redemptions += 1;
                outcome.icp_paid_e8s = outcome.icp_paid_e8s.saturating_add(op.amount_e8s);
            }
            Err(StreamManagerError::DuplicateTransaction) => mark_completed(&op.operation_id),
            Err(err) => {
                outcome
                    .errors
                    .push(format!("redemption commit failed: {err:?}"));
                return false;
            }
        }
    }
}

pub async fn scheduler_tick_once() -> DebugTickOutcome {
    #[cfg(not(target_family = "wasm"))]
    {
        DebugTickOutcome {
            scanned_icp_transactions: 0,
            scanned_io_transactions: 0,
            processed_authorized_streams: 0,
            processed_redemptions: 0,
            io_issued_e8s: 0,
            icp_paid_e8s: 0,
            errors: vec!["canister scheduler external calls run only on wasm".to_string()],
        }
    }

    #[cfg(target_family = "wasm")]
    {
        let config = CANISTER_STATE.with(|cell| cell.borrow().config.clone());
        let icp_ledger = principal(&config.icp_index_principal_text)
            .or_else(|| principal(&config.icp_ledger_principal_text));
        let io_ledger = principal(&config.io_index_principal_text)
            .or_else(|| principal(&config.io_ledger_principal_text));
        let io_transfer_ledger = principal(&config.io_ledger_principal_text);
        let icp_transfer_ledger = principal(&config.icp_ledger_principal_text);
        let sns_governance = principal(&config.sns_governance_principal_text);

        let mut outcome = DebugTickOutcome {
            scanned_icp_transactions: 0,
            scanned_io_transactions: 0,
            processed_authorized_streams: 0,
            processed_redemptions: 0,
            io_issued_e8s: 0,
            icp_paid_e8s: 0,
            errors: Vec::new(),
        };

        let neurons = match sns_governance {
            Some(canister) => match sns_governance::debug_list_neurons(canister).await {
                Ok(neurons) => neurons,
                Err(err) => {
                    outcome.errors.push(err);
                    Vec::new()
                }
            },
            None => Vec::new(),
        };

        if let Some(io_canister) = io_transfer_ledger {
            if !retry_pending_io_issuances(io_canister, &mut outcome).await {
                return outcome;
            }
            if !retry_pending_two_week_streams(io_canister, &mut outcome).await {
                return outcome;
            }
            if !retry_pending_redemptions(icp_transfer_ledger, io_canister, &mut outcome).await {
                return outcome;
            }
        }

        if let Some(canister) = icp_ledger {
            let scan_state = CANISTER_STATE.with(|cell| {
                let cursors = &cell.borrow().scheduler_cursors;
                if cursors
                    .icp_account_history_scan
                    .cursor
                    .latest_cursor
                    .is_none()
                {
                    match cursors.last_scanned_icp_index_block {
                        Some(cursor) => legacy_icp_account_history_scan_state(cursor),
                        None => AccountHistoryScanState::default(),
                    }
                } else {
                    cursors.icp_account_history_scan.clone()
                }
            });
            let start_after = scan_state.cursor.latest_cursor.map(|block| block.0);
            match scan_account_through_index(
                canister,
                icp_ledger::mock_account(STREAM_MANAGER_DEPOSIT_ACCOUNT),
                scan_state,
            )
            .await
            {
                Ok((transactions, next_scan_state, latest_seen)) => {
                    let relevant = transactions
                        .into_iter()
                        .filter(|tx| {
                            tx.to == STREAM_MANAGER_DEPOSIT_ACCOUNT
                                && start_after
                                    .map(|cursor| tx.block_index > cursor)
                                    .unwrap_or(true)
                        })
                        .collect::<Vec<_>>();
                    outcome.scanned_icp_transactions = relevant.len() as u64;
                    let page_error_count = outcome.errors.len();

                    for tx in relevant {
                        let tx_id = format!("icp:{}", tx.block_index);
                        let already_journaled = CANISTER_STATE.with(|cell| {
                            cell.borrow()
                                .operation_journal
                                .iter()
                                .any(|op| op.operation_id == tx_id)
                        });
                        if already_journaled {
                            advance_icp_cursor(tx.block_index);
                            continue;
                        }

                        let preview = CANISTER_STATE.with(|cell| {
                            let state = cell.borrow();
                            let kind =
                                match crate::StreamManager::classify_stream(&tx.from, &tx.memo) {
                                    Ok(kind) => kind,
                                    Err(err) => return Err(err),
                                };
                            state.manager.preview_authorized_stream(
                                kind,
                                tx.amount_e8s,
                                tx_id.clone(),
                            )
                        });
                        let preview = match preview {
                            Ok(preview) => preview,
                            Err(StreamManagerError::DuplicateTransaction) => {
                                advance_icp_cursor(tx.block_index);
                                continue;
                            }
                            Err(err @ StreamManagerError::UnknownOrUnauthorizedStream { .. }) => {
                                journal_rejected_icp_deposit(
                                    tx.block_index,
                                    tx.amount_e8s,
                                    format!("{err:?}"),
                                );
                                advance_icp_cursor(tx.block_index);
                                continue;
                            }
                            Err(err) => {
                                outcome.errors.push(format!("stream {tx_id}: {err:?}"));
                                continue;
                            }
                        };

                        if let Some(io_canister) = io_transfer_ledger {
                            match ApiIoRecipientPolicy::from(preview.outcome.recipient_policy) {
                                ApiIoRecipientPolicy::JupiterFaucet => {
                                    ensure_stream_operation(
                                        "icp",
                                        tx.block_index,
                                        kind_from_api(preview.outcome.kind.into()),
                                        tx.amount_e8s,
                                        preview.post_state,
                                        preview.outcome.io_issued_e8s,
                                        OperationPhase::AwaitingIoIssuance,
                                    );
                                    if !retry_pending_io_issuances(io_canister, &mut outcome).await
                                    {
                                        return outcome;
                                    }
                                    advance_icp_cursor(tx.block_index);
                                    continue;
                                }
                                ApiIoRecipientPolicy::EligibleIoSnsNeurons => {
                                    let allocations = CANISTER_STATE.with(|cell| {
                                        cell.borrow().manager.allocate_two_week_maturity_io(
                                            preview.outcome.io_issued_e8s,
                                            &neurons,
                                        )
                                    });
                                    if !allocations.allocations.is_empty() {
                                        CANISTER_STATE.with(|cell| {
                                            let mut state = cell.borrow_mut();
                                            if !state
                                                .operation_journal
                                                .iter()
                                                .any(|op| op.operation_id == tx_id)
                                            {
                                                let mut op = StreamOperation::stream(
                                                    "icp",
                                                    tx.block_index,
                                                    StreamOperationKind::TwoWeekMaturityStream,
                                                    tx.amount_e8s,
                                                    preview.post_state,
                                                    preview.outcome.io_issued_e8s,
                                                    OperationPhase::PartiallyDistributed,
                                                );
                                                op.two_week_recipients = allocations
                                                    .allocations
                                                    .into_iter()
                                                    .map(|allocation| TwoWeekRecipientTransfer {
                                                        neuron_id: allocation.neuron_id,
                                                        amount_e8s: allocation.io_e8s,
                                                        transfer_status: TransferStatus::Pending,
                                                        transfer_block_index: None,
                                                        last_error: None,
                                                    })
                                                    .collect();
                                                state.operation_journal.push(op);
                                            }
                                        });
                                        retry_pending_two_week_streams(io_canister, &mut outcome)
                                            .await;
                                        if !no_new_page_errors(&outcome, page_error_count) {
                                            return outcome;
                                        }
                                        advance_icp_cursor(tx.block_index);
                                        continue;
                                    }
                                }
                                ApiIoRecipientPolicy::None => {}
                            }
                        }

                        let committed = CANISTER_STATE.with(|cell| {
                            cell.borrow_mut()
                                .manager
                                .commit_previewed_stream(tx_id.clone(), preview.post_state)
                        });
                        match committed {
                            Ok(()) => {
                                ensure_stream_operation(
                                    "icp",
                                    tx.block_index,
                                    kind_from_api(preview.outcome.kind.into()),
                                    tx.amount_e8s,
                                    preview.post_state,
                                    preview.outcome.io_issued_e8s,
                                    OperationPhase::Completed,
                                );
                                mark_completed(&tx_id);
                                advance_icp_cursor(tx.block_index);
                                outcome.processed_authorized_streams += 1;
                                outcome.io_issued_e8s = outcome
                                    .io_issued_e8s
                                    .saturating_add(preview.outcome.io_issued_e8s);
                            }
                            Err(err) => outcome.errors.push(format!("stream {tx_id}: {err:?}")),
                        }
                    }
                    if no_new_page_errors(&outcome, page_error_count) {
                        commit_icp_scan_state(next_scan_state, latest_seen);
                    }
                }
                Err(err) => outcome.errors.push(err),
            }
        }

        if let Some(canister) = io_ledger {
            let scan_state = CANISTER_STATE.with(|cell| {
                let cursors = &cell.borrow().scheduler_cursors;
                if cursors
                    .io_account_history_scan
                    .cursor
                    .latest_cursor
                    .is_none()
                {
                    match cursors.last_scanned_io_index_block {
                        Some(cursor) => legacy_io_account_history_scan_state(cursor),
                        None => AccountHistoryScanState::default(),
                    }
                } else {
                    cursors.io_account_history_scan.clone()
                }
            });
            let start_after = scan_state.cursor.latest_cursor.map(|block| block.0);
            match scan_account_through_index(
                canister,
                io_ledger::mock_account(REDEMPTION_ACCOUNT),
                scan_state,
            )
            .await
            {
                Ok((transactions, next_scan_state, latest_seen)) => {
                    let relevant = transactions
                        .into_iter()
                        .filter(|tx| {
                            tx.to == REDEMPTION_ACCOUNT
                                && start_after
                                    .map(|cursor| tx.block_index > cursor)
                                    .unwrap_or(true)
                        })
                        .collect::<Vec<_>>();
                    outcome.scanned_io_transactions = relevant.len() as u64;
                    let page_error_count = outcome.errors.len();

                    for tx in relevant {
                        let tx_id = format!("io:{}", tx.block_index);
                        if CANISTER_STATE.with(|cell| {
                            cell.borrow()
                                .operation_journal
                                .iter()
                                .any(|op| op.operation_id == tx_id)
                        }) {
                            advance_io_cursor(tx.block_index);
                            continue;
                        }

                        let preview = CANISTER_STATE.with(|cell| {
                            cell.borrow()
                                .manager
                                .preview_redemption(tx.amount_e8s, tx_id.clone())
                        });
                        let preview = match preview {
                            Ok(preview) => preview,
                            Err(StreamManagerError::DuplicateTransaction) => {
                                advance_io_cursor(tx.block_index);
                                continue;
                            }
                            Err(err) => {
                                outcome.errors.push(format!("redemption {tx_id}: {err:?}"));
                                continue;
                            }
                        };

                        CANISTER_STATE.with(|cell| {
                            cell.borrow_mut()
                                .operation_journal
                                .push(StreamOperation::redemption(
                                    tx.block_index,
                                    tx.amount_e8s,
                                    preview.outcome.icp_paid_e8s,
                                    tx.from.clone(),
                                    preview.post_state,
                                ));
                        });

                        if let Some(io_canister) = io_transfer_ledger {
                            if !retry_pending_redemptions(
                                icp_transfer_ledger,
                                io_canister,
                                &mut outcome,
                            )
                            .await
                            {
                                return outcome;
                            }
                        }
                        advance_io_cursor(tx.block_index);
                    }
                    if no_new_page_errors(&outcome, page_error_count) {
                        commit_io_scan_state(next_scan_state, latest_seen);
                    }
                }
                Err(err) => outcome.errors.push(err),
            }
        }

        outcome
    }
}

#[cfg(target_family = "wasm")]
fn ensure_stream_operation(
    source_ledger: &str,
    source_block_index: u64,
    kind: StreamOperationKind,
    amount_e8s: u128,
    post_state: io_core_model::ProtocolState,
    io_issued_e8s: u128,
    phase: OperationPhase,
) {
    let operation_id = format!("{source_ledger}:{source_block_index}");
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if !state
            .operation_journal
            .iter()
            .any(|op| op.operation_id == operation_id)
        {
            state.operation_journal.push(StreamOperation::stream(
                source_ledger,
                source_block_index,
                kind,
                amount_e8s,
                post_state,
                io_issued_e8s,
                phase,
            ));
        }
    });
}

#[cfg(target_family = "wasm")]
fn mark_io_issuance(operation_id: &str, block: u64) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            op.downstream_io_issuance_block = Some(block);
            op.mark_updated(OperationPhase::Previewed);
        }
    });
}

#[cfg(target_family = "wasm")]
fn mark_completed(operation_id: &str) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            op.mark_updated(OperationPhase::Completed);
        }
    });
}

#[cfg(target_family = "wasm")]
fn mark_operation_error(operation_id: &str, err: String, phase: OperationPhase) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            op.mark_retryable_error(err, phase);
        }
    });
}

#[cfg(target_family = "wasm")]
fn journal_rejected_icp_deposit(source_block_index: u64, amount_e8s: u128, err: String) {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let operation_id = format!("icp:{source_block_index}");
        if state
            .operation_journal
            .iter()
            .any(|op| op.operation_id == operation_id)
        {
            return;
        }
        let mut op = StreamOperation::stream(
            "icp",
            source_block_index,
            StreamOperationKind::UnknownIcpDeposit,
            amount_e8s,
            state.manager.state,
            0,
            OperationPhase::FailedTerminal,
        );
        op.last_error = Some(err);
        state.operation_journal.push(op);
    });
}

#[cfg(target_family = "wasm")]
fn advance_icp_cursor(block: u64) {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let current = state.scheduler_cursors.last_scanned_icp_index_block;
        state.scheduler_cursors.last_scanned_icp_index_block =
            Some(current.unwrap_or(0).max(block));
    });
}

#[cfg(target_family = "wasm")]
fn commit_icp_scan_state(scan_state: AccountHistoryScanState, latest_seen: Option<u64>) {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.scheduler_cursors.icp_account_history_scan = scan_state;
        if let Some(latest_seen) = latest_seen {
            let current = state.scheduler_cursors.last_scanned_icp_index_block;
            state.scheduler_cursors.last_scanned_icp_index_block =
                Some(current.unwrap_or(0).max(latest_seen));
        }
    });
}

#[cfg(target_family = "wasm")]
fn commit_io_scan_state(scan_state: AccountHistoryScanState, latest_seen: Option<u64>) {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.scheduler_cursors.io_account_history_scan = scan_state;
        if let Some(latest_seen) = latest_seen {
            let current = state.scheduler_cursors.last_scanned_io_index_block;
            state.scheduler_cursors.last_scanned_io_index_block =
                Some(current.unwrap_or(0).max(latest_seen));
        }
    });
}

#[cfg(target_family = "wasm")]
fn advance_io_cursor(block: u64) {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let current = state.scheduler_cursors.last_scanned_io_index_block;
        state.scheduler_cursors.last_scanned_io_index_block = Some(current.unwrap_or(0).max(block));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::JUPITER_FAUCET_SOURCE;
    use io_ledger_types::{
        Account, IndexTransaction, LedgerBlock, LedgerOperationKind, LedgerTransferRequest, Memo,
        Subaccount,
    };

    fn block(index: u64) -> IndexTransaction {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        IndexTransaction {
            block_index: BlockIndex(index),
            transaction: LedgerBlock {
                block_index: BlockIndex(index),
                timestamp_nanos: index,
                from: Some(Account::new(principal, Some(Subaccount([1; 32])))),
                to: Some(Account::new(principal, None)),
                amount_e8s: 1,
                fee_e8s: Some(10),
                memo: Some(Memo::from("scan")),
                operation_kind: LedgerOperationKind::Transfer,
            },
        }
    }

    fn transfer_request(amount_e8s: u128, to: &str, memo: &str) -> LedgerTransferRequest {
        LedgerTransferRequest {
            from_subaccount: Some(crate::clients::icp_ledger::mock_subaccount(
                PROTOCOL_RESERVE_ACCOUNT,
            )),
            to: crate::clients::icp_ledger::mock_account(to),
            amount_e8s,
            fee_e8s: None,
            memo: Some(Memo::from(memo)),
            created_at_time: None,
        }
    }

    fn duplicate_proof_block(amount_e8s: u128, to: &str, memo: &str) -> LedgerBlock {
        LedgerBlock {
            block_index: BlockIndex(9),
            timestamp_nanos: 0,
            from: Some(crate::clients::icp_ledger::mock_account(
                PROTOCOL_RESERVE_ACCOUNT,
            )),
            to: Some(crate::clients::icp_ledger::mock_account(to)),
            amount_e8s,
            fee_e8s: None,
            memo: Some(Memo::from(memo)),
            operation_kind: LedgerOperationKind::Transfer,
        }
    }

    #[test]
    fn plan_only_tick_is_idempotent_without_configured_work() {
        assert_eq!(scheduler_tick_plan_only(), scheduler_tick_plan_only());
    }

    #[test]
    fn outcome_is_debuggable_and_candid_serializable() {
        let outcome = scheduler_tick_plan_only();
        assert!(format!("{outcome:?}").contains("planned_steps"));
        candid::encode_one(outcome).unwrap();
    }

    #[test]
    fn contiguous_boundary_cursor_empty_page_does_not_advance() {
        let result = IndexScanResult {
            transactions: vec![],
            last_seen_block: None,
            index_tip: Some(BlockIndex(10)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_contiguous_page(Some(BlockIndex(5)), &result),
            Ok(Some(BlockIndex(5)))
        );
    }

    #[test]
    fn contiguous_boundary_cursor_skips_already_processed_blocks_and_advances_once() {
        let result = IndexScanResult {
            transactions: vec![block(4), block(5), block(6)],
            last_seen_block: Some(BlockIndex(6)),
            index_tip: Some(BlockIndex(6)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_contiguous_page(Some(BlockIndex(5)), &result),
            Ok(Some(BlockIndex(6)))
        );
    }

    #[test]
    fn contiguous_boundary_cursor_rejects_duplicate_new_blocks() {
        let result = IndexScanResult {
            transactions: vec![block(6), block(6)],
            last_seen_block: Some(BlockIndex(6)),
            index_tip: Some(BlockIndex(6)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert!(matches!(
            boundary_cursor_after_contiguous_page(Some(BlockIndex(5)), &result),
            Err(IndexError::MissingBlock {
                block_index: BlockIndex(7)
            })
        ));
    }

    #[test]
    fn contiguous_boundary_cursor_rejects_gap_and_does_not_skip_unknown_range() {
        let result = IndexScanResult {
            transactions: vec![block(7)],
            last_seen_block: Some(BlockIndex(7)),
            index_tip: Some(BlockIndex(7)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_contiguous_page(Some(BlockIndex(5)), &result),
            Err(IndexError::MissingBlock {
                block_index: BlockIndex(6)
            })
        );
    }

    #[test]
    fn contiguous_boundary_cursor_reports_archive_required_before_advancing() {
        let result = IndexScanResult {
            transactions: vec![block(6)],
            last_seen_block: Some(BlockIndex(6)),
            index_tip: Some(BlockIndex(100)),
            archive_required: true,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_contiguous_page(Some(BlockIndex(5)), &result),
            Err(IndexError::ArchiveRequired {
                from: BlockIndex(5)
            })
        );
    }

    #[test]
    fn contiguous_boundary_cursor_reports_index_lag() {
        let result = IndexScanResult {
            transactions: vec![],
            last_seen_block: None,
            index_tip: Some(BlockIndex(4)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_contiguous_page(Some(BlockIndex(5)), &result),
            Err(IndexError::IndexLag {
                requested: BlockIndex(5),
                tip: Some(BlockIndex(4))
            })
        );
    }

    #[test]
    fn account_boundary_cursor_allows_global_block_gaps() {
        let result = IndexScanResult {
            transactions: vec![block(25)],
            last_seen_block: Some(BlockIndex(25)),
            index_tip: Some(BlockIndex(30)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Ok(Some(BlockIndex(25)))
        );
    }

    #[test]
    fn account_boundary_cursor_rejects_duplicate_returned_blocks() {
        let result = IndexScanResult {
            transactions: vec![block(25), block(25)],
            last_seen_block: Some(BlockIndex(25)),
            index_tip: Some(BlockIndex(30)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Err(IndexError::MissingBlock {
                block_index: BlockIndex(25)
            })
        );
    }

    #[test]
    fn account_boundary_cursor_rejects_non_monotonic_pages() {
        let result = IndexScanResult {
            transactions: vec![block(25), block(24)],
            last_seen_block: Some(BlockIndex(25)),
            index_tip: Some(BlockIndex(30)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Err(IndexError::MissingBlock {
                block_index: BlockIndex(24)
            })
        );
    }

    #[test]
    fn account_boundary_cursor_ignores_stale_blocks_without_advancing() {
        let result = IndexScanResult {
            transactions: vec![block(8), block(10)],
            last_seen_block: Some(BlockIndex(10)),
            index_tip: Some(BlockIndex(30)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Ok(Some(BlockIndex(10)))
        );
    }

    #[test]
    fn account_boundary_cursor_empty_page_does_not_advance() {
        let result = IndexScanResult {
            transactions: vec![],
            last_seen_block: None,
            index_tip: Some(BlockIndex(30)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Ok(Some(BlockIndex(10)))
        );
    }

    #[test]
    fn account_boundary_cursor_archive_required_does_not_advance() {
        let result = IndexScanResult {
            transactions: vec![block(25)],
            last_seen_block: Some(BlockIndex(25)),
            index_tip: Some(BlockIndex(30)),
            archive_required: true,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Err(IndexError::ArchiveRequired {
                from: BlockIndex(10)
            })
        );
    }

    #[test]
    fn account_boundary_cursor_reports_lag_before_current_without_advancing() {
        let result = IndexScanResult {
            transactions: vec![block(25)],
            last_seen_block: Some(BlockIndex(25)),
            index_tip: Some(BlockIndex(9)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Err(IndexError::IndexLag {
                requested: BlockIndex(10),
                tip: Some(BlockIndex(9))
            })
        );
    }

    #[test]
    fn legacy_icp_cursor_seed_accepts_descending_head_page_without_replay() {
        let state = legacy_icp_account_history_scan_state(10);
        assert_eq!(
            state.cursor.order,
            Some(AccountHistoryPageOrder::Descending)
        );
        assert_eq!(state.next_request_start(), None);

        let result = IndexScanResult {
            transactions: vec![block(12), block(10), block(7)],
            last_seen_block: Some(BlockIndex(12)),
            index_tip: Some(BlockIndex(12)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Descending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        let outcome = state.observe_page(&result, None, 100, 1, 1, None).unwrap();
        assert_eq!(
            outcome
                .transactions_chronological
                .iter()
                .map(|tx| tx.block_index)
                .collect::<Vec<_>>(),
            vec![BlockIndex(12)]
        );
        assert_eq!(
            outcome.next_state.cursor.latest_cursor,
            Some(BlockIndex(12))
        );
        assert_eq!(
            outcome.next_state.cursor.oldest_cursor,
            Some(BlockIndex(10))
        );
    }

    #[test]
    fn duplicate_transfer_without_proof_is_not_success() {
        let request = transfer_request(100, JUPITER_FAUCET_SOURCE, "icp:1");
        assert!(matches!(
            classify_boundary_transfer_result(
                &request,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(9)
                }),
                None,
            ),
            BoundaryTransferDecision::Retryable(_)
        ));
    }

    #[test]
    fn duplicate_transfer_matching_expected_operation_is_idempotent_success() {
        let request = transfer_request(100, JUPITER_FAUCET_SOURCE, "icp:1");
        let duplicate = duplicate_proof_block(100, JUPITER_FAUCET_SOURCE, "icp:1");
        assert_eq!(
            classify_boundary_transfer_result(
                &request,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(9)
                }),
                Some(&duplicate),
            ),
            BoundaryTransferDecision::Succeeded(9)
        );
    }

    #[test]
    fn duplicate_transfer_mismatched_amount_account_or_memo_is_not_success() {
        let request = transfer_request(100, JUPITER_FAUCET_SOURCE, "icp:1");
        for duplicate in [
            duplicate_proof_block(99, JUPITER_FAUCET_SOURCE, "icp:1"),
            duplicate_proof_block(100, "other_account", "icp:1"),
            duplicate_proof_block(100, JUPITER_FAUCET_SOURCE, "other_memo"),
        ] {
            assert!(matches!(
                classify_boundary_transfer_result(
                    &request,
                    Err(LedgerTransferError::Duplicate {
                        duplicate_of: BlockIndex(9)
                    }),
                    Some(&duplicate),
                ),
                BoundaryTransferDecision::Retryable(_)
            ));
        }
    }

    #[test]
    fn boundary_transfer_error_classes_remain_retryable() {
        let request = transfer_request(100, JUPITER_FAUCET_SOURCE, "icp:1");
        for err in [
            LedgerTransferError::TemporarilyUnavailable,
            LedgerTransferError::CanisterCallFailed {
                method: "icrc1_transfer".to_string(),
                message: "reject".to_string(),
            },
            LedgerTransferError::BadFee {
                expected_fee_e8s: 10,
            },
            LedgerTransferError::InsufficientFunds { balance_e8s: 1 },
        ] {
            assert!(matches!(
                classify_boundary_transfer_result(&request, Err(err), None),
                BoundaryTransferDecision::Retryable(_)
            ));
        }
    }
}
