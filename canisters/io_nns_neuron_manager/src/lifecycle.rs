use crate::state::{self, Lifecycle};
use io_nns_types::backing::{
    PoolCommand, PoolCommandKind, PoolCommandPhase, TopUpPermit, DYNAMIC_ANCHOR_TARGET_E8S,
};
use sha2::{Digest, Sha256};

fn validate_prelaunch_baseline(
    role: &str,
    seeded_principal_e8s: u128,
    observation: &crate::execution::NeuronObservation,
) -> Result<(), crate::api::ApiError> {
    crate::execution::validate_permanent_configuration(observation)
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

fn validate_dynamic_readiness_partition(
    claim_bearing_principal_e8s: u128,
    anchor_available_e8s: u128,
    physical_principal_e8s: u128,
    require_launch_anchor: bool,
) -> Result<(), crate::api::ApiError> {
    let accounted = claim_bearing_principal_e8s
        .checked_add(anchor_available_e8s)
        .ok_or_else(|| crate::api::ApiError::Invalid("Dynamic accounting overflow".into()))?;
    if anchor_available_e8s > DYNAMIC_ANCHOR_TARGET_E8S
        || (require_launch_anchor && anchor_available_e8s != DYNAMIC_ANCHOR_TARGET_E8S)
        || physical_principal_e8s < accounted
    {
        return Err(crate::api::ApiError::Invalid(
            "Dynamic launch anchor partition is inconsistent".into(),
        ));
    }
    Ok(())
}

pub async fn readiness_preflight(
    canister_self: candid::Principal,
    captured_control_epoch: u64,
) -> Result<(), crate::api::ApiError> {
    let mut snapshot = state::read();
    let two_year_baseline_needed = !snapshot.two_year_maturity_baseline_reconciled;
    let launch_bootstrap_active = matches!(
        snapshot.active_operation,
        Some(state::NnsOperation::Pool(ref operation))
            if operation.kind == PoolCommandKind::Bootstrap
    );
    if (snapshot.active_operation.is_some() && !launch_bootstrap_active)
        || snapshot.pending_two_year_maturity.is_some()
        || snapshot.pending_two_week_maturity.is_some()
        || !snapshot.live_cohorts.is_empty()
    {
        return Err(crate::api::ApiError::Busy);
    }
    snapshot
        .config
        .validate(canister_self)
        .map_err(crate::api::ApiError::Invalid)?;
    let fee = io_ledger_boundary::icp_fee(snapshot.config.icp_ledger)
        .await
        .map_err(crate::api::ApiError::Stuck)?;
    if fee != snapshot.config.expected_icp_fee_e8s {
        return Err(crate::api::ApiError::Invalid(
            "canonical ICP fee differs from approved config".into(),
        ));
    }
    let permanent = crate::api::observe_permanent_policy(&snapshot).await?;
    if two_year_baseline_needed {
        validate_prelaunch_baseline(
            "two-year protected NNS neuron",
            snapshot.config.audited_permanent_principal_e8s,
            &permanent,
        )?;
    }
    if snapshot.pooled_parent_id.is_none() {
        if !launch_bootstrap_active {
            begin_dynamic_bootstrap(&snapshot).await?;
        }
        let Some(state::NnsOperation::Pool(operation)) = state::read().active_operation else {
            return Err(crate::api::ApiError::Busy);
        };
        match crate::pool_flow::resume(operation).await? {
            io_nns_types::backing::PoolProgress::Completed { .. } => {}
            _ => {
                return Err(crate::api::ApiError::Pending(
                    "Dynamic parent launch bootstrap is durably accepted".into(),
                ))
            }
        }
        snapshot = state::read();
    }
    let parent_id = snapshot
        .pooled_parent_id
        .ok_or_else(|| crate::api::ApiError::Invalid("Dynamic parent is absent".into()))?;
    let observation =
        crate::execution::query_neuron_observation(&snapshot.config, parent_id).await?;
    crate::execution::validate_parent_configuration(
        &observation,
        io_nns_types::backing::FollowPolicy {
            followee_neuron_id: snapshot.config.pooled_parent_followee_id,
        },
    )
    .map_err(crate::api::ApiError::Invalid)?;
    let physical = observation.snapshot.cached_stake_e8s;
    validate_dynamic_readiness_partition(
        snapshot.claim_bearing_dynamic_principal_e8s,
        snapshot.anchor_available_e8s,
        physical,
        two_year_baseline_needed,
    )?;
    crate::api::best_effort_voting_power_maintenance(&snapshot).await?;
    let latest = state::read();
    if latest.active_operation.is_some()
        || latest.pending_two_year_maturity.is_some()
        || latest.pending_two_week_maturity.is_some()
        || !latest.live_cohorts.is_empty()
        || latest.lifecycle != Lifecycle::Paused
        || latest.control_epoch != captured_control_epoch
        || latest.config != snapshot.config
    {
        return Err(crate::api::ApiError::Busy);
    }
    let mut latest = latest;
    latest.two_year_maturity_baseline_reconciled |= two_year_baseline_needed;
    latest.lifecycle = Lifecycle::Ready;
    state::write(latest);
    Ok(())
}

async fn begin_dynamic_bootstrap(snapshot: &state::NnsStateV1) -> Result<(), crate::api::ApiError> {
    let staking_account = crate::execution::parent_staking_account(
        &snapshot.config,
        snapshot.config.pooled_parent_memo,
    );
    let observed_seed_e8s =
        crate::execution::icp_balance(&snapshot.config, &staking_account).await?;
    if state::read() != *snapshot {
        return Err(crate::api::ApiError::Busy);
    }
    if observed_seed_e8s < DYNAMIC_ANCHOR_TARGET_E8S {
        return Err(crate::api::ApiError::Pending(format!(
            "Dynamic anchor seed {observed_seed_e8s} is below required {DYNAMIC_ANCHOR_TARGET_E8S} e8s"
        )));
    }
    let fingerprint = Sha256::digest(
        candid::encode_one((&staking_account, observed_seed_e8s, snapshot.control_epoch)).map_err(
            |error| {
                crate::api::ApiError::Invalid(format!("Dynamic seed fingerprint failed: {error}"))
            },
        )?,
    )
    .to_vec();
    let mut latest = state::read();
    if latest != *snapshot || latest.active_operation.is_some() {
        return Err(crate::api::ApiError::Busy);
    }
    let operation_sequence = latest.next_operation_sequence;
    latest.next_operation_sequence = operation_sequence
        .checked_add(1)
        .ok_or_else(|| crate::api::ApiError::Invalid("operation sequence exhausted".into()))?;
    let operation = PoolCommand {
        kind: PoolCommandKind::Bootstrap,
        permit: TopUpPermit {
            generation: 1,
            operation_sequence,
            expected_parent_principal_e8s: 0,
            expected_parent_physical_e8s: observed_seed_e8s,
            destination: staking_account,
            expected_credit_e8s: 0,
            claim_credit_e8s: 0,
            fee_e8s: snapshot.config.expected_icp_fee_e8s,
            memo: b"io-dynamic-anchor-seed-v1".to_vec(),
            prepared_at_nanos: ic_cdk::api::time(),
            snapshot_fingerprint: fingerprint,
        },
        transfer_block_index: None,
        parent_neuron_id: None,
        phase: PoolCommandPhase::SeedObserved,
    };
    operation
        .validate(latest.next_operation_sequence)
        .map_err(crate::api::ApiError::Invalid)?;
    latest.active_operation = Some(state::NnsOperation::Pool(operation));
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
                crate::execution::APPROVED_PERMANENT_DISSOLVE_DELAY_SECONDS,
            )),
            followees: vec![],
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
    fn launch_requires_full_anchor_but_recovery_accepts_exact_depletion() {
        let target = DYNAMIC_ANCHOR_TARGET_E8S;
        assert_eq!(
            validate_dynamic_readiness_partition(0, target, target, true),
            Ok(())
        );
        assert!(validate_dynamic_readiness_partition(0, target - 1, target - 1, true).is_err());
        assert_eq!(
            validate_dynamic_readiness_partition(40, target - 10, target + 30, false),
            Ok(())
        );
        assert!(validate_dynamic_readiness_partition(40, target - 10, target + 29, false).is_err());
        assert!(validate_dynamic_readiness_partition(0, target + 1, target + 1, false).is_err());
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
            crate::execution::APPROVED_PERMANENT_DISSOLVE_DELAY_SECONDS - 1,
        ));
        assert!(validate_prelaunch_baseline("fixture", 100, &wrong_delay).is_err());
    }
}
