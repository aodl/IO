use ic_cdk_timers::{clear_timer, set_timer, TimerId};
use std::{cell::RefCell, time::Duration};

use crate::{
    redemption::RedemptionPhase,
    state::{
        Lifecycle, ReconciliationCheckpoint, RedemptionStreamOperation, RewardEventId,
        RewardEventObservation, StreamOperation,
    },
    transfer::TransferState,
};

const OBSERVATION_MARGIN_SECONDS: u64 = 300;
const RETRY_DELAY_SECONDS: u64 = 60;
const NANOS_PER_SECOND: u64 = 1_000_000_000;

thread_local! {
    static ACTIVE_SCHEDULER_TIMER: RefCell<Option<(TimerId, u64)>> = const { RefCell::new(None) };
    #[cfg(debug_assertions)]
    static SCHEDULER_CALLBACK_INVOCATIONS: RefCell<u64> = const { RefCell::new(0) };
    #[cfg(debug_assertions)]
    static SCHEDULER_RECOVERY_DEFERRALS: RefCell<u64> = const { RefCell::new(0) };
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
        callback_invocations: SCHEDULER_CALLBACK_INVOCATIONS.with(|value| *value.borrow()),
        recovery_deferrals: SCHEDULER_RECOVERY_DEFERRALS.with(|value| *value.borrow()),
    }
}

#[cfg(debug_assertions)]
pub fn debug_install_at(deadline_seconds: u64) {
    install(Some(deadline_seconds));
}

pub(crate) fn install_for_ready_state() {
    let state = crate::state::read();
    if state.lifecycle != Lifecycle::Ready {
        install(None);
        return;
    }
    let now_nanos = ic_cdk::api::time();
    let now = now_nanos / NANOS_PER_SECOND;
    if state.active_operation.is_some() {
        install(active_operation_deadline(&state, now_nanos));
        return;
    }
    if !state.reward_checkpoint.reward_processing_paused
        && (state.reward_checkpoint.reward_work_due
            || state.stake_observation_due
            || state.structural_reconciliation_due
            || state.reward_checkpoint.last_processed_event.is_none()
            || state.latest_reconciliation_checkpoint.is_none())
    {
        if !has_future_timer(now) {
            install(now.checked_add(1));
        }
        return;
    }
    let reward = (!state.reward_checkpoint.reward_processing_paused)
        .then(|| {
            reward_deadline(
                state
                    .reward_checkpoint
                    .last_processed_event
                    .expect("checked"),
                state.config.approved_reward_event_duration_seconds,
                state.reward_checkpoint.latest_observation.as_ref(),
            )
        })
        .flatten();
    let structural = (!state.reward_checkpoint.reward_processing_paused)
        .then(|| {
            structural_deadline(
                state
                    .latest_reconciliation_checkpoint
                    .as_ref()
                    .expect("checked"),
            )
        })
        .flatten();
    let redemption = redemption_deadline(&state, now_nanos);
    install(reward.into_iter().chain(structural).chain(redemption).min());
}

fn has_future_timer(now_seconds: u64) -> bool {
    ACTIVE_SCHEDULER_TIMER.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|(_, deadline)| *deadline > now_seconds)
    })
}

fn absolute_deadline_seconds(start_nanos: u64, delay_nanos: u64) -> Option<u64> {
    let due_nanos = start_nanos.checked_add(delay_nanos)?;
    (due_nanos / NANOS_PER_SECOND).checked_add(u64::from(due_nanos % NANOS_PER_SECOND != 0))
}

fn active_operation_deadline(state: &crate::state::StreamStateV1, now_nanos: u64) -> Option<u64> {
    let Some(StreamOperation::Redemption(operation)) = &state.active_operation else {
        return (now_nanos / NANOS_PER_SECOND).checked_add(1);
    };
    let RedemptionStreamOperation::Active(operation) = operation.as_ref();
    let attempt_deadline = |attempt: &crate::transfer::TransferAttempt| match attempt.state {
        TransferState::Submitted {
            last_submitted_at, ..
        } => absolute_deadline_seconds(last_submitted_at, state.config.retry_delay_nanos),
        _ => (now_nanos / NANOS_PER_SECOND).checked_add(1),
    };
    match operation.phase {
        RedemptionPhase::PayoutSubmitted => attempt_deadline(&operation.icp_payout),
        RedemptionPhase::SweepSubmitted => {
            operation.reserve_sweep.as_ref().and_then(attempt_deadline)
        }
        RedemptionPhase::PayoutSucceeded if operation.last_external_call_started_at_nanos != 0 => {
            absolute_deadline_seconds(
                operation.last_external_call_started_at_nanos,
                state.config.retry_delay_nanos,
            )
        }
        RedemptionPhase::Stuck => None,
        _ => (now_nanos / NANOS_PER_SECOND).checked_add(1),
    }
}

