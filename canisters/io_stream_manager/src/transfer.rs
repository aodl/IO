use candid::{CandidType, Nat, Principal};
use serde::Deserialize;

use crate::state::Account;

pub const MAX_MEMO_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum LedgerMethod {
    Icrc1Transfer,
    Icrc2TransferFrom,
    IcpTransfer,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum TransferState {
    Prepared,
    Submitted,
    Succeeded { block: u128 },
    Stuck { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct OwnTransferAttempt {
    pub ledger: Principal,
    pub method: LedgerMethod,
    pub source_subaccount: Option<Vec<u8>>,
    pub source_account: Option<Account>,
    pub destination: Account,
    pub amount_e8s: u128,
    pub fee_e8s: u128,
    pub memo: Vec<u8>,
    pub created_at_time_nanos: u64,
    pub state: TransferState,
}

impl OwnTransferAttempt {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .source_subaccount
            .as_ref()
            .is_some_and(|s| s.len() != 32)
        {
            return Err("source subaccount must contain exactly 32 bytes".into());
        }
        if self.memo.len() > MAX_MEMO_BYTES {
            return Err("memo exceeds launch bound".into());
        }
        self.destination.validate()?;
        if let Some(source) = &self.source_account {
            source.validate()?;
        }
        if self.amount_e8s == 0 || self.created_at_time_nanos == 0 {
            return Err("amount and created_at_time must be non-zero".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct IcrcTransferArg {
    pub from_subaccount: Option<Vec<u8>>,
    pub to: Account,
    pub amount: Nat,
    pub fee: Option<Nat>,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct IcrcTransferFromArg {
    pub spender_subaccount: Option<Vec<u8>>,
    pub from: Account,
    pub to: Account,
    pub amount: Nat,
    pub fee: Option<Nat>,
    pub memo: Option<Vec<u8>>,
    pub created_at_time: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum TransferError {
    BadFee { expected_fee: Nat },
    BadBurn { min_burn_amount: Nat },
    InsufficientFunds { balance: Nat },
    InsufficientAllowance { allowance: Nat },
    TooOld,
    CreatedInFuture { ledger_time: u64 },
    Duplicate { duplicate_of: Nat },
    TemporarilyUnavailable,
    GenericError { error_code: Nat, message: String },
}

pub type TransferResult = Result<Nat, TransferError>;

pub fn nat_to_u128(value: Nat) -> Result<u128, String> {
    value
        .0
        .try_into()
        .map_err(|_| "ledger value does not fit u128".into())
}

pub fn classify_result(
    result: TransferResult,
    attempt: &mut OwnTransferAttempt,
) -> Result<u128, String> {
    match result {
        Ok(block) => {
            let block = nat_to_u128(block)?;
            attempt.state = TransferState::Succeeded { block };
            Ok(block)
        }
        Err(TransferError::Duplicate { duplicate_of }) => {
            let block = nat_to_u128(duplicate_of)?;
            attempt.state = TransferState::Succeeded { block };
            Ok(block)
        }
        Err(
            error @ (TransferError::BadFee { .. }
            | TransferError::BadBurn { .. }
            | TransferError::InsufficientFunds { .. }
            | TransferError::InsufficientAllowance { .. }
            | TransferError::CreatedInFuture { .. }),
        ) => Err(format!("{error:?}")),
        Err(error) => {
            attempt.state = TransferState::Stuck {
                reason: format!("{error:?}"),
            };
            Err(format!("{error:?}"))
        }
    }
}

pub fn deterministic_memo(domain: &[u8], principal: Principal, nonce: u64) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(principal.as_slice());
    hasher.update(nonce.to_be_bytes());
    hasher.finalize()[..32].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_duplicate_is_success_evidence() {
        let principal = Principal::from_slice(&[1]);
        let mut attempt = OwnTransferAttempt {
            ledger: principal,
            method: LedgerMethod::Icrc1Transfer,
            source_subaccount: None,
            source_account: None,
            destination: Account {
                owner: principal,
                subaccount: None,
            },
            amount_e8s: 10,
            fee_e8s: 1,
            memo: vec![1],
            created_at_time_nanos: 1,
            state: TransferState::Submitted,
        };
        assert_eq!(
            classify_result(
                Err(TransferError::Duplicate {
                    duplicate_of: Nat::from(9u8)
                }),
                &mut attempt
            ),
            Ok(9)
        );
        assert_eq!(attempt.state, TransferState::Succeeded { block: 9 });
    }
}
