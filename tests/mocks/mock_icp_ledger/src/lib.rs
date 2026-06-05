use candid::CandidType;
use serde::Deserialize;
use std::cell::RefCell;

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

#[derive(Default)]
struct LedgerState {
    balances: Vec<(String, u128)>,
    transactions: Vec<LedgerTransaction>,
    rejected_to_accounts: Vec<String>,
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

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn icrc1_balance_of(args: AccountBalanceArgs) -> u128 {
    STATE.with(|cell| balance_of(&cell.borrow(), &args.account))
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn icrc1_transfer(args: TransferArgs) -> Result<u64, String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if state
            .rejected_to_accounts
            .iter()
            .any(|account| account == &args.to)
        {
            return Err(format!("transfer to {} rejected", args.to));
        }
        let from_balance = balance_of(&state, &args.from);
        if from_balance < args.amount_e8s {
            return Err("insufficient funds".to_string());
        }
        let to_balance = balance_of(&state, &args.to);
        set_balance(&mut state, &args.from, from_balance - args.amount_e8s);
        set_balance(
            &mut state,
            &args.to,
            to_balance.saturating_add(args.amount_e8s),
        );
        Ok(record(
            &mut state,
            args.from,
            args.to,
            args.amount_e8s,
            args.memo,
        ))
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
pub fn debug_clear_rejections() {
    STATE.with(|cell| cell.borrow_mut().rejected_to_accounts.clear());
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
