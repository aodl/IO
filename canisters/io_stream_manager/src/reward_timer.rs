use ic_cdk_timers::{clear_timer, set_timer, TimerId};
use std::{cell::RefCell, time::Duration};

use crate::state::{Lifecycle, ReconciliationCheckpoint, RewardEventId, RewardEventObservation};

const OBSERVATION_MARGIN_SECONDS: u64 = 300;
const RETRY_DELAY_SECONDS: u64 = 60;

thread_local! {
    static ACTIVE_SCHEDULER_TIMER: RefCell<Option<(TimerId, u64)>> = const { RefCell::new(None) };
    #[cfg(debug_assertions)]
    static CALLBACK_INVOCATIONS: RefCell<u64> = const { RefCell::new(0) };
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, candid::CandidType, serde::Deserialize)]
pub struct DebugSchedulerStatus {
    pub active_deadline_seconds: Option<u64>,
    pub callback_invocations: u64,
    pub recovery_deferrals: u64,
}

#[cfg(debug_assertions)]
pub fn debug_status() -> DebugSchedulerStatus {
    DebugSchedulerStatus {
        active_deadline_seconds: ACTIVE_SCHEDULER_TIMER
            .with(|slot| slot.borrow().as_ref().map(|(_, deadline)| *deadline)),
        callback_invocations: CALLBACK_INVOCATIONS.with(|value| *value.borrow()),
        recovery_deferrals: 0,
    }
}

#[cfg(debug_assertions)]
pub fn debug_install_at(deadline_seconds: u64) {
    install(Some(deadline_seconds));
}

pub(crate) fn install_for_ready_state() {
    let state = crate::state::read();
    if state.lifecycle != Lifecycle::Ready || state.reward_checkpoint.reward_processing_paused {
        install_retry();
        return;
    }
    let now = ic_cdk::api::time() / 1_000_000_000;
    if state.reward_checkpoint.reward_work_due
        || state.stake_observation_due
        || state.structural_reconciliation_due
        || state.reward_checkpoint.last_processed_event.is_none()
        || state.latest_reconciliation_checkpoint.is_none()
    {
        install(now.checked_add(1));
        return;
    }
    let reward = reward_deadline(
        state
            .reward_checkpoint
            .last_processed_event
            .expect("checked"),
        state.config.approved_reward_event_duration_seconds,
        state.reward_checkpoint.latest_observation.as_ref(),
    );
    let structural = structural_deadline(
        state
            .latest_reconciliation_checkpoint
            .as_ref()
            .expect("checked"),
    );
    install(reward.into_iter().chain(structural).min());
}

fn observation_deadline(event: RewardEventId, duration_seconds: u64) -> Option<u64> {
    event
        .end_timestamp_seconds
        .checked_add(duration_seconds)?
        .checked_add(OBSERVATION_MARGIN_SECONDS)
}

fn reward_deadline(
    event: RewardEventId,
    duration_seconds: u64,
    latest_observation: Option<&RewardEventObservation>,
) -> Option<u64> {
    let canonical = observation_deadline(event, duration_seconds)?;
    match latest_observation
        .filter(|value| value.event == event)
        .map(|value| value.observed_at_nanos / 1_000_000_000)
        .filter(|observed| *observed >= canonical)
    {
        Some(observed) => observed.checked_add(RETRY_DELAY_SECONDS),
        None => Some(canonical),
    }
}

fn structural_deadline(checkpoint: &ReconciliationCheckpoint) -> Option<u64> {
    (checkpoint.observed_at_nanos / 1_000_000_000)
        .checked_add(io_core_model::STRUCTURAL_SYNC_INTERVAL_SECONDS)
}

pub(crate) fn install_retry() {
    install((ic_cdk::api::time() / 1_000_000_000).checked_add(RETRY_DELAY_SECONDS));
}

pub(crate) fn install_readiness() {
    install((ic_cdk::api::time() / 1_000_000_000).checked_add(1));
}

