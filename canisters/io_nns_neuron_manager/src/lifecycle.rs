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
        return Err("cannot unpause with active NNS operation".into());
    }
    state.lifecycle = if paused {
        Lifecycle::Paused
    } else {
        Lifecycle::Ready
    };
    state::write(state);
    Ok(())
}
