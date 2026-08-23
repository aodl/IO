pub use io_receipt_types::{
    ClaimBackingReceiptKind, ClaimBackingReceiptPermit, ClaimBackingReceiptProgress,
    ClaimBackingReceiptResult, PrepareClaimBackingReceiptArgs, ProveClaimBackingReceiptArgs,
};
use sha2::{Digest, Sha256};

pub fn receipt_memo(source_operation_id: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"io-claim-backing-receipt-v1");
    hasher.update(source_operation_id);
    hasher.finalize().to_vec()
}
