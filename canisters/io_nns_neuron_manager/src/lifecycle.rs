use crate::state::{self, Lifecycle};

fn validate_prelaunch_baseline(
    role: &str,
    seeded_principal_e8s: u128,
    observation: &crate::execution::NeuronObservation,
) -> Result<(), crate::api::ApiError> {
    crate::execution::validate_maturity_configuration(observation)
        .map_err(crate::api::ApiError::Invalid)?;
    let observed_principal_e8s = observation.snapshot.cached_stake_e8s;
    if observed_principal_e8s != seeded_principal_e8s {
        return Err(crate::api::ApiError::Invalid(format!(
            "{role} principal {observed_principal_e8s} does not match seeded principal {seeded_principal_e8s}"
        )));
    }
    if observation.maturity_e8s != 0 || observation.staked_maturity_e8s != 0 {
        return Err(crate::api::ApiError::Pending(format!(
            "BaselineUnreconciled: {role} has ordinary/staked maturity {}/{} e8s",
            observation.maturity_e8s, observation.staked_maturity_e8s
        )));
    }
    if !observation.maturity_disbursements.is_empty() {
        return Err(crate::api::ApiError::Pending(
            "BaselineUnreconciled: {role} has a pending maturity disbursement".into(),
        ));
    }
    Ok(())
}

pub async fn readiness_preflight(
    canister_self: candid::Principal,
    captured_control_epoch: u64,
) -> Result<(), crate::api::ApiError> {
    let snapshot = state::read();
    let two_year_baseline_needed = !snapshot.two_year_maturity_baseline_reconciled;
    let two_week_baseline_needed = !snapshot.two_week_maturity_baseline_reconciled;
    if snapshot.active_operation.is_some()
        || snapshot.pending_two_year_maturity.is_some()
        || snapshot.pending_two_week_maturity.is_some()
        || (two_week_baseline_needed && snapshot.pending_unwind.is_some())
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
    if two_year_baseline_needed {
        let observation = crate::execution::query_neuron_observation(
            &snapshot.config,
            snapshot.config.two_year_neuron_id,
        )
        .await?;
        validate_prelaunch_baseline(
            "two-year protected NNS neuron",
            snapshot.config.seeded_two_year_principal_e8s,
            &observation,
        )?;
    }
    if two_week_baseline_needed {
        let observation = crate::execution::query_neuron_observation(
            &snapshot.config,
            snapshot.config.two_week_neuron_id,
        )
        .await?;
        validate_prelaunch_baseline(
            "two-week reward-backing NNS neuron",
            snapshot.config.seeded_two_week_principal_e8s,
            &observation,
        )?;
    }
    let latest = state::read();
    if latest.active_operation.is_some()
        || latest.pending_two_year_maturity.is_some()
        || latest.pending_two_week_maturity.is_some()
        || (two_week_baseline_needed && latest.pending_unwind.is_some())
        || latest.lifecycle != Lifecycle::Paused
        || latest.control_epoch != captured_control_epoch
        || latest.config != snapshot.config
    {
        return Err(crate::api::ApiError::Busy);
    }
    let mut latest = latest;
    latest.two_year_maturity_baseline_reconciled |= two_year_baseline_needed;
    latest.two_week_maturity_baseline_reconciled |= two_week_baseline_needed;
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

    fn observation() -> crate::execution::NeuronObservation {
        crate::execution::NeuronObservation {
            snapshot: crate::jupiter::NeuronSnapshot {
                neuron_id: 2,
                staking_subaccount: [2; 32],
                cached_stake_e8s: 100,
            },
            maturity_e8s: 0,
            staked_maturity_e8s: 0,
            auto_stake_maturity: false,
            maturity_disbursements: vec![],
            dissolve_state: Some(crate::execution::DissolveState::DissolveDelaySeconds(
                crate::execution::APPROVED_REWARD_BACKING_DISSOLVE_DELAY_SECONDS,
            )),
        }
    }

    #[test]
    fn complete_exact_baseline_is_required() {
        let valid = observation();
        assert_eq!(validate_prelaunch_baseline("fixture", 100, &valid), Ok(()));
        let mut wrong_principal = valid.clone();
        wrong_principal.snapshot.cached_stake_e8s = 101;
        assert!(matches!(
            validate_prelaunch_baseline("fixture", 100, &wrong_principal),
            Err(crate::api::ApiError::Invalid(_))
        ));
        let mut ordinary = valid.clone();
        ordinary.maturity_e8s = 1;
        assert!(matches!(
            validate_prelaunch_baseline("fixture", 100, &ordinary),
            Err(crate::api::ApiError::Pending(message)) if message.contains("BaselineUnreconciled")
        ));
        let mut pending = valid;
        pending
            .maturity_disbursements
            .push(crate::execution::placeholder_maturity_disbursement());
        assert!(matches!(
            validate_prelaunch_baseline("fixture", 100, &pending),
            Err(crate::api::ApiError::Pending(message)) if message.contains("BaselineUnreconciled")
        ));
    }

    #[test]
    fn staked_auto_stake_dissolving_and_wrong_delay_are_rejected() {
        let mut staked = observation();
        staked.staked_maturity_e8s = 1;
        assert!(validate_prelaunch_baseline("fixture", 100, &staked).is_err());
        let mut auto = observation();
        auto.auto_stake_maturity = true;
        assert!(validate_prelaunch_baseline("fixture", 100, &auto).is_err());
        let mut dissolving = observation();
        dissolving.dissolve_state =
            Some(crate::execution::DissolveState::WhenDissolvedTimestampSeconds(u64::MAX));
        assert!(validate_prelaunch_baseline("fixture", 100, &dissolving).is_err());
        let mut wrong_delay = observation();
        wrong_delay.dissolve_state = Some(crate::execution::DissolveState::DissolveDelaySeconds(
            crate::execution::APPROVED_REWARD_BACKING_DISSOLVE_DELAY_SECONDS - 1,
        ));
        assert!(validate_prelaunch_baseline("fixture", 100, &wrong_delay).is_err());
    }
}
