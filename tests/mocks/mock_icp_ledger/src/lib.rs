use candid::{CandidType, Nat};
use io_ledger_types::{
    Account, IcrcAccount, IcrcTransferArg, IcrcTransferError, LedgerBlock, LedgerOperationKind,
    Memo, Subaccount,
};
use serde::Deserialize;
use std::cell::RefCell;

const FEE_E8S: u128 = 10_000;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct AccountBalanceArgs {
    pub account: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TransferArgs {
    pub from: String,
    pub to: String,
    pub amount_e8s: u128,
    pub memo: String,
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

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugMintArgs {
    pub to: String,
    pub amount_e8s: u128,
    pub memo: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugRejectAccountArgs {
    pub account: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugTransferFailureArgs {
    pub account: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugDuplicateResponseArgs {
    pub duplicate_of: u64,
}

#[derive(Default)]
struct LedgerState {
    balances: Vec<(String, u128)>,
    transactions: Vec<LedgerTransaction>,
    rejected_to_accounts: Vec<String>,
    duplicate_response: Option<u64>,
}

thread_local! {
    static STATE: RefCell<LedgerState> = RefCell::new(LedgerState::default());
}

fn now() -> u64 {
    #[cfg(target_family = "wasm")]
    {
        ic_cdk::api::time()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        0
    }
}

fn balance_of(state: &LedgerState, account: &str) -> u128 {
    state
        .balances
        .iter()
        .find(|(name, _)| name == account)
        .map(|(_, balance)| *balance)
        .unwrap_or(0)
}

fn set_balance(state: &mut LedgerState, account: &str, balance: u128) {
    match state.balances.iter_mut().find(|(name, _)| name == account) {
        Some((_, current)) => *current = balance,
        None => state.balances.push((account.to_string(), balance)),
    }
}

fn record(
    state: &mut LedgerState,
    from: String,
    to: String,
    amount_e8s: u128,
    memo: String,
) -> u64 {
    let block_index = state.transactions.len() as u64;
    state.transactions.push(LedgerTransaction {
        from,
        to,
        amount_e8s,
        memo,
        block_index,
        timestamp: now(),
    });
    block_index
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

fn mock_label_from_account(account: &Account) -> String {
    account
        .subaccount
        .as_ref()
        .and_then(mock_label_from_subaccount)
        .unwrap_or_else(|| account.owner.to_text())
}

fn account_from_icrc(account: IcrcAccount) -> Result<Account, IcrcTransferError> {
    let subaccount = match account.subaccount {
        Some(bytes) => Some(Subaccount(bytes.try_into().map_err(|bytes: Vec<u8>| {
            IcrcTransferError::GenericError {
                error_code: Nat::from(1_u64),
                message: format!("subaccount must be 32 bytes, got {}", bytes.len()),
            }
        })?)),
        None => None,
    };
    Ok(Account::new(account.owner, subaccount))
}

fn label_from_icrc(account: IcrcAccount) -> Result<String, IcrcTransferError> {
    Ok(mock_label_from_account(&account_from_icrc(account)?))
}

fn label_from_from_subaccount(subaccount: Option<Vec<u8>>) -> Result<String, IcrcTransferError> {
    match subaccount {
        Some(bytes) => {
            let subaccount = Subaccount(bytes.try_into().map_err(|bytes: Vec<u8>| {
                IcrcTransferError::GenericError {
                    error_code: Nat::from(1_u64),
                    message: format!("subaccount must be 32 bytes, got {}", bytes.len()),
                }
            })?);
            Ok(mock_label_from_subaccount(&subaccount)
                .unwrap_or_else(|| "boundary_from_subaccount".to_string()))
        }
        None => Ok("anonymous".to_string()),
    }
}

fn nat_to_u128(value: &Nat, field: &str) -> Result<u128, IcrcTransferError> {
    value
        .0
        .to_str_radix(10)
        .parse::<u128>()
        .map_err(|err| IcrcTransferError::GenericError {
            error_code: Nat::from(1_u64),
            message: format!("{field} does not fit in u128: {err}"),
        })
}

fn memo_to_string(memo: Option<Vec<u8>>) -> String {
    memo.map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn icrc1_fee() -> Nat {
    Nat::from(FEE_E8S)
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn icrc1_balance_of(account: IcrcAccount) -> Nat {
    let label = label_from_icrc(account).unwrap_or_else(|_| String::new());
    STATE.with(|cell| Nat::from(balance_of(&cell.borrow(), &label)))
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn icrc1_transfer(args: IcrcTransferArg) -> Result<Nat, IcrcTransferError> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if let Some(duplicate_of) = state.duplicate_response {
            state.duplicate_response = None;
            return Err(IcrcTransferError::Duplicate {
                duplicate_of: Nat::from(duplicate_of),
            });
        }
        if let Some(fee) = args.fee.as_ref() {
            let fee = nat_to_u128(fee, "fee")?;
            if fee != FEE_E8S {
                return Err(IcrcTransferError::BadFee {
                    expected_fee: Nat::from(FEE_E8S),
                });
            }
        }
        let from = label_from_from_subaccount(args.from_subaccount)?;
        let to = label_from_icrc(args.to)?;
        let amount_e8s = nat_to_u128(&args.amount, "amount")?;
        let memo = memo_to_string(args.memo);
        if state
            .rejected_to_accounts
            .iter()
            .any(|account| account == &to)
        {
            return Err(IcrcTransferError::TemporarilyUnavailable);
        }
        let from_balance = balance_of(&state, &from);
        if from_balance < amount_e8s {
            return Err(IcrcTransferError::InsufficientFunds {
                balance: Nat::from(from_balance),
            });
        }
        let to_balance = balance_of(&state, &to);
        set_balance(&mut state, &from, from_balance - amount_e8s);
        set_balance(&mut state, &to, to_balance.saturating_add(amount_e8s));
        Ok(Nat::from(record(&mut state, from, to, amount_e8s, memo)))
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_reject_to(args: DebugRejectAccountArgs) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if !state
            .rejected_to_accounts
            .iter()
            .any(|account| account == &args.account)
        {
            state.rejected_to_accounts.push(args.account);
        }
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_transfer_failure(args: DebugTransferFailureArgs) {
    debug_reject_to(DebugRejectAccountArgs {
        account: args.account,
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_duplicate_response(args: DebugDuplicateResponseArgs) {
    STATE.with(|cell| cell.borrow_mut().duplicate_response = Some(args.duplicate_of));
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_clear_rejections() {
    STATE.with(|cell| cell.borrow_mut().rejected_to_accounts.clear());
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_clear() {
    STATE.with(|cell| *cell.borrow_mut() = LedgerState::default());
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_mint(args: DebugMintArgs) -> u64 {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let balance = balance_of(&state, &args.to);
        set_balance(
            &mut state,
            &args.to,
            balance.saturating_add(args.amount_e8s),
        );
        record(
            &mut state,
            "mint".to_string(),
            args.to,
            args.amount_e8s,
            args.memo,
        )
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_transactions() -> Vec<LedgerTransaction> {
    STATE.with(|cell| cell.borrow().transactions.clone())
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_boundary_transactions() -> Vec<LedgerBlock> {
    STATE.with(|cell| {
        cell.borrow()
            .transactions
            .iter()
            .map(|tx| LedgerBlock {
                block_index: io_ledger_types::BlockIndex(tx.block_index),
                timestamp_nanos: tx.timestamp,
                from: Some(Account::new(
                    candid::Principal::anonymous(),
                    Some(mock_subaccount(&tx.from)),
                )),
                to: Some(Account::new(
                    candid::Principal::anonymous(),
                    Some(mock_subaccount(&tx.to)),
                )),
                amount_e8s: tx.amount_e8s,
                fee_e8s: Some(FEE_E8S),
                memo: Some(Memo::from(tx.memo.clone())),
                operation_kind: LedgerOperationKind::Transfer,
            })
            .collect()
    })
}
