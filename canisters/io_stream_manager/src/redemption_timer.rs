use ic_cdk_timers::{clear_timer, set_timer, TimerId};
use std::{cell::Cell, cell::RefCell, time::Duration};

use crate::state::Lifecycle;

thread_local! {
    static REDEMPTION_TIMER: RefCell<Option<(TimerId, bool)>> = const { RefCell::new(None) };
    static LAST_MANUAL_WAKE_AT: Cell<Option<u64>> = const { Cell::new(None) };
}

const MANUAL_WAKE_COOLDOWN_NANOS: u64 = 10_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WakeKind {
    NormalPoll,
    NearTerm,
}

pub(crate) fn install_normal() {
    install(
        crate::state::read().config.redemption_poll_interval_seconds,
        false,
    );
}

pub(crate) fn wake_soon() {
    let already_urgent =
        REDEMPTION_TIMER.with(|slot| slot.borrow().as_ref().is_some_and(|(_, urgent)| *urgent));
    if already_urgent {
        return;
    }
    let now = ic_cdk::api::time();
    let admitted = LAST_MANUAL_WAKE_AT.with(|last| {
        if last
            .get()
            .is_some_and(|last| now.saturating_sub(last) < MANUAL_WAKE_COOLDOWN_NANOS)
        {
            false
        } else {
            last.set(Some(now));
            true
        }
    });
    if !admitted {
        return;
    }
    install(1, true);
}

pub(crate) fn install_near_term() {
    install(1, true);
}

pub(crate) fn cancel() {
    REDEMPTION_TIMER.with(|slot| {
        if let Some((timer, _)) = slot.borrow_mut().take() {
            clear_timer(timer);
        }
    });
}

fn install(delay_seconds: u64, urgent: bool) {
    if crate::state::read().lifecycle != Lifecycle::Ready {
        cancel();
        return;
    }
    let retained = REDEMPTION_TIMER.with(|slot| {
        let mut slot = slot.borrow_mut();
        match slot.as_ref() {
            Some((_, already_urgent)) if *already_urgent || !urgent => true,
            Some(_) => {
                let (timer, _) = slot.take().expect("checked");
                clear_timer(timer);
                false
            }
            None => false,
        }
    });
    if retained {
        return;
    }
    let timer = set_timer(Duration::from_secs(delay_seconds), async move {
        REDEMPTION_TIMER.with(|slot| {
            slot.borrow_mut().take();
        });
        let wake = if urgent {
            WakeKind::NearTerm
        } else {
            WakeKind::NormalPoll
        };
        if let Err(error) = crate::api::run_redemption_worker(ic_cdk::api::time(), wake).await {
            ic_cdk::api::debug_print(format!("redemption wake deferred: {error:?}"));
        }
        install_normal();
    });
    REDEMPTION_TIMER.with(|slot| *slot.borrow_mut() = Some((timer, urgent)));
}
