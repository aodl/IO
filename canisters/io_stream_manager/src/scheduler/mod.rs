#[cfg(target_family = "wasm")]
use crate::clients::{icp_ledger, io_ledger, sns_governance};
#[cfg(target_family = "wasm")]
use crate::state::JUPITER_FAUCET_SOURCE;
use crate::DebugTickOutcome;
#[cfg(target_family = "wasm")]
use crate::{
    ApiIoRecipientPolicy, PendingIoAllocation, PendingRedemption, PendingTwoWeekStream,
    StreamManagerError, CANISTER_STATE,
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
            state.pending_two_week_streams.first().and_then(|pending| {
                pending
                    .allocations
                    .iter()
                    .position(|allocation| !allocation.transferred)
                    .map(|allocation_index| {
                        (
                            pending.transaction_id.clone(),
                            allocation_index,
                            pending.allocations[allocation_index].clone(),
                        )
                    })
            })
        });

        let Some((tx_id, allocation_index, allocation)) = next else {
            break;
        };
        let transfer = io_ledger::TransferArgs {
            from: PROTOCOL_RESERVE_ACCOUNT.to_string(),
            to: format!("{TWO_WEEK_REWARD_ACCOUNT_PREFIX}{}", allocation.neuron_id),
            amount_e8s: allocation.io_e8s,
            memo: tx_id.clone(),
        };
        if let Err(err) = io_ledger::transfer(io_canister, transfer).await {
            outcome.errors.push(err);
            return false;
        }

        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            if let Some(pending) = state.pending_two_week_streams.first_mut() {
                if pending.transaction_id == tx_id {
                    pending.allocations[allocation_index].transferred = true;
                }
            }
        });
    }

    let completed = CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if state
            .pending_two_week_streams
            .first()
            .map(|pending| {
                pending
                    .allocations
                    .iter()
                    .all(|allocation| allocation.transferred)
            })
            .unwrap_or(false)
        {
            Some(state.pending_two_week_streams.remove(0))
        } else {
            None
        }
    });

    if let Some(pending) = completed {
        let committed = CANISTER_STATE.with(|cell| {
            cell.borrow_mut()
                .manager
                .commit_previewed_stream(pending.transaction_id.clone(), pending.post_state)
        });
        match committed {
            Ok(()) => {
                outcome.processed_authorized_streams += 1;
                outcome.io_issued_e8s = outcome.io_issued_e8s.saturating_add(pending.io_issued_e8s);
            }
            Err(err) => outcome
                .errors
                .push(format!("stream {}: {err:?}", pending.transaction_id)),
        }
    }
    CANISTER_STATE.with(|cell| cell.borrow().pending_two_week_streams.is_empty())
}

