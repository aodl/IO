#[cfg(target_family = "wasm")]
use crate::clients::{icp_ledger, nns_governance};
use crate::DebugTickOutcome;
#[cfg(target_family = "wasm")]
use crate::{NnsNeuronManagerModel, PendingIcpTransfer, RebalanceAction, CANISTER_STATE};
use candid::CandidType;
#[cfg(target_family = "wasm")]
use candid::Principal;
use serde::Deserialize;

pub const IO_NNS_NEURON_MANAGER_ACCOUNT: &str = "io_nns_neuron_manager";
pub const STREAM_MANAGER_DEPOSIT_ACCOUNT: &str = "stream_manager_deposit";
pub const TWO_YEAR_MATURITY_MEMO: &str = "two_year_maturity";
pub const TWO_WEEK_MATURITY_MEMO: &str = "two_week_maturity";
pub const PRINCIPAL_UNWIND_MEMO: &str = "principal_unwind";

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SchedulerTickOutcome {
    pub checked_two_year_maturity: u64,
    pub checked_two_week_maturity: u64,
    pub planned_pool_rebalances: u64,
    pub checked_ready_unwind_neurons: u64,
    pub planned_steps: Vec<String>,
}

impl SchedulerTickOutcome {
    fn no_work_configured() -> Self {
        Self {
            checked_two_year_maturity: 0,
            checked_two_week_maturity: 0,
            planned_pool_rebalances: 0,
            checked_ready_unwind_neurons: 0,
            planned_steps: vec![
                "check and disburse 2-year maturity".to_string(),
                "check and disburse 2-week maturity".to_string(),
                "rebalance pooled 2-week neuron".to_string(),
                "disburse ready unwind child neurons".to_string(),
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
fn record_disbursement(outcome: &mut DebugTickOutcome, amount: u128, memo: &str) {
    match memo {
        TWO_YEAR_MATURITY_MEMO => {
            outcome.disbursed_two_year_maturity_e8s = outcome
                .disbursed_two_year_maturity_e8s
                .saturating_add(amount);
        }
        TWO_WEEK_MATURITY_MEMO => {
            outcome.disbursed_two_week_maturity_e8s = outcome
                .disbursed_two_week_maturity_e8s
                .saturating_add(amount);
        }
        PRINCIPAL_UNWIND_MEMO => {
            outcome.disbursed_unwind_principal_e8s = outcome
                .disbursed_unwind_principal_e8s
                .saturating_add(amount);
        }
        _ => {}
    }
}

#[cfg(target_family = "wasm")]
async fn retry_pending_icp_transfers(ledger: Principal, outcome: &mut DebugTickOutcome) -> bool {
    loop {
        let pending =
            CANISTER_STATE.with(|cell| cell.borrow().pending_icp_transfers.first().cloned());
        let Some(pending) = pending else {
            return true;
        };
        let transfer = icp_ledger::TransferArgs {
            from: IO_NNS_NEURON_MANAGER_ACCOUNT.to_string(),
            to: STREAM_MANAGER_DEPOSIT_ACCOUNT.to_string(),
            amount_e8s: pending.amount_e8s,
            memo: pending.memo.clone(),
        };
        if let Err(err) = icp_ledger::transfer(ledger, transfer).await {
            outcome.errors.push(err);
            return false;
        }
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            let pending = state.pending_icp_transfers.remove(0);
            if let Some(post_model) = pending.post_model {
                state.model = post_model;
            }
            record_disbursement(outcome, pending.amount_e8s, &pending.memo);
        });
    }
}

#[cfg(target_family = "wasm")]
async fn finish_disbursement_after_ledger(
    ledger: Option<Principal>,
    amount_e8s: u128,
    memo: &str,
    post_model: Option<NnsNeuronManagerModel>,
    outcome: &mut DebugTickOutcome,
) -> bool {
    if amount_e8s == 0 {
        return true;
    }
    if let Some(ledger) = ledger {
        CANISTER_STATE.with(|cell| {
            cell.borrow_mut()
                .pending_icp_transfers
                .push(PendingIcpTransfer {
                    amount_e8s,
                    memo: memo.to_string(),
                    post_model,
                });
        });
        retry_pending_icp_transfers(ledger, outcome).await
    } else {
        if let Some(post_model) = post_model {
            CANISTER_STATE.with(|cell| {
                cell.borrow_mut().model = post_model;
            });
        }
        record_disbursement(outcome, amount_e8s, memo);
        true
    }
}

pub async fn scheduler_tick_once() -> DebugTickOutcome {
    #[cfg(not(target_family = "wasm"))]
    {
        DebugTickOutcome {
            disbursed_two_year_maturity_e8s: 0,
            disbursed_two_week_maturity_e8s: 0,
            disbursed_unwind_principal_e8s: 0,
            planned_pool_rebalances: 0,
            errors: vec!["canister scheduler external calls run only on wasm".to_string()],
        }
    }

    #[cfg(target_family = "wasm")]
    {
        let config = CANISTER_STATE.with(|cell| cell.borrow().config.clone());
        let icp_ledger = principal(&config.icp_ledger_principal_text);
        let nns_governance = principal(&config.nns_governance_principal_text);
        let mut outcome = DebugTickOutcome {
            disbursed_two_year_maturity_e8s: 0,
            disbursed_two_week_maturity_e8s: 0,
            disbursed_unwind_principal_e8s: 0,
            planned_pool_rebalances: 0,
            errors: Vec::new(),
        };

        if let Some(ledger) = icp_ledger {
            if !retry_pending_icp_transfers(ledger, &mut outcome).await {
                return outcome;
            }
        }

        if let Some(governance) = nns_governance {
            match nns_governance::debug_disburse_maturity(governance, config.two_year_nns_neuron_id)
                .await
            {
                Ok(amount) => {
                    if !finish_disbursement_after_ledger(
                        icp_ledger,
                        amount,
                        TWO_YEAR_MATURITY_MEMO,
                        None,
                        &mut outcome,
                    )
                    .await
                    {
                        return outcome;
                    }
                }
                Err(err) => {
                    outcome.errors.push(err);
                }
            }
        } else {
            let (amount, post_model) = CANISTER_STATE.with(|cell| {
                let state = cell.borrow();
                let mut post_model = state.model.clone();
                let amount = post_model.disburse_two_year_maturity();
                (amount, post_model)
            });
            if !finish_disbursement_after_ledger(
                icp_ledger,
                amount,
                TWO_YEAR_MATURITY_MEMO,
                Some(post_model),
                &mut outcome,
            )
            .await
            {
                return outcome;
            }
        }

        let (two_week, post_model) = CANISTER_STATE.with(|cell| {
            let state = cell.borrow();
            let mut post_model = state.model.clone();
            let amount = post_model.disburse_two_week_maturity();
            (amount, post_model)
        });
        if !finish_disbursement_after_ledger(
            icp_ledger,
            two_week,
            TWO_WEEK_MATURITY_MEMO,
            Some(post_model),
            &mut outcome,
        )
        .await
        {
            return outcome;
        }

        let action = CANISTER_STATE.with(|cell| cell.borrow().two_week_pool_state.plan_rebalance());
        match action {
            RebalanceAction::None => {}
            RebalanceAction::StakeMore { amount_e8s } => {
                outcome.planned_pool_rebalances += 1;
                CANISTER_STATE.with(|cell| {
                    cell.borrow_mut().model.stake_more_two_week(amount_e8s);
                });
                if let Some(governance) = nns_governance {
                    if let Err(err) =
                        nns_governance::debug_stop_dissolving(governance, 10_000).await
                    {
                        outcome.errors.push(err);
                    } else if let Err(err) =
                        nns_governance::debug_merge(governance, 10_000, amount_e8s).await
                    {
                        outcome.errors.push(err);
                    }
                }
            }
            RebalanceAction::SplitAndDissolve { amount_e8s } => {
                outcome.planned_pool_rebalances += 1;
                let split = CANISTER_STATE
                    .with(|cell| cell.borrow_mut().model.split_and_start_unwind(amount_e8s));
                match split {
                    Ok(model_child_id) => {
                        if let Some(governance) = nns_governance {
                            match nns_governance::debug_split(governance, 2, amount_e8s).await {
                                Ok(governance_child_id) => {
                                    if let Err(err) = nns_governance::debug_start_dissolving(
                                        governance,
                                        governance_child_id,
                                    )
                                    .await
                                    {
                                        outcome.errors.push(err);
                                    }
                                }
                                Err(err) => outcome.errors.push(err),
                            }
                        }
                        let _ = model_child_id;
                    }
                    Err(err) => outcome.errors.push(format!("split unwind failed: {err:?}")),
                }
            }
        }

        let ready_ids = CANISTER_STATE.with(|cell| {
            let state = cell.borrow();
            state
                .model
                .unwind_neurons
                .iter()
                .filter(|n| n.is_ready_to_disburse(state.model.now_seconds))
                .map(|n| n.neuron_id)
                .collect::<Vec<_>>()
        });
        for neuron_id in ready_ids {
            let model_disbursement = CANISTER_STATE.with(|cell| {
                let state = cell.borrow();
                let mut post_model = state.model.clone();
                post_model
                    .disburse_ready_unwind(neuron_id)
                    .map(|amount| (amount, post_model))
            });
            match model_disbursement {
                Ok((amount, post_model)) => {
                    let governance_amount = if let Some(governance) = nns_governance {
                        match nns_governance::debug_disburse_principal(governance, neuron_id).await
                        {
                            Ok(governance_amount) => governance_amount,
                            Err(err) => {
                                outcome.errors.push(err);
                                amount
                            }
                        }
                    } else {
                        amount
                    };
                    if !finish_disbursement_after_ledger(
                        icp_ledger,
                        governance_amount,
                        PRINCIPAL_UNWIND_MEMO,
                        Some(post_model),
                        &mut outcome,
                    )
                    .await
                    {
                        return outcome;
                    }
                }
                Err(err) => outcome
                    .errors
                    .push(format!("unwind disburse failed: {err:?}")),
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
