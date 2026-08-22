use candid::{CandidType, Nat, Reserved};
use ic_cdk::call::Call;
use serde::Deserialize;
use sha2::Digest;

use crate::{
    redemption::{CanonicalRedemptionSnapshot, StructuralStakeObservation},
    state::{Account, BackingRewardStatus, StreamConfig, StructuralStakeState},
    transfer::nat_to_u128,
};

async fn nat_call<A: candid::CandidType>(
    canister: candid::Principal,
    method: &str,
    arg: A,
) -> Result<u128, String> {
    let response = Call::bounded_wait(canister, method)
        .with_arg(arg)
        .await
        .map_err(|error| format!("{method} call failed: {error:?}"))?;
    let value: Nat = response
        .candid()
        .map_err(|error| format!("{method} response decode failed: {error:?}"))?;
    nat_to_u128(value)
}

pub async fn redemption_snapshot(
    config: &StreamConfig,
) -> Result<CanonicalRedemptionSnapshot, String> {
    let nns_before = nns_observation(config.nns_manager).await?;
    let io_fee = nat_call(config.io_ledger, "icrc1_fee", ()).await?;
    let icp_fee = nat_call(config.icp_ledger, "icrc1_fee", ()).await?;
    let total_supply = nat_call(config.io_ledger, "icrc1_total_supply", ()).await?;
    let reserve = nat_call(
        config.io_ledger,
        "icrc1_balance_of",
        config.io_reserve.clone(),
    )
    .await?;
    let mut excluded_io_balances =
        Vec::with_capacity(config.nonredeemable_governance_io_accounts.len());
    for account in &config.nonredeemable_governance_io_accounts {
        let balance = nat_call(config.io_ledger, "icrc1_balance_of", account.clone()).await?;
        excluded_io_balances.push((account.clone(), balance));
    }
    let liquid = nat_call(
        config.icp_ledger,
        "icrc1_balance_of",
        config.liquid_icp.clone(),
    )
    .await?;
    let registry = crate::state::read().backing_registry;
    let neurons = io_sns_reward_boundary::list_all_neurons(config.sns_governance)
        .await
        .map_err(|error| format!("SNS structural observation failed: {error:?}"))?;
    let mut structural_stakes = Vec::with_capacity(neurons.len());
    let mut seen = std::collections::BTreeSet::new();
    let mut active_backing_io_e8s = 0u128;
    let mut active_reward_io_e8s = 0u128;
    for neuron in neurons {
        if neuron.id.len() != 32 || !seen.insert(neuron.id.clone()) {
            return Err("SNS structural observation has a malformed or duplicate neuron ID".into());
        }
        let staking_account = Account {
            owner: config.sns_governance,
            subaccount: Some(neuron.id.clone()),
        };
        if config
            .nonredeemable_governance_io_accounts
            .iter()
            .any(|account| account.effective_eq(&staking_account).unwrap_or(false))
        {
            continue;
        }
        let state = match neuron.dissolve_state {
            io_sns_reward_boundary::DissolveState::NotDissolving {
                dissolve_delay_seconds,
            } if dissolve_delay_seconds == io_core_model::TWO_WEEK_SECONDS => {
                StructuralStakeState::Active
            }
            io_sns_reward_boundary::DissolveState::Dissolving => StructuralStakeState::Dissolving,
            _ => StructuralStakeState::LiquidOrDissolved,
        };
        let ledger_balance_e8s = nat_call(
            config.io_ledger,
            "icrc1_balance_of",
            staking_account.clone(),
        )
        .await?;
        let prior_record = registry
            .binary_search_by(|record| record.sns_neuron_id.cmp(&neuron.id))
            .ok()
            .map(|index| registry[index].clone());
        if state == StructuralStakeState::Active {
            active_backing_io_e8s = active_backing_io_e8s
                .checked_add(ledger_balance_e8s)
                .ok_or("active backing stake overflow")?;
            if prior_record.as_ref().is_some_and(|record| {
                matches!(record.status, BackingRewardStatus::ActiveEligible { .. })
            }) {
                active_reward_io_e8s = active_reward_io_e8s
                    .checked_add(ledger_balance_e8s)
                    .ok_or("active reward stake overflow")?;
            }
        }
        structural_stakes.push(StructuralStakeObservation {
            sns_neuron_id: neuron.id,
            staking_account,
            state,
            ledger_balance_e8s,
            prior_record,
        });
    }
    structural_stakes.sort_by(|left, right| left.sns_neuron_id.cmp(&right.sns_neuron_id));
    let nns_after = nns_observation(config.nns_manager).await?;
    if nns_after != nns_before {
        return Err("NNS claim-backing observation drifted across the canonical reads".into());
    }
    let claims = excluded_io_balances
        .iter()
        .try_fold(0u128, |total, (_, balance)| total.checked_add(*balance))
        .ok_or("nonredeemable balance overflow")?;
    let total_claim_backing_e8s = io_core_model::claim_backing(io_core_model::Backing {
        liquid,
        pooled: nns_before.pooled_principal_e8s,
        unwinding: nns_before.unwinding_principal_e8s,
        transit: nns_before.transit_backing_e8s,
    })
    .map_err(|error| format!("claim backing failed: {error:?}"))?;
    let observation_bytes = candid::encode_one((
        total_supply,
        reserve,
        claims,
        liquid,
        total_claim_backing_e8s,
        active_backing_io_e8s,
        active_reward_io_e8s,
        &nns_before.fingerprint,
        &structural_stakes,
    ))
    .map_err(|error| format!("canonical snapshot encoding failed: {error}"))?;
    Ok(CanonicalRedemptionSnapshot {
        total_supply_e8s: total_supply,
        reserve_io_e8s: reserve,
        excluded_io_balances,
        liquid_icp_e8s: liquid,
        pooled_principal_e8s: nns_before.pooled_principal_e8s,
        unwinding_principal_e8s: nns_before.unwinding_principal_e8s,
        transit_backing_e8s: nns_before.transit_backing_e8s,
        total_claim_backing_e8s,
        active_backing_io_e8s,
        active_reward_io_e8s,
        structural_stakes,
        nns_control_epoch: nns_before.control_epoch,
        nns_operation_sequence: nns_before.active_operation_sequence,
        active_unwind_generation: nns_before.active_unwind_generation,
        live_cohort_generations: nns_before
            .live_cohorts
            .iter()
            .map(|cohort| cohort.generation)
            .collect(),
        nns_fingerprint: nns_before.fingerprint,
        observation_fingerprint: sha2::Sha256::digest(observation_bytes).to_vec(),
        io_fee_e8s: io_fee,
        icp_fee_e8s: icp_fee,
    })
}

