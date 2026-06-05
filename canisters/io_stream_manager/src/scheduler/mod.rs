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
use serde::Deserialize;

pub const STREAM_MANAGER_DEPOSIT_ACCOUNT: &str = "stream_manager_deposit";
pub const REDEMPTION_ACCOUNT: &str = "redemption";
pub const PROTOCOL_RESERVE_ACCOUNT: &str = "protocol_reserve";
pub const REDEMPTION_PAYOUT_MEMO: &str = "redemption_payout";
pub const REDEEMED_IO_MEMO: &str = "redeemed_io_to_reserve";
pub const TWO_WEEK_REWARD_ACCOUNT_PREFIX: &str = "sns_neuron_";

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

        let transfer = io_ledger::TransferArgs {
            from: PROTOCOL_RESERVE_ACCOUNT.to_string(),
            to: format!("{TWO_WEEK_REWARD_ACCOUNT_PREFIX}{}", recipient.neuron_id),
            amount_e8s: recipient.amount_e8s,
            memo: operation_id.clone(),
        };
        match io_ledger::transfer(io_canister, transfer).await {
            Ok(block) => CANISTER_STATE.with(|cell| {
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
            Err(err) => {
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
            let transfer = io_ledger::TransferArgs {
                from: PROTOCOL_RESERVE_ACCOUNT.to_string(),
                to: JUPITER_FAUCET_SOURCE.to_string(),
                amount_e8s: op.io_issued_e8s,
                memo: op.operation_id.clone(),
            };
            match io_ledger::transfer(io_canister, transfer).await {
                Ok(block) => mark_io_issuance(&op.operation_id, block),
                Err(err) => {
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

            let transfer = icp_ledger::TransferArgs {
                from: STREAM_MANAGER_DEPOSIT_ACCOUNT.to_string(),
                to: op.user_account.clone().unwrap_or_default(),
                amount_e8s: op.amount_e8s,
                memo: REDEMPTION_PAYOUT_MEMO.to_string(),
            };
            match icp_ledger::transfer(icp_canister, transfer).await {
                Ok(block) => {
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
                Err(err) => {
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
            let transfer = io_ledger::TransferArgs {
                from: REDEMPTION_ACCOUNT.to_string(),
                to: PROTOCOL_RESERVE_ACCOUNT.to_string(),
                amount_e8s: op.io_amount,
                memo: REDEEMED_IO_MEMO.to_string(),
            };
            match io_ledger::transfer(io_canister, transfer).await {
                Ok(block) => {
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
                Err(err) => {
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
            match icp_ledger::debug_get_transactions(canister).await {
                Ok(transactions) => {
                    let start_after = CANISTER_STATE
                        .with(|cell| cell.borrow().scheduler_cursors.last_scanned_icp_index_block);
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
                                outcome.errors.push(format!("stream {tx_id}: {err:?}"));
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
                                    advance_icp_cursor(tx.block_index);
                                    if !retry_pending_io_issuances(io_canister, &mut outcome).await
                                    {
                                        return outcome;
                                    }
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
                                        advance_icp_cursor(tx.block_index);
                                        retry_pending_two_week_streams(io_canister, &mut outcome)
                                            .await;
                                        if !outcome.errors.is_empty() {
                                            return outcome;
                                        }
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
                }
                Err(err) => outcome.errors.push(err),
            }
        }

        if let Some(canister) = io_ledger {
            match io_ledger::debug_get_transactions(canister).await {
                Ok(transactions) => {
                    let start_after = CANISTER_STATE
                        .with(|cell| cell.borrow().scheduler_cursors.last_scanned_io_index_block);
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
                        advance_io_cursor(tx.block_index);

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
}
