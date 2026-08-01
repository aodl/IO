use candid::{CandidType, Principal};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use io_accounts::Account;

pub const MAX_MEMO_BYTES: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsTransferIntent {
    pub ledger: Principal,
    pub source_subaccount: [u8; 32],
    pub destination: Account,
    pub amount_e8s: u128,
    pub fee_e8s: u128,
    pub memo: Vec<u8>,
    pub created_at_time_nanos: u64,
}

impl NnsTransferIntent {
    pub fn validate(&self) -> Result<(), String> {
        if self.ledger == Principal::anonymous()
            || self.ledger == Principal::management_canister()
            || self.amount_e8s == 0
            || self.fee_e8s == 0
            || self.memo.len() > MAX_MEMO_BYTES
            || self.created_at_time_nanos == 0
        {
            return Err("NNS transfer intent is malformed".into());
        }
        self.destination.validate()
    }

    pub fn fingerprint(&self) -> Vec<u8> {
        Sha256::digest(candid::encode_one(self).expect("NNS transfer intent must encode")).to_vec()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum TransferState {
    Prepared,
    Submitted {
        epoch: u64,
        first_submitted_at_nanos: u64,
        last_submitted_at_nanos: u64,
    },
    Succeeded {
        block: u128,
    },
    Paused {
        epoch: u64,
        first_submitted_at_nanos: u64,
        last_submitted_at_nanos: u64,
        classification: TransferOutcomeClassification,
        reason: String,
    },
    Stuck {
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum TransferOutcomeClassification {
    AmbiguousPossibleEffect,
    BadFee,
    InsufficientFunds,
    RejectedNoEffect,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsTransferAttempt {
    pub intent: NnsTransferIntent,
    pub fingerprint: Vec<u8>,
    pub state: TransferState,
}

impl NnsTransferAttempt {
    pub fn prepared(intent: NnsTransferIntent) -> Result<Self, String> {
        intent.validate()?;
        Ok(Self {
            fingerprint: intent.fingerprint(),
            intent,
            state: TransferState::Prepared,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        self.intent.validate()?;
        if self.fingerprint.len() != 32 || self.fingerprint != self.intent.fingerprint() {
            return Err("NNS transfer fingerprint is invalid".into());
        }
        match &self.state {
            TransferState::Prepared | TransferState::Succeeded { .. } => Ok(()),
            TransferState::Submitted {
                epoch,
                first_submitted_at_nanos,
                last_submitted_at_nanos,
            } if *epoch > 0
                && *first_submitted_at_nanos > 0
                && last_submitted_at_nanos >= first_submitted_at_nanos =>
            {
                Ok(())
            }
            TransferState::Paused {
                epoch,
                first_submitted_at_nanos,
                last_submitted_at_nanos,
                reason,
                ..
            } if *epoch > 0
                && *first_submitted_at_nanos > 0
                && last_submitted_at_nanos >= first_submitted_at_nanos
                && !reason.is_empty()
                && reason.len() <= 512 =>
            {
                Ok(())
            }
            TransferState::Stuck { reason } if !reason.is_empty() && reason.len() <= 512 => Ok(()),
            _ => Err("NNS transfer state is malformed".into()),
        }
    }

    pub fn succeeded_block(&self) -> Result<u128, String> {
        match self.state {
            TransferState::Succeeded { block } => Ok(block),
            _ => Err("NNS transfer lacks exact success evidence".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submitted_transfer_requires_a_nonzero_ordered_dispatch_epoch() {
        let mut attempt = NnsTransferAttempt::prepared(NnsTransferIntent {
            ledger: Principal::from_slice(&[1; 29]),
            source_subaccount: [1; 32],
            destination: Account {
                owner: Principal::from_slice(&[2; 29]),
                subaccount: None,
            },
            amount_e8s: 1,
            fee_e8s: 1,
            memo: vec![1],
            created_at_time_nanos: 1,
        })
        .unwrap();
        attempt.state = TransferState::Submitted {
            epoch: 0,
            first_submitted_at_nanos: 1,
            last_submitted_at_nanos: 1,
        };
        assert!(attempt.validate().is_err());
        attempt.state = TransferState::Submitted {
            epoch: 1,
            first_submitted_at_nanos: 2,
            last_submitted_at_nanos: 1,
        };
        assert!(attempt.validate().is_err());
    }
}
