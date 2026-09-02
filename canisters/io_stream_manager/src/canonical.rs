use candid::{CandidType, Nat, Reserved};
use ic_cdk::call::Call;
use serde::Deserialize;
use sha2::Digest;

use crate::{
    redemption::ClaimSnapshot,
    state::{Account, StreamConfig},
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

const NNS_SNAPSHOT_DRIFT: &str = "NNS claim-backing observation drifted across the canonical reads";

pub async fn claim_snapshot(config: &StreamConfig) -> Result<ClaimSnapshot, String> {
    match claim_snapshot_once(config).await {
        Err(error) if error == NNS_SNAPSHOT_DRIFT => claim_snapshot_once(config).await,
        result => result,
    }
}

async fn claim_snapshot_once(config: &StreamConfig) -> Result<ClaimSnapshot, String> {
    let nns_before = claim_asset_observation(config.nns_manager).await?;
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
    let stream_snapshot = crate::state::read();
    let nns_after = claim_asset_observation(config.nns_manager).await?;
    if nns_after != nns_before {
        return Err(NNS_SNAPSHOT_DRIFT.into());
    }
    let excluded = excluded_io_balances
        .iter()
        .try_fold(0u128, |total, (_, balance)| total.checked_add(*balance))
        .ok_or("nonredeemable balance overflow")?;
    let claim_supply_e8s = io_core_model::claim_supply(total_supply, reserve, &[excluded])
        .map_err(|error| format!("claim supply failed: {error:?}"))?;
    let stream_transit = stream_transit_backing(&stream_snapshot, &nns_before)?;
    let transit_backing_e8s = nns_before
        .transit_backing_e8s
        .checked_add(stream_transit)
        .ok_or("combined transit backing overflow")?;
    validate_transit_fee_bases(&nns_before, icp_fee)?;
    if crate::state::read() != stream_snapshot {
        return Err("Stream state drifted across the canonical reads".into());
    }
    let total_claim_backing_e8s = io_core_model::claim_backing(io_core_model::Backing {
        liquid,
        pooled: nns_before.claim_bearing_dynamic_principal_e8s,
        unwinding: nns_before.live_child_net_backing_e8s,
        transit: transit_backing_e8s,
    })
    .map_err(|error| format!("claim backing failed: {error:?}"))?;
    let observation_bytes = candid::encode_one((
        total_supply,
        reserve,
        &excluded_io_balances,
        claim_supply_e8s,
        liquid,
        total_claim_backing_e8s,
        &nns_before.fingerprint,
        stream_snapshot.control_epoch,
    ))
    .map_err(|error| format!("canonical snapshot encoding failed: {error}"))?;
    Ok(ClaimSnapshot {
        total_supply_e8s: total_supply,
        reserve_io_e8s: reserve,
        excluded_io_balances,
        claim_supply_e8s,
        liquid_icp_e8s: liquid,
        pooled_principal_e8s: nns_before.claim_bearing_dynamic_principal_e8s,
        unwinding_net_backing_e8s: nns_before.live_child_net_backing_e8s,
        transit_backing_e8s,
        total_claim_backing_e8s,
        nns_control_epoch: nns_before.control_epoch,
        nns_operation_sequence: nns_before.active_operation_sequence,
        last_completed_pool_operation_sequence: nns_before.last_completed_pool_operation_sequence,
        nns_fingerprint: nns_before.fingerprint,
        pool_staking_account: nns_before.pool_staking_account,
        anchor_target_e8s: nns_before.anchor_target_e8s,
        anchor_available_e8s: nns_before.anchor_available_e8s,
        excluded_dynamic_surplus_e8s: nns_before.excluded_dynamic_surplus_e8s,
        stream_control_epoch: stream_snapshot.control_epoch,
        observation_fingerprint: sha2::Sha256::digest(observation_bytes).to_vec(),
        io_fee_e8s: io_fee,
        icp_fee_e8s: icp_fee,
    })
}

fn validate_transit_fee_bases(
    observation: &io_nns_types::backing::ClaimAssetObservation,
    canonical_fee_e8s: u128,
) -> Result<(), String> {
    let cohort_drift = observation.live_cohorts.iter().any(|cohort| {
        cohort.physical_principal_e8s > 0 && cohort.committed_fee_e8s != canonical_fee_e8s
    });
    if cohort_drift
        || observation
            .transit_components
            .iter()
            .filter_map(|component| component.fee_basis_e8s)
            .any(|basis| basis != canonical_fee_e8s)
    {
        return Err(format!(
            "committed transit fee basis differs from current canonical ICP fee {canonical_fee_e8s}"
        ));
    }
    Ok(())
}

fn stream_transit_backing(
    stream: &crate::state::StreamStateV1,
    nns: &io_nns_types::backing::ClaimAssetObservation,
) -> Result<u128, String> {
    use crate::{state::StreamOperation, transfer::TransferState};
    match &stream.active_operation {
        Some(StreamOperation::PoolTopUp(operation)) => match operation.transfer.state {
            TransferState::Submitted { .. } => {
                Err("pool top-up transfer has an ambiguous submitted effect".into())
            }
            TransferState::Succeeded { .. } => {
                let before = operation.permit.expected_parent_principal_e8s;
                let observed = nns.claim_bearing_dynamic_principal_e8s;
                let remaining = io_nns_types::backing::remaining_parent_transit(
                    before,
                    operation.permit.claim_credit_e8s,
                    observed,
                )
                .map_err(|error| format!("pool top-up transit failed: {error:?}"))?;
                let nns_owns_transit = operation.nns_transfer_proved
                    || nns.last_completed_pool_operation_sequence
                        == Some(operation.permit.operation_sequence)
                    || (nns.active_operation_sequence == operation.permit.operation_sequence
                        && nns.transit_backing_e8s == remaining);
                Ok(if nns_owns_transit { 0 } else { remaining })
            }
            _ => Ok(0),
        },
        Some(StreamOperation::ClaimReceipt(operation)) => {
            claim_receipt_ownership(operation.liquid_block).map(|()| 0)
        }
        _ => Ok(0),
    }
}

fn claim_receipt_ownership(liquid_block: Option<u128>) -> Result<(), String> {
    liquid_block
        .map(|_| ())
        .ok_or_else(|| "claim receipt ownership awaits exact liquid-block proof".into())
}

pub(crate) async fn claim_asset_observation(
    nns_manager: candid::Principal,
) -> Result<io_nns_types::backing::ClaimAssetObservation, String> {
    let result: Result<io_nns_types::backing::ClaimAssetObservation, Reserved> =
        Call::bounded_wait(nns_manager, "observe_claim_assets")
            .with_arg(())
            .await
            .map_err(|error| format!("NNS observation call failed: {error:?}"))?
            .candid()
            .map_err(|error| format!("NNS observation decode failed: {error:?}"))?;
    let observation = result.map_err(|_| "NNS observation rejected".to_string())?;
    observation.validate()?;
    Ok(observation)
}

pub(crate) async fn pool_policy_observation(
    nns_manager: candid::Principal,
) -> Result<io_nns_types::backing::PoolPolicyObservation, String> {
    let result: Result<io_nns_types::backing::PoolPolicyObservation, Reserved> =
        Call::bounded_wait(nns_manager, "observe_pool_policy")
            .with_arg(())
            .await
            .map_err(|error| format!("NNS policy observation call failed: {error:?}"))?
            .candid()
            .map_err(|error| format!("NNS policy observation decode failed: {error:?}"))?;
    let observation = result.map_err(|_| "NNS policy observation rejected".to_string())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> io_nns_types::backing::ClaimAssetObservation {
        io_nns_types::backing::ClaimAssetObservation {
            parent: Some(io_nns_types::backing::ParentAssetObservation {
                neuron_id: 1,
                staking_account: Account {
                    owner: candid::Principal::from_slice(&[1; 29]),
                    subaccount: Some(vec![1; 32]),
                },
                physical_principal_e8s: io_nns_types::backing::DYNAMIC_ANCHOR_TARGET_E8S,
            }),
            pool_staking_account: Account {
                owner: candid::Principal::from_slice(&[1; 29]),
                subaccount: Some(vec![1; 32]),
            },
            claim_bearing_dynamic_principal_e8s: 0,
            anchor_target_e8s: io_nns_types::backing::DYNAMIC_ANCHOR_TARGET_E8S,
            anchor_available_e8s: io_nns_types::backing::DYNAMIC_ANCHOR_TARGET_E8S,
            excluded_dynamic_surplus_e8s: 0,
            live_cohorts: vec![io_nns_types::backing::CohortObservation {
                generation: 1,
                child_neuron_id: 2,
                physical_principal_e8s: 100,
                net_backing_e8s: 90,
                committed_fee_e8s: 10,
                ready_at_seconds: 3,
                proof: io_nns_types::backing::CohortProofState::Dissolving,
            }],
            live_child_physical_principal_e8s: 100,
            live_child_net_backing_e8s: 90,
            live_child_committed_fee_liability_e8s: 10,
            transit_backing_e8s: 180,
            transit_components: vec![
                io_nns_types::backing::TransitComponentObservation {
                    kind: io_nns_types::backing::TransitComponentKind::ActiveJupiter,
                    backing_e8s: 60,
                    fee_basis_e8s: Some(10),
                },
                io_nns_types::backing::TransitComponentObservation {
                    kind: io_nns_types::backing::TransitComponentKind::PendingTwoYearMaturity,
                    backing_e8s: 120,
                    fee_basis_e8s: Some(10),
                },
            ],
            active_operation_sequence: 0,
            last_completed_pool_operation_sequence: None,
            control_epoch: 1,
            fingerprint: vec![1; 32],
            oldest_ready_at_seconds: Some(3),
        }
    }

    #[test]
    fn every_coexisting_transit_fee_basis_must_match_the_live_fee() {
        let mut observation = observation();
        assert_eq!(validate_transit_fee_bases(&observation, 10), Ok(()));
        assert!(validate_transit_fee_bases(&observation, 11)
            .unwrap_err()
            .contains("current canonical ICP fee 11"));
        observation.transit_components[1].fee_basis_e8s = Some(11);
        assert!(validate_transit_fee_bases(&observation, 10).is_err());
        observation.transit_components[1].fee_basis_e8s = Some(10);
        observation
            .live_cohorts
            .push(io_nns_types::backing::CohortObservation {
                generation: 2,
                child_neuron_id: 3,
                physical_principal_e8s: 100,
                net_backing_e8s: 89,
                committed_fee_e8s: 11,
                ready_at_seconds: 4,
                proof: io_nns_types::backing::CohortProofState::Dissolving,
            });
        assert!(validate_transit_fee_bases(&observation, 10).is_err());
    }

    #[test]
    fn claim_receipt_handoff_requires_exact_liquid_block_proof() {
        assert!(claim_receipt_ownership(None)
            .unwrap_err()
            .contains("exact liquid-block proof"));
        assert_eq!(claim_receipt_ownership(Some(7)), Ok(()));
    }
}
