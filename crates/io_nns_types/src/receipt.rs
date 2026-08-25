pub use io_receipt_types::{
    ClaimBackingReceiptKind, ClaimBackingReceiptPermit, ClaimBackingReceiptProgress,
    ClaimBackingReceiptResult, PrepareClaimBackingReceiptArgs, ProveClaimBackingReceiptArgs,
};
use sha2::{Digest, Sha256};

pub fn receipt_memo(nns_operation_sequence: u64) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"io-paired-backing-inflow-v2");
    hasher.update(nns_operation_sequence.to_be_bytes());
    hasher.finalize().to_vec()
}
