use crate::{
    api::Status,
    state::{self, JupiterReceiptStreamOperation, RedemptionStreamOperation, StreamOperation},
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
        Some(StreamOperation::JupiterReceipt(operation)) => match *operation {
            JupiterReceiptStreamOperation::Preparing(_) => {
                (Some("JupiterReceipt".into()), Some("Preparing".into()))
            }
            JupiterReceiptStreamOperation::Active(operation) => (
                Some("JupiterReceipt".into()),
                Some(format!("{:?}", operation.phase())),
            ),
        },
        Some(StreamOperation::BackingInflow(operation)) => (
            Some("BackingInflow".into()),
            Some(operation.phase_name().into()),
        ),
        Some(StreamOperation::PoolTopUp(_)) => {
            (Some("BackingReconciliation".into()), Some("TopUp".into()))
        }
        None => (None, None),
    };
    let accumulated_eligible_credit = state
        .reward_entitlements
        .entries
        .iter()
        .try_fold(0u128, |sum, entry| {
            sum.checked_add(entry.accumulated_eligible_credit)
        })
        .expect("validated entitlement accumulator total");
    Status {
        lifecycle: state.lifecycle,
        operation_kind,
        operation_phase,
        next_nns_receipt_sequence: state.next_nns_receipt_sequence,
        latest_entitlement_batch_generation: state.latest_entitlement_batch_generation,
        latest_processed_reward_event: state.reward_entitlements.last_processed_event,
        latest_reward_event_classification: state
            .reward_entitlements
            .latest_observation
            .as_ref()
            .map(|observation| observation.classification),
        accumulated_entitlements: state.reward_entitlements.entries,
        accumulated_eligible_credit,
        accumulated_policy_credit: state.reward_entitlements.accumulated_policy_credit,
        processed_reward_event_count: state.reward_entitlements.processed_event_count,
        missed_reward_event_count: state.reward_entitlements.missed_event_count,
        reward_work_due: state.reward_entitlements.reward_work_due,
        reward_processing_paused: state.reward_entitlements.reward_processing_paused,
        governance_parameters_fresh: state.reward_entitlements.governance_parameters_fresh,
        pending_entitlement_batch_eligible_credit: state
            .pending_entitlement_batch
            .as_ref()
            .map(|batch| batch.eligible_credit_total),
        pending_entitlement_batch_policy_credit: state
            .pending_entitlement_batch
            .as_ref()
            .map(|batch| batch.policy_credit_total),
        latest_reconciliation_checkpoint: state.latest_reconciliation_checkpoint,
    }
}
