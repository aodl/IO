use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::state::Account;

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
    Stuck {
        reason: String,
    },
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

    pub fn validate(&self) -> Result<(), String> {
        if self.ledger == Principal::anonymous()
            || self.ledger == Principal::management_canister()
            || self.amount_e8s == 0
            || self.fee_e8s == 0
            || self.created_at_time_nanos == 0
            || self
                .source_subaccount
                .as_ref()
                .is_some_and(|value| value.len() != 32)
        {
            return Err("NNS transfer intent is malformed".into());
        }
        self.destination.validate()?;
        match &self.state {
            TransferState::Prepared | TransferState::Succeeded { .. } => {}
            TransferState::Submitted {
                epoch,
                first_submitted_at_nanos,
                last_submitted_at_nanos,
            } if *epoch > 0
                && *first_submitted_at_nanos > 0
                && last_submitted_at_nanos >= first_submitted_at_nanos => {}
            TransferState::Stuck { reason } if !reason.is_empty() && reason.len() <= 512 => {}
            _ => return Err("NNS transfer state is malformed".into()),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submitted_transfer_requires_a_nonzero_ordered_dispatch_epoch() {
        let principal = Principal::from_slice(&[1; 29]);
        let mut attempt = NnsTransferAttempt {
            ledger: principal,
            source_subaccount: Some(vec![1; 32]),
            destination: Account {
                owner: Principal::from_slice(&[2; 29]),
                subaccount: None,
            },
            amount_e8s: 1,
            fee_e8s: 1,
            memo_u64: 1,
            created_at_time_nanos: 1,
            state: TransferState::Submitted {
                epoch: 0,
                first_submitted_at_nanos: 1,
                last_submitted_at_nanos: 1,
            },
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
