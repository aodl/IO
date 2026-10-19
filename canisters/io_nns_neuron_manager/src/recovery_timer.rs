#[cfg(target_family = "wasm")]
use ic_cdk_timers::set_timer_interval_serial;
use ic_cdk_timers::{clear_timer, set_timer, TimerId};
use std::{cell::RefCell, time::Duration};

use crate::{
    api::{ApiError, MaturityProgress, NnsProgress},
    maturity::MaturityKind,
    state::NnsStateV1,
};

const RETRY_DELAY_SECONDS: u64 = 60;
#[cfg(target_family = "wasm")]
const TWO_YEAR_MATURITY_INTERVAL_SECONDS: u64 = 604_800;

thread_local! {
    static ACTIVE_RECOVERY_TIMER: RefCell<Option<(TimerId, u64)>> = const { RefCell::new(None) };
    #[cfg(target_family = "wasm")]
    static ACTIVE_TWO_YEAR_TIMER: RefCell<Option<TimerId>> = const { RefCell::new(None) };
}

pub(crate) fn install_for_state() {
    let now_nanos = ic_cdk::api::time();
    install(next_deadline(&crate::state::read(), now_nanos));
}

#[cfg(target_family = "wasm")]
pub(crate) fn replace_for_state() {
    ACTIVE_RECOVERY_TIMER.with(|slot| {
        if let Some((timer, _)) = slot.borrow_mut().take() {
            clear_timer(timer);
        }
    });
    install_for_state();
    install_two_year_timer();
}

pub(crate) fn install_readiness() {
    install(Some(after_seconds(ic_cdk::api::time(), 1)));
}

fn next_deadline(state: &NnsStateV1, now_nanos: u64) -> Option<u64> {
    if state.lifecycle == crate::state::Lifecycle::Paused {
        return Some(after_seconds(now_nanos, RETRY_DELAY_SECONDS));
    }
    if state.active_operation.is_some() {
        return Some(after_seconds(now_nanos, RETRY_DELAY_SECONDS));
    }
    let child = state
        .live_cohorts
        .iter()
        .map(|cohort| absolute_seconds(cohort.ready_at_seconds))
        .min();
    let maturity = [
        state.pending_two_year_maturity.as_ref(),
        state.pending_two_week_maturity.as_ref(),
    ]
    .into_iter()
    .flatten()
    .map(|pending| {
        if pending.captured_e8s.is_some() {
            after_seconds(now_nanos, 1)
        } else {
            absolute_seconds(pending.scheduled_finalization_timestamp_seconds)
        }
    })
    .min();
    child.into_iter().chain(maturity).min()
}

#[cfg(target_family = "wasm")]
fn install_two_year_timer() {
    if crate::state::read().lifecycle != crate::state::Lifecycle::Ready {
        return;
    }
    ACTIVE_TWO_YEAR_TIMER.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }
        let timer = set_timer_interval_serial(
            Duration::from_secs(TWO_YEAR_MATURITY_INTERVAL_SECONDS),
            async || match crate::maturity_flow::try_start_two_year_maturity().await {
                Ok(_)
                | Err(
                    ApiError::Busy
                    | ApiError::Pending(_)
                    | ApiError::Paused
                    | ApiError::BelowMaturityThreshold { .. },
                ) => {}
                Err(error) => schedule_result(Err(error)),
            },
        );
        *slot.borrow_mut() = Some(timer);
    });
}

fn after_seconds(now_nanos: u64, seconds: u64) -> u64 {
    now_nanos.saturating_add(seconds.saturating_mul(1_000_000_000))
}

fn absolute_seconds(seconds: u64) -> u64 {
    seconds.saturating_mul(1_000_000_000)
}

