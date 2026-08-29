use crate::{
    redemption::RedemptionPhase,
    state::{self, Lifecycle, RedemptionStreamOperation, StreamOperation},
};

fn is_readiness_resumable_operation(operation: &Option<StreamOperation>) -> bool {
    operation.is_none()
        || matches!(operation, Some(StreamOperation::Redemption(redemption))
            if matches!(redemption.as_ref(), RedemptionStreamOperation::Active(active)
                if active.phase == RedemptionPhase::PayoutSucceeded))
}

pub async fn readiness_preflight(
    canister_self: candid::Principal,
    captured_control_epoch: u64,
) -> Result<(), crate::api::ApiError> {
    let snapshot = state::read();
    if !is_readiness_resumable_operation(&snapshot.active_operation)
        || snapshot.prepared_exit_reconciliation.is_some()
    {
        return Err(crate::api::ApiError::Busy);
    }
    snapshot
        .config
        .validate(canister_self)
        .map_err(crate::api::ApiError::Invalid)?;
    let installed = io_sns_reward_boundary::installed_governance(
        snapshot.config.sns_root,
        snapshot.config.sns_governance,
    )
    .await
    .map_err(|error| match error {
        io_sns_reward_boundary::Error::Retryable { method, message } => {
            crate::api::ApiError::Pending(format!("SNS {method} readiness failed: {message}"))
        }
        other => crate::api::ApiError::Invalid(format!(
            "SNS Governance readiness verification failed: {other:?}"
        )),
    })?;
    validate_installed_governance(&snapshot.config, &installed)?;
    let activation_baseline = if snapshot.reward_checkpoint.last_processed_event.is_none() {
        let event =
            crate::reward_evidence::latest_reward_event(snapshot.config.sns_governance).await?;
        Some(crate::reward_evidence::event_id(&event)?)
    } else {
        None
    };
    let io_standards = crate::canonical::supported_standards(snapshot.config.io_ledger)
        .await
        .map_err(crate::api::ApiError::Ledger)?;
    for required in ["ICRC-1", "ICRC-2", "ICRC-3"] {
        if !io_standards
            .iter()
            .any(|standard| standard.name == required)
        {
            return Err(crate::api::ApiError::Invalid(format!(
                "IO ledger lacks {required}"
            )));
        }
    }
    let icp_standards = crate::canonical::supported_standards(snapshot.config.icp_ledger)
        .await
        .map_err(crate::api::ApiError::Ledger)?;
    if !icp_standards
        .iter()
        .any(|standard| standard.name == "ICRC-1")
    {
        return Err(crate::api::ApiError::Invalid(
            "ICP ledger lacks ICRC-1".into(),
        ));
    }
    let canonical = crate::canonical::claim_snapshot(&snapshot.config)
        .await
        .map_err(crate::api::ApiError::Ledger)?;
    let policy = crate::canonical::pool_policy_observation(snapshot.config.nns_manager)
        .await
        .map_err(|error| {
            crate::api::ApiError::Invalid(format!("pool policy is not ready: {error}"))
        })?;
    if policy.control_epoch != canonical.nns_control_epoch
        || policy.active_operation_sequence != canonical.nns_operation_sequence
    {
        return Err(crate::api::ApiError::Busy);
    }
    if canonical.io_fee_e8s != snapshot.config.expected_io_fee_e8s
        || canonical.icp_fee_e8s != snapshot.config.expected_icp_fee_e8s
    {
        return Err(crate::api::ApiError::Invalid(
            "canonical fee differs from approved config".into(),
        ));
    }
    let excluded = canonical
        .excluded_io_balances
        .iter()
        .try_fold(0u128, |sum, (_, value)| sum.checked_add(*value))
        .ok_or_else(|| crate::api::ApiError::Invalid("excluded balance overflow".into()))?;
    if canonical
        .reserve_io_e8s
        .checked_add(excluded)
        .is_none_or(|value| value > canonical.total_supply_e8s)
    {
        return Err(crate::api::ApiError::Invalid(
            "reserve plus exclusions exceed supply".into(),
        ));
    }
    let mut latest = state::read();
    if latest.active_operation != snapshot.active_operation
        || !is_readiness_resumable_operation(&latest.active_operation)
        || latest.prepared_exit_reconciliation.is_some()
        || latest.lifecycle != Lifecycle::Paused
        || latest.control_epoch != captured_control_epoch
        || latest.reward_checkpoint.last_processed_event
            != snapshot.reward_checkpoint.last_processed_event
    {
        return Err(crate::api::ApiError::Busy);
    }
    latest.lifecycle = Lifecycle::Ready;
    if let Some(event) = activation_baseline {
        latest.reward_checkpoint.last_processed_event = Some(event);
    }
    latest.reward_checkpoint.reward_processing_paused = false;
    latest.reward_checkpoint.reward_work_due = activation_baseline.is_none();
    latest.reward_checkpoint.governance_parameters_fresh = true;
    latest.stake_observation_due = true;
    latest
        .validate(canister_self)
        .map_err(crate::api::ApiError::Invalid)?;
    state::write(latest);
    Ok(())
}

