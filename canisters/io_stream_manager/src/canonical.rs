use candid::{CandidType, Func, Nat};
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
    let mut excluded = 0u128;
    for account in &config.excluded_io_accounts {
        excluded = excluded
            .checked_add(nat_call(config.io_ledger, "icrc1_balance_of", account.clone()).await?)
            .ok_or("excluded balance sum overflow")?;
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
        excluded_io_e8s: excluded,
        liquid_icp_e8s: liquid,
        io_fee_e8s: io_fee,
        icp_fee_e8s: icp_fee,
    })
}

pub async fn balance(ledger: candid::Principal, account: Account) -> Result<u128, String> {
    nat_call(ledger, "icrc1_balance_of", account).await
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct GetTransactionsRequest {
    start: Nat,
    length: Nat,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct GetTransactionsResponse {
    log_length: Nat,
    transactions: Vec<Transaction>,
    first_index: Nat,
    archived_transactions: Vec<ArchivedTransactions>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ArchivedTransactions {
    start: Nat,
    length: Nat,
    callback: Func,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct TransactionRange {
    transactions: Vec<Transaction>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Transaction {
    burn: Option<candid::Reserved>,
    kind: String,
    mint: Option<candid::Reserved>,
    approve: Option<candid::Reserved>,
    fee_collector: Option<candid::Reserved>,
    authorized_mint: Option<candid::Reserved>,
    authorized_burn: Option<candid::Reserved>,
    timestamp: u64,
    transfer: Option<Transfer>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Transfer {
    to: Account,
    fee: Option<Nat>,
    from: Account,
    memo: Option<Vec<u8>>,
    created_at_time: Option<u64>,
    amount: Nat,
    spender: Option<Account>,
}

pub struct ExactTransfer {
    pub from: Account,
    pub to: Account,
    pub amount_e8s: u128,
    pub fee_e8s: Option<u128>,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: Option<u64>,
    pub spender: Option<Account>,
}

pub async fn exact_icrc_transfer(
    ledger: candid::Principal,
    block_index: u128,
) -> Result<ExactTransfer, String> {
    let request = GetTransactionsRequest {
        start: Nat::from(block_index),
        length: Nat::from(1u8),
    };
    let response: GetTransactionsResponse = Call::bounded_wait(ledger, "get_transactions")
        .with_arg(request.clone())
        .await
        .map_err(|error| format!("get_transactions call failed: {error:?}"))?
        .candid()
        .map_err(|error| format!("get_transactions decode failed: {error:?}"))?;
    let first = nat_to_u128(response.first_index.clone())?;
    let transaction = if first == block_index {
        response.transactions.into_iter().next()
    } else {
        None
    };
    let transaction = if let Some(transaction) = transaction {
        transaction
    } else {
        let archived = response
            .archived_transactions
            .into_iter()
            .find(|range| {
                let start = nat_to_u128(range.start.clone()).ok();
                let length = nat_to_u128(range.length.clone()).ok();
                start.zip(length).is_some_and(|(start, length)| {
                    block_index >= start && block_index < start.saturating_add(length)
                })
            })
            .ok_or("exact block is neither current nor in returned archive ranges")?;
        let range: TransactionRange =
            Call::bounded_wait(archived.callback.principal, &archived.callback.method)
                .with_arg(request)
                .await
                .map_err(|error| format!("archive callback failed: {error:?}"))?
                .candid()
                .map_err(|error| format!("archive response decode failed: {error:?}"))?;
        range
            .transactions
            .into_iter()
            .next()
            .ok_or("archive did not return exact transaction")?
    };
    let transfer = transaction
        .transfer
        .ok_or("exact block is not a transfer transaction")?;
    Ok(ExactTransfer {
        from: transfer.from,
        to: transfer.to,
        amount_e8s: nat_to_u128(transfer.amount)?,
        fee_e8s: transfer.fee.map(nat_to_u128).transpose()?,
        memo: transfer.memo,
        created_at_time: transfer.created_at_time,
        spender: transfer.spender,
    })
}
