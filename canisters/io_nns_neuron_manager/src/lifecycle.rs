use crate::state::{self, Lifecycle};

fn validate_prelaunch_baseline(
    seeded_principal_e8s: u128,
    observed_principal_e8s: u128,
    ordinary_maturity_e8s: u64,
    has_pending_maturity: bool,
) -> Result<(), crate::api::ApiError> {
    if observed_principal_e8s != seeded_principal_e8s {
        return Err(crate::api::ApiError::Invalid(format!(
            "protected two-week principal {observed_principal_e8s} does not match seeded principal {seeded_principal_e8s}"
        )));
    }
    if ordinary_maturity_e8s != 0 {
        return Err(crate::api::ApiError::Pending(format!(
            "BaselineUnreconciled: protected two-week neuron has {ordinary_maturity_e8s} e8s of prelaunch ordinary maturity"
        )));
    }
    if has_pending_maturity {
        return Err(crate::api::ApiError::Pending(
            "BaselineUnreconciled: protected two-week neuron has a pending maturity disbursement"
                .into(),
        ));
    }
    Ok(())
}

pub async fn readiness_preflight(
    canister_self: candid::Principal,
    captured_control_epoch: u64,
) -> Result<(), crate::api::ApiError> {
    let snapshot = state::read();
    if snapshot.active_operation.is_some()
        || snapshot.pending_two_year_maturity.is_some()
        || snapshot.pending_two_week_maturity.is_some()
    {
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
    let baseline_proved = !snapshot.two_week_maturity_baseline_reconciled;
    if baseline_proved {
        let observation = crate::execution::query_neuron_observation(
            &snapshot.config,
            snapshot.config.two_week_neuron_id,
        )
        .await?;
        validate_prelaunch_baseline(
            snapshot.config.seeded_two_week_principal_e8s,
            observation.snapshot.cached_stake_e8s,
            observation.maturity_e8s,
            !observation.maturity_disbursements.is_empty(),
        )?;
    }
    let latest = state::read();
    if latest.active_operation.is_some()
        || latest.pending_two_year_maturity.is_some()
        || latest.pending_two_week_maturity.is_some()
        || latest.lifecycle != Lifecycle::Paused
        || latest.control_epoch != captured_control_epoch
        || latest.config != snapshot.config
    {
        return Err(crate::api::ApiError::Busy);
    }
    let mut latest = latest;
    latest.two_week_maturity_baseline_reconciled |= baseline_proved;
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
    state::write(state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_maturity_exact_seed_is_the_only_new_baseline() {
        assert_eq!(validate_prelaunch_baseline(100, 100, 0, false), Ok(()));
        assert!(matches!(
            validate_prelaunch_baseline(100, 101, 0, false),
            Err(crate::api::ApiError::Invalid(_))
        ));
        assert!(matches!(
            validate_prelaunch_baseline(100, 100, 1, false),
            Err(crate::api::ApiError::Pending(message)) if message.contains("BaselineUnreconciled")
        ));
        assert!(matches!(
            validate_prelaunch_baseline(100, 100, 0, true),
            Err(crate::api::ApiError::Pending(message)) if message.contains("BaselineUnreconciled")
        ));
    }

    #[test]
    fn baseline_gap_cannot_distinguish_staked_auto_stake_or_dissolve_state() {
        assert_eq!(validate_prelaunch_baseline(100, 100, 0, false), Ok(()));
        let source = include_str!("lifecycle.rs");
        let signature = &source[source.find("fn validate_prelaunch_baseline").unwrap()
            ..source
                .find(") -> Result<(), crate::api::ApiError>")
                .unwrap()];
        assert!(!signature.contains("staked_maturity"));
        assert!(!signature.contains("auto_stake"));
        assert!(!signature.contains("dissolve_state"));
    }
}
