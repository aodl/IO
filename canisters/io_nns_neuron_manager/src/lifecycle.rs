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
    let fee: candid::Nat =
        ic_cdk::call::Call::bounded_wait(snapshot.config.icp_ledger, "icrc1_fee")
            .with_arg(())
            .await
            .map_err(|error| {
                crate::api::ApiError::Stuck(format!("ICP fee query failed: {error:?}"))
            })?
            .candid()
            .map_err(|error| {
                crate::api::ApiError::Invalid(format!("ICP fee decode failed: {error:?}"))
            })?;
    let fee: u128 = fee
        .0
        .try_into()
        .map_err(|_| crate::api::ApiError::Invalid("ICP fee does not fit u128".into()))?;
    if fee != snapshot.config.expected_icp_fee_e8s {
        return Err(crate::api::ApiError::Invalid(
            "canonical ICP fee differs from approved config".into(),
        ));
    }
    for (name, account, required) in [
        (
            "Jupiter",
            snapshot.config.jupiter_staging.clone(),
            snapshot.config.jupiter_fee_float_e8s,
        ),
        (
            "two-week maturity",
            snapshot.config.two_week_maturity_staging.clone(),
            snapshot.config.two_week_fee_float_e8s,
        ),
    ] {
        let balance: candid::Nat =
            ic_cdk::call::Call::bounded_wait(snapshot.config.icp_ledger, "icrc1_balance_of")
                .with_arg(account)
                .await
                .map_err(|error| {
                    crate::api::ApiError::Stuck(format!(
                        "{name} staging balance query failed: {error:?}"
                    ))
                })?
                .candid()
                .map_err(|error| {
                    crate::api::ApiError::Invalid(format!(
                        "{name} staging balance decode failed: {error:?}"
                    ))
                })?;
        let balance: u128 = balance
            .0
            .try_into()
            .map_err(|_| crate::api::ApiError::Invalid("ICP balance does not fit u128".into()))?;
        if balance < required {
            return Err(crate::api::ApiError::Invalid(format!(
                "{name} staging fee float is below its configured minimum"
            )));
        }
    }
    let latest = state::read();
    if latest.active_operation.is_some()
        || latest.lifecycle != Lifecycle::Paused
        || latest.control_epoch != captured_control_epoch
    {
        return Err(crate::api::ApiError::Busy);
    }
    Err(crate::api::ApiError::ImplementationIncomplete(
        "executable NNS Jupiter/maturity operations are not complete".into(),
    ))
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
    state::write(state);
}
