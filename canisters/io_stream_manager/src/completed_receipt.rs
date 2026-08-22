use crate::{
    receipt::{LastCompletedReceipt, ReceiptContext},
    state::StreamConfig,
};

impl LastCompletedReceipt {
    pub fn validate(&self, config: &StreamConfig, next_sequence: u64) -> Result<(), String> {
        ReceiptContext {
            request: self.request.clone(),
            request_fingerprint: self.request_fingerprint.clone(),
            source: config.jupiter_receipt_source.clone(),
            permit: self.permit.clone(),
            backing_snapshot: self.backing_snapshot.clone(),
        }
        .validate(config)?;
        if self.request.receipt_sequence.checked_add(1) != Some(next_sequence) {
            return Err("completed receipt sequence does not precede next sequence".into());
        }
        if self.result.request_fingerprint == self.request_fingerprint
            && self.result.receipt_block == self.receipt_block
            && self.result.backed_io_e8s > 0
            && self.result.io_fee_e8s == config.expected_io_fee_e8s
            && self.result.completed_at_nanos > 0
        {
            Ok(())
        } else {
            Err("completed Jupiter receipt values do not match request".into())
        }
    }
}
