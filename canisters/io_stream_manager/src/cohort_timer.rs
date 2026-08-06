use ic_cdk_timers::{clear_timer, set_timer, TimerId};
use std::{cell::RefCell, time::Duration};

thread_local! {
    static ACTIVE_COHORT_TIMER: RefCell<Option<TimerId>> = const { RefCell::new(None) };
}

pub(crate) fn reinstall_from_state() {
    let deadline = crate::state::read()
        .active_reward_cohort
        .map(|cohort| cohort.closes_at_timestamp_seconds);
    install(deadline);
}

pub(crate) fn install(deadline_seconds: Option<u64>) {
    ACTIVE_COHORT_TIMER.with(|slot| {
        if let Some(timer) = slot.borrow_mut().take() {
            clear_timer(timer);
        }
        let Some(deadline_seconds) = deadline_seconds else {
            return;
        };
        let now_seconds = ic_cdk::api::time() / 1_000_000_000;
        #[allow(clippy::manual_saturating_arithmetic)]
        let delay = deadline_seconds.checked_sub(now_seconds).unwrap_or(0);
        let timer = set_timer(Duration::from_secs(delay), async move {
            ACTIVE_COHORT_TIMER.with(|slot| {
                slot.borrow_mut().take();
            });
            let now_seconds = ic_cdk::api::time() / 1_000_000_000;
            if let Err(error) = crate::rewards::close(now_seconds).await {
                ic_cdk::api::debug_print(format!(
                    "cohort deadline work remains due after failure: {error:?}"
                ));
            }
        });
        *slot.borrow_mut() = Some(timer);
    });
}