fn install(deadline_nanos: Option<u64>) {
    let retained = ACTIVE_RECOVERY_TIMER.with(|slot| {
        let mut slot = slot.borrow_mut();
        match (slot.as_ref(), deadline_nanos) {
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
    let Some(deadline_nanos) = deadline_nanos else {
        return;
    };
    let timer = set_timer(
        Duration::from_nanos(deadline_nanos.saturating_sub(ic_cdk::api::time())),
        async move {
            ACTIVE_RECOVERY_TIMER.with(|slot| {
                slot.borrow_mut().take();
            });
            let snapshot = crate::state::read();
            let has_passive_work = !snapshot.live_cohorts.is_empty()
                || snapshot.pending_two_year_maturity.is_some()
                || snapshot.pending_two_week_maturity.is_some();
            let result = if snapshot.active_operation.is_some() {
                match crate::api::resume().await {
                    Ok(progress) => continue_after_progress(&progress).await,
                    Err(error) => Err(error),
                }
            } else if snapshot.lifecycle == crate::state::Lifecycle::Paused {
                crate::lifecycle::readiness_preflight(
                    ic_cdk::api::canister_self(),
                    snapshot.control_epoch,
                )
                .await
            } else if has_passive_work {
                crate::api::resume().await.map(|_| ())
            } else {
                Ok(())
            };
            schedule_result(result);
        },
    );
    ACTIVE_RECOVERY_TIMER.with(|slot| {
        *slot.borrow_mut() = Some((timer, deadline_nanos));
    });
}

pub(crate) async fn continue_after_progress(progress: &NnsProgress) -> Result<(), ApiError> {
    if matches!(
        progress,
        NnsProgress::Maturity(MaturityProgress::Completed(completed))
            if completed.kind == MaturityKind::TwoYear
    ) {
        crate::maturity_flow::try_start_two_year_maturity()
            .await
            .map(|_| ())
    } else {
        Ok(())
    }
}

pub(crate) fn schedule_result(result: Result<(), ApiError>) {
    match result {
        Ok(()) => install_for_state(),
        Err(ApiError::Busy | ApiError::Pending(_) | ApiError::Paused) => install(Some(
            after_seconds(ic_cdk::api::time(), RETRY_DELAY_SECONDS),
        )),
        Err(ApiError::BelowMaturityThreshold { .. }) => install_for_state(),
        Err(ApiError::Invalid(_) | ApiError::Stuck(_) | ApiError::Unauthorized) => install(Some(
            after_seconds(ic_cdk::api::time(), RETRY_DELAY_SECONDS),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_child_readiness_precedes_later_passive_work() {
        let (_, mut state) = crate::state::tests::valid_state();
        state.lifecycle = crate::state::Lifecycle::Ready;
        state.live_cohorts = vec![crate::pool::PassiveCohort {
            generation: 1,
            reconciliation_request_fingerprint: vec![1; 32],
            child_neuron_id: 2,
            principal_e8s: 100,
            committed_fee_e8s: 10,
            child_staking_subaccount: vec![1; 32],
            ready_at_seconds: 200,
            proof: io_nns_types::backing::CohortProofState::Dissolving,
            disbursement_block: None,
        }];
        assert_eq!(
            next_deadline(&state, absolute_seconds(10)),
            Some(absolute_seconds(200))
        );
        state.active_operation = Some(crate::state::NnsOperation::Pool(
            io_nns_types::backing::PoolCommand {
                kind: io_nns_types::backing::PoolCommandKind::Bootstrap,
                permit: io_nns_types::backing::TopUpPermit {
                    generation: 0,
                    operation_sequence: 1,
                    expected_parent_principal_e8s: 0,
                    expected_parent_physical_e8s: io_nns_types::backing::DYNAMIC_ANCHOR_TARGET_E8S,
                    destination: crate::state::Account {
                        owner: state.config.nns_governance,
                        subaccount: Some(vec![2; 32]),
                    },
                    expected_credit_e8s: 0,
                    claim_credit_e8s: 0,
                    fee_e8s: state.config.expected_icp_fee_e8s,
                    memo: vec![1],
                    prepared_at_nanos: 1,
                    snapshot_fingerprint: vec![1; 32],
                },
                transfer_block_index: None,
                parent_neuron_id: None,
                phase: io_nns_types::backing::PoolCommandPhase::SeedObserved,
            },
        ));
        assert_eq!(
            next_deadline(&state, absolute_seconds(10)),
            Some(absolute_seconds(70))
        );
    }

    #[test]
    fn ready_idle_state_has_no_recovery_deadline() {
        let (_, mut state) = crate::state::tests::valid_state();
        state.lifecycle = crate::state::Lifecycle::Ready;
        assert_eq!(next_deadline(&state, absolute_seconds(10)), None);
        state.lifecycle = crate::state::Lifecycle::Paused;
        assert_eq!(
            next_deadline(&state, absolute_seconds(10)),
            Some(absolute_seconds(70))
        );
    }

    #[test]
    fn relative_deadline_requires_the_complete_interval() {
        assert_eq!(after_seconds(10_000_000_001, 1), 11_000_000_001);
        assert_eq!(after_seconds(u64::MAX, 1), u64::MAX);
    }
}
