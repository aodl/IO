use crate::state::{self, Lifecycle};

pub async fn readiness_preflight(
    canister_self: candid::Principal,
    captured_control_epoch: u64,
) -> Result<(), crate::api::ApiError> {
    let snapshot = state::read();
    if snapshot.active_operation.is_some() {
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
    let canonical = crate::canonical::redemption_snapshot(&snapshot.config)
        .await
        .map_err(crate::api::ApiError::Ledger)?;
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
    if latest.active_operation.is_some()
        || latest.lifecycle != Lifecycle::Paused
        || latest.control_epoch != captured_control_epoch
    {
        return Err(crate::api::ApiError::Busy);
    }
    latest.lifecycle = Lifecycle::Ready;
    latest.reward_entitlements.reward_processing_paused = false;
    latest.reward_entitlements.reward_work_due = true;
    latest.reward_entitlements.governance_parameters_fresh = true;
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
    ) || matches!(
        &state.active_operation,
        Some(crate::state::StreamOperation::LiquidReceipt(operation))
            if matches!(operation.as_ref(), crate::state::LiquidReceiptStreamOperation::Preparing(_))
    ) {
        state.active_operation = None;
    }
    state::write(state);
}
