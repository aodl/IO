use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::state::Account;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum TransferState {
    Prepared,
    Submitted,
    Succeeded { block: u128 },
    Stuck { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsTransferAttempt {
    pub ledger: Principal,
    pub source_subaccount: Option<Vec<u8>>,
    pub destination: Account,
    pub amount_e8s: u128,
    pub fee_e8s: u128,
    pub memo_u64: u64,
    pub created_at_time_nanos: u64,
    pub state: TransferState,
}

impl NnsTransferAttempt {
    pub fn memo_bytes(&self) -> [u8; 8] {
        self.memo_u64.to_be_bytes()
    }
}
