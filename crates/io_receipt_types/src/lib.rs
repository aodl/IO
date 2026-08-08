use candid::CandidType;
use io_accounts::Account;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ReceiptKind {
    Jupiter,
    TwoWeekMaturity,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PrepareLiquidReceiptArgs {
    pub receipt_sequence: u64,
    pub receipt_kind: ReceiptKind,
    pub source_operation_id: Vec<u8>,
    pub liquid_amount_e8s: u128,
    pub entitlement_batch_generation: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LiquidReceiptPermit {
    pub sequence: u64,
    pub destination: Account,
    pub memo: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CompleteLiquidReceiptArgs {
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
pub struct TwoWeekReceiptResult {
    pub request_fingerprint: Vec<u8>,
    pub receipt_block: u128,
    pub backed_io_pool_e8s: u128,
    pub distributed_io_e8s: u128,
    pub rounding_dust_io_e8s: u128,
    pub completed_at_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum CompletedReceiptResult {
    Jupiter(JupiterReceiptResult),
    TwoWeek(TwoWeekReceiptResult),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum LiquidReceiptProgress {
    AwaitingReceipt,
    ReceiptProved,
    Settling,
    Completed(CompletedReceiptResult),
    Stuck(String),
}
