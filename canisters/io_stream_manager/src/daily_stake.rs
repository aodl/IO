use crate::{
    api::ApiError,
    canonical,
    redemption::{ClaimSnapshot, StructuralStakeObservation},
    state::{Account, StreamConfig, StructuralStakeState},
};

pub struct DailyStakeObservation {
    pub claim: ClaimSnapshot,
    pub reward_event: io_sns_reward_boundary::RewardEvent,
    pub neurons: Vec<io_sns_reward_boundary::Neuron>,
    pub stakes: Vec<StructuralStakeObservation>,
    pub active_backing_io_e8s: u128,
    pub assets: io_nns_types::backing::ClaimAssetObservation,
}

pub async fn observe(config: &StreamConfig) -> Result<DailyStakeObservation, ApiError> {
    let installed =
        crate::reward_evidence::installed_governance(config.sns_root, config.sns_governance)
            .await?;
    crate::lifecycle::validate_installed_governance(config, &installed)?;
    let claim_before = canonical::claim_snapshot(config)
        .await
        .map_err(ApiError::Ledger)?;
    let event_before = crate::reward_evidence::latest_reward_event(config.sns_governance).await?;
    let neurons = crate::reward_evidence::list_all_neurons(config.sns_governance).await?;
    let mut ids = std::collections::BTreeSet::new();
    let mut accounts = std::collections::BTreeSet::new();
    let mut stakes = Vec::new();
    let mut active_backing = 0u128;
    for neuron in &neurons {
        if neuron.id.len() != 32 || !ids.insert(neuron.id.clone()) {
            return Err(ApiError::Invalid(
                "SNS neuron ID is malformed or duplicated".into(),
            ));
        }
        let account = Account {
            owner: config.sns_governance,
            subaccount: Some(neuron.id.clone()),
        };
        let canonical_account = account.canonical().map_err(ApiError::Invalid)?;
        if !accounts.insert(canonical_account) {
            return Err(ApiError::Invalid(
                "SNS staking Account is duplicated".into(),
            ));
        }
        if config
            .nonredeemable_governance_io_accounts
            .iter()
            .try_fold(false, |matched, excluded| {
                account.effective_eq(excluded).map(|same| matched || same)
            })
            .map_err(ApiError::Invalid)?
        {
            continue;
        }
        let structural = match neuron.dissolve_state {
            io_sns_reward_boundary::DissolveState::NotDissolving {
                dissolve_delay_seconds,
            } if dissolve_delay_seconds == io_core_model::TWO_WEEK_SECONDS => {
                StructuralStakeState::Active
            }
            io_sns_reward_boundary::DissolveState::NotDissolving { .. } => {
                StructuralStakeState::IneligibleActive
            }
            io_sns_reward_boundary::DissolveState::Dissolving => StructuralStakeState::Dissolving,
            io_sns_reward_boundary::DissolveState::Dissolved => {
                StructuralStakeState::LiquidOrDissolved
            }
        };
        let balance = if structural == StructuralStakeState::Active {
            canonical::balance(config.io_ledger, account.clone())
                .await
                .map_err(ApiError::Ledger)?
        } else {
            0
        };
        if structural == StructuralStakeState::Active {
            active_backing = active_backing
                .checked_add(balance)
                .ok_or_else(|| ApiError::Invalid("active backing stake overflow".into()))?;
        }
        stakes.push(StructuralStakeObservation {
            sns_neuron_id: neuron.id.clone(),
            staking_account: account,
            state: structural,
            ledger_balance_e8s: balance,
        });
    }
    let event_after = crate::reward_evidence::latest_reward_event(config.sns_governance).await?;
    crate::reward_evidence::require_consistent_event(&event_before, &event_after)?;
    let claim_after = canonical::claim_snapshot(config)
        .await
        .map_err(ApiError::Ledger)?;
    if claim_after != claim_before {
        return Err(ApiError::Pending(
            "claim snapshot drifted during daily stake observation".into(),
        ));
    }
    let assets = canonical::claim_asset_observation(config.nns_manager)
        .await
        .map_err(ApiError::Ledger)?;
    if assets.fingerprint != claim_before.nns_fingerprint
        || assets.control_epoch != claim_before.nns_control_epoch
        || assets.active_operation_sequence != claim_before.nns_operation_sequence
    {
        return Err(ApiError::Pending(
            "NNS observation drifted during daily stake observation".into(),
        ));
    }
    let policy = canonical::pool_policy_observation(config.nns_manager)
        .await
        .map_err(|error| ApiError::Invalid(format!("pool policy is not ready: {error}")))?;
    if policy.control_epoch != assets.control_epoch
        || policy.active_operation_sequence != assets.active_operation_sequence
    {
        return Err(ApiError::Pending(
            "NNS asset and policy observations drifted".into(),
        ));
    }
    stakes.sort_by(|left, right| left.sns_neuron_id.cmp(&right.sns_neuron_id));
    Ok(DailyStakeObservation {
        claim: claim_before,
        reward_event: event_before,
        neurons,
        stakes,
        active_backing_io_e8s: active_backing,
        assets,
    })
}
