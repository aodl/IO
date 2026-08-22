use candid::CandidType;
use io_accounts::Account;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PrepareJupiterReceiptArgs {
    pub receipt_sequence: u64,
    pub source_operation_id: Vec<u8>,
    pub liquid_amount_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct JupiterReceiptPermit {
    pub sequence: u64,
    pub destination: Account,
    pub memo: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CompleteJupiterReceiptArgs {
    pub receipt_sequence: u64,
    pub block_index: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct JupiterReceiptResult {
    pub request_fingerprint: Vec<u8>,
    pub receipt_block: u128,
    pub backed_io_e8s: u128,
    pub io_transfer_block: u128,
    pub io_fee_e8s: u128,
    pub completed_at_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum JupiterReceiptProgress {
    AwaitingReceipt,
    ReceiptProved,
    Settling,
    Completed(JupiterReceiptResult),
    Stuck(String),
}
