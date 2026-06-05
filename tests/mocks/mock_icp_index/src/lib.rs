use candid::{CandidType, Principal};
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
