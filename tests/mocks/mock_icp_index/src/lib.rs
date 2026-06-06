use candid::{CandidType, Nat, Principal};
use io_ledger_types::{
    Account, IcrcIndexError, IcrcIndexGetAccountTransactionsArgs,
    IcrcIndexGetAccountTransactionsResult, IcrcIndexTransaction, LedgerBlock, LedgerOperationKind,
    Memo, Subaccount,
};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct InitArgs {
    pub ledger_principal_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LedgerTransaction {
    pub from: String,
    pub to: String,
    pub amount_e8s: u128,
    pub memo: String,
    pub block_index: u64,
    pub timestamp: u64,
}

thread_local! {
    static LEDGER: std::cell::RefCell<Option<Principal>> = const { std::cell::RefCell::new(None) };
    static LAG: std::cell::RefCell<u64> = const { std::cell::RefCell::new(0) };
    static ARCHIVE_REQUIRED: std::cell::RefCell<bool> = const { std::cell::RefCell::new(false) };
    static PAGE_LIMIT: std::cell::RefCell<Option<u64>> = const { std::cell::RefCell::new(None) };
}

#[cfg_attr(target_family = "wasm", ic_cdk::init)]
pub fn init(args: InitArgs) {
    let ledger = args
        .ledger_principal_text
        .as_deref()
        .and_then(|text| Principal::from_text(text).ok());
    LEDGER.with(|cell| *cell.borrow_mut() = ledger);
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub async fn debug_get_transactions() -> Vec<LedgerTransaction> {
    let ledger = LEDGER.with(|cell| *cell.borrow());
    match ledger {
        #[cfg(target_family = "wasm")]
        Some(canister) => ic_cdk::call::Call::bounded_wait(canister, "debug_get_transactions")
            .await
            .ok()
            .and_then(|response| response.candid_tuple::<(Vec<LedgerTransaction>,)>().ok())
            .map(|(txs,)| txs)
            .unwrap_or_default(),
        #[cfg(not(target_family = "wasm"))]
        _ => Vec::new(),
        #[cfg(target_family = "wasm")]
        None => Vec::new(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugLagArgs {
    pub lag_blocks: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugArchiveRequiredArgs {
    pub archive_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugPageArgs {
    pub max_results: Option<u64>,
}

fn mock_subaccount(label: &str) -> Subaccount {
    let bytes = label.as_bytes();
    let mut subaccount = [0; 32];
    let len = bytes.len().min(31);
    subaccount[0] = len as u8;
    subaccount[1..=len].copy_from_slice(&bytes[..len]);
    Subaccount(subaccount)
}

fn mock_label_from_subaccount(subaccount: &Subaccount) -> Option<String> {
    let len = subaccount.0[0] as usize;
    if len == 0 || len > 31 {
        return None;
    }
    std::str::from_utf8(&subaccount.0[1..=len])
        .ok()
        .map(ToString::to_string)
}

fn account_from_icrc(account: io_ledger_types::IcrcAccount) -> Option<Account> {
    let subaccount = match account.subaccount {
        Some(bytes) => Some(Subaccount(bytes.try_into().ok()?)),
        None => None,
    };
    Some(Account::new(account.owner, subaccount))
}

fn mock_label_from_account(account: &Account) -> String {
    account
        .subaccount
        .as_ref()
        .and_then(mock_label_from_subaccount)
        .unwrap_or_else(|| account.owner.to_text())
}

fn tx_to_block(tx: LedgerTransaction) -> LedgerBlock {
    LedgerBlock {
        block_index: io_ledger_types::BlockIndex(tx.block_index),
        timestamp_nanos: tx.timestamp,
        from: Some(Account::new(
            Principal::anonymous(),
            Some(mock_subaccount(&tx.from)),
        )),
        to: Some(Account::new(
            Principal::anonymous(),
            Some(mock_subaccount(&tx.to)),
        )),
        amount_e8s: tx.amount_e8s,
        fee_e8s: Some(10_000),
        memo: Some(Memo::from(tx.memo)),
        operation_kind: LedgerOperationKind::Transfer,
    }
}

fn nat_to_u64(value: &Nat) -> Result<u64, IcrcIndexError> {
    value
        .0
        .to_str_radix(10)
        .parse::<u64>()
        .map_err(|err| IcrcIndexError::GenericError {
            error_code: Nat::from(1_u64),
            message: format!("nat does not fit in u64: {err}"),
        })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub async fn get_account_transactions(
    args: IcrcIndexGetAccountTransactionsArgs,
) -> Result<IcrcIndexGetAccountTransactionsResult, IcrcIndexError> {
    if ARCHIVE_REQUIRED.with(|cell| *cell.borrow()) {
        return Ok(IcrcIndexGetAccountTransactionsResult {
            transactions: Vec::new(),
            oldest_tx_id: None,
            tip: debug_get_tip(),
            archive_required: true,
        });
    }

    let account = account_from_icrc(args.account).ok_or_else(|| IcrcIndexError::GenericError {
        error_code: Nat::from(1_u64),
        message: "invalid account".to_string(),
    })?;
    let label = mock_label_from_account(&account);
    let start = args
        .start
        .as_ref()
        .map(nat_to_u64)
        .transpose()?
        .unwrap_or(0);
    let requested_limit = nat_to_u64(&args.max_results)?;
    let page_limit = PAGE_LIMIT.with(|cell| (*cell.borrow()).unwrap_or(requested_limit));
    let limit = requested_limit.min(page_limit) as usize;
    let all_transactions = debug_get_transactions().await;
    let visible_tip = all_transactions
        .iter()
        .map(|tx| tx.block_index)
        .max()
        .map(|tip| tip.saturating_sub(LAG.with(|cell| *cell.borrow())));
    let visible_tip_nat = visible_tip.map(Nat::from);

    let transactions = all_transactions
        .into_iter()
        .filter(|tx| tx.block_index >= start)
        .filter(|tx| visible_tip.map(|tip| tx.block_index <= tip).unwrap_or(true))
        .filter(|tx| tx.from == label || tx.to == label)
        .take(limit)
        .map(|tx| IcrcIndexTransaction {
            id: Nat::from(tx.block_index),
            transaction: tx_to_block(tx),
        })
        .collect::<Vec<_>>();

    Ok(IcrcIndexGetAccountTransactionsResult {
        oldest_tx_id: transactions.first().map(|tx| tx.id.clone()),
        transactions,
        tip: visible_tip_nat,
        archive_required: false,
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_tip() -> Option<Nat> {
    None
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_lag(args: DebugLagArgs) {
    LAG.with(|cell| *cell.borrow_mut() = args.lag_blocks);
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_archive_required(args: DebugArchiveRequiredArgs) {
    ARCHIVE_REQUIRED.with(|cell| *cell.borrow_mut() = args.archive_required);
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_page(args: DebugPageArgs) {
    PAGE_LIMIT.with(|cell| *cell.borrow_mut() = args.max_results);
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_clear() {
    LAG.with(|cell| *cell.borrow_mut() = 0);
    ARCHIVE_REQUIRED.with(|cell| *cell.borrow_mut() = false);
    PAGE_LIMIT.with(|cell| *cell.borrow_mut() = None);
}
