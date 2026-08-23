use candid::CandidType;
use io_accounts::Account;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ClaimBackingReceiptKind {
    Jupiter,
    PermanentMaturity { maturity_generation: u64 },
    PooledMaturity { entitlement_batch_generation: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PrepareClaimBackingReceiptArgs {
    pub source_operation_id: Vec<u8>,
    pub kind: ClaimBackingReceiptKind,
    pub source_account: Account,
    pub source_block: u128,
    pub net_liquid_credit_e8s: u128,
    pub nns_fingerprint: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ClaimBackingReceiptPermit {
    pub stream_operation_sequence: u64,
    pub destination: Account,
    pub amount_e8s: u128,
    pub memo: Vec<u8>,
    pub request_fingerprint: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ProveClaimBackingReceiptArgs {
    pub stream_operation_sequence: u64,
    pub block_index: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ClaimBackingReceiptResult {
    pub source_operation_id: Vec<u8>,
    pub request_fingerprint: Vec<u8>,
    pub kind: ClaimBackingReceiptKind,
    pub liquid_credit_e8s: u128,
    pub distributed_io_e8s: u128,
    pub recipient_transfer_block: Option<u128>,
    pub io_fee_e8s: u128,
    pub completed_at_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ClaimBackingReceiptProgress {
    AwaitingLiquidProof(ClaimBackingReceiptPermit),
    SettlingRecipients,
    Completed(ClaimBackingReceiptResult),
    Stuck(String),
}
