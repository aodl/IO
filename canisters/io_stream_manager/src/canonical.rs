use candid::{CandidType, Nat};
use ic_cdk::call::Call;
use serde::Deserialize;

use crate::{
    redemption::CanonicalRedemptionSnapshot,
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

pub async fn redemption_snapshot(
    config: &StreamConfig,
) -> Result<CanonicalRedemptionSnapshot, String> {
    let io_fee = nat_call(config.io_ledger, "icrc1_fee", ()).await?;
    let icp_fee = nat_call(config.icp_ledger, "icrc1_fee", ()).await?;
    let total_supply = nat_call(config.io_ledger, "icrc1_total_supply", ()).await?;
    let reserve = nat_call(
        config.io_ledger,
        "icrc1_balance_of",
        config.io_reserve.clone(),
    )
    .await?;
    let mut excluded_io_balances = Vec::with_capacity(config.excluded_io_accounts.len());
    for account in &config.excluded_io_accounts {
        let balance = nat_call(config.io_ledger, "icrc1_balance_of", account.clone()).await?;
        excluded_io_balances.push((account.clone(), balance));
    }
    let liquid = nat_call(
        config.icp_ledger,
        "icrc1_balance_of",
        config.liquid_icp.clone(),
    )
    .await?;
    Ok(CanonicalRedemptionSnapshot {
        total_supply_e8s: total_supply,
        reserve_io_e8s: reserve,
        excluded_io_balances,
        liquid_icp_e8s: liquid,
        io_fee_e8s: io_fee,
        icp_fee_e8s: icp_fee,
    })
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
