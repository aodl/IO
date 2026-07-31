//! Minimal canonical ledger boundary for IO's value-moving canisters.
//!
//! This crate deliberately retrieves one caller-selected block. It does not scan
//! account history, prove absence, reconcile ranges, or own transfer state.

use candid::{CandidType, Nat, Principal};
use ic_cdk::call::Call;
use io_accounts::Account;
use serde::Deserialize;
use sha2::{Digest, Sha224};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactIcrcTransfer {
    pub from: Account,
    pub to: Account,
    pub amount_e8s: u128,
    pub fee_e8s: Option<u128>,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: Option<u64>,
    pub spender: Option<Account>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactIcpTransfer {
    pub from: Vec<u8>,
    pub to: Vec<u8>,
    pub amount_e8s: u128,
    pub fee_e8s: u128,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: u64,
    pub spender: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactIcpMint {
    pub to: Vec<u8>,
    pub amount_e8s: u128,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: u64,
}

impl ExactIcrcTransfer {
    pub fn matches(
        &self,
        from: &Account,
        to: &Account,
        amount_e8s: u128,
        fee_e8s: Option<u128>,
        memo: Option<&[u8]>,
        created_at_time: Option<u64>,
        spender: Option<&Account>,
    ) -> Result<bool, String> {
        Ok(self.from.effective_eq(from)?
            && self.to.effective_eq(to)?
            && self.amount_e8s == amount_e8s
            && self.fee_e8s == fee_e8s
            && self.memo.as_deref() == memo
            && self.created_at_time == created_at_time
            && match (&self.spender, spender) {
                (Some(actual), Some(expected)) => actual.effective_eq(expected)?,
                (None, None) => true,
                _ => false,
            })
    }
}

impl ExactIcpTransfer {
    pub fn matches(
        &self,
        from: &[u8],
        to: &[u8],
        amount_e8s: u128,
        fee_e8s: u128,
        memo: Option<&[u8]>,
        created_at_time: u64,
        spender: Option<&[u8]>,
    ) -> bool {
        self.from == from
            && self.to == to
            && self.amount_e8s == amount_e8s
            && self.fee_e8s == fee_e8s
            && self.memo.as_deref() == memo
            && self.created_at_time == created_at_time
            && self.spender.as_deref() == spender
    }
}

impl ExactIcpMint {
    pub fn matches(
        &self,
        to: &[u8],
        amount_e8s: u128,
        memo: Option<&[u8]>,
        created_at_time: u64,
    ) -> bool {
        self.to == to
            && self.amount_e8s == amount_e8s
            && self.memo.as_deref() == memo
            && self.created_at_time == created_at_time
    }
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct IcrcTransferArg {
    pub from_subaccount: Option<Vec<u8>>,
    pub to: Account,
    pub amount: Nat,
    pub fee: Option<Nat>,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum IcrcTransferError {
    BadFee { expected_fee: Nat },
    BadBurn { min_burn_amount: Nat },
    InsufficientFunds { balance: Nat },
    TooOld,
    CreatedInFuture { ledger_time: u64 },
    Duplicate { duplicate_of: Nat },
    TemporarilyUnavailable,
    GenericError { error_code: Nat, message: String },
}

pub type IcrcTransferResult = Result<Nat, IcrcTransferError>;

#[derive(Clone, Debug, CandidType, Deserialize)]
struct GetTransactionsRequest {
    start: Nat,
    length: Nat,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct GetTransactionsResponse {
    transactions: Vec<IcrcTransaction>,
    first_index: Nat,
    archived_transactions: Vec<ArchivedTransactions>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ArchivedTransactions {
    start: Nat,
    length: Nat,
    callback: IcrcArchiveCallback,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct TransactionRange {
    transactions: Vec<IcrcTransaction>,
}

candid::define_function!(IcrcArchiveCallback : (GetTransactionsRequest) -> (TransactionRange) query);

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcrcTransaction {
    kind: String,
    transfer: Option<IcrcTransfer>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcrcTransfer {
    to: Account,
    fee: Option<Nat>,
    from: Account,
    memo: Option<Vec<u8>>,
    created_at_time: Option<u64>,
    amount: Nat,
    spender: Option<Account>,
}

pub async fn exact_icrc_transfer(
    ledger: Principal,
    block_index: u128,
) -> Result<ExactIcrcTransfer, String> {
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
    let transaction = match transaction {
        Some(transaction) => transaction,
        None => {
            let archived = response
                .archived_transactions
                .into_iter()
                .find(|range| range_contains(&range.start, &range.length, block_index))
                .ok_or("exact ICRC block is neither current nor archived")?;
            let range: TransactionRange =
                Call::bounded_wait(archived.callback.0.principal, &archived.callback.0.method)
                    .with_arg(request)
                    .await
                    .map_err(|error| format!("ICRC archive callback failed: {error:?}"))?
                    .candid()
                    .map_err(|error| format!("ICRC archive decode failed: {error:?}"))?;
            range
                .transactions
                .into_iter()
                .next()
                .ok_or("ICRC archive returned no exact transaction")?
        }
    };
    if transaction.kind != "transfer" {
        return Err("exact ICRC block is not a transfer transaction".into());
    }
    let transfer = transaction
        .transfer
        .ok_or("exact ICRC block lacks transfer details")?;
    Ok(ExactIcrcTransfer {
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
    },
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpBlock {
    transaction: IcpTransaction,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpArchivedRange {
    start: u64,
    length: u64,
    callback: IcpArchiveCallback,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IcpQueryBlocksResponse {
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

candid::define_function!(IcpArchiveCallback : (IcpGetBlocksArgs) -> (IcpArchiveResult) query);

pub async fn exact_icp_block(
    ledger: Principal,
    block_index: u128,
) -> Result<IcpExactResult, String> {
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
    let block = match block {
        Some(block) => block,
        None => {
            let archived = response
                .archived_blocks
                .into_iter()
                .find(|range| {
                    block_index >= range.start
                        && block_index < range.start.saturating_add(range.length)
                })
                .ok_or("exact ICP block is neither current nor archived")?;
            match Call::bounded_wait(archived.callback.0.principal, &archived.callback.0.method)
                .with_arg(request)
                .await
                .map_err(|error| format!("ICP archive callback failed: {error:?}"))?
                .candid::<IcpArchiveResult>()
                .map_err(|error| format!("ICP archive decode failed: {error:?}"))?
            {
                IcpArchiveResult::Ok(range) => range
                    .blocks
                    .into_iter()
                    .next()
                    .ok_or("ICP archive returned no exact block")?,
                IcpArchiveResult::Err(_) => {
                    return Err("ICP archive rejected exact block request".into())
                }
            }
        }
    };
    let memo = block.transaction.icrc1_memo;
    let created_at_time = block.transaction.created_at_time.timestamp_nanos;
    match block.transaction.operation {
        Some(IcpOperation::Transfer {
            from,
            to,
            amount,
            fee,
            spender,
        }) => Ok(IcpExactResult::Transfer(ExactIcpTransfer {
            from,
            to,
            amount_e8s: amount.e8s.into(),
            fee_e8s: fee.e8s.into(),
            memo,
            created_at_time,
            spender,
        })),
        Some(IcpOperation::Mint { to, amount }) => Ok(IcpExactResult::Mint(ExactIcpMint {
            to,
            amount_e8s: amount.e8s.into(),
            memo,
            created_at_time,
        })),
        _ => Err("exact ICP block is neither Transfer nor Mint".into()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IcpExactResult {
    Transfer(ExactIcpTransfer),
    Mint(ExactIcpMint),
}

pub async fn exact_icp_transfer(
    ledger: Principal,
    block_index: u128,
) -> Result<ExactIcpTransfer, String> {
    match exact_icp_block(ledger, block_index).await? {
        IcpExactResult::Transfer(transfer) => Ok(transfer),
        IcpExactResult::Mint(_) => Err("exact ICP block is not a transfer".into()),
    }
}

pub fn icp_account_identifier(account: &Account) -> Result<Vec<u8>, String> {
    let account = account.canonical()?;
    let mut hasher = Sha224::new();
    hasher.update(b"\x0Aaccount-id");
    hasher.update(account.owner.as_slice());
    hasher.update(account.subaccount);
    let hash = hasher.finalize();
    Ok(crc32(&hash).to_be_bytes().into_iter().chain(hash).collect())
}

fn nat_to_u128(value: Nat) -> Result<u128, String> {
    value
        .0
        .try_into()
        .map_err(|_| "ledger value does not fit u128".into())
}

fn range_contains(start: &Nat, length: &Nat, block_index: u128) -> bool {
    nat_to_u128(start.clone())
        .ok()
        .zip(nat_to_u128(length.clone()).ok())
        .is_some_and(|(start, length)| {
            block_index >= start && block_index < start.saturating_add(length)
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_identifier_normalizes_zero_subaccount() {
        let owner = Principal::from_slice(&[1]);
        let implicit = Account {
            owner,
            subaccount: None,
        };
        let explicit = Account {
            owner,
            subaccount: Some(vec![0; 32]),
        };
        assert_eq!(
            icp_account_identifier(&implicit),
            icp_account_identifier(&explicit)
        );
    }

    #[test]
    fn exact_match_rejects_changed_transfer_semantics() {
        let account = Account {
            owner: Principal::from_slice(&[1]),
            subaccount: None,
        };
        let transfer = ExactIcrcTransfer {
            from: account.clone(),
            to: Account {
                owner: Principal::from_slice(&[2]),
                subaccount: None,
            },
            amount_e8s: 10,
            fee_e8s: Some(1),
            memo: Some(vec![7]),
            created_at_time: Some(8),
            spender: None,
        };
        assert!(transfer
            .matches(
                &account,
                &transfer.to,
                10,
                Some(1),
                Some(&[7]),
                Some(8),
                None,
            )
            .unwrap());
        assert!(!transfer
            .matches(
                &account,
                &transfer.to,
                11,
                Some(1),
                Some(&[7]),
                Some(8),
                None,
            )
            .unwrap());
    }
}