fn redemption_deadline(state: &crate::state::StreamStateV1, now_nanos: u64) -> Option<u64> {
    if state.reward_checkpoint.reward_processing_paused
        && (state.reward_checkpoint.reward_work_due
            || state.stake_observation_due
            || state.structural_reconciliation_due)
    {
        return absolute_deadline_seconds(
            now_nanos,
            RETRY_DELAY_SECONDS.checked_mul(NANOS_PER_SECOND)?,
        );
    }
    if state.last_redemption_poll_started_at_nanos == 0 {
        (now_nanos / NANOS_PER_SECOND).checked_add(1)
    } else {
        absolute_deadline_seconds(
            state.last_redemption_poll_started_at_nanos,
            state
                .config
                .redemption_poll_interval_seconds
                .checked_mul(NANOS_PER_SECOND)?,
        )
    }
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
        .map(|observation| observation.observed_at_nanos)
        .filter(|observed_at| *observed_at / NANOS_PER_SECOND >= canonical_deadline);
    match observed_same_event_at {
        Some(observed_at) => absolute_deadline_seconds(
            observed_at,
            RETRY_DELAY_SECONDS.checked_mul(NANOS_PER_SECOND)?,
        ),
        None => Some(canonical_deadline),
    }
}

fn structural_deadline(checkpoint: &ReconciliationCheckpoint) -> Option<u64> {
    absolute_deadline_seconds(
        checkpoint.observed_at_nanos,
        io_core_model::STRUCTURAL_SYNC_INTERVAL_SECONDS.checked_mul(NANOS_PER_SECOND)?,
    )
}