pub(crate) fn validate_installed_governance(
    config: &crate::state::StreamConfig,
    installed: &io_sns_reward_boundary::InstalledGovernance,
) -> Result<(), crate::api::ApiError> {
    if installed.canister != config.sns_governance
        || installed.module_hash != config.expected_sns_governance_module_hash
        || installed.initial_reward_rate_basis_points != 0
        || installed.final_reward_rate_basis_points != 0
        || installed.round_duration_seconds != config.approved_reward_event_duration_seconds
        || installed.round_duration_seconds != 86_400
        || installed.max_number_of_neurons == 0
        || installed.max_number_of_neurons > io_sns_reward_boundary::MAX_NUMBER_OF_NEURONS
        || installed.max_dissolve_delay_bonus_percentage != 0
        || installed.max_age_bonus_percentage != 0
    {
        return Err(crate::api::ApiError::Invalid(
            "installed SNS Governance hash or reward parameters differ from reviewed readiness configuration"
                .into(),
        ));
    }
    Ok(())
}

pub fn begin_control_request() -> Result<u64, String> {
    let mut state = state::read();
    state.control_epoch = state
        .control_epoch
        .checked_add(1)
        .ok_or("control epoch overflow")?;
    let epoch = state.control_epoch;
    state::write(state);
    Ok(epoch)
}

pub fn set_paused() {
    let mut state = state::read();
    state.lifecycle = Lifecycle::Paused;
    if matches!(
        &state.active_operation,
        Some(crate::state::StreamOperation::Redemption(operation))
            if matches!(operation.as_ref(), crate::state::RedemptionStreamOperation::Preparing(_))
    ) {
        state.active_operation = None;
    }
    state::write(state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;

    fn principal(byte: u8) -> Principal {
        Principal::from_slice(&[byte; 29])
    }

    fn config() -> crate::state::StreamConfig {
        let account = |owner, byte| crate::state::Account {
            owner,
            subaccount: Some(vec![byte; 32]),
        };
        let governance = principal(5);
        crate::state::StreamConfig {
            io_ledger: principal(2),
            icp_ledger: principal(3),
            nns_manager: principal(4),
            jupiter_io_account: account(principal(7), 3),
            sns_governance: governance,
            sns_root: principal(6),
            expected_sns_governance_module_hash: vec![8; 32],
            approved_reward_event_duration_seconds: 86_400,
            io_reserve: account(principal(1), 4),
            liquid_icp: account(principal(1), 5),
            nonredeemable_governance_io_accounts: vec![account(governance, 9)],
            minimum_redemption_io_e8s: 20_000,
            expected_io_fee_e8s: 10_000,
            expected_icp_fee_e8s: 10_000,
            maximum_request_lifetime_nanos: 1_000_000,
            retry_delay_nanos: 1,
            ledger_deduplication_window_nanos: 2_000_000,
        }
    }

    fn installed(maximum: u64) -> io_sns_reward_boundary::InstalledGovernance {
        io_sns_reward_boundary::InstalledGovernance {
            canister: principal(5),
            module_hash: vec![8; 32],
            initial_reward_rate_basis_points: 0,
            final_reward_rate_basis_points: 0,
            round_duration_seconds: 86_400,
            max_number_of_neurons: maximum,
            max_dissolve_delay_bonus_percentage: 0,
            max_age_bonus_percentage: 0,
        }
    }

    #[test]
    fn reviewed_total_neuron_maximum_accepts_one_thousand_and_rejects_one_thousand_one() {
        assert_eq!(
            validate_installed_governance(&config(), &installed(1_000)),
            Ok(())
        );
        assert!(matches!(
            validate_installed_governance(&config(), &installed(1_001)),
            Err(crate::api::ApiError::Invalid(_))
        ));
    }
}
