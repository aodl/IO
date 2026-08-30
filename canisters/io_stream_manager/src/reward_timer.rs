use ic_cdk_timers::{clear_timer, set_timer, TimerId};
use std::{cell::RefCell, time::Duration};

use crate::state::{Lifecycle, ReconciliationCheckpoint, RewardEventId, RewardEventObservation};

const OBSERVATION_MARGIN_SECONDS: u64 = 300;
const RETRY_DELAY_SECONDS: u64 = 60;

thread_local! {
    static ACTIVE_SCHEDULER_TIMER: RefCell<Option<(TimerId, u64)>> = const { RefCell::new(None) };
}

pub(crate) fn install_for_ready_state() {
    let state = crate::state::read();
    if state.lifecycle != Lifecycle::Ready || state.reward_checkpoint.reward_processing_paused {
        install(None);
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
    let canonical_deadline = observation_deadline(event, duration_seconds)?;
    let observed_same_event_at = latest_observation
        .filter(|observation| observation.event == event)
        .map(|observation| observation.observed_at_nanos / 1_000_000_000)
        .filter(|observed_at| *observed_at >= canonical_deadline);
    match observed_same_event_at {
        Some(observed_at) => observed_at.checked_add(RETRY_DELAY_SECONDS),
        None => Some(canonical_deadline),
    }
}

fn structural_deadline(checkpoint: &ReconciliationCheckpoint) -> Option<u64> {
    (checkpoint.observed_at_nanos / 1_000_000_000)
        .checked_add(io_core_model::STRUCTURAL_SYNC_INTERVAL_SECONDS)
}

pub(crate) fn install_retry() {
    install((ic_cdk::api::time() / 1_000_000_000).checked_add(RETRY_DELAY_SECONDS));
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
    let now_seconds = ic_cdk::api::time() / 1_000_000_000;
    let delay = deadline_seconds.saturating_sub(now_seconds);
    let timer = set_timer(Duration::from_secs(delay), async move {
        ACTIVE_SCHEDULER_TIMER.with(|slot| {
            slot.borrow_mut().take();
        });
        let mut state = crate::state::read();
        if state.lifecycle != Lifecycle::Ready || state.reward_checkpoint.reward_processing_paused {
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
                ic_cdk::api::debug_print(format!(
                    "structural/reward scheduler work remains due after failure: {error:?}"
                ));
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
    fn reward_and_structural_deadlines_are_independent() {
        let event = RewardEventId {
            end_timestamp_seconds: 1_000,
            round: 1,
        };
        assert_eq!(observation_deadline(event, 86_400), Some(87_700));
        let same_event = RewardEventObservation {
            event,
            proposal_count: 0,
            classification: crate::state::RewardEventClassification::StructuralOnly,
            policy_credit: 0,
            eligible_credit_total: 0,
            observed_at_nanos: 87_701_000_000_000,
        };
        assert_eq!(
            reward_deadline(event, 86_400, Some(&same_event)),
            Some(87_761)
        );
        let checkpoint = ReconciliationCheckpoint {
            generation: 1,
            event_marker: 1,
            observed_at_nanos: 2_000_000_000,
            claim_supply_e8s: 1,
            liquid_backing_e8s: 1,
            pooled_backing_e8s: 0,
            unwinding_backing_e8s: 0,
            transit_backing_e8s: 0,
            total_claim_backing_e8s: 1,
            active_backing_io_e8s: 0,
            active_reward_io_e8s: 0,
            live_cohort_count: 0,
            oldest_ready_at_seconds: None,
            pooled_target_e8s: 0,
            observed_pooled_e8s: 0,
            snapshot_fingerprint: vec![1; 32],
        };
        assert_eq!(structural_deadline(&checkpoint), Some(43_202));
    }
}
