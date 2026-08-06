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
    let root = snapshot.config.sns_root.ok_or_else(|| {
        crate::api::ApiError::Invalid("SNS Root readiness configuration is absent".into())
    })?;
    let expected_hash = snapshot
        .config
        .expected_sns_governance_module_hash
        .as_deref()
        .ok_or_else(|| {
            crate::api::ApiError::Invalid("expected SNS Governance module hash is absent".into())
        })?;
    let approved_duration = snapshot
        .config
        .approved_reward_event_duration_seconds
        .ok_or_else(|| {
            crate::api::ApiError::Invalid("approved reward-event duration is absent".into())
        })?;
    let approved_initial = snapshot
        .config
        .approved_initial_reward_rate_basis_points
        .ok_or_else(|| {
            crate::api::ApiError::Invalid("approved initial reward rate is absent".into())
        })?;
    let approved_final = snapshot
        .config
        .approved_final_reward_rate_basis_points
        .ok_or_else(|| {
            crate::api::ApiError::Invalid("approved final reward rate is absent".into())
        })?;
    let installed =
        io_sns_reward_boundary::installed_governance(root, snapshot.config.sns_governance)
            .await
            .map_err(|error| match error {
                io_sns_reward_boundary::Error::Retryable { method, message } => {
                    crate::api::ApiError::Pending(format!(
                        "SNS {method} readiness failed: {message}"
                    ))
                }
                other => crate::api::ApiError::Invalid(format!(
                    "SNS Governance readiness verification failed: {other:?}"
                )),
            })?;
    if installed.canister != snapshot.config.sns_governance
        || installed.module_hash.as_slice() != expected_hash
        || installed.initial_reward_rate_basis_points != approved_initial
        || installed.final_reward_rate_basis_points != approved_final
        || installed.initial_reward_rate_basis_points != 0
        || installed.final_reward_rate_basis_points != 0
        || installed.round_duration_seconds != approved_duration
    {
        return Err(crate::api::ApiError::Invalid(
            "installed SNS Governance hash or reward parameters differ from reviewed readiness configuration"
                .into(),
        ));
    }
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
    state::write(latest);
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
        Some(
            crate::state::StreamOperation::RedemptionPreparation(_)
                | crate::state::StreamOperation::ReceiptPreparation(_)
        )
    ) {
        state.active_operation = None;
    }
    state::write(state);
}