pub(crate) fn install(deadline_seconds: Option<u64>) {
    let retained = ACTIVE_SCHEDULER_TIMER.with(|slot| {
        let mut slot = slot.borrow_mut();
        match (slot.as_ref(), deadline_seconds) {
            (Some((_, current)), Some(next)) if *current <= next => true,
            (None, None) => true,
            _ => {
                if let Some((timer, _)) = slot.take() {
                    clear_timer(timer);
                }
                false
            }
        }
    });
    if retained {
        return;
    }
    let Some(deadline_seconds) = deadline_seconds else {
        return;
    };
    let delay = deadline_seconds.saturating_sub(ic_cdk::api::time() / 1_000_000_000);
    let timer = set_timer(Duration::from_secs(delay), async move {
        #[cfg(debug_assertions)]
        CALLBACK_INVOCATIONS.with(|value| *value.borrow_mut() += 1);
        ACTIVE_SCHEDULER_TIMER.with(|slot| {
            slot.borrow_mut().take();
        });
        let mut state = crate::state::read();
        if state.lifecycle != Lifecycle::Ready || state.reward_checkpoint.reward_processing_paused {
            if state.lifecycle == Lifecycle::Ready {
                state.lifecycle = Lifecycle::Paused;
                crate::state::write(state.clone());
            }
            if state.active_operation.is_some() {
                if let Err(error) = crate::api::resume_stream(ic_cdk::api::time()).await {
                    ic_cdk::api::debug_print(format!(
                        "stream recovery remains pending before readiness: {error:?}"
                    ));
                    install_retry();
                    return;
                }
                state = crate::state::read();
            }
            if state.active_operation.is_some() || state.prepared_exit_reconciliation.is_some() {
                install_retry();
                return;
            }
            let result = crate::lifecycle::readiness_preflight(
                ic_cdk::api::canister_self(),
                state.control_epoch,
            )
            .await;
            if let Err(error) = result {
                ic_cdk::api::debug_print(format!("stream readiness remains pending: {error:?}"));
                install_retry();
                return;
            }
            install_for_ready_state();
            crate::redemption_timer::install_normal();
            return;
        }
        let now_nanos = ic_cdk::api::time();
        let now_seconds = now_nanos / 1_000_000_000;
        if state
            .latest_reconciliation_checkpoint
            .as_ref()
            .is_none_or(|checkpoint| {
                structural_deadline(checkpoint).is_none_or(|deadline| deadline <= now_seconds)
            })
        {
            state.stake_observation_due = true;
        }
        if state
            .reward_checkpoint
            .last_processed_event
            .is_none_or(|event| {
                reward_deadline(
                    event,
                    state.config.approved_reward_event_duration_seconds,
                    state.reward_checkpoint.latest_observation.as_ref(),
                )
                .is_none_or(|deadline| deadline <= now_seconds)
            })
        {
            state.reward_checkpoint.reward_work_due = true;
        }
        let work_due = state.reward_checkpoint.reward_work_due
            || state.stake_observation_due
            || state.structural_reconciliation_due;
        crate::state::write(state);
        if work_due {
            if let Err(error) = crate::rewards::observe(now_nanos).await {
                ic_cdk::api::debug_print(format!("structural/reward work remains due: {error:?}"));
                install_retry();
            }
        } else {
            install_for_ready_state();
        }
    });
    ACTIVE_SCHEDULER_TIMER.with(|slot| {
        *slot.borrow_mut() = Some((timer, deadline_seconds));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reward_deadline_retains_the_coarse_retry_policy() {
        let event = RewardEventId {
            end_timestamp_seconds: 1_000,
            round: 1,
        };
        assert_eq!(observation_deadline(event, 86_400), Some(87_700));
        let observed = RewardEventObservation {
            event,
            proposal_count: 0,
            classification: crate::state::RewardEventClassification::StructuralOnly,
            policy_credit: 0,
            eligible_credit_total: 0,
            observed_at_nanos: 87_701_900_000_000,
        };
        assert_eq!(
            reward_deadline(event, 86_400, Some(&observed)),
            Some(87_761)
        );
    }
}