#[cfg(target_family = "wasm")]
async fn retry_pending_redemptions(io_canister: Principal, outcome: &mut DebugTickOutcome) -> bool {
    loop {
        let pending =
            CANISTER_STATE.with(|cell| cell.borrow().pending_redemptions.first().cloned());
        let Some(pending) = pending else {
            return true;
        };

        let transfer = io_ledger::TransferArgs {
            from: REDEMPTION_ACCOUNT.to_string(),
            to: PROTOCOL_RESERVE_ACCOUNT.to_string(),
            amount_e8s: pending.io_redeemed_e8s,
            memo: REDEEMED_IO_MEMO.to_string(),
        };
        if let Err(err) = io_ledger::transfer(io_canister, transfer).await {
            outcome.errors.push(err);
            return false;
        }

        let committed = CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            let pending = state.pending_redemptions.remove(0);
            state
                .manager
                .commit_previewed_redemption(pending.transaction_id.clone(), pending.post_state)
                .map(|()| pending)
        });
        match committed {
            Ok(_pending) => {
                outcome.processed_redemptions += 1;
            }
            Err(err) => outcome
                .errors
                .push(format!("redemption commit failed: {err:?}")),
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
            if !retry_pending_two_week_streams(io_canister, &mut outcome).await {
                return outcome;
            }
            if !retry_pending_redemptions(io_canister, &mut outcome).await {
                return outcome;
            }
        }

        if let Some(canister) = icp_ledger {
            match icp_ledger::debug_get_transactions(canister).await {
                Ok(transactions) => {
                    outcome.scanned_icp_transactions = transactions.len() as u64;
                    for tx in transactions
                        .into_iter()
                        .filter(|tx| tx.to == STREAM_MANAGER_DEPOSIT_ACCOUNT)
                    {
                        let tx_id = format!("icp:{}", tx.block_index);
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
                            Err(StreamManagerError::DuplicateTransaction) => continue,
                            Err(err) => {
                                outcome.errors.push(format!("stream {tx_id}: {err:?}"));
                                continue;
                            }
                        };

                        if let Some(io_canister) = io_transfer_ledger {
                            match ApiIoRecipientPolicy::from(preview.outcome.recipient_policy) {
                                ApiIoRecipientPolicy::JupiterFaucet => {
                                    let transfer = io_ledger::TransferArgs {
                                        from: PROTOCOL_RESERVE_ACCOUNT.to_string(),
                                        to: JUPITER_FAUCET_SOURCE.to_string(),
                                        amount_e8s: preview.outcome.io_issued_e8s,
                                        memo: tx_id.clone(),
                                    };
                                    if let Err(err) =
                                        io_ledger::transfer(io_canister, transfer).await
                                    {
                                        outcome.errors.push(err);
                                        continue;
                                    }
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
                                                .pending_two_week_streams
                                                .iter()
                                                .any(|pending| pending.transaction_id == tx_id)
                                            {
                                                state.pending_two_week_streams.push(
                                                    PendingTwoWeekStream {
                                                        transaction_id: tx_id.clone(),
                                                        post_state: preview.post_state,
                                                        io_issued_e8s: preview
                                                            .outcome
                                                            .io_issued_e8s,
                                                        allocations: allocations
                                                            .allocations
                                                            .into_iter()
                                                            .map(|allocation| PendingIoAllocation {
                                                                neuron_id: allocation.neuron_id,
                                                                io_e8s: allocation.io_e8s,
                                                                transferred: false,
                                                            })
                                                            .collect(),
                                                    },
                                                );
                                            }
                                        });
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
                    outcome.scanned_io_transactions = transactions.len() as u64;
                    for tx in transactions
                        .into_iter()
                        .filter(|tx| tx.to == REDEMPTION_ACCOUNT)
                    {
                        let tx_id = format!("io:{}", tx.block_index);
                        let preview = CANISTER_STATE.with(|cell| {
                            cell.borrow()
                                .manager
                                .preview_redemption(tx.amount_e8s, tx_id.clone())
                        });
                        let preview = match preview {
                            Ok(preview) => preview,
                            Err(StreamManagerError::DuplicateTransaction) => continue,
                            Err(err) => {
                                outcome.errors.push(format!("redemption {tx_id}: {err:?}"));
                                continue;
                            }
                        };
                        if let Some(icp_canister) = icp_transfer_ledger {
                            let transfer = icp_ledger::TransferArgs {
                                from: STREAM_MANAGER_DEPOSIT_ACCOUNT.to_string(),
                                to: tx.from.clone(),
                                amount_e8s: preview.outcome.icp_paid_e8s,
                                memo: REDEMPTION_PAYOUT_MEMO.to_string(),
                            };
                            if let Err(err) = icp_ledger::transfer(icp_canister, transfer).await {
                                outcome.errors.push(err);
                                continue;
                            }
                        }
                        if let Some(io_canister) = io_transfer_ledger {
                            let transfer = io_ledger::TransferArgs {
                                from: REDEMPTION_ACCOUNT.to_string(),
                                to: PROTOCOL_RESERVE_ACCOUNT.to_string(),
                                amount_e8s: tx.amount_e8s,
                                memo: REDEEMED_IO_MEMO.to_string(),
                            };
                            if let Err(err) = io_ledger::transfer(io_canister, transfer).await {
                                outcome.errors.push(err);
                                CANISTER_STATE.with(|cell| {
                                    let mut state = cell.borrow_mut();
                                    if !state
                                        .pending_redemptions
                                        .iter()
                                        .any(|pending| pending.transaction_id == tx_id)
                                    {
                                        state.pending_redemptions.push(PendingRedemption {
                                            transaction_id: tx_id.clone(),
                                            post_state: preview.post_state,
                                            io_redeemed_e8s: preview.outcome.io_redeemed_e8s,
                                            icp_paid_e8s: preview.outcome.icp_paid_e8s,
                                            user_account: tx.from.clone(),
                                        });
                                    }
                                });
                                continue;
                            }
                        }
                        let committed = CANISTER_STATE.with(|cell| {
                            cell.borrow_mut()
                                .manager
                                .commit_previewed_redemption(tx_id.clone(), preview.post_state)
                        });
                        match committed {
                            Ok(()) => {
                                outcome.processed_redemptions += 1;
                                outcome.icp_paid_e8s = outcome
                                    .icp_paid_e8s
                                    .saturating_add(preview.outcome.icp_paid_e8s);
                            }
                            Err(err) => outcome.errors.push(format!("redemption {tx_id}: {err:?}")),
                        }
                    }
                }
                Err(err) => outcome.errors.push(err),
            }
        }

        outcome
    }
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
