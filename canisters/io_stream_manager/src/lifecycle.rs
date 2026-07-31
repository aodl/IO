use crate::state::{self, Lifecycle};

pub async fn readiness_preflight(
    canister_self: candid::Principal,
) -> Result<(), crate::api::ApiError> {
    let snapshot = state::read();
    if snapshot.active_operation.is_some() {
        return Err(crate::api::ApiError::Busy);
    }
    snapshot
        .config
        .validate(canister_self)
        .map_err(crate::api::ApiError::Invalid)?;
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
    if latest.active_operation.is_some() || latest.lifecycle != Lifecycle::Paused {
        return Err(crate::api::ApiError::Busy);
    }
    latest.lifecycle = Lifecycle::Ready;
    state::write(latest);
    Ok(())
}

pub fn set_paused(paused: bool) -> Result<(), String> {
    let mut state = state::read();
    if !paused && state.active_operation.is_some() {
        return Err("cannot unpause with an active operation".into());
    }
    state.lifecycle = if paused {
        Lifecycle::Paused
    } else {
        Lifecycle::Ready
    };
    state::write(state);
    Ok(())
}
