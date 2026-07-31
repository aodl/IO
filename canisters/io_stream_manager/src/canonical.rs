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

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpGetBlocksArgs {
    start: u64,
    length: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpTokens {
    e8s: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpTimestamp {
    timestamp_nanos: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpTransaction {
    memo: u64,
    icrc1_memo: Option<Vec<u8>>,
    operation: Option<IcpOperation>,
    created_at_time: IcpTimestamp,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum IcpOperation {
    Mint {
        to: Vec<u8>,
        amount: IcpTokens,
    },
    Burn {
        from: Vec<u8>,
        spender: Option<Vec<u8>>,
        amount: IcpTokens,
    },
    Transfer {
        from: Vec<u8>,
        to: Vec<u8>,
        amount: IcpTokens,
        fee: IcpTokens,
        spender: Option<Vec<u8>>,
    },
    Approve {
        from: Vec<u8>,
        spender: Vec<u8>,
        allowance_e8s: i128,
        allowance: IcpTokens,
        fee: IcpTokens,
        expires_at: Option<IcpTimestamp>,
        expected_allowance: Option<IcpTokens>,
    },
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpBlock {
    parent_hash: Option<Vec<u8>>,
    transaction: IcpTransaction,
    timestamp: IcpTimestamp,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpArchivedRange {
    start: u64,
    length: u64,
    callback: Func,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpQueryBlocksResponse {
    chain_length: u64,
    certificate: Option<Vec<u8>>,
    blocks: Vec<IcpBlock>,
    first_block_index: u64,
    archived_blocks: Vec<IcpArchivedRange>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpBlockRange {
    blocks: Vec<IcpBlock>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum IcpArchiveResult {
    Ok(IcpBlockRange),
    Err(candid::Reserved),
}

pub struct ExactIcpLedgerBlock {
    pub from: Vec<u8>,
    pub to: Vec<u8>,
    pub amount_e8s: u128,
    pub fee_e8s: u128,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: u64,
    pub spender: Option<Vec<u8>>,
}

pub async fn exact_icp_transfer(
    ledger: candid::Principal,
    block_index: u128,
) -> Result<ExactIcpLedgerBlock, String> {
    let block_index: u64 = block_index
        .try_into()
        .map_err(|_| "ICP block index does not fit u64")?;
    let request = IcpGetBlocksArgs {
        start: block_index,
        length: 1,
    };
    let response: IcpQueryBlocksResponse = Call::bounded_wait(ledger, "query_blocks")
        .with_arg(request.clone())
        .await
        .map_err(|error| format!("query_blocks call failed: {error:?}"))?
        .candid()
        .map_err(|error| format!("query_blocks decode failed: {error:?}"))?;
    let block = if response.first_block_index == block_index {
        response.blocks.into_iter().next()
    } else {
        None
    };
    let block = if let Some(block) = block {
        block
    } else {
        let archived = response
            .archived_blocks
            .into_iter()
            .find(|range| {
                block_index >= range.start && block_index < range.start.saturating_add(range.length)
            })
            .ok_or("exact ICP block is neither current nor archived")?;
        let result: IcpArchiveResult =
            Call::bounded_wait(archived.callback.principal, &archived.callback.method)
                .with_arg(request)
                .await
                .map_err(|error| format!("ICP archive callback failed: {error:?}"))?
                .candid()
                .map_err(|error| format!("ICP archive decode failed: {error:?}"))?;
        match result {
            IcpArchiveResult::Ok(range) => range
                .blocks
                .into_iter()
                .next()
                .ok_or("ICP archive returned no exact block")?,
            IcpArchiveResult::Err(_) => {
                return Err("ICP archive rejected exact block request".into())
            }
        }
    };
    let (from, to, amount, fee, spender) = match block.transaction.operation {
        Some(IcpOperation::Transfer {
            from,
            to,
            amount,
            fee,
            spender,
        }) => (from, to, amount.e8s as u128, fee.e8s as u128, spender),
        _ => return Err("exact ICP block is not a transfer".into()),
    };
    Ok(ExactIcpLedgerBlock {
        from,
        to,
        amount_e8s: amount,
        fee_e8s: fee,
        memo: block.transaction.icrc1_memo,
        created_at_time: block.transaction.created_at_time.timestamp_nanos,
        spender,
    })
}

pub fn icp_account_identifier(account: &Account) -> Result<Vec<u8>, String> {
    use sha2::{Digest, Sha224};
    let account = account.canonical()?;
    let mut hasher = Sha224::new();
    hasher.update(b"\x0Aaccount-id");
    hasher.update(account.owner.as_slice());
    hasher.update(account.subaccount);
    let hash = hasher.finalize();
    let checksum = crc32(&hash).to_be_bytes();
    Ok(checksum.into_iter().chain(hash).collect())
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut value = !0u32;
    for byte in bytes {
        value ^= u32::from(*byte);
        for _ in 0..8 {
            value = (value >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(value & 1));
        }
    }
    !value
}
