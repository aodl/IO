use crate::{
    api::Status,
    state::{self, RedemptionStreamOperation, StreamOperation},
};

pub fn get_status() -> Status {
    let state = state::read();
    let (operation_kind, operation_phase) = match state.active_operation {
        Some(StreamOperation::Redemption(operation)) => match *operation {
            RedemptionStreamOperation::Preparing(_) => {
                (Some("Redemption".into()), Some("Preparing".into()))
            }
            RedemptionStreamOperation::Active(operation) => (
                Some("Redemption".into()),
                Some(format!("{:?}", operation.phase)),
            ),
        },
        Some(StreamOperation::ClaimReceipt(operation)) => (
            Some("ClaimReceipt".into()),
            Some(if operation.liquid_block.is_none() {
                "AwaitingLiquidProof".into()
            } else {
                "SettlingRecipients".into()
            }),
        ),
        Some(StreamOperation::PoolTopUp(_)) => {
            (Some("BackingReconciliation".into()), Some("TopUp".into()))
        }
        None => (None, None),
    };
    let accumulated_eligible_credit = state
        .neuron_registry
        .iter()
        .try_fold(0u128, |sum, entry| {
            sum.checked_add(entry.accumulated_eligible_credit)
        })
        .expect("validated entitlement accumulator total");
    Status {
        lifecycle: state.lifecycle,
        operation_kind,
        operation_phase,
        next_operation_sequence: state.next_operation_sequence.0,
        latest_entitlement_batch_generation: state.latest_entitlement_batch_generation,
        latest_processed_reward_event: state.reward_checkpoint.last_processed_event,
        latest_reward_event_classification: state
            .reward_checkpoint
            .latest_observation
            .as_ref()
            .map(|observation| observation.classification),
        accumulated_entitlements: state
            .neuron_registry
            .iter()
            .filter(|record| record.accumulated_eligible_credit > 0)
            .map(|record| crate::state::FrozenEntitlement {
                sns_neuron_id: record.sns_neuron_id.clone(),
                destination: record.staking_account.clone(),
                accumulated_eligible_credit: record.accumulated_eligible_credit,
            })
            .collect(),
        accumulated_eligible_credit,
        accumulated_policy_credit: state.reward_checkpoint.accumulated_policy_credit,
        processed_reward_event_count: state.reward_checkpoint.processed_event_count,
        missed_reward_event_count: state.reward_checkpoint.missed_event_count,
        reward_work_due: state.reward_checkpoint.reward_work_due || state.stake_observation_due,
        reward_processing_paused: state.reward_checkpoint.reward_processing_paused,
        governance_parameters_fresh: state.reward_checkpoint.governance_parameters_fresh,
        pending_entitlement_batch_eligible_credit: state
            .pending_entitlement_batch
            .as_ref()
            .map(|batch| batch.eligible_credit_total),
        pending_entitlement_batch_policy_credit: state
            .pending_entitlement_batch
            .as_ref()
            .map(|batch| batch.policy_credit_total),
        latest_reconciliation_checkpoint: state.latest_reconciliation_checkpoint,
        prepared_exit_generation: state
            .prepared_exit_reconciliation
            .as_ref()
            .map(|request| request.generation),
        prepared_exit_member_count: state.neuron_registry.iter().fold(0, |count, record| {
            count
                + u32::from(matches!(
                    record.status,
                    crate::state::BackingRewardStatus::ExitPrepared { .. }
                ))
        }),
        committed_exit_member_count: state.neuron_registry.iter().fold(0, |count, record| {
            count
                + u32::from(matches!(
                    record.status,
                    crate::state::BackingRewardStatus::ExitCommitted { .. }
                ))
        }),
    }
}
