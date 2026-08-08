use ic_cdk_timers::{clear_timer, set_timer, TimerId};
use std::{cell::RefCell, time::Duration};

use crate::state::{Lifecycle, RewardEventId};

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
    install(event.end_timestamp_seconds.checked_add(86_401));
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
            }
        });
        *slot.borrow_mut() = Some(timer);
    });
}
