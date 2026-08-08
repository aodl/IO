use ic_cdk_timers::{clear_timer, set_timer, TimerId};
use std::{cell::RefCell, time::Duration};

use crate::state::{Lifecycle, RewardEventId};

const OBSERVATION_MARGIN_SECONDS: u64 = 1;

thread_local! {
    static ACTIVE_REWARD_TIMER: RefCell<Option<TimerId>> = const { RefCell::new(None) };
}

pub(crate) fn install_for_ready_state() {
    let state = crate::state::read();
    if state.lifecycle != Lifecycle::Ready || state.reward_entitlements.reward_processing_paused {
        install(None);
        return;
    }
    if state.reward_entitlements.reward_work_due
        || state.reward_entitlements.last_processed_event.is_none()
    {
        install((ic_cdk::api::time() / 1_000_000_000).checked_add(1));
    } else if let Some(event) = state.reward_entitlements.last_processed_event {
        install_after(event);
    }
}

pub(crate) fn install_after(event: RewardEventId) {
    install(observation_deadline(event));
}

fn observation_deadline(event: RewardEventId) -> Option<u64> {
    event
        .end_timestamp_seconds
        .checked_add(86_400)?
        .checked_add(OBSERVATION_MARGIN_SECONDS)
}

fn retry_deadline(_error: &crate::api::ApiError, _now_seconds: u64) -> Option<u64> {
    None
}

pub(crate) fn install(deadline_seconds: Option<u64>) {
    ACTIVE_REWARD_TIMER.with(|slot| {
        if let Some(timer) = slot.borrow_mut().take() {
            clear_timer(timer);
        }
        let Some(deadline_seconds) = deadline_seconds else {
            return;
        };
        let now_seconds = ic_cdk::api::time() / 1_000_000_000;
        let delay = deadline_seconds.saturating_sub(now_seconds);
        let timer = set_timer(Duration::from_secs(delay), async move {
            ACTIVE_REWARD_TIMER.with(|slot| {
                slot.borrow_mut().take();
            });
            let mut state = crate::state::read();
            if state.lifecycle != Lifecycle::Ready
                || state.reward_entitlements.reward_processing_paused
            {
                return;
            }
            state.reward_entitlements.reward_work_due = true;
            crate::state::write(state);
            if let Err(error) = crate::rewards::observe(ic_cdk::api::time()).await {
                ic_cdk::api::debug_print(format!(
                    "daily reward-event work remains due after failure: {error:?}"
                ));
                if let Some(deadline) = retry_deadline(&error, ic_cdk::api::time() / 1_000_000_000)
                {
                    install(Some(deadline));
                }
            }
        });
        *slot.borrow_mut() = Some(timer);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_one_second_margin_has_no_pending_replacement_timer() {
        let event = RewardEventId {
            end_timestamp_seconds: 1_000,
            round: 1,
        };
        assert_eq!(observation_deadline(event), Some(87_401));
        assert_eq!(
            retry_deadline(
                &crate::api::ApiError::Pending("event has not advanced".into()),
                87_401,
            ),
            None
        );
    }
}
