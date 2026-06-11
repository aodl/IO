use candid::{CandidType, Decode, Encode, Nat, Principal};
use serde::Deserialize;
use sha2::{Digest, Sha224};
use std::future::Future;
use std::pin::Pin;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, CandidType, Deserialize)]
pub struct Subaccount(pub [u8; 32]);

impl Subaccount {
    pub fn zero() -> Self {
        Self([0; 32])
    }

    pub fn from_vec(bytes: Vec<u8>, field: &str) -> Result<Self, LedgerTransferError> {
        Ok(Self(bytes.try_into().map_err(|bytes: Vec<u8>| {
            LedgerTransferError::DecodeError {
                message: format!("{field} must be 32 bytes, got {}", bytes.len()),
            }
        })?))
    }

    pub fn from_vec_for_index(bytes: Vec<u8>, field: &str) -> Result<Self, IndexError> {
        Ok(Self(bytes.try_into().map_err(|bytes: Vec<u8>| {
            IndexError::DecodeError {
                message: format!("{field} must be 32 bytes, got {}", bytes.len()),
            }
        })?))
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

    pub fn to_icrc_account(&self) -> IcrcAccount {
        IcrcAccount {
            owner: self.owner,
            subaccount: self.subaccount.map(|subaccount| subaccount.0.to_vec()),
        }
    }

    pub fn icp_account_identifier_bytes(&self) -> [u8; 32] {
        icp_account_identifier_bytes(self.owner, self.subaccount)
    }

    pub fn icp_account_identifier_text(&self) -> String {
        hex::encode(self.icp_account_identifier_bytes())
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

pub fn icrc_memo_bytes(memo: Option<Memo>) -> Option<Vec<u8>> {
    memo.map(|memo| memo.0)
}

pub fn memo_to_icp_u64(memo: Option<&Memo>) -> Result<u64, LedgerTransferError> {
    match memo {
        None => Ok(0),
        Some(memo) => {
            let bytes: [u8; 8] = memo
                .0
                .as_slice()
                .try_into()
                .map_err(|_| LedgerTransferError::Unsupported)?;
            Ok(u64::from_le_bytes(bytes))
        }
    }
}

pub fn icp_account_identifier_bytes(owner: Principal, subaccount: Option<Subaccount>) -> [u8; 32] {
    let subaccount = subaccount.unwrap_or_else(Subaccount::zero);
    let mut hasher = Sha224::new();
    hasher.update(b"\x0Aaccount-id");
    hasher.update(owner.as_slice());
    hasher.update(subaccount.0);
    let hash = hasher.finalize();
    let checksum = crc32fast::hash(&hash).to_be_bytes();
    let mut bytes = [0_u8; 32];
    bytes[..4].copy_from_slice(&checksum);
    bytes[4..].copy_from_slice(&hash);
    bytes
}

pub fn icp_account_identifier_text(owner: Principal, subaccount: Option<Subaccount>) -> String {
    hex::encode(icp_account_identifier_bytes(owner, subaccount))
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
    pub account_aliases: Vec<AccountAlias>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct AccountAlias {
    pub account: Account,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IndexScanResult {
    pub transactions: Vec<IndexTransaction>,
    pub last_seen_block: Option<BlockIndex>,
    pub index_tip: Option<BlockIndex>,
    pub archive_required: bool,
    pub page_order: Option<AccountHistoryPageOrder>,
    pub account_balance_e8s: Option<u128>,
    pub num_blocks_synced: Option<BlockIndex>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum AccountHistoryPageOrder {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum AccountHistoryScanPhase {
    AscendingForward,
    DescendingHead,
    DescendingBackfill,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct AccountHistoryCursor {
    pub order: Option<AccountHistoryPageOrder>,
    pub latest_cursor: Option<BlockIndex>,
    pub oldest_cursor: Option<BlockIndex>,
    pub backfill_complete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct AccountHistoryScanStatus {
    pub last_success_timestamp_nanos: Option<u64>,
    pub latest_page_unreadable_count: u64,
    pub invariant_broken_count: u64,
    pub last_observed_newest_tx_id: Option<BlockIndex>,
    pub last_observed_account_balance_e8s: Option<u128>,
    pub num_blocks_synced: Option<BlockIndex>,
    pub page_cap_reached: bool,
    pub lag_suspected: bool,
    pub scan_incomplete: bool,
    pub last_error: Option<String>,
    pub safe_to_continue: bool,
}

impl Default for AccountHistoryScanStatus {
    fn default() -> Self {
        Self {
            last_success_timestamp_nanos: None,
            latest_page_unreadable_count: 0,
            invariant_broken_count: 0,
            last_observed_newest_tx_id: None,
            last_observed_account_balance_e8s: None,
            num_blocks_synced: None,
            page_cap_reached: false,
            lag_suspected: false,
            scan_incomplete: false,
            last_error: None,
            safe_to_continue: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize, Default)]
pub struct AccountHistoryScanState {
    pub cursor: AccountHistoryCursor,
    pub status: AccountHistoryScanStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountHistoryFault {
    IndexUnreadable(String),
    ArchiveRequired(BlockIndex),
    IndexLag {
        requested: BlockIndex,
        tip: Option<BlockIndex>,
    },
    DuplicateReturnedId(BlockIndex),
    NonMonotonicPage(BlockIndex),
    NonProgressingPage(BlockIndex),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountHistoryPageOutcome {
    pub transactions_chronological: Vec<IndexTransaction>,
    pub next_state: AccountHistoryScanState,
    pub phase: AccountHistoryScanPhase,
    pub page_cap_reached: bool,
}

impl AccountHistoryScanState {
    pub fn next_request_start(&self) -> Option<BlockIndex> {
        match self.cursor.order {
            Some(AccountHistoryPageOrder::Descending) => {
                if self.cursor.latest_cursor.is_some() && self.cursor.backfill_complete {
                    None
                } else {
                    self.cursor.oldest_cursor
                }
            }
            Some(AccountHistoryPageOrder::Ascending) | None => self
                .cursor
                .latest_cursor
                .map(|block| BlockIndex(block.0.saturating_add(1))),
        }
    }

    pub fn record_unreadable(&self, message: impl Into<String>) -> Self {
        let mut next = self.clone();
        next.status.latest_page_unreadable_count =
            next.status.latest_page_unreadable_count.saturating_add(1);
        next.status.last_error = Some(message.into());
        next.status.safe_to_continue = true;
        next
    }

    pub fn observe_page(
        &self,
        page: &IndexScanResult,
        requested_start: Option<BlockIndex>,
        requested_limit: u64,
        pages_scanned_this_tick: u64,
        max_pages_per_tick: u64,
        now_nanos: Option<u64>,
    ) -> Result<AccountHistoryPageOutcome, AccountHistoryFault> {
        let mut next = self.clone();
        next.status.last_success_timestamp_nanos =
            now_nanos.or(next.status.last_success_timestamp_nanos);
        next.status.page_cap_reached = pages_scanned_this_tick >= max_pages_per_tick;
        next.status.scan_incomplete = next.status.page_cap_reached;
        next.status.last_error = None;
        next.status.safe_to_continue = true;
        next.status.last_observed_account_balance_e8s = page
            .account_balance_e8s
            .or(next.status.last_observed_account_balance_e8s);
        next.status.num_blocks_synced = page.num_blocks_synced.or(next.status.num_blocks_synced);

        if page.archive_required {
            let from = requested_start
                .or(self.cursor.oldest_cursor)
                .or(self.cursor.latest_cursor)
                .unwrap_or(BlockIndex(0));
            return Err(AccountHistoryFault::ArchiveRequired(from));
        }

        let observed_order = page
            .page_order
            .or_else(|| detect_account_history_page_order(&page.transactions));
        let order = match (self.cursor.order, observed_order) {
            (Some(existing), Some(observed))
                if existing != observed && page.transactions.len() > 1 =>
            {
                next.status.invariant_broken_count =
                    next.status.invariant_broken_count.saturating_add(1);
                return Err(AccountHistoryFault::NonMonotonicPage(
                    page.transactions[1].block_index,
                ));
            }
            (Some(existing), _) => existing,
            (None, Some(observed)) => observed,
            (None, None) => {
                let single = page.transactions.first().map(|tx| tx.block_index);
                if single
                    .zip(self.cursor.oldest_cursor.or(self.cursor.latest_cursor))
                    .map(|(tx_id, cursor)| tx_id < cursor)
                    .unwrap_or(false)
                {
                    AccountHistoryPageOrder::Descending
                } else {
                    AccountHistoryPageOrder::Ascending
                }
            }
        };
        next.cursor.order = Some(order);

        if order == AccountHistoryPageOrder::Ascending {
            if let (Some(requested), Some(tip)) =
                (requested_start, page.index_tip.or(page.num_blocks_synced))
            {
                if tip < requested {
                    next.status.lag_suspected = true;
                    next.status.last_error = Some(format!(
                        "index lag: requested {}, tip {}",
                        requested.0, tip.0
                    ));
                    return Err(AccountHistoryFault::IndexLag {
                        requested,
                        tip: Some(tip),
                    });
                }
            }
        }

        validate_account_history_ids(&page.transactions, order)?;

        let newest = page.transactions.iter().map(|tx| tx.block_index).max();
        next.status.last_observed_newest_tx_id = newest.or(next.status.last_observed_newest_tx_id);

        let short_page = page.transactions.len() < requested_limit as usize;
        let (phase, mut process) = match order {
            AccountHistoryPageOrder::Ascending => {
                let mut skipped_cursor = false;
                let mut process = Vec::new();
                for tx in &page.transactions {
                    if let Some(cursor) = self.cursor.latest_cursor {
                        if tx.block_index == cursor && !skipped_cursor {
                            skipped_cursor = true;
                            continue;
                        }
                        if tx.block_index <= cursor {
                            return Err(AccountHistoryFault::NonProgressingPage(tx.block_index));
                        }
                    }
                    process.push(tx.clone());
                }
                if let Some(max_seen) = process.iter().map(|tx| tx.block_index).max() {
                    next.cursor.latest_cursor = Some(max_seen);
                    next.cursor.oldest_cursor = next.cursor.oldest_cursor.or(Some(max_seen));
                }
                next.cursor.backfill_complete = short_page;
                (AccountHistoryScanPhase::AscendingForward, process)
            }
            AccountHistoryPageOrder::Descending => {
                if let (Some(latest), true) =
                    (self.cursor.latest_cursor, self.cursor.backfill_complete)
                {
                    let mut process = Vec::new();
                    for tx in &page.transactions {
                        if tx.block_index > latest {
                            process.push(tx.clone());
                        } else {
                            break;
                        }
                    }
                    if process.is_empty() {
                        next.status.scan_incomplete = false;
                    }
                    if let Some(max_seen) = process.iter().map(|tx| tx.block_index).max() {
                        next.cursor.latest_cursor = Some(latest.max(max_seen));
                    }
                    (AccountHistoryScanPhase::DescendingHead, process)
                } else {
                    let mut process = Vec::new();
                    for tx in &page.transactions {
                        match self.cursor.oldest_cursor {
                            Some(oldest) if tx.block_index >= oldest => continue,
                            _ => process.push(tx.clone()),
                        }
                    }
                    if process.is_empty() && !page.transactions.is_empty() && !short_page {
                        return Err(AccountHistoryFault::NonProgressingPage(
                            page.transactions.last().expect("non-empty").block_index,
                        ));
                    }
                    if let Some(max_seen) = process.iter().map(|tx| tx.block_index).max() {
                        next.cursor.latest_cursor = Some(
                            next.cursor
                                .latest_cursor
                                .map_or(max_seen, |old| old.max(max_seen)),
                        );
                    }
                    if let Some(min_seen) = process.iter().map(|tx| tx.block_index).min() {
                        next.cursor.oldest_cursor = Some(
                            next.cursor
                                .oldest_cursor
                                .map_or(min_seen, |old| old.min(min_seen)),
                        );
                    }
                    if short_page || page.transactions.is_empty() {
                        next.cursor.backfill_complete = true;
                        next.status.scan_incomplete = false;
                    } else {
                        next.status.scan_incomplete = true;
                    }
                    (AccountHistoryScanPhase::DescendingBackfill, process)
                }
            }
        };

        process.sort_by_key(|tx| tx.block_index);

        Ok(AccountHistoryPageOutcome {
            transactions_chronological: process,
            next_state: next,
            phase,
            page_cap_reached: pages_scanned_this_tick >= max_pages_per_tick,
        })
    }
}

pub fn detect_account_history_page_order(
    transactions: &[IndexTransaction],
) -> Option<AccountHistoryPageOrder> {
    if transactions.len() < 2 {
        return None;
    }
    let first = transactions.first()?.block_index;
    let last = transactions.last()?.block_index;
    if first < last {
        Some(AccountHistoryPageOrder::Ascending)
    } else if first > last {
        Some(AccountHistoryPageOrder::Descending)
    } else {
        None
    }
}

fn validate_account_history_ids(
    transactions: &[IndexTransaction],
    order: AccountHistoryPageOrder,
) -> Result<(), AccountHistoryFault> {
    let mut previous = None;
    for tx in transactions {
        if let Some(previous) = previous {
            match order {
                AccountHistoryPageOrder::Ascending if tx.block_index == previous => {
                    return Err(AccountHistoryFault::DuplicateReturnedId(tx.block_index));
                }
                AccountHistoryPageOrder::Ascending if tx.block_index < previous => {
                    return Err(AccountHistoryFault::NonMonotonicPage(tx.block_index));
                }
                AccountHistoryPageOrder::Descending if tx.block_index == previous => {
                    return Err(AccountHistoryFault::DuplicateReturnedId(tx.block_index));
                }
                AccountHistoryPageOrder::Descending if tx.block_index > previous => {
                    return Err(AccountHistoryFault::NonMonotonicPage(tx.block_index));
                }
                _ => {}
            }
        }
        previous = Some(tx.block_index);
    }
    Ok(())
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
        value.to_icrc_account()
    }
}

impl TryFrom<IcrcAccount> for Account {
    type Error = LedgerTransferError;

    fn try_from(value: IcrcAccount) -> Result<Self, Self::Error> {
        let subaccount = match value.subaccount {
            Some(bytes) => Some(Subaccount::from_vec(bytes, "subaccount")?),
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
            memo: icrc_memo_bytes(value.memo),
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
    pub from_subaccount: Option<Vec<u8>>,
    pub to: Vec<u8>,
    pub created_at_time: Option<IcpTimeStamp>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcpTransferFeeArgs {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcpTransferFee {
    pub transfer_fee: IcpTokens,
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
    TemporarilyUnavailable,
    GenericError { error_code: u64, message: String },
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
    let amount = u128_to_icp_tokens(request.amount_e8s, "amount")?;
    let fee = u128_to_icp_tokens(request.fee_e8s.unwrap_or(default_fee_e8s), "fee")?;
    let memo = memo_to_icp_u64(request.memo.as_ref())?;
    Ok(IcpTransferArgs {
        memo,
        amount,
        fee,
        from_subaccount: request
            .from_subaccount
            .map(|subaccount| subaccount.0.to_vec()),
        to: to_account_identifier,
        created_at_time: request
            .created_at_time
            .map(|timestamp_nanos| IcpTimeStamp { timestamp_nanos }),
    })
}

pub fn icp_transfer_args_for_request(
    request: LedgerTransferRequest,
    default_fee_e8s: u128,
) -> Result<IcpTransferArgs, LedgerTransferError> {
    let to = request.to.icp_account_identifier_bytes().to_vec();
    icp_transfer_args(request, to, default_fee_e8s)
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
        Err(IcpTransferError::TemporarilyUnavailable) => {
            Err(LedgerTransferError::TemporarilyUnavailable)
        }
        Err(IcpTransferError::GenericError {
            error_code,
            message,
        }) => Err(LedgerTransferError::GenericError {
            error_code,
            message,
        }),
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
pub struct IcrcIndexNgGetTransactionsResult {
    pub balance: Nat,
    pub transactions: Vec<IcrcIndexNgTransactionWithId>,
    pub oldest_tx_id: Option<Nat>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcrcIndexNgTransactionWithId {
    pub id: Nat,
    pub transaction: IcrcIndexNgTransaction,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcrcIndexNgTransaction {
    pub burn: Option<IcrcIndexNgTransfer>,
    pub mint: Option<IcrcIndexNgTransfer>,
    pub approve: Option<IcrcIndexNgApprove>,
    pub transfer: Option<IcrcIndexNgTransfer>,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcrcIndexNgTransfer {
    pub from: IcrcAccount,
    pub to: IcrcAccount,
    pub amount: Nat,
    pub fee: Option<Nat>,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcrcIndexNgApprove {
    pub from: IcrcAccount,
    pub spender: IcrcAccount,
    pub amount: Nat,
    pub expected_allowance: Option<Nat>,
    pub expires_at: Option<u64>,
    pub fee: Option<Nat>,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: Option<u64>,
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

fn nat_to_u128_index(value: &Nat, field: &str) -> Result<u128, IndexError> {
    value
        .0
        .to_str_radix(10)
        .parse::<u128>()
        .map_err(|err| IndexError::DecodeError {
            message: format!("{field} does not fit in u128: {err}"),
        })
}

fn account_from_icrc_for_index(value: IcrcAccount) -> Result<Account, IndexError> {
    let subaccount = value
        .subaccount
        .map(|bytes| Subaccount::from_vec_for_index(bytes, "subaccount"))
        .transpose()?;
    Ok(Account {
        owner: value.owner,
        subaccount,
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
            let result = IndexScanResult {
                transactions,
                last_seen_block,
                index_tip: page
                    .tip
                    .as_ref()
                    .map(|tip| nat_to_u64_index(tip, "index tip").map(BlockIndex))
                    .transpose()?,
                archive_required: page.archive_required,
                page_order: Some(AccountHistoryPageOrder::Ascending),
                account_balance_e8s: None,
                num_blocks_synced: page
                    .tip
                    .as_ref()
                    .map(|tip| nat_to_u64_index(tip, "num blocks synced").map(BlockIndex))
                    .transpose()?,
            };
            result.validate_monotonic()?;
            Ok(result)
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

fn icrc_index_ng_transfer_block(
    block_index: BlockIndex,
    timestamp_nanos: u64,
    transfer: IcrcIndexNgTransfer,
    operation_kind: LedgerOperationKind,
) -> Result<LedgerBlock, IndexError> {
    Ok(LedgerBlock {
        block_index,
        timestamp_nanos,
        from: Some(account_from_icrc_for_index(transfer.from)?),
        to: Some(account_from_icrc_for_index(transfer.to)?),
        amount_e8s: nat_to_u128_index(&transfer.amount, "transaction amount")?,
        fee_e8s: transfer
            .fee
            .as_ref()
            .map(|fee| nat_to_u128_index(fee, "transaction fee"))
            .transpose()?,
        memo: transfer.memo.map(Memo),
        operation_kind,
    })
}

fn icrc_index_ng_transaction_block(
    tx: IcrcIndexNgTransactionWithId,
) -> Result<IndexTransaction, IndexError> {
    let block_index = BlockIndex(nat_to_u64_index(&tx.id, "transaction id")?);
    let timestamp_nanos = tx.transaction.timestamp;
    let block = if let Some(transfer) = tx.transaction.transfer {
        icrc_index_ng_transfer_block(
            block_index,
            timestamp_nanos,
            transfer,
            LedgerOperationKind::Transfer,
        )?
    } else if let Some(mint) = tx.transaction.mint {
        let mut block = icrc_index_ng_transfer_block(
            block_index,
            timestamp_nanos,
            mint,
            LedgerOperationKind::Mint,
        )?;
        block.from = None;
        block
    } else if let Some(burn) = tx.transaction.burn {
        let mut block = icrc_index_ng_transfer_block(
            block_index,
            timestamp_nanos,
            burn,
            LedgerOperationKind::Burn,
        )?;
        block.to = None;
        block
    } else if let Some(approve) = tx.transaction.approve {
        LedgerBlock {
            block_index,
            timestamp_nanos,
            from: Some(account_from_icrc_for_index(approve.from)?),
            to: Some(account_from_icrc_for_index(approve.spender)?),
            amount_e8s: nat_to_u128_index(&approve.amount, "approval amount")?,
            fee_e8s: approve
                .fee
                .as_ref()
                .map(|fee| nat_to_u128_index(fee, "approval fee"))
                .transpose()?,
            memo: approve.memo.map(Memo),
            operation_kind: LedgerOperationKind::Approve,
        }
    } else {
        return Err(IndexError::DecodeError {
            message: format!(
                "ICRC index-ng transaction {} has no operation",
                block_index.0
            ),
        });
    };

    Ok(IndexTransaction {
        block_index,
        transaction: block,
    })
}

pub fn map_icrc_index_ng_result(
    result: Result<IcrcIndexNgGetTransactionsResult, String>,
) -> Result<IndexScanResult, IndexError> {
    match result {
        Ok(page) => {
            let mut transactions = Vec::with_capacity(page.transactions.len());
            for tx in page.transactions {
                transactions.push(icrc_index_ng_transaction_block(tx)?);
            }
            let last_seen_block = transactions.iter().map(|tx| tx.block_index).max();
            let result = IndexScanResult {
                transactions,
                last_seen_block,
                index_tip: None,
                archive_required: false,
                page_order: Some(AccountHistoryPageOrder::Ascending),
                account_balance_e8s: Some(nat_to_u128_index(&page.balance, "account balance")?),
                num_blocks_synced: None,
            };
            result.validate_monotonic()?;
            Ok(result)
        }
        Err(message) if message.contains("archive") => Err(IndexError::ArchiveRequired {
            from: BlockIndex(0),
        }),
        Err(message) => Err(IndexError::DecodeError { message }),
    }
}

pub fn decode_icrc_index_response_bytes(
    bytes: &[u8],
) -> Result<Result<IcrcIndexGetAccountTransactionsResult, IcrcIndexError>, IndexError> {
    if let Ok(page) = Decode!(bytes, IcrcIndexGetAccountTransactionsResult) {
        return Ok(Ok(page));
    }

    Decode!(
        bytes,
        Result<IcrcIndexGetAccountTransactionsResult, IcrcIndexError>
    )
    .map_err(|err| IndexError::DecodeError {
        message: format!("{err:?}"),
    })
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IndexArchiveCallback {
    pub canister_id: Principal,
    pub method: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IndexArchiveRange {
    pub start: BlockIndex,
    pub limit: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IndexArchiveRequest {
    pub range: IndexArchiveRange,
    pub callback: IndexArchiveCallback,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IndexArchiveTraversal {
    pub requested: IndexArchiveRange,
    pub completed: Vec<IndexArchiveRange>,
    pub incomplete: bool,
}

impl IndexArchiveTraversal {
    pub fn validate_no_skipped_ranges(&self) -> Result<(), IndexError> {
        let mut next = self.requested.start.0;
        let end = self
            .requested
            .start
            .0
            .checked_add(self.requested.limit)
            .ok_or(IndexError::MissingBlock {
                block_index: self.requested.start,
            })?;
        for range in &self.completed {
            if range.start.0 != next {
                return Err(IndexError::MissingBlock {
                    block_index: BlockIndex(next),
                });
            }
            next = range
                .start
                .0
                .checked_add(range.limit)
                .ok_or(IndexError::MissingBlock {
                    block_index: range.start,
                })?;
            if next > end {
                return Err(IndexError::MissingBlock {
                    block_index: BlockIndex(end),
                });
            }
        }
        if self.incomplete || next < end {
            return Err(IndexError::ArchiveRequired {
                from: BlockIndex(next),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcpIndexTokens {
    pub e8s: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcpIndexTimeStamp {
    pub timestamp_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum IcpIndexOperation {
    Approve {
        fee: IcpIndexTokens,
        from: String,
        allowance: IcpIndexTokens,
        expires_at: Option<IcpIndexTimeStamp>,
        spender: String,
        expected_allowance: Option<IcpIndexTokens>,
    },
    Burn {
        from: String,
        amount: IcpIndexTokens,
        spender: Option<String>,
    },
    Mint {
        to: String,
        amount: IcpIndexTokens,
    },
    Transfer {
        to: String,
        fee: IcpIndexTokens,
        from: String,
        amount: IcpIndexTokens,
        spender: Option<String>,
    },
    TransferFrom {
        to: String,
        fee: IcpIndexTokens,
        from: String,
        amount: IcpIndexTokens,
        spender: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcpIndexTransaction {
    pub memo: u64,
    pub icrc1_memo: Option<Vec<u8>>,
    pub operation: IcpIndexOperation,
    pub created_at_time: Option<IcpIndexTimeStamp>,
    pub timestamp: Option<IcpIndexTimeStamp>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcpIndexTransactionWithId {
    pub id: u64,
    pub transaction: IcpIndexTransaction,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcpIndexGetAccountIdentifierTransactionsArgs {
    pub max_results: u64,
    pub start: Option<u64>,
    pub account_identifier: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcpIndexGetAccountIdentifierTransactionsError {
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcpIndexGetAccountIdentifierTransactionsResponse {
    pub balance: u64,
    pub transactions: Vec<IcpIndexTransactionWithId>,
    pub oldest_tx_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum IcpIndexGetAccountIdentifierTransactionsResult {
    Ok(IcpIndexGetAccountIdentifierTransactionsResponse),
    Err(IcpIndexGetAccountIdentifierTransactionsError),
}

impl TryFrom<IndexScanRequest> for IcpIndexGetAccountIdentifierTransactionsArgs {
    type Error = IndexError;

    fn try_from(value: IndexScanRequest) -> Result<Self, Self::Error> {
        let account = value.account_filter.ok_or(IndexError::Unsupported)?;
        Ok(Self {
            max_results: value.limit,
            start: value.start.map(|block| block.0),
            account_identifier: account.icp_account_identifier_text(),
        })
    }
}

fn matching_icp_account(
    legacy_account_identifier: &str,
    account_filter: Option<&Account>,
    account_aliases: &[AccountAlias],
) -> Option<Account> {
    account_filter
        .filter(|account| account.icp_account_identifier_text() == legacy_account_identifier)
        .cloned()
        .or_else(|| {
            account_aliases
                .iter()
                .find(|alias| {
                    alias.account.icp_account_identifier_text() == legacy_account_identifier
                })
                .map(|alias| alias.account.clone())
        })
}

fn icp_index_memo(transaction: &IcpIndexTransaction) -> Memo {
    transaction.icrc1_memo.clone().map(Memo).unwrap_or_else(|| {
        if transaction.memo == 0 {
            Memo(Vec::new())
        } else {
            Memo(transaction.memo.to_le_bytes().to_vec())
        }
    })
}

fn map_icp_index_transaction(
    account_filter: Option<&Account>,
    account_aliases: &[AccountAlias],
    tx: IcpIndexTransactionWithId,
) -> IndexTransaction {
    let timestamp_nanos = tx
        .transaction
        .timestamp
        .as_ref()
        .or(tx.transaction.created_at_time.as_ref())
        .map(|timestamp| timestamp.timestamp_nanos)
        .unwrap_or(0);
    let memo = Some(icp_index_memo(&tx.transaction));
    let block_index = BlockIndex(tx.id);
    let transaction = match tx.transaction.operation {
        IcpIndexOperation::Transfer {
            to,
            fee,
            from,
            amount,
            ..
        }
        | IcpIndexOperation::TransferFrom {
            to,
            fee,
            from,
            amount,
            ..
        } => LedgerBlock {
            block_index,
            timestamp_nanos,
            from: matching_icp_account(&from, account_filter, account_aliases),
            to: matching_icp_account(&to, account_filter, account_aliases),
            amount_e8s: amount.e8s.into(),
            fee_e8s: Some(fee.e8s.into()),
            memo,
            operation_kind: LedgerOperationKind::Transfer,
        },
        IcpIndexOperation::Mint { to, amount } => LedgerBlock {
            block_index,
            timestamp_nanos,
            from: None,
            to: matching_icp_account(&to, account_filter, account_aliases),
            amount_e8s: amount.e8s.into(),
            fee_e8s: None,
            memo,
            operation_kind: LedgerOperationKind::Mint,
        },
        IcpIndexOperation::Burn { from, amount, .. } => LedgerBlock {
            block_index,
            timestamp_nanos,
            from: matching_icp_account(&from, account_filter, account_aliases),
            to: None,
            amount_e8s: amount.e8s.into(),
            fee_e8s: None,
            memo,
            operation_kind: LedgerOperationKind::Burn,
        },
        IcpIndexOperation::Approve { fee, from, .. } => LedgerBlock {
            block_index,
            timestamp_nanos,
            from: matching_icp_account(&from, account_filter, account_aliases),
            to: None,
            amount_e8s: 0,
            fee_e8s: Some(fee.e8s.into()),
            memo,
            operation_kind: LedgerOperationKind::Approve,
        },
    };

    IndexTransaction {
        block_index,
        transaction,
    }
}

pub fn map_icp_index_result(
    account_filter: Option<&Account>,
    account_aliases: &[AccountAlias],
    _request_start: Option<BlockIndex>,
    result: IcpIndexGetAccountIdentifierTransactionsResult,
) -> Result<IndexScanResult, IndexError> {
    match result {
        IcpIndexGetAccountIdentifierTransactionsResult::Ok(response) => {
            let mut transactions = response
                .transactions
                .into_iter()
                .map(|tx| map_icp_index_transaction(account_filter, account_aliases, tx))
                .collect::<Vec<_>>();
            let mut previous = None;
            for tx in &transactions {
                if let Some(previous) = previous {
                    if tx.block_index >= previous {
                        return Err(IndexError::MissingBlock {
                            block_index: tx.block_index,
                        });
                    }
                }
                previous = Some(tx.block_index);
            }
            transactions.reverse();
            let result = IndexScanResult {
                last_seen_block: transactions.iter().map(|tx| tx.block_index).max(),
                index_tip: None,
                archive_required: false,
                page_order: Some(AccountHistoryPageOrder::Descending),
                account_balance_e8s: Some(response.balance.into()),
                num_blocks_synced: None,
                transactions,
            };
            result.validate_monotonic()?;
            Ok(result)
        }
        IcpIndexGetAccountIdentifierTransactionsResult::Err(err)
            if err.message.contains("archive") =>
        {
            Err(IndexError::ArchiveRequired {
                from: BlockIndex(0),
            })
        }
        IcpIndexGetAccountIdentifierTransactionsResult::Err(err) => Err(IndexError::DecodeError {
            message: err.message,
        }),
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
    pub expected_operation_kind: LedgerOperationKind,
    pub actual_operation_kind: LedgerOperationKind,
    pub expected_ledger_kind: Option<LedgerKind>,
    pub actual_ledger_kind: Option<LedgerKind>,
}

pub fn duplicate_matches_expected(
    expected: &LedgerTransferRequest,
    duplicate_block: &LedgerBlock,
) -> Result<BlockIndex, Box<DuplicateProof>> {
    duplicate_matches_expected_for_ledger(expected, duplicate_block, None, None)
}

pub fn duplicate_matches_expected_for_ledger(
    expected: &LedgerTransferRequest,
    duplicate_block: &LedgerBlock,
    expected_ledger_kind: Option<LedgerKind>,
    actual_ledger_kind: Option<LedgerKind>,
) -> Result<BlockIndex, Box<DuplicateProof>> {
    if duplicate_block.amount_e8s == expected.amount_e8s
        && duplicate_block.to.as_ref() == Some(&expected.to)
        && duplicate_block.memo == expected.memo
        && duplicate_block.operation_kind == LedgerOperationKind::Transfer
        && expected_ledger_kind == actual_ledger_kind
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
            expected_operation_kind: LedgerOperationKind::Transfer,
            actual_operation_kind: duplicate_block.operation_kind,
            expected_ledger_kind,
            actual_ledger_kind,
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
pub struct IcpLedgerCanisterClient {
    pub canister: Principal,
    pub default_fee_e8s: u128,
}

#[cfg(target_family = "wasm")]
impl LedgerTransferClient for IcpLedgerCanisterClient {
    fn transfer<'a>(
        &'a self,
        request: LedgerTransferRequest,
    ) -> Pin<Box<dyn Future<Output = Result<LedgerTransferSuccess, LedgerTransferError>> + 'a>>
    {
        Box::pin(async move {
            let arg = icp_transfer_args_for_request(request, self.default_fee_e8s)?;
            let response = ic_cdk::call::Call::bounded_wait(self.canister, "transfer")
                .with_arg(arg)
                .await
                .map_err(|err| LedgerTransferError::CanisterCallFailed {
                    method: "transfer".to_string(),
                    message: format!("{err:?}"),
                })?;
            let (result,) = response
                .candid_tuple::<(Result<u64, IcpTransferError>,)>()
                .map_err(|err| LedgerTransferError::DecodeError {
                    message: format!("{err:?}"),
                })?;
            map_icp_transfer_result(result)
        })
    }

    fn fee<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<u128, LedgerQueryError>> + 'a>> {
        Box::pin(async move {
            let response = ic_cdk::call::Call::bounded_wait(self.canister, "transfer_fee")
                .with_arg(IcpTransferFeeArgs {})
                .await
                .map_err(|err| LedgerQueryError::CanisterCallFailed {
                    method: "transfer_fee".to_string(),
                    message: format!("{err:?}"),
                })?;
            let (fee,) = response
                .candid_tuple::<(IcpTransferFee,)>()
                .map_err(|err| LedgerQueryError::DecodeError {
                    message: format!("{err:?}"),
                })?;
            Ok(fee.transfer_fee.e8s.into())
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
            match response.candid_tuple::<(IcrcIndexGetAccountTransactionsResult,)>() {
                Ok((page,)) => return map_icrc_index_result(Ok(page)),
                Err(direct_err) => {
                    match response.candid_tuple::<(
                        Result<IcrcIndexGetAccountTransactionsResult, IcrcIndexError>,
                    )>() {
                        Ok((result,)) => return map_icrc_index_result(result),
                        Err(result_err) => {
                            match response
                                .candid_tuple::<(Result<IcrcIndexNgGetTransactionsResult, String>,)>()
                            {
                                Ok((result,)) => map_icrc_index_ng_result(result),
                                Err(ng_err) => Err(IndexError::DecodeError {
                                    message: format!(
                                        "direct ICRC index response decode failed: {direct_err:?}; result response decode failed: {result_err:?}; index-ng response decode failed: {ng_err:?}"
                                    ),
                                }),
                            }
                        }
                    }
                }
            }
        })
    }

    fn get_tip<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<BlockIndex>, IndexError>> + 'a>> {
        Box::pin(async move { Err(IndexError::Unsupported) })
    }
}

#[cfg(target_family = "wasm")]
#[derive(Clone, Copy, Debug)]
pub struct IcpIndexCanisterClient {
    pub canister: Principal,
}

#[cfg(target_family = "wasm")]
impl LedgerIndexClient for IcpIndexCanisterClient {
    fn get_account_transactions<'a>(
        &'a self,
        request: IndexScanRequest,
    ) -> Pin<Box<dyn Future<Output = Result<IndexScanResult, IndexError>> + 'a>> {
        Box::pin(async move {
            let account_filter = request.account_filter.clone();
            let account_aliases = request.account_aliases.clone();
            let arg = IcpIndexGetAccountIdentifierTransactionsArgs::try_from(request)?;
            let request_start = arg.start.map(BlockIndex);
            let response = ic_cdk::call::Call::bounded_wait(
                self.canister,
                "get_account_identifier_transactions",
            )
            .with_arg(arg)
            .await
            .map_err(|err| IndexError::CanisterCallFailed {
                method: "get_account_identifier_transactions".to_string(),
                message: format!("{err:?}"),
            })?;
            let (result,) = response
                .candid_tuple::<(IcpIndexGetAccountIdentifierTransactionsResult,)>()
                .map_err(|err| IndexError::DecodeError {
                    message: format!("{err:?}"),
                })?;
            map_icp_index_result(
                account_filter.as_ref(),
                &account_aliases,
                request_start,
                result,
            )
        })
    }

    fn get_tip<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<BlockIndex>, IndexError>> + 'a>> {
        Box::pin(async move { Err(IndexError::Unsupported) })
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

    fn icp_index_mint_tx(id: u64) -> IcpIndexTransactionWithId {
        IcpIndexTransactionWithId {
            id,
            transaction: IcpIndexTransaction {
                memo: id,
                icrc1_memo: None,
                operation: IcpIndexOperation::Mint {
                    to: "mint-destination".to_string(),
                    amount: IcpIndexTokens { e8s: id },
                },
                created_at_time: None,
                timestamp: Some(IcpIndexTimeStamp {
                    timestamp_nanos: id,
                }),
            },
        }
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

    fn icp_request() -> LedgerTransferRequest {
        LedgerTransferRequest {
            memo: Some(Memo(42_u64.to_le_bytes().to_vec())),
            ..request()
        }
    }

    fn block(block_index: u64) -> LedgerBlock {
        LedgerBlock {
            block_index: BlockIndex(block_index),
            timestamp_nanos: block_index,
            from: Some(account()),
            to: Some(account()),
            amount_e8s: 7,
            fee_e8s: Some(10),
            memo: Some(Memo::from("idx")),
            operation_kind: LedgerOperationKind::Transfer,
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
    fn icrc_transfer_arg_preserves_intentional_omitted_fee() {
        let mut req = request();
        req.fee_e8s = None;
        let arg = IcrcTransferArg::from(req);
        assert_eq!(arg.fee, None);
    }

    #[test]
    fn icrc_account_conversion_rejects_invalid_subaccount_length() {
        let err = Account::try_from(IcrcAccount {
            owner: principal(),
            subaccount: Some(vec![1; 31]),
        })
        .unwrap_err();
        assert!(matches!(err, LedgerTransferError::DecodeError { .. }));
    }

    #[test]
    fn icp_account_identifier_matches_known_vectors() {
        let owner = Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap();
        assert_eq!(
            icp_account_identifier_text(owner, None),
            "f3a58ea11bc128ab8a455dd7bce0a29b0a20f400625d1a46871fbfe82efed38d"
        );

        let mut subaccount = [0_u8; 32];
        subaccount[31] = 1;
        assert_eq!(
            icp_account_identifier_text(owner, Some(Subaccount(subaccount))),
            "439a264f2ce4d3aeeb10b8ad65dc3610512ef3c6c4bc8c2985a15ce8cc2ce3c0"
        );
    }

    #[test]
    fn memo_conversion_is_explicit_for_icp_and_icrc() {
        assert_eq!(
            memo_to_icp_u64(Some(&Memo(42_u64.to_le_bytes().to_vec()))),
            Ok(42)
        );
        assert_eq!(memo_to_icp_u64(None), Ok(0));
        assert_eq!(
            memo_to_icp_u64(Some(&Memo::from("lossy"))),
            Err(LedgerTransferError::Unsupported)
        );
        assert_eq!(
            icrc_memo_bytes(Some(Memo::from("bytes"))),
            Some(b"bytes".to_vec())
        );
    }

    #[test]
    fn icp_transfer_args_derive_destination_and_preserve_explicit_fee_amounts() {
        let req = icp_request();
        let expected_to = req.to.icp_account_identifier_bytes().to_vec();
        let args = icp_transfer_args_for_request(req, 10_000).unwrap();
        assert_eq!(args.to, expected_to);
        assert_eq!(args.amount.e8s, 123);
        assert_eq!(args.fee.e8s, 10);
        assert_eq!(args.memo, 42);
        assert_eq!(args.from_subaccount, Some(vec![1; 32]));
    }

    #[test]
    fn icp_transfer_args_fill_configured_default_only_when_fee_is_omitted() {
        let mut req = icp_request();
        req.fee_e8s = None;
        let args = icp_transfer_args_for_request(req, 10_000).unwrap();
        assert_eq!(args.fee.e8s, 10_000);
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
    fn transfer_success_and_temporal_errors_map_for_icrc_and_icp() {
        assert_eq!(
            map_icrc_transfer_result(Ok(Nat::from(12_u64))),
            Ok(LedgerTransferSuccess {
                block_index: BlockIndex(12)
            })
        );
        assert_eq!(
            map_icp_transfer_result(Ok(13)),
            Ok(LedgerTransferSuccess {
                block_index: BlockIndex(13)
            })
        );
        assert_eq!(
            map_icrc_transfer_result(Err(IcrcTransferError::TooOld)),
            Err(LedgerTransferError::TooOld)
        );
        assert_eq!(
            map_icrc_transfer_result(Err(IcrcTransferError::CreatedInFuture { ledger_time: 9 })),
            Err(LedgerTransferError::CreatedInFuture { ledger_time: 9 })
        );
        assert_eq!(
            map_icp_transfer_result(Err(IcpTransferError::TxTooOld {
                allowed_window_nanos: 1
            })),
            Err(LedgerTransferError::TooOld)
        );
        assert_eq!(
            map_icp_transfer_result(Err(IcpTransferError::TxCreatedInFuture)),
            Err(LedgerTransferError::CreatedInFuture { ledger_time: 0 })
        );
    }

    #[test]
    fn temporary_and_generic_errors_map_for_icrc_and_icp() {
        assert_eq!(
            map_icrc_transfer_result(Err(IcrcTransferError::TemporarilyUnavailable)),
            Err(LedgerTransferError::TemporarilyUnavailable)
        );
        assert_eq!(
            map_icp_transfer_result(Err(IcpTransferError::TemporarilyUnavailable)),
            Err(LedgerTransferError::TemporarilyUnavailable)
        );
        assert_eq!(
            map_icp_transfer_result(Err(IcpTransferError::GenericError {
                error_code: 5,
                message: "busy".to_string()
            })),
            Err(LedgerTransferError::GenericError {
                error_code: 5,
                message: "busy".to_string()
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
    fn nat_overflow_maps_to_decode_errors() {
        let too_large = Nat::from(u128::MAX) + Nat::from(1_u8);
        assert!(matches!(
            map_icrc_transfer_result(Ok(too_large.clone())),
            Err(LedgerTransferError::DecodeError { .. })
        ));
        assert!(matches!(
            map_icrc_transfer_result(Err(IcrcTransferError::BadFee {
                expected_fee: too_large.clone()
            })),
            Err(LedgerTransferError::DecodeError { .. })
        ));
        assert!(matches!(
            map_icrc_transfer_result(Err(IcrcTransferError::GenericError {
                error_code: too_large,
                message: "overflow".to_string()
            })),
            Err(LedgerTransferError::DecodeError { .. })
        ));
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
    fn duplicate_transfer_proof_checks_operation_and_ledger_kind_when_available() {
        let req = request();
        let mut duplicate = LedgerBlock {
            block_index: BlockIndex(9),
            timestamp_nanos: 0,
            from: None,
            to: Some(req.to.clone()),
            amount_e8s: req.amount_e8s,
            fee_e8s: req.fee_e8s,
            memo: req.memo.clone(),
            operation_kind: LedgerOperationKind::Mint,
        };
        assert!(duplicate_matches_expected(&req, &duplicate).is_err());

        duplicate.operation_kind = LedgerOperationKind::Transfer;
        assert_eq!(
            duplicate_matches_expected_for_ledger(
                &req,
                &duplicate,
                Some(LedgerKind::IcpLedger),
                Some(LedgerKind::IcpLedger),
            ),
            Ok(BlockIndex(9))
        );
        assert!(duplicate_matches_expected_for_ledger(
            &req,
            &duplicate,
            Some(LedgerKind::IcpLedger),
            Some(LedgerKind::IoLedger),
        )
        .is_err());
    }

    #[test]
    fn index_cursor_keeps_empty_page_cursor_unchanged() {
        let result = IndexScanResult {
            transactions: vec![],
            last_seen_block: None,
            index_tip: Some(BlockIndex(10)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
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
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
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
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
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
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            result.next_cursor(Some(BlockIndex(50))),
            Err(IndexError::ArchiveRequired {
                from: BlockIndex(50)
            })
        );
    }

    fn index_tx(id: u64) -> IndexTransaction {
        IndexTransaction {
            block_index: BlockIndex(id),
            transaction: block(id),
        }
    }

    fn scan_page(ids: &[u64], order: Option<AccountHistoryPageOrder>) -> IndexScanResult {
        IndexScanResult {
            transactions: ids.iter().copied().map(index_tx).collect(),
            last_seen_block: ids.iter().copied().max().map(BlockIndex),
            index_tip: ids.iter().copied().max().map(BlockIndex),
            archive_required: false,
            page_order: order,
            account_balance_e8s: Some(123),
            num_blocks_synced: ids.iter().copied().max().map(BlockIndex),
        }
    }

    #[test]
    fn account_history_scan_detects_page_order_and_single_item_conservatively() {
        assert_eq!(
            detect_account_history_page_order(&scan_page(&[1, 3], None).transactions),
            Some(AccountHistoryPageOrder::Ascending)
        );
        assert_eq!(
            detect_account_history_page_order(&scan_page(&[3, 1], None).transactions),
            Some(AccountHistoryPageOrder::Descending)
        );
        assert_eq!(
            detect_account_history_page_order(&scan_page(&[3], None).transactions),
            None
        );

        let outcome = AccountHistoryScanState::default()
            .observe_page(&scan_page(&[3], None), None, 10, 1, 10, Some(7))
            .unwrap();
        assert_eq!(
            outcome.next_state.cursor.order,
            Some(AccountHistoryPageOrder::Ascending)
        );
    }

    #[test]
    fn descending_scan_processes_pages_chronologically_and_tracks_two_cursors() {
        let state = AccountHistoryScanState::default();
        let page = scan_page(&[30, 20, 10], Some(AccountHistoryPageOrder::Descending));
        let outcome = state.observe_page(&page, None, 3, 1, 10, Some(1)).unwrap();
        assert_eq!(outcome.phase, AccountHistoryScanPhase::DescendingBackfill);
        assert_eq!(
            outcome
                .transactions_chronological
                .iter()
                .map(|tx| tx.block_index)
                .collect::<Vec<_>>(),
            vec![BlockIndex(10), BlockIndex(20), BlockIndex(30)]
        );
        assert_eq!(
            outcome.next_state.cursor.latest_cursor,
            Some(BlockIndex(30))
        );
        assert_eq!(
            outcome.next_state.cursor.oldest_cursor,
            Some(BlockIndex(10))
        );
        assert!(!outcome.next_state.cursor.backfill_complete);

        let short = scan_page(&[9, 7], Some(AccountHistoryPageOrder::Descending));
        let backfilled = outcome
            .next_state
            .observe_page(&short, Some(BlockIndex(10)), 3, 1, 10, Some(2))
            .unwrap();
        assert_eq!(
            backfilled.next_state.cursor.oldest_cursor,
            Some(BlockIndex(7))
        );
        assert!(backfilled.next_state.cursor.backfill_complete);

        let head = scan_page(&[40, 35, 30], Some(AccountHistoryPageOrder::Descending));
        let caught_up = backfilled
            .next_state
            .observe_page(&head, None, 3, 1, 10, Some(3))
            .unwrap();
        assert_eq!(caught_up.phase, AccountHistoryScanPhase::DescendingHead);
        assert_eq!(
            caught_up
                .transactions_chronological
                .iter()
                .map(|tx| tx.block_index)
                .collect::<Vec<_>>(),
            vec![BlockIndex(35), BlockIndex(40)]
        );
        assert_eq!(
            caught_up.next_state.cursor.latest_cursor,
            Some(BlockIndex(40))
        );
    }

    #[test]
    fn ascending_scan_allows_gaps_and_skips_repeated_cursor_once() {
        let state = AccountHistoryScanState {
            cursor: AccountHistoryCursor {
                order: Some(AccountHistoryPageOrder::Ascending),
                latest_cursor: Some(BlockIndex(10)),
                oldest_cursor: Some(BlockIndex(10)),
                backfill_complete: false,
            },
            status: AccountHistoryScanStatus::default(),
        };
        let outcome = state
            .observe_page(
                &scan_page(&[10, 25, 40], Some(AccountHistoryPageOrder::Ascending)),
                Some(BlockIndex(11)),
                10,
                1,
                10,
                None,
            )
            .unwrap();
        assert_eq!(
            outcome
                .transactions_chronological
                .iter()
                .map(|tx| tx.block_index)
                .collect::<Vec<_>>(),
            vec![BlockIndex(25), BlockIndex(40)]
        );
        assert_eq!(
            outcome.next_state.cursor.latest_cursor,
            Some(BlockIndex(40))
        );
    }

    #[test]
    fn account_history_scan_faults_without_advancing_on_bad_pages() {
        let state = AccountHistoryScanState::default();
        assert!(matches!(
            state.observe_page(
                &scan_page(&[2, 2], Some(AccountHistoryPageOrder::Ascending)),
                None,
                10,
                1,
                10,
                None,
            ),
            Err(AccountHistoryFault::DuplicateReturnedId(BlockIndex(2)))
        ));
        let seeded = AccountHistoryScanState {
            cursor: AccountHistoryCursor {
                order: Some(AccountHistoryPageOrder::Ascending),
                latest_cursor: Some(BlockIndex(10)),
                oldest_cursor: Some(BlockIndex(10)),
                backfill_complete: false,
            },
            status: AccountHistoryScanStatus::default(),
        };
        assert!(matches!(
            seeded.observe_page(
                &scan_page(&[9], Some(AccountHistoryPageOrder::Ascending)),
                None,
                10,
                1,
                10,
                None,
            ),
            Err(AccountHistoryFault::NonProgressingPage(BlockIndex(9)))
        ));
        assert_eq!(seeded.cursor.latest_cursor, Some(BlockIndex(10)));
    }

    #[test]
    fn account_history_scan_tracks_unreadable_lag_status_and_page_cap() {
        let unreadable = AccountHistoryScanState::default().record_unreadable("index read failed");
        assert_eq!(unreadable.status.latest_page_unreadable_count, 1);
        assert_eq!(unreadable.cursor.latest_cursor, None);

        let state = AccountHistoryScanState::default();
        let lagged = IndexScanResult {
            index_tip: Some(BlockIndex(4)),
            ..scan_page(&[5], Some(AccountHistoryPageOrder::Ascending))
        };
        assert!(matches!(
            state.observe_page(&lagged, Some(BlockIndex(5)), 10, 1, 10, None),
            Err(AccountHistoryFault::IndexLag {
                requested: BlockIndex(5),
                tip: Some(BlockIndex(4))
            })
        ));

        let outcome = state
            .observe_page(
                &scan_page(&[1, 2], Some(AccountHistoryPageOrder::Ascending)),
                None,
                2,
                2,
                2,
                None,
            )
            .unwrap();
        assert!(outcome.page_cap_reached);
        assert!(outcome.next_state.status.page_cap_reached);
    }

    #[test]
    fn icrc_index_args_require_an_account_filter() {
        let err = IcrcIndexGetAccountTransactionsArgs::try_from(IndexScanRequest {
            start: Some(BlockIndex(1)),
            limit: 10,
            account_filter: None,
            account_aliases: vec![],
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
    fn icrc_index_response_decodes_direct_real_index_shape() {
        let page = IcrcIndexGetAccountTransactionsResult {
            transactions: vec![],
            oldest_tx_id: None,
            tip: Some(Nat::from(4_u64)),
            archive_required: false,
        };
        let bytes = Encode!(&page).expect("direct page should encode");

        assert_eq!(decode_icrc_index_response_bytes(&bytes).unwrap(), Ok(page));
    }

    #[test]
    fn icrc_index_response_decodes_result_mock_shape() {
        let page = IcrcIndexGetAccountTransactionsResult {
            transactions: vec![],
            oldest_tx_id: None,
            tip: Some(Nat::from(5_u64)),
            archive_required: false,
        };
        let result: Result<IcrcIndexGetAccountTransactionsResult, IcrcIndexError> =
            Ok(page.clone());
        let bytes = Encode!(&result).expect("result page should encode");

        assert_eq!(decode_icrc_index_response_bytes(&bytes).unwrap(), Ok(page));
    }

    #[test]
    fn icrc_index_ng_result_maps_official_account_history_shape() {
        let from = account();
        let to = Account::new(principal(), Some(Subaccount([9; 32])));
        let result = map_icrc_index_ng_result(Ok(IcrcIndexNgGetTransactionsResult {
            balance: Nat::from(123_u64),
            transactions: vec![IcrcIndexNgTransactionWithId {
                id: Nat::from(7_u64),
                transaction: IcrcIndexNgTransaction {
                    burn: None,
                    mint: None,
                    approve: None,
                    transfer: Some(IcrcIndexNgTransfer {
                        from: from.to_icrc_account(),
                        to: to.to_icrc_account(),
                        amount: Nat::from(42_u64),
                        fee: Some(Nat::from(10_u64)),
                        memo: Some(b"memo".to_vec()),
                        created_at_time: Some(11),
                    }),
                    timestamp: 12,
                },
            }],
            oldest_tx_id: Some(Nat::from(7_u64)),
        }))
        .unwrap();

        assert_eq!(result.account_balance_e8s, Some(123));
        assert_eq!(result.transactions[0].block_index, BlockIndex(7));
        assert_eq!(result.transactions[0].transaction.from, Some(from));
        assert_eq!(result.transactions[0].transaction.to, Some(to));
        assert_eq!(result.transactions[0].transaction.amount_e8s, 42);
        assert_eq!(result.transactions[0].transaction.fee_e8s, Some(10));
        assert_eq!(
            result.transactions[0].transaction.operation_kind,
            LedgerOperationKind::Transfer
        );
    }

    #[test]
    fn icp_index_args_and_result_map_account_filtered_descending_pages() {
        let account = account();
        let account_identifier = account.icp_account_identifier_text();
        let args = IcpIndexGetAccountIdentifierTransactionsArgs::try_from(IndexScanRequest {
            start: None,
            limit: 50,
            account_filter: Some(account.clone()),
            account_aliases: vec![],
        })
        .unwrap();
        assert_eq!(args.start, None);
        assert_eq!(args.max_results, 50);
        assert_eq!(args.account_identifier, account_identifier);

        let result = map_icp_index_result(
            Some(&account),
            &[],
            None,
            IcpIndexGetAccountIdentifierTransactionsResult::Ok(
                IcpIndexGetAccountIdentifierTransactionsResponse {
                    balance: 0,
                    oldest_tx_id: Some(20),
                    transactions: vec![
                        IcpIndexTransactionWithId {
                            id: 21,
                            transaction: IcpIndexTransaction {
                                memo: 8,
                                icrc1_memo: None,
                                operation: IcpIndexOperation::Transfer {
                                    to: account.icp_account_identifier_text(),
                                    fee: IcpIndexTokens { e8s: 10_000 },
                                    from: "other".to_string(),
                                    amount: IcpIndexTokens { e8s: 124 },
                                    spender: None,
                                },
                                created_at_time: None,
                                timestamp: Some(IcpIndexTimeStamp {
                                    timestamp_nanos: 100,
                                }),
                            },
                        },
                        IcpIndexTransactionWithId {
                            id: 20,
                            transaction: IcpIndexTransaction {
                                memo: 7,
                                icrc1_memo: None,
                                operation: IcpIndexOperation::Transfer {
                                    to: account.icp_account_identifier_text(),
                                    fee: IcpIndexTokens { e8s: 10_000 },
                                    from: "other".to_string(),
                                    amount: IcpIndexTokens { e8s: 123 },
                                    spender: None,
                                },
                                created_at_time: None,
                                timestamp: Some(IcpIndexTimeStamp {
                                    timestamp_nanos: 99,
                                }),
                            },
                        },
                    ],
                },
            ),
        )
        .unwrap();
        assert_eq!(result.transactions[0].block_index, BlockIndex(20));
        assert_eq!(result.transactions[1].block_index, BlockIndex(21));
        assert_eq!(result.transactions[0].transaction.to, Some(account));
        assert_eq!(result.transactions[0].transaction.amount_e8s, 123);
        assert_eq!(result.last_seen_block, Some(BlockIndex(21)));
        assert_eq!(result.index_tip, None);
        assert_eq!(result.next_cursor(None), Ok(Some(BlockIndex(21))));
    }

    #[test]
    fn icp_index_result_maps_known_legacy_counterparty_alias() {
        let deposit_account = account();
        let jupiter_account = Account::new(principal(), Some(Subaccount([11; 32])));
        let result = map_icp_index_result(
            Some(&deposit_account),
            &[AccountAlias {
                account: jupiter_account.clone(),
                label: "jupiter_faucet".to_string(),
            }],
            None,
            IcpIndexGetAccountIdentifierTransactionsResult::Ok(
                IcpIndexGetAccountIdentifierTransactionsResponse {
                    balance: 100_000_000,
                    oldest_tx_id: Some(44),
                    transactions: vec![IcpIndexTransactionWithId {
                        id: 44,
                        transaction: IcpIndexTransaction {
                            memo: 0,
                            icrc1_memo: None,
                            operation: IcpIndexOperation::Transfer {
                                to: deposit_account.icp_account_identifier_text(),
                                fee: IcpIndexTokens { e8s: 10_000 },
                                from: jupiter_account.icp_account_identifier_text(),
                                amount: IcpIndexTokens { e8s: 100_000_000 },
                                spender: None,
                            },
                            created_at_time: None,
                            timestamp: Some(IcpIndexTimeStamp {
                                timestamp_nanos: 100,
                            }),
                        },
                    }],
                },
            ),
        )
        .unwrap();

        assert_eq!(
            result.transactions[0].transaction.from,
            Some(jupiter_account)
        );
        assert_eq!(result.transactions[0].transaction.to, Some(deposit_account));
        assert_eq!(result.transactions[0].transaction.amount_e8s, 100_000_000);
    }

    #[test]
    fn icp_index_start_some_maps_as_descending_cursor_page() {
        let account = account();
        let args = IcpIndexGetAccountIdentifierTransactionsArgs::try_from(IndexScanRequest {
            start: Some(BlockIndex(10)),
            limit: 50,
            account_filter: Some(account),
            account_aliases: vec![],
        })
        .unwrap();
        assert_eq!(args.start, Some(10));

        let result = map_icp_index_result(
            None,
            &[],
            Some(BlockIndex(10)),
            IcpIndexGetAccountIdentifierTransactionsResult::Ok(
                IcpIndexGetAccountIdentifierTransactionsResponse {
                    balance: 0,
                    oldest_tx_id: Some(8),
                    transactions: vec![icp_index_mint_tx(9), icp_index_mint_tx(8)],
                },
            ),
        );
        let result = result.unwrap();
        assert_eq!(
            result
                .transactions
                .iter()
                .map(|tx| tx.block_index)
                .collect::<Vec<_>>(),
            vec![BlockIndex(8), BlockIndex(9)]
        );
        assert_eq!(result.page_order, Some(AccountHistoryPageOrder::Descending));
    }

    #[test]
    fn icp_index_rejects_duplicate_or_non_descending_wire_pages() {
        let duplicate = map_icp_index_result(
            None,
            &[],
            None,
            IcpIndexGetAccountIdentifierTransactionsResult::Ok(
                IcpIndexGetAccountIdentifierTransactionsResponse {
                    balance: 0,
                    oldest_tx_id: Some(2),
                    transactions: vec![icp_index_mint_tx(2), icp_index_mint_tx(2)],
                },
            ),
        );
        assert!(matches!(
            duplicate,
            Err(IndexError::MissingBlock {
                block_index: BlockIndex(2)
            })
        ));

        let ascending = map_icp_index_result(
            None,
            &[],
            None,
            IcpIndexGetAccountIdentifierTransactionsResult::Ok(
                IcpIndexGetAccountIdentifierTransactionsResponse {
                    balance: 0,
                    oldest_tx_id: Some(2),
                    transactions: vec![icp_index_mint_tx(1), icp_index_mint_tx(2)],
                },
            ),
        );
        assert!(matches!(
            ascending,
            Err(IndexError::MissingBlock {
                block_index: BlockIndex(2)
            })
        ));
    }

    #[test]
    fn icp_index_archive_required_and_start_some_errors_do_not_advance() {
        assert_eq!(
            map_icp_index_result(
                None,
                &[],
                Some(BlockIndex(10)),
                IcpIndexGetAccountIdentifierTransactionsResult::Err(
                    IcpIndexGetAccountIdentifierTransactionsError {
                        message: "archive required".to_string(),
                    },
                ),
            ),
            Err(IndexError::ArchiveRequired {
                from: BlockIndex(0)
            })
        );
    }

    #[test]
    fn icp_and_icrc_index_pages_reject_duplicate_or_non_monotonic_entries() {
        let icp = map_icp_index_result(
            None,
            &[],
            None,
            IcpIndexGetAccountIdentifierTransactionsResult::Ok(
                IcpIndexGetAccountIdentifierTransactionsResponse {
                    balance: 0,
                    oldest_tx_id: None,
                    transactions: vec![
                        IcpIndexTransactionWithId {
                            id: 2,
                            transaction: IcpIndexTransaction {
                                memo: 0,
                                icrc1_memo: None,
                                operation: IcpIndexOperation::Mint {
                                    to: "a".to_string(),
                                    amount: IcpIndexTokens { e8s: 1 },
                                },
                                created_at_time: None,
                                timestamp: None,
                            },
                        },
                        IcpIndexTransactionWithId {
                            id: 2,
                            transaction: IcpIndexTransaction {
                                memo: 0,
                                icrc1_memo: None,
                                operation: IcpIndexOperation::Mint {
                                    to: "a".to_string(),
                                    amount: IcpIndexTokens { e8s: 1 },
                                },
                                created_at_time: None,
                                timestamp: None,
                            },
                        },
                    ],
                },
            ),
        );
        assert!(matches!(icp, Err(IndexError::MissingBlock { .. })));

        let icrc = map_icrc_index_result(Ok(IcrcIndexGetAccountTransactionsResult {
            transactions: vec![
                IcrcIndexTransaction {
                    id: Nat::from(1_u64),
                    transaction: block(1),
                },
                IcrcIndexTransaction {
                    id: Nat::from(1_u64),
                    transaction: block(1),
                },
            ],
            oldest_tx_id: Some(Nat::from(1_u64)),
            tip: Some(Nat::from(1_u64)),
            archive_required: false,
        }));
        assert!(matches!(icrc, Err(IndexError::MissingBlock { .. })));
    }

    #[test]
    fn archive_traversal_requires_complete_contiguous_ranges() {
        let request = IndexArchiveRequest {
            range: IndexArchiveRange {
                start: BlockIndex(10),
                limit: 10,
            },
            callback: IndexArchiveCallback {
                canister_id: principal(),
                method: "get_blocks".to_string(),
            },
        };
        assert_eq!(candid_round_trip(&request), request);

        let complete = IndexArchiveTraversal {
            requested: request.range,
            completed: vec![
                IndexArchiveRange {
                    start: BlockIndex(10),
                    limit: 4,
                },
                IndexArchiveRange {
                    start: BlockIndex(14),
                    limit: 6,
                },
            ],
            incomplete: false,
        };
        assert_eq!(complete.validate_no_skipped_ranges(), Ok(()));

        let skipped = IndexArchiveTraversal {
            completed: vec![IndexArchiveRange {
                start: BlockIndex(11),
                limit: 9,
            }],
            ..complete.clone()
        };
        assert_eq!(
            skipped.validate_no_skipped_ranges(),
            Err(IndexError::MissingBlock {
                block_index: BlockIndex(10)
            })
        );

        let incomplete = IndexArchiveTraversal {
            incomplete: true,
            ..complete
        };
        assert_eq!(
            incomplete.validate_no_skipped_ranges(),
            Err(IndexError::ArchiveRequired {
                from: BlockIndex(20)
            })
        );
    }

    #[test]
    fn icp_transfer_fee_decodes_official_record_shape() {
        #[derive(CandidType)]
        struct OfficialTransferFee {
            transfer_fee: IcpTokens,
        }

        let bytes = Encode!(&OfficialTransferFee {
            transfer_fee: IcpTokens { e8s: 10_000 },
        })
        .unwrap();
        let decoded = Decode!(&bytes, IcpTransferFee).unwrap();
        assert_eq!(decoded.transfer_fee.e8s, 10_000);
    }

    #[test]
    fn icp_index_decodes_official_operation_and_transaction_shapes() {
        #[derive(CandidType)]
        #[allow(dead_code)]
        enum OfficialResult {
            Ok(OfficialResponse),
            Err(IcpIndexGetAccountIdentifierTransactionsError),
        }

        #[derive(CandidType)]
        struct OfficialResponse {
            balance: u64,
            transactions: Vec<OfficialTransactionWithId>,
            oldest_tx_id: Option<u64>,
        }

        #[derive(CandidType)]
        struct OfficialTransactionWithId {
            id: u64,
            transaction: OfficialTransaction,
        }

        #[derive(CandidType)]
        struct OfficialTransaction {
            memo: u64,
            icrc1_memo: Option<Vec<u8>>,
            operation: OfficialOperation,
            created_at_time: Option<IcpIndexTimeStamp>,
            timestamp: Option<IcpIndexTimeStamp>,
        }

        #[derive(CandidType)]
        enum OfficialOperation {
            Approve {
                fee: IcpIndexTokens,
                from: String,
                allowance: IcpIndexTokens,
                expires_at: Option<IcpIndexTimeStamp>,
                spender: String,
                expected_allowance: Option<IcpIndexTokens>,
            },
            Burn {
                from: String,
                amount: IcpIndexTokens,
                spender: Option<String>,
            },
            Mint {
                to: String,
                amount: IcpIndexTokens,
            },
            Transfer {
                to: String,
                fee: IcpIndexTokens,
                from: String,
                amount: IcpIndexTokens,
                spender: Option<String>,
            },
        }

        let bytes = Encode!(&OfficialResult::Ok(OfficialResponse {
            balance: 0,
            oldest_tx_id: Some(1),
            transactions: vec![
                OfficialTransactionWithId {
                    id: 4,
                    transaction: OfficialTransaction {
                        memo: 4,
                        icrc1_memo: None,
                        operation: OfficialOperation::Transfer {
                            to: "to".to_string(),
                            fee: IcpIndexTokens { e8s: 10_000 },
                            from: "from".to_string(),
                            amount: IcpIndexTokens { e8s: 40 },
                            spender: None,
                        },
                        created_at_time: None,
                        timestamp: Some(IcpIndexTimeStamp { timestamp_nanos: 4 }),
                    },
                },
                OfficialTransactionWithId {
                    id: 3,
                    transaction: OfficialTransaction {
                        memo: 3,
                        icrc1_memo: Some(vec![3]),
                        operation: OfficialOperation::Approve {
                            fee: IcpIndexTokens { e8s: 10_000 },
                            from: "from".to_string(),
                            allowance: IcpIndexTokens { e8s: 30 },
                            expires_at: Some(IcpIndexTimeStamp {
                                timestamp_nanos: 30
                            }),
                            spender: "spender".to_string(),
                            expected_allowance: None,
                        },
                        created_at_time: None,
                        timestamp: Some(IcpIndexTimeStamp { timestamp_nanos: 3 }),
                    },
                },
                OfficialTransactionWithId {
                    id: 2,
                    transaction: OfficialTransaction {
                        memo: 2,
                        icrc1_memo: None,
                        operation: OfficialOperation::Burn {
                            from: "from".to_string(),
                            amount: IcpIndexTokens { e8s: 20 },
                            spender: Some("spender".to_string()),
                        },
                        created_at_time: None,
                        timestamp: Some(IcpIndexTimeStamp { timestamp_nanos: 2 }),
                    },
                },
                OfficialTransactionWithId {
                    id: 1,
                    transaction: OfficialTransaction {
                        memo: 1,
                        icrc1_memo: None,
                        operation: OfficialOperation::Mint {
                            to: "to".to_string(),
                            amount: IcpIndexTokens { e8s: 10 },
                        },
                        created_at_time: None,
                        timestamp: Some(IcpIndexTimeStamp { timestamp_nanos: 1 }),
                    },
                },
            ],
        }))
        .unwrap();
        let decoded = Decode!(&bytes, IcpIndexGetAccountIdentifierTransactionsResult).unwrap();
        let page = map_icp_index_result(None, &[], None, decoded).unwrap();
        assert_eq!(
            page.transactions
                .iter()
                .map(|tx| tx.transaction.operation_kind)
                .collect::<Vec<_>>(),
            vec![
                LedgerOperationKind::Mint,
                LedgerOperationKind::Burn,
                LedgerOperationKind::Approve,
                LedgerOperationKind::Transfer,
            ]
        );
        assert_eq!(page.last_seen_block, Some(BlockIndex(4)));
    }

    #[test]
    fn icp_index_tolerates_jupiter_transfer_from_as_transfer_like_operation() {
        let account = account();
        let encoded = Encode!(&IcpIndexGetAccountIdentifierTransactionsResult::Ok(
            IcpIndexGetAccountIdentifierTransactionsResponse {
                balance: 0,
                oldest_tx_id: Some(41),
                transactions: vec![IcpIndexTransactionWithId {
                    id: 42,
                    transaction: IcpIndexTransaction {
                        memo: 0,
                        icrc1_memo: None,
                        operation: IcpIndexOperation::TransferFrom {
                            to: account.icp_account_identifier_text(),
                            fee: IcpIndexTokens { e8s: 10_000 },
                            from: "from-account".to_string(),
                            amount: IcpIndexTokens { e8s: 123_456 },
                            spender: "spender-account".to_string(),
                        },
                        created_at_time: None,
                        timestamp: Some(IcpIndexTimeStamp {
                            timestamp_nanos: 456,
                        }),
                    },
                }],
            },
        ))
        .unwrap();
        let decoded = Decode!(&encoded, IcpIndexGetAccountIdentifierTransactionsResult).unwrap();
        let page = map_icp_index_result(Some(&account), &[], None, decoded).unwrap();
        assert_eq!(
            page.transactions[0].transaction.operation_kind,
            LedgerOperationKind::Transfer
        );
        assert_eq!(page.transactions[0].transaction.to, Some(account));
        assert_eq!(page.transactions[0].transaction.amount_e8s, 123_456);
    }

    #[test]
    fn production_dtos_candid_round_trip() {
        let icp_args = icp_transfer_args_for_request(icp_request(), 10_000).unwrap();
        assert_eq!(candid_round_trip(&icp_args), icp_args);
        assert_eq!(
            candid_round_trip(&IcpTransferFeeArgs {}),
            IcpTransferFeeArgs {}
        );
        assert_eq!(
            candid_round_trip(&IcpTransferFee {
                transfer_fee: IcpTokens { e8s: 10_000 },
            }),
            IcpTransferFee {
                transfer_fee: IcpTokens { e8s: 10_000 },
            }
        );

        let icp_index = IcpIndexGetAccountIdentifierTransactionsResult::Ok(
            IcpIndexGetAccountIdentifierTransactionsResponse {
                balance: 0,
                transactions: vec![IcpIndexTransactionWithId {
                    id: 1,
                    transaction: IcpIndexTransaction {
                        memo: 1,
                        icrc1_memo: Some(vec![1, 2]),
                        operation: IcpIndexOperation::Burn {
                            from: "legacy".to_string(),
                            amount: IcpIndexTokens { e8s: 1 },
                            spender: None,
                        },
                        created_at_time: Some(IcpIndexTimeStamp { timestamp_nanos: 1 }),
                        timestamp: None,
                    },
                }],
                oldest_tx_id: Some(1),
            },
        );
        assert_eq!(candid_round_trip(&icp_index), icp_index);

        let icrc_index = IcrcIndexGetAccountTransactionsResult {
            transactions: vec![IcrcIndexTransaction {
                id: Nat::from(1_u64),
                transaction: block(1),
            }],
            oldest_tx_id: Some(Nat::from(1_u64)),
            tip: Some(Nat::from(2_u64)),
            archive_required: false,
        };
        assert_eq!(candid_round_trip(&icrc_index), icrc_index);
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

    #[test]
    fn local_sns_reserve_account_shape_is_icrc_representable() {
        let reserve_owner = principal();
        let reserve = Account::new(reserve_owner, Some(Subaccount([42; 32])));
        let icrc = reserve.to_icrc_account();
        assert_eq!(icrc.owner, reserve_owner);
        assert_eq!(icrc.subaccount, Some(vec![42; 32]));
        assert_eq!(Account::try_from(icrc).unwrap(), reserve);
    }

    #[test]
    fn local_sns_issuance_is_reserve_transfer_not_mint() {
        let user = account();
        let issuance = LedgerTransferRequest {
            from_subaccount: Some(Subaccount([42; 32])),
            to: user.clone(),
            amount_e8s: 100_000_000,
            fee_e8s: Some(10_000),
            memo: Some(Memo::from("IO local issuance rehearsal")),
            created_at_time: Some(1_000),
        };
        let arg = IcrcTransferArg::from(issuance);
        assert_eq!(arg.from_subaccount, Some(vec![42; 32]));
        assert_eq!(arg.to, user.to_icrc_account());
        assert_eq!(arg.amount, Nat::from(100_000_000_u128));
        assert_eq!(arg.fee, Some(Nat::from(10_000_u128)));
    }

    #[test]
    fn local_sns_redemption_return_is_user_to_reserve_transfer() {
        let reserve = Account::new(principal(), None);
        let redemption_return = LedgerTransferRequest {
            from_subaccount: None,
            to: reserve.clone(),
            amount_e8s: 100_000_000,
            fee_e8s: Some(10_000),
            memo: Some(Memo::from("IO local redemption return")),
            created_at_time: Some(2_000),
        };
        let arg = IcrcTransferArg::from(redemption_return);
        assert_eq!(arg.from_subaccount, None);
        assert_eq!(arg.to, reserve.to_icrc_account());
        assert_eq!(arg.amount, Nat::from(100_000_000_u128));
    }

    #[test]
    fn local_sns_total_supply_constant_model_uses_transfer_blocks() {
        let before_supply = 100_000_000_000_000_u128;
        let issuance = block(11);
        let redemption_return = block(12);
        assert_eq!(issuance.operation_kind, LedgerOperationKind::Transfer);
        assert_eq!(
            redemption_return.operation_kind,
            LedgerOperationKind::Transfer
        );
        assert_eq!(before_supply, 100_000_000_000_000_u128);
    }

    #[test]
    fn local_sns_required_error_observations_map_to_boundary_errors() {
        assert!(matches!(
            map_icrc_transfer_result(Err(IcrcTransferError::BadFee {
                expected_fee: Nat::from(10_000_u128),
            })),
            Err(LedgerTransferError::BadFee {
                expected_fee_e8s: 10_000
            })
        ));
        assert!(matches!(
            map_icrc_transfer_result(Err(IcrcTransferError::InsufficientFunds {
                balance: Nat::from(0_u128),
            })),
            Err(LedgerTransferError::InsufficientFunds { balance_e8s: 0 })
        ));
        assert_eq!(
            map_icrc_transfer_result(Err(IcrcTransferError::Duplicate {
                duplicate_of: Nat::from(77_u64),
            }))
            .unwrap_err()
            .idempotent_success_block(),
            Some(BlockIndex(77))
        );
    }
}
