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

pub async fn claim_snapshot(config: &StreamConfig) -> Result<ClaimSnapshot, String> {
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
    let stream_snapshot = crate::state::read();
    let nns_after = nns_observation(config.nns_manager).await?;
    if nns_after != nns_before {
        return Err("NNS claim-backing observation drifted across the canonical reads".into());
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
    if crate::state::read() != stream_snapshot {
        return Err("Stream state drifted across the canonical reads".into());
    }
    let total_claim_backing_e8s = io_core_model::claim_backing(io_core_model::Backing {
        liquid,
        pooled: nns_before.pooled_principal_e8s,
        unwinding: nns_before.unwinding_principal_e8s,
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
        pooled_principal_e8s: nns_before.pooled_principal_e8s,
        unwinding_principal_e8s: nns_before.unwinding_principal_e8s,
        transit_backing_e8s,
        total_claim_backing_e8s,
        nns_control_epoch: nns_before.control_epoch,
        nns_operation_sequence: nns_before.active_operation_sequence,
        last_completed_pool_operation_sequence: nns_before.last_completed_pool_operation_sequence,
        nns_fingerprint: nns_before.fingerprint,
        pool_staking_account: nns_before.pool_staking_account,
        minimum_parent_stake_e8s: nns_before.minimum_parent_stake_e8s,
        pooled_parent_exists: nns_before.parent.is_some(),
        stream_control_epoch: stream_snapshot.control_epoch,
        observation_fingerprint: sha2::Sha256::digest(observation_bytes).to_vec(),
        io_fee_e8s: io_fee,
        icp_fee_e8s: icp_fee,
    })
}

fn stream_transit_backing(
    stream: &crate::state::StreamStateV1,
    nns: &io_nns_types::backing::ClaimBackingObservation,
) -> Result<u128, String> {
    use crate::{state::StreamOperation, transfer::TransferState};
    match &stream.active_operation {
        Some(StreamOperation::PoolTopUp(operation)) => match operation.transfer.state {
            TransferState::Submitted { .. } => {
                Err("pool top-up transfer has an ambiguous submitted effect".into())
            }
            TransferState::Succeeded { .. } => {
                let before = operation.permit.expected_parent_principal_e8s;
                let observed = nns.pooled_principal_e8s;
                let remaining = io_nns_types::backing::remaining_parent_transit(
                    before,
                    operation.permit.expected_credit_e8s,
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
        _ => Ok(0),
    }
}

pub(crate) async fn nns_observation(
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