pub(crate) fn install_retry() {
    install(absolute_deadline_seconds(
        ic_cdk::api::time(),
        RETRY_DELAY_SECONDS * NANOS_PER_SECOND,
    ));
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
        #[cfg(debug_assertions)]
        SCHEDULER_CALLBACK_INVOCATIONS.with(|value| {
            let mut current = value.borrow_mut();
            *current = current.saturating_add(1);
        });
        ACTIVE_SCHEDULER_TIMER.with(|slot| {
            slot.borrow_mut().take();
        });
        let mut state = crate::state::read();
        if state.lifecycle != Lifecycle::Ready {
            return;
        }
        if state.active_operation.is_some() {
            if let Err(error) = crate::api::resume_stream(ic_cdk::api::time()).await {
                #[cfg(debug_assertions)]
                SCHEDULER_RECOVERY_DEFERRALS.with(|value| {
                    let mut current = value.borrow_mut();
                    *current = current.saturating_add(1);
                });
                ic_cdk::api::debug_print(format!(
                    "scheduler exact recovery remains pending: {error:?}"
                ));
                install_retry();
                return;
            }
            install_for_ready_state();
            return;
        }
        let now_nanos = ic_cdk::api::time();
        let now_seconds = now_nanos / 1_000_000_000;
        if !state.reward_checkpoint.reward_processing_paused
            && state
                .latest_reconciliation_checkpoint
                .as_ref()
                .is_none_or(|checkpoint| {
                    structural_deadline(checkpoint).is_none_or(|deadline| deadline <= now_seconds)
                })
        {
            state.stake_observation_due = true;
        }
        if !state.reward_checkpoint.reward_processing_paused
            && state
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
        let reward_processing_paused = state.reward_checkpoint.reward_processing_paused;
        crate::state::write(state);
        if work_due && !reward_processing_paused {
            if let Err(error) = crate::rewards::observe(now_nanos).await {
                ic_cdk::api::debug_print(format!(
                    "structural/reward scheduler work remains due after failure: {error:?}"
                ));
                install_retry();
                return;
            }
        } else if let Err(error) = crate::api::run_scheduled_redemption_work(now_nanos).await {
            #[cfg(debug_assertions)]
            SCHEDULER_RECOVERY_DEFERRALS.with(|value| {
                let mut current = value.borrow_mut();
                *current = current.saturating_add(1);
            });
            ic_cdk::api::debug_print(format!(
                "scheduled redemption work remains pending: {error:?}"
            ));
            install_retry();
            return;
        }
        install_for_ready_state();
    });
    ACTIVE_SCHEDULER_TIMER.with(|slot| {
        *slot.borrow_mut() = Some((timer, deadline_seconds));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        redemption::RedemptionOperation,
        state::{OperationSequence, RedemptionStreamOperation, StreamOperation},
        transfer::{OwnTransferIntent, TransferAttempt},
    };
    use candid::Principal;

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

    #[test]
    fn active_redemption_deadline_respects_attempt_and_fee_retry_boundaries() {
        let (_, mut state) = crate::state::tests::valid_state();
        state.lifecycle = Lifecycle::Ready;
        state.config.retry_delay_nanos = 1_000_000_000;
        let attempt = TransferAttempt {
            intent: OwnTransferIntent::Icrc1 {
                ledger: Principal::from_slice(&[1]),
                from_subaccount: [1; 32],
                to: crate::state::Account {
                    owner: Principal::from_slice(&[2]),
                    subaccount: None,
                },
                amount: 90,
                fee: 10,
                memo: vec![],
                created_at_time: 1,
            },
            state: TransferState::Submitted {
                epoch: crate::state::DispatchEpoch(1),
                first_submitted_at: 20_900_000_000,
                last_submitted_at: 20_900_000_000,
            },
        };
        let mut operation = RedemptionOperation {
            sequence: OperationSequence(1),
            source_io_block: 1,
            source_account: crate::state::Account {
                owner: Principal::from_slice(&[3]),
                subaccount: None,
            },
            staged_io_amount_e8s: 100,
            gross_icp_e8s: 100,
            net_icp_e8s: 90,
            icp_fee_e8s: 10,
            io_sweep_fee_e8s: 10,
            icp_payout: attempt,
            reserve_sweep: None,
            last_external_call_started_at_nanos: 0,
            phase: RedemptionPhase::PayoutSubmitted,
        };
        state.active_operation = Some(StreamOperation::Redemption(Box::new(
            RedemptionStreamOperation::Active(Box::new(operation.clone())),
        )));
        assert_eq!(active_operation_deadline(&state, 21_100_000_000), Some(22));

        operation.icp_payout.state = TransferState::Succeeded { block: 7 };
        operation.phase = RedemptionPhase::PayoutSucceeded;
        operation.last_external_call_started_at_nanos = 30_900_000_000;
        state.active_operation = Some(StreamOperation::Redemption(Box::new(
            RedemptionStreamOperation::Active(Box::new(operation.clone())),
        )));
        assert_eq!(active_operation_deadline(&state, 31_100_000_000), Some(32));

        operation.phase = RedemptionPhase::Stuck;
        state.active_operation = Some(StreamOperation::Redemption(Box::new(
            RedemptionStreamOperation::Active(Box::new(operation)),
        )));
        assert_eq!(active_operation_deadline(&state, 31_100_000_000), None);
    }

    #[test]
    fn overdue_redemption_poll_backs_off_while_reward_facets_are_paused_and_due() {
        let (_, mut state) = crate::state::tests::valid_state();
        state.lifecycle = Lifecycle::Ready;
        state.reward_checkpoint.reward_processing_paused = true;
        state.last_redemption_poll_started_at_nanos = 100_900_000_000;
        state.config.redemption_poll_interval_seconds = 60;

        for due in 0..3 {
            state.reward_checkpoint.reward_work_due = due == 0;
            state.stake_observation_due = due == 1;
            state.structural_reconciliation_due = due == 2;
            assert_eq!(redemption_deadline(&state, 200_000_000_000), Some(260));
        }
        state.reward_checkpoint.reward_processing_paused = false;
        state.reward_checkpoint.reward_work_due = false;
        state.stake_observation_due = false;
        state.structural_reconciliation_due = false;
        assert_eq!(redemption_deadline(&state, 200_000_000_000), Some(161));
        assert_eq!(
            absolute_deadline_seconds(21_100_000_000, RETRY_DELAY_SECONDS * NANOS_PER_SECOND),
            Some(82),
            "a Busy recovery at 21.1s is rearmed beyond 81.1s rather than at an expired deadline"
        );
    }
}
