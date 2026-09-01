use ic_cdk_timers::{clear_timer, set_timer, TimerId};
use std::{cell::RefCell, time::Duration};

use crate::{api::ApiError, state::NnsStateV1};

const RETRY_DELAY_SECONDS: u64 = 60;

thread_local! {
    static ACTIVE_RECOVERY_TIMER: RefCell<Option<(TimerId, u64)>> = const { RefCell::new(None) };
}

pub(crate) fn install_for_state() {
    let now = ic_cdk::api::time() / 1_000_000_000;
    install(next_deadline(&crate::state::read(), now));
}

fn next_deadline(state: &NnsStateV1, now_seconds: u64) -> Option<u64> {
    if state.active_operation.is_some() {
        return now_seconds.checked_add(RETRY_DELAY_SECONDS);
    }
    let child = state
        .live_cohorts
        .iter()
        .map(|cohort| cohort.ready_at_seconds)
        .min();
    let maturity = [
        state.pending_two_year_maturity.as_ref(),
        state.pending_two_week_maturity.as_ref(),
    ]
    .into_iter()
    .flatten()
    .map(|pending| {
        if pending.captured_e8s.is_some() {
            now_seconds.saturating_add(1)
        } else {
            pending.scheduled_finalization_timestamp_seconds
        }
    })
    .min();
    child.into_iter().chain(maturity).min()
}

fn install(deadline_seconds: Option<u64>) {
    let retained = ACTIVE_RECOVERY_TIMER.with(|slot| {
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
    let now_nanos = ic_cdk::api::time();
    let timer = set_timer(
        Duration::from_secs(representable_delay_seconds(deadline_seconds, now_nanos)),
        async move {
            ACTIVE_RECOVERY_TIMER.with(|slot| {
                slot.borrow_mut().take();
            });
            match crate::api::resume().await {
                Ok(_) => install_for_state(),
                Err(ApiError::Busy | ApiError::Pending(_) | ApiError::Paused) => {
                    install((ic_cdk::api::time() / 1_000_000_000).checked_add(RETRY_DELAY_SECONDS));
                }
                Err(
                    ApiError::Invalid(_)
                    | ApiError::Stuck(_)
                    | ApiError::Unauthorized
                    | ApiError::BelowMaturityThreshold { .. },
                ) => install(None),
            }
        },
    );
    ACTIVE_RECOVERY_TIMER.with(|slot| {
        *slot.borrow_mut() = Some((timer, deadline_seconds));
    });
}

fn representable_delay_seconds(deadline_seconds: u64, now_nanos: u64) -> u64 {
    let requested = deadline_seconds.saturating_sub(now_nanos / 1_000_000_000);
    let representable = (u64::MAX - now_nanos) / 1_000_000_000;
    requested.min(representable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_child_readiness_precedes_later_passive_work() {
        let (_, mut state) = crate::state::tests::valid_state();
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
        assert_eq!(next_deadline(&state, 10), Some(200));
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
        assert_eq!(next_deadline(&state, 10), Some(70));
    }

    #[test]
    fn timer_delay_is_clamped_to_the_ic_time_horizon() {
        assert_eq!(representable_delay_seconds(200, 10_000_000_000), 190);
        assert_eq!(
            representable_delay_seconds(u64::MAX, 10_000_000_000),
            (u64::MAX - 10_000_000_000) / 1_000_000_000
        );
        assert_eq!(representable_delay_seconds(u64::MAX, u64::MAX), 0);
    }
}
