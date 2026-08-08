use crate::{
    receipt::{CompletedReceiptResult, LastCompletedReceipt, ReceiptContext, ReceiptKind},
    state::StreamConfig,
};

impl LastCompletedReceipt {
    pub fn validate(&self, config: &StreamConfig, next_sequence: u64) -> Result<(), String> {
        ReceiptContext {
            request: self.request.clone(),
            request_fingerprint: self.request_fingerprint.clone(),
            source: match self.request.receipt_kind {
                ReceiptKind::Jupiter => config.jupiter_receipt_source.clone(),
                ReceiptKind::TwoWeekMaturity => config.two_week_receipt_source.clone(),
            },
            permit: self.permit.clone(),
            backing_snapshot: self.backing_snapshot.clone(),
        }
        .validate(config)?;
        if self.request.receipt_sequence.checked_add(1) != Some(next_sequence) {
            return Err("completed receipt sequence does not precede next sequence".into());
        }
        match (&self.request.receipt_kind, &self.result) {
            (ReceiptKind::Jupiter, CompletedReceiptResult::Jupiter(result))
                if result.request_fingerprint == self.request_fingerprint
                    && result.receipt_block == self.receipt_block
                    && result.backed_io_e8s > 0
                    && result.io_fee_e8s == config.expected_io_fee_e8s
                    && result.completed_at_nanos > 0 =>
            {
                Ok(())
            }
            (ReceiptKind::TwoWeekMaturity, CompletedReceiptResult::TwoWeek(result))
                if result.request_fingerprint == self.request_fingerprint
                    && result.receipt_block == self.receipt_block
                    && result.backed_io_pool_e8s
                        == result
                            .distributed_io_e8s
                            .checked_add(result.rounding_dust_io_e8s)
                            .ok_or("completed two-week result overflow")?
                    && result.completed_at_nanos > 0 =>
            {
                Ok(())
            }
            _ => Err("completed receipt result kind or values do not match request".into()),
        }
    }
}
