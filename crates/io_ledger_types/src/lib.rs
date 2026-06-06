use candid::{CandidType, Decode, Encode, Nat, Principal};
use serde::Deserialize;
use std::future::Future;
use std::pin::Pin;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, CandidType, Deserialize)]
pub struct Subaccount(pub [u8; 32]);

impl Subaccount {
    pub fn zero() -> Self {
        Self([0; 32])
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, CandidType, Deserialize)]
pub struct Account {
    pub owner: Principal,
    pub subaccount: Option<Subaccount>,
}

impl Account {
    pub fn new(owner: Principal, subaccount: Option<Subaccount>) -> Self {
        Self { owner, subaccount }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, CandidType, Deserialize)]
pub struct BlockIndex(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash, CandidType, Deserialize)]
pub struct Memo(pub Vec<u8>);

impl From<&str> for Memo {
    fn from(value: &str) -> Self {
        Self(value.as_bytes().to_vec())
    }
}

impl From<String> for Memo {
    fn from(value: String) -> Self {
        Self(value.into_bytes())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, CandidType, Deserialize)]
pub struct TokenAmountE8s(pub u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, CandidType, Deserialize)]
pub struct TransferFeeE8s(pub u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, CandidType, Deserialize)]
pub enum LedgerKind {
    IcpLedger,
    IoLedger,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LedgerId {
    pub kind: LedgerKind,
    pub canister: Option<Principal>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LedgerTransferRequest {
    pub from_subaccount: Option<Subaccount>,
    pub to: Account,
    pub amount_e8s: u128,
    pub fee_e8s: Option<u128>,
    pub memo: Option<Memo>,
    pub created_at_time: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LedgerTransferSuccess {
    pub block_index: BlockIndex,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum LedgerTransferError {
    InsufficientFunds { balance_e8s: u128 },
    BadFee { expected_fee_e8s: u128 },
    TemporarilyUnavailable,
    Duplicate { duplicate_of: BlockIndex },
    TooOld,
    CreatedInFuture { ledger_time: u64 },
    GenericError { error_code: u64, message: String },
    CanisterCallFailed { method: String, message: String },
    DecodeError { message: String },
    Unsupported,
}

impl LedgerTransferError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::TemporarilyUnavailable
                | Self::CanisterCallFailed { .. }
                | Self::DecodeError { .. }
        )
    }

    pub fn idempotent_success_block(&self) -> Option<BlockIndex> {
        match self {
            Self::Duplicate { duplicate_of } => Some(*duplicate_of),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum LedgerQueryError {
    TemporarilyUnavailable,
    CanisterCallFailed { method: String, message: String },
    DecodeError { message: String },
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum LedgerOperationKind {
    Transfer,
    Mint,
    Burn,
    Approve,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LedgerBlock {
    pub block_index: BlockIndex,
    pub timestamp_nanos: u64,
    pub from: Option<Account>,
    pub to: Option<Account>,
    pub amount_e8s: u128,
    pub fee_e8s: Option<u128>,
    pub memo: Option<Memo>,
    pub operation_kind: LedgerOperationKind,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IndexTransaction {
    pub block_index: BlockIndex,
    pub transaction: LedgerBlock,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IndexScanRequest {
    pub start: Option<BlockIndex>,
    pub limit: u64,
    pub account_filter: Option<Account>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IndexScanResult {
    pub transactions: Vec<IndexTransaction>,
    pub last_seen_block: Option<BlockIndex>,
    pub index_tip: Option<BlockIndex>,
    pub archive_required: bool,
}

impl IndexScanResult {
    pub fn validate_monotonic(&self) -> Result<(), IndexError> {
        let mut last = None;
        for tx in &self.transactions {
            if let Some(previous) = last {
                if tx.block_index <= previous {
                    return Err(IndexError::MissingBlock {
                        block_index: tx.block_index,
                    });
                }
            }
            last = Some(tx.block_index);
        }
        Ok(())
    }

    pub fn next_cursor(
        &self,
        current: Option<BlockIndex>,
    ) -> Result<Option<BlockIndex>, IndexError> {
        if self.archive_required {
            return Err(IndexError::ArchiveRequired {
                from: current.unwrap_or(BlockIndex(0)),
            });
        }
        self.validate_monotonic()?;
        Ok(self
            .transactions
            .iter()
            .map(|tx| tx.block_index)
            .max()
            .or(current))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum IndexError {
    TemporarilyUnavailable,
    IndexLag {
        requested: BlockIndex,
        tip: Option<BlockIndex>,
    },
    ArchiveRequired {
        from: BlockIndex,
    },
    MissingBlock {
        block_index: BlockIndex,
    },
    DecodeError {
        message: String,
    },
    CanisterCallFailed {
        method: String,
        message: String,
    },
    Unsupported,
}

pub trait LedgerTransferClient {
    fn transfer<'a>(
        &'a self,
        request: LedgerTransferRequest,
    ) -> Pin<Box<dyn Future<Output = Result<LedgerTransferSuccess, LedgerTransferError>> + 'a>>;

    fn fee<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<u128, LedgerQueryError>> + 'a>>;
}

pub trait LedgerIndexClient {
    fn get_account_transactions<'a>(
        &'a self,
        request: IndexScanRequest,
    ) -> Pin<Box<dyn Future<Output = Result<IndexScanResult, IndexError>> + 'a>>;

    fn get_tip<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<BlockIndex>, IndexError>> + 'a>>;
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcrcAccount {
    pub owner: Principal,
    pub subaccount: Option<Vec<u8>>,
}

impl From<Account> for IcrcAccount {
    fn from(value: Account) -> Self {
        Self {
            owner: value.owner,
            subaccount: value.subaccount.map(|subaccount| subaccount.0.to_vec()),
        }
    }
}

impl TryFrom<IcrcAccount> for Account {
    type Error = LedgerTransferError;

    fn try_from(value: IcrcAccount) -> Result<Self, Self::Error> {
        let subaccount = match value.subaccount {
            Some(bytes) => Some(Subaccount(bytes.try_into().map_err(|bytes: Vec<u8>| {
                LedgerTransferError::DecodeError {
                    message: format!("subaccount must be 32 bytes, got {}", bytes.len()),
                }
            })?)),
            None => None,
        };
        Ok(Self {
            owner: value.owner,
            subaccount,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcrcTransferArg {
    pub from_subaccount: Option<Vec<u8>>,
    pub to: IcrcAccount,
    pub amount: Nat,
    pub fee: Option<Nat>,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: Option<u64>,
}

impl From<LedgerTransferRequest> for IcrcTransferArg {
    fn from(value: LedgerTransferRequest) -> Self {
        Self {
            from_subaccount: value
                .from_subaccount
                .map(|subaccount| subaccount.0.to_vec()),
            to: value.to.into(),
            amount: Nat::from(value.amount_e8s),
            fee: value.fee_e8s.map(Nat::from),
            memo: value.memo.map(|memo| memo.0),
            created_at_time: value.created_at_time,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
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

pub fn nat_to_u128(value: &Nat, field: &str) -> Result<u128, LedgerTransferError> {
    value
        .0
        .to_str_radix(10)
        .parse::<u128>()
        .map_err(|err| LedgerTransferError::DecodeError {
            message: format!("{field} does not fit in u128: {err}"),
        })
}

pub fn nat_to_u64(value: &Nat, field: &str) -> Result<u64, LedgerTransferError> {
    value
        .0
        .to_str_radix(10)
        .parse::<u64>()
        .map_err(|err| LedgerTransferError::DecodeError {
            message: format!("{field} does not fit in u64: {err}"),
        })
}

pub fn map_icrc_transfer_result(
    result: Result<Nat, IcrcTransferError>,
) -> Result<LedgerTransferSuccess, LedgerTransferError> {
    match result {
        Ok(block) => Ok(LedgerTransferSuccess {
            block_index: BlockIndex(nat_to_u64(&block, "block index")?),
        }),
        Err(IcrcTransferError::BadFee { expected_fee }) => Err(LedgerTransferError::BadFee {
            expected_fee_e8s: nat_to_u128(&expected_fee, "expected fee")?,
        }),
        Err(IcrcTransferError::BadBurn { .. }) => Err(LedgerTransferError::Unsupported),
        Err(IcrcTransferError::InsufficientFunds { balance }) => {
            Err(LedgerTransferError::InsufficientFunds {
                balance_e8s: nat_to_u128(&balance, "balance")?,
            })
        }
        Err(IcrcTransferError::TooOld) => Err(LedgerTransferError::TooOld),
        Err(IcrcTransferError::CreatedInFuture { ledger_time }) => {
            Err(LedgerTransferError::CreatedInFuture { ledger_time })
        }
        Err(IcrcTransferError::Duplicate { duplicate_of }) => Err(LedgerTransferError::Duplicate {
            duplicate_of: BlockIndex(nat_to_u64(&duplicate_of, "duplicate block index")?),
        }),
        Err(IcrcTransferError::TemporarilyUnavailable) => {
            Err(LedgerTransferError::TemporarilyUnavailable)
        }
        Err(IcrcTransferError::GenericError {
            error_code,
            message,
        }) => Err(LedgerTransferError::GenericError {
            error_code: nat_to_u64(&error_code, "error code")?,
            message,
        }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcpTransferArgs {
    pub memo: u64,
    pub amount: IcpTokens,
    pub fee: IcpTokens,
    pub from_subaccount: Option<Subaccount>,
    pub to: Vec<u8>,
    pub created_at_time: Option<IcpTimeStamp>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcpTokens {
    pub e8s: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcpTimeStamp {
    pub timestamp_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum IcpTransferError {
    BadFee { expected_fee: IcpTokens },
    InsufficientFunds { balance: IcpTokens },
    TxTooOld { allowed_window_nanos: u64 },
    TxCreatedInFuture,
    TxDuplicate { duplicate_of: u64 },
}

pub fn u128_to_icp_tokens(value: u128, field: &str) -> Result<IcpTokens, LedgerTransferError> {
    Ok(IcpTokens {
        e8s: value
            .try_into()
            .map_err(|_| LedgerTransferError::DecodeError {
                message: format!("{field} does not fit in u64 ICP e8s"),
            })?,
    })
}

pub fn icp_transfer_args(
    request: LedgerTransferRequest,
    to_account_identifier: Vec<u8>,
    default_fee_e8s: u128,
) -> Result<IcpTransferArgs, LedgerTransferError> {
    let memo = request
        .memo
        .as_ref()
        .and_then(|memo| {
            let bytes: [u8; 8] = memo.0.as_slice().try_into().ok()?;
            Some(u64::from_le_bytes(bytes))
        })
        .unwrap_or(0);
    Ok(IcpTransferArgs {
        memo,
        amount: u128_to_icp_tokens(request.amount_e8s, "amount")?,
        fee: u128_to_icp_tokens(request.fee_e8s.unwrap_or(default_fee_e8s), "fee")?,
        from_subaccount: request.from_subaccount,
        to: to_account_identifier,
        created_at_time: request
            .created_at_time
            .map(|timestamp_nanos| IcpTimeStamp { timestamp_nanos }),
    })
}

pub fn map_icp_transfer_result(
    result: Result<u64, IcpTransferError>,
) -> Result<LedgerTransferSuccess, LedgerTransferError> {
    match result {
        Ok(block) => Ok(LedgerTransferSuccess {
            block_index: BlockIndex(block),
        }),
        Err(IcpTransferError::BadFee { expected_fee }) => Err(LedgerTransferError::BadFee {
            expected_fee_e8s: expected_fee.e8s.into(),
        }),
        Err(IcpTransferError::InsufficientFunds { balance }) => {
            Err(LedgerTransferError::InsufficientFunds {
                balance_e8s: balance.e8s.into(),
            })
        }
        Err(IcpTransferError::TxTooOld { .. }) => Err(LedgerTransferError::TooOld),
        Err(IcpTransferError::TxCreatedInFuture) => {
            Err(LedgerTransferError::CreatedInFuture { ledger_time: 0 })
        }
        Err(IcpTransferError::TxDuplicate { duplicate_of }) => {
            Err(LedgerTransferError::Duplicate {
                duplicate_of: BlockIndex(duplicate_of),
            })
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcrcIndexGetAccountTransactionsArgs {
    pub account: IcrcAccount,
    pub start: Option<Nat>,
    pub max_results: Nat,
}

impl TryFrom<IndexScanRequest> for IcrcIndexGetAccountTransactionsArgs {
    type Error = IndexError;

    fn try_from(value: IndexScanRequest) -> Result<Self, Self::Error> {
        let account = value.account_filter.ok_or(IndexError::Unsupported)?;
        Ok(Self {
            account: account.into(),
            start: value.start.map(|block| Nat::from(block.0)),
            max_results: Nat::from(value.limit),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcrcIndexTransaction {
    pub id: Nat,
    pub transaction: LedgerBlock,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcrcIndexGetAccountTransactionsResult {
    pub transactions: Vec<IcrcIndexTransaction>,
    pub oldest_tx_id: Option<Nat>,
    pub tip: Option<Nat>,
    pub archive_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum IcrcIndexError {
    GenericError { error_code: Nat, message: String },
    TemporarilyUnavailable,
}

fn nat_to_u64_index(value: &Nat, field: &str) -> Result<u64, IndexError> {
    value
        .0
        .to_str_radix(10)
        .parse::<u64>()
        .map_err(|err| IndexError::DecodeError {
            message: format!("{field} does not fit in u64: {err}"),
        })
}

pub fn map_icrc_index_result(
    result: Result<IcrcIndexGetAccountTransactionsResult, IcrcIndexError>,
) -> Result<IndexScanResult, IndexError> {
    match result {
        Ok(page) => {
            let mut transactions = Vec::with_capacity(page.transactions.len());
            for tx in page.transactions {
                transactions.push(IndexTransaction {
                    block_index: BlockIndex(nat_to_u64_index(&tx.id, "transaction id")?),
                    transaction: tx.transaction,
                });
            }
            let last_seen_block = transactions.iter().map(|tx| tx.block_index).max();
            Ok(IndexScanResult {
                transactions,
                last_seen_block,
                index_tip: page
                    .tip
                    .as_ref()
                    .map(|tip| nat_to_u64_index(tip, "index tip").map(BlockIndex))
                    .transpose()?,
                archive_required: page.archive_required,
            })
        }
        Err(IcrcIndexError::TemporarilyUnavailable) => Err(IndexError::TemporarilyUnavailable),
        Err(IcrcIndexError::GenericError { message, .. }) if message.contains("archive") => {
            Err(IndexError::ArchiveRequired {
                from: BlockIndex(0),
            })
        }
        Err(IcrcIndexError::GenericError { message, .. }) => {
            Err(IndexError::DecodeError { message })
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DuplicateProof {
    pub expected_amount_e8s: u128,
    pub actual_amount_e8s: u128,
    pub expected_to: Account,
    pub actual_to: Account,
    pub expected_memo: Option<Memo>,
    pub actual_memo: Option<Memo>,
}

pub fn duplicate_matches_expected(
    expected: &LedgerTransferRequest,
    duplicate_block: &LedgerBlock,
) -> Result<BlockIndex, Box<DuplicateProof>> {
    if duplicate_block.amount_e8s == expected.amount_e8s
        && duplicate_block.to.as_ref() == Some(&expected.to)
        && duplicate_block.memo == expected.memo
    {
        Ok(duplicate_block.block_index)
    } else {
        Err(Box::new(DuplicateProof {
            expected_amount_e8s: expected.amount_e8s,
            actual_amount_e8s: duplicate_block.amount_e8s,
            expected_to: expected.to.clone(),
            actual_to: duplicate_block.to.clone().unwrap_or_else(|| Account {
                owner: Principal::anonymous(),
                subaccount: None,
            }),
            expected_memo: expected.memo.clone(),
            actual_memo: duplicate_block.memo.clone(),
        }))
    }
}

pub fn candid_round_trip<T>(value: &T) -> T
where
    T: CandidType + for<'de> Deserialize<'de>,
{
    let bytes = Encode!(value).expect("fixture should encode");
    Decode!(&bytes, T).expect("fixture should decode")
}

#[cfg(target_family = "wasm")]
#[derive(Clone, Copy, Debug)]
pub struct IcrcLedgerCanisterClient {
    pub canister: Principal,
}

#[cfg(target_family = "wasm")]
impl LedgerTransferClient for IcrcLedgerCanisterClient {
    fn transfer<'a>(
        &'a self,
        request: LedgerTransferRequest,
    ) -> Pin<Box<dyn Future<Output = Result<LedgerTransferSuccess, LedgerTransferError>> + 'a>>
    {
        Box::pin(async move {
            let arg = IcrcTransferArg::from(request);
            let response = ic_cdk::call::Call::bounded_wait(self.canister, "icrc1_transfer")
                .with_arg(arg)
                .await
                .map_err(|err| LedgerTransferError::CanisterCallFailed {
                    method: "icrc1_transfer".to_string(),
                    message: format!("{err:?}"),
                })?;
            let (result,) = response
                .candid_tuple::<(Result<Nat, IcrcTransferError>,)>()
                .map_err(|err| LedgerTransferError::DecodeError {
                    message: format!("{err:?}"),
                })?;
            map_icrc_transfer_result(result)
        })
    }

    fn fee<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<u128, LedgerQueryError>> + 'a>> {
        Box::pin(async move {
            let response = ic_cdk::call::Call::bounded_wait(self.canister, "icrc1_fee")
                .await
                .map_err(|err| LedgerQueryError::CanisterCallFailed {
                    method: "icrc1_fee".to_string(),
                    message: format!("{err:?}"),
                })?;
            let (fee,) =
                response
                    .candid_tuple::<(Nat,)>()
                    .map_err(|err| LedgerQueryError::DecodeError {
                        message: format!("{err:?}"),
                    })?;
            fee.0
                .to_str_radix(10)
                .parse::<u128>()
                .map_err(|err| LedgerQueryError::DecodeError {
                    message: format!("ledger fee does not fit in u128: {err}"),
                })
        })
    }
}

#[cfg(target_family = "wasm")]
#[derive(Clone, Copy, Debug)]
pub struct IcrcIndexCanisterClient {
    pub canister: Principal,
}

#[cfg(target_family = "wasm")]
impl LedgerIndexClient for IcrcIndexCanisterClient {
    fn get_account_transactions<'a>(
        &'a self,
        request: IndexScanRequest,
    ) -> Pin<Box<dyn Future<Output = Result<IndexScanResult, IndexError>> + 'a>> {
        Box::pin(async move {
            let arg = IcrcIndexGetAccountTransactionsArgs::try_from(request)?;
            let response =
                ic_cdk::call::Call::bounded_wait(self.canister, "get_account_transactions")
                    .with_arg(arg)
                    .await
                    .map_err(|err| IndexError::CanisterCallFailed {
                        method: "get_account_transactions".to_string(),
                        message: format!("{err:?}"),
                    })?;
            let (result,) = response
                .candid_tuple::<(Result<IcrcIndexGetAccountTransactionsResult, IcrcIndexError>,)>()
                .map_err(|err| IndexError::DecodeError {
                    message: format!("{err:?}"),
                })?;
            map_icrc_index_result(result)
        })
    }

    fn get_tip<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<BlockIndex>, IndexError>> + 'a>> {
        Box::pin(async move {
            let response = ic_cdk::call::Call::bounded_wait(self.canister, "debug_get_tip")
                .await
                .map_err(|err| IndexError::CanisterCallFailed {
                    method: "debug_get_tip".to_string(),
                    message: format!("{err:?}"),
                })?;
            let (tip,) = response.candid_tuple::<(Option<Nat>,)>().map_err(|err| {
                IndexError::DecodeError {
                    message: format!("{err:?}"),
                }
            })?;
            tip.as_ref()
                .map(|tip| nat_to_u64_index(tip, "index tip").map(BlockIndex))
                .transpose()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal() -> Principal {
        Principal::from_text("aaaaa-aa").unwrap()
    }

    fn account() -> Account {
        Account::new(principal(), Some(Subaccount([7; 32])))
    }

    fn request() -> LedgerTransferRequest {
        LedgerTransferRequest {
            from_subaccount: Some(Subaccount([1; 32])),
            to: account(),
            amount_e8s: 123,
            fee_e8s: Some(10),
            memo: Some(Memo::from("memo")),
            created_at_time: Some(99),
        }
    }

    #[test]
    fn account_candid_round_trips() {
        let decoded = candid_round_trip(&account());
        assert_eq!(decoded, account());
    }

    #[test]
    fn transfer_request_candid_round_trips_without_lossy_amounts() {
        let mut req = request();
        req.amount_e8s = u128::MAX;
        assert_eq!(candid_round_trip(&req), req);
    }

    #[test]
    fn icrc_transfer_arg_encodes_subaccounts_and_nat_amounts() {
        let arg = IcrcTransferArg::from(request());
        assert_eq!(arg.from_subaccount, Some(vec![1; 32]));
        assert_eq!(arg.to.subaccount, Some(vec![7; 32]));
        assert_eq!(arg.amount, Nat::from(123_u128));
        assert_eq!(arg.fee, Some(Nat::from(10_u128)));
    }

    #[test]
    fn icrc_error_mapping_preserves_duplicate_and_bad_fee() {
        assert_eq!(
            map_icrc_transfer_result(Err(IcrcTransferError::Duplicate {
                duplicate_of: Nat::from(42_u64)
            })),
            Err(LedgerTransferError::Duplicate {
                duplicate_of: BlockIndex(42)
            })
        );
        assert_eq!(
            map_icrc_transfer_result(Err(IcrcTransferError::BadFee {
                expected_fee: Nat::from(10_u128)
            })),
            Err(LedgerTransferError::BadFee {
                expected_fee_e8s: 10
            })
        );
    }

    #[test]
    fn icrc_error_mapping_preserves_insufficient_funds_and_generic_error() {
        assert_eq!(
            map_icrc_transfer_result(Err(IcrcTransferError::InsufficientFunds {
                balance: Nat::from(5_u128)
            })),
            Err(LedgerTransferError::InsufficientFunds { balance_e8s: 5 })
        );
        assert_eq!(
            map_icrc_transfer_result(Err(IcrcTransferError::GenericError {
                error_code: Nat::from(2_u64),
                message: "busy".to_string()
            })),
            Err(LedgerTransferError::GenericError {
                error_code: 2,
                message: "busy".to_string()
            })
        );
    }

    #[test]
    fn icp_transfer_arg_rejects_amounts_that_do_not_fit_legacy_icp_e8s() {
        let mut req = request();
        req.amount_e8s = u128::from(u64::MAX) + 1;
        let err = icp_transfer_args(req, vec![0; 32], 10).unwrap_err();
        assert!(matches!(err, LedgerTransferError::DecodeError { .. }));
    }

    #[test]
    fn icp_error_mapping_preserves_duplicate_bad_fee_and_insufficient_funds() {
        assert_eq!(
            map_icp_transfer_result(Err(IcpTransferError::TxDuplicate { duplicate_of: 77 })),
            Err(LedgerTransferError::Duplicate {
                duplicate_of: BlockIndex(77)
            })
        );
        assert_eq!(
            map_icp_transfer_result(Err(IcpTransferError::BadFee {
                expected_fee: IcpTokens { e8s: 10 }
            })),
            Err(LedgerTransferError::BadFee {
                expected_fee_e8s: 10
            })
        );
        assert_eq!(
            map_icp_transfer_result(Err(IcpTransferError::InsufficientFunds {
                balance: IcpTokens { e8s: 1 }
            })),
            Err(LedgerTransferError::InsufficientFunds { balance_e8s: 1 })
        );
    }

    #[test]
    fn duplicate_transfer_matches_only_same_amount_account_and_memo() {
        let req = request();
        let block = LedgerBlock {
            block_index: BlockIndex(9),
            timestamp_nanos: 0,
            from: None,
            to: Some(req.to.clone()),
            amount_e8s: req.amount_e8s,
            fee_e8s: req.fee_e8s,
            memo: req.memo.clone(),
            operation_kind: LedgerOperationKind::Transfer,
        };
        assert_eq!(duplicate_matches_expected(&req, &block), Ok(BlockIndex(9)));

        let mut mismatched = block;
        mismatched.amount_e8s += 1;
        assert!(duplicate_matches_expected(&req, &mismatched).is_err());
    }

    #[test]
    fn index_cursor_keeps_empty_page_cursor_unchanged() {
        let result = IndexScanResult {
            transactions: vec![],
            last_seen_block: None,
            index_tip: Some(BlockIndex(10)),
            archive_required: false,
        };
        assert_eq!(
            result.next_cursor(Some(BlockIndex(4))),
            Ok(Some(BlockIndex(4)))
        );
    }

    #[test]
    fn index_cursor_advances_to_page_max_for_monotonic_page() {
        let tx = |block| IndexTransaction {
            block_index: BlockIndex(block),
            transaction: LedgerBlock {
                block_index: BlockIndex(block),
                timestamp_nanos: block,
                from: None,
                to: Some(account()),
                amount_e8s: 1,
                fee_e8s: None,
                memo: None,
                operation_kind: LedgerOperationKind::Transfer,
            },
        };
        let result = IndexScanResult {
            transactions: vec![tx(5), tx(6)],
            last_seen_block: Some(BlockIndex(6)),
            index_tip: Some(BlockIndex(6)),
            archive_required: false,
        };
        assert_eq!(
            result.next_cursor(Some(BlockIndex(4))),
            Ok(Some(BlockIndex(6)))
        );
    }

    #[test]
    fn index_cursor_rejects_duplicate_or_non_monotonic_blocks() {
        let tx = |block| IndexTransaction {
            block_index: BlockIndex(block),
            transaction: LedgerBlock {
                block_index: BlockIndex(block),
                timestamp_nanos: 0,
                from: None,
                to: None,
                amount_e8s: 0,
                fee_e8s: None,
                memo: None,
                operation_kind: LedgerOperationKind::Unknown,
            },
        };
        let duplicate = IndexScanResult {
            transactions: vec![tx(5), tx(5)],
            last_seen_block: Some(BlockIndex(5)),
            index_tip: Some(BlockIndex(5)),
            archive_required: false,
        };
        assert!(matches!(
            duplicate.next_cursor(Some(BlockIndex(4))),
            Err(IndexError::MissingBlock { .. })
        ));
    }

    #[test]
    fn index_cursor_reports_archive_required_without_advancing() {
        let result = IndexScanResult {
            transactions: vec![],
            last_seen_block: None,
            index_tip: Some(BlockIndex(100)),
            archive_required: true,
        };
        assert_eq!(
            result.next_cursor(Some(BlockIndex(50))),
            Err(IndexError::ArchiveRequired {
                from: BlockIndex(50)
            })
        );
    }

    #[test]
    fn icrc_index_args_require_an_account_filter() {
        let err = IcrcIndexGetAccountTransactionsArgs::try_from(IndexScanRequest {
            start: Some(BlockIndex(1)),
            limit: 10,
            account_filter: None,
        })
        .unwrap_err();
        assert_eq!(err, IndexError::Unsupported);
    }

    #[test]
    fn icrc_index_result_maps_pages_tip_and_archive_flag() {
        let block = LedgerBlock {
            block_index: BlockIndex(3),
            timestamp_nanos: 9,
            from: Some(account()),
            to: Some(account()),
            amount_e8s: 7,
            fee_e8s: Some(10),
            memo: Some(Memo::from("idx")),
            operation_kind: LedgerOperationKind::Transfer,
        };
        let result = map_icrc_index_result(Ok(IcrcIndexGetAccountTransactionsResult {
            transactions: vec![IcrcIndexTransaction {
                id: Nat::from(3_u64),
                transaction: block,
            }],
            oldest_tx_id: Some(Nat::from(3_u64)),
            tip: Some(Nat::from(8_u64)),
            archive_required: true,
        }))
        .unwrap();
        assert_eq!(result.transactions[0].block_index, BlockIndex(3));
        assert_eq!(result.last_seen_block, Some(BlockIndex(3)));
        assert_eq!(result.index_tip, Some(BlockIndex(8)));
        assert!(result.archive_required);
    }

    #[test]
    fn icrc_index_result_classifies_errors() {
        assert_eq!(
            map_icrc_index_result(Err(IcrcIndexError::TemporarilyUnavailable)),
            Err(IndexError::TemporarilyUnavailable)
        );
        assert!(matches!(
            map_icrc_index_result(Err(IcrcIndexError::GenericError {
                error_code: Nat::from(1_u64),
                message: "archive required".to_string()
            })),
            Err(IndexError::ArchiveRequired { .. })
        ));
    }

    #[test]
    fn fee_and_dust_values_are_explicit_at_boundary() {
        let tiny = LedgerTransferRequest {
            amount_e8s: 1,
            fee_e8s: Some(10_000),
            ..request()
        };
        assert_eq!(tiny.amount_e8s, 1);
        assert_eq!(tiny.fee_e8s, Some(10_000));
    }
}