async fn nns_observation(
    nns_manager: candid::Principal,
) -> Result<io_nns_types::backing::ClaimBackingObservation, String> {
    let result: Result<io_nns_types::backing::ClaimBackingObservation, Reserved> =
        Call::bounded_wait(nns_manager, "observe_claim_backing")
            .with_arg(())
            .await
            .map_err(|error| format!("NNS observation call failed: {error:?}"))?
            .candid()
            .map_err(|error| format!("NNS observation decode failed: {error:?}"))?;
    let observation = result.map_err(|_| "NNS observation rejected".to_string())?;
    observation.validate()?;
    Ok(observation)
}

pub async fn balance(ledger: candid::Principal, account: Account) -> Result<u128, String> {
    nat_call(ledger, "icrc1_balance_of", account).await
}

pub async fn fee(ledger: candid::Principal) -> Result<u128, String> {
    nat_call(ledger, "icrc1_fee", ()).await
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct AllowanceArgs {
    account: Account,
    spender: Account,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Allowance {
    allowance: Nat,
    expires_at: Option<u64>,
}

pub async fn allowance(
    ledger: candid::Principal,
    account: Account,
    spender: Account,
) -> Result<(u128, Option<u64>), String> {
    let value: Allowance = Call::bounded_wait(ledger, "icrc2_allowance")
        .with_arg(AllowanceArgs { account, spender })
        .await
        .map_err(|error| format!("icrc2_allowance call failed: {error:?}"))?
        .candid()
        .map_err(|error| format!("icrc2_allowance decode failed: {error:?}"))?;
    Ok((nat_to_u128(value.allowance)?, value.expires_at))
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SupportedStandard {
    pub name: String,
    pub url: String,
}

pub async fn supported_standards(
    ledger: candid::Principal,
) -> Result<Vec<SupportedStandard>, String> {
    Call::bounded_wait(ledger, "icrc1_supported_standards")
        .with_arg(())
        .await
        .map_err(|error| format!("icrc1_supported_standards call failed: {error:?}"))?
        .candid()
        .map_err(|error| format!("icrc1_supported_standards decode failed: {error:?}"))
}

pub use io_ledger_boundary::{exact_icp_transfer, exact_icrc_transfer, icp_account_identifier};
