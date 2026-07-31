use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::{state::Account, transfer::TransferAttempt};

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ReceiptKind {
    Jupiter,
    TwoYearMaturity,
    TwoWeekMaturity,
    UnwindPrincipal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ReceiptPhase {
    Prepared,
    AwaitingReceipt,
    ReceiptProved,
    Settling,
    Completed,
    Stuck,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PrepareLiquidReceiptArgs {
    pub receipt_sequence: u64,
    pub receipt_kind: ReceiptKind,
    pub source_operation_id: Vec<u8>,
    pub liquid_amount_e8s: u128,
    pub cohort_generation: Option<u64>,
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
pub struct LiquidReceiptOperation {
    pub sequence: u64,
    pub kind: ReceiptKind,
    pub source_operation_id: Vec<u8>,
    pub liquid_amount_e8s: u128,
    pub cohort_generation: Option<u64>,
    pub source: Account,
    pub destination: Account,
    pub memo: Vec<u8>,
    pub phase: ReceiptPhase,
    pub proved_block: Option<u128>,
    pub active_transfer: Option<TransferAttempt>,
    pub recipient_index: u32,
}

pub fn receipt_memo(manager: Principal, sequence: u64) -> Vec<u8> {
    crate::transfer::deterministic_memo(b"io-liquid-receipt-v1", manager, sequence)
}
