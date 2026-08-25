use candid::{CandidType, Nat, Principal};
use io_accounts::Account;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::state::DispatchEpoch;

pub const MAX_MEMO_BYTES: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum OwnTransferIntent {
    Icrc1 {
        ledger: Principal,
        from_subaccount: [u8; 32],
        to: Account,
        amount: u128,
        fee: u128,
        memo: Vec<u8>,
        created_at_time: u64,
    },
    Icrc2TransferFrom {
        ledger: Principal,
        spender_subaccount: [u8; 32],
        from: Account,
        to: Account,
        amount: u128,
        fee: u128,
        memo: Vec<u8>,
        created_at_time: u64,
    },
}

impl OwnTransferIntent {
    pub fn validate(&self) -> Result<(), String> {
        let (ledger, source, to, amount, fee, memo, created_at_time) = match self {
            Self::Icrc1 {
                ledger,
                from_subaccount,
                to,
                amount,
                fee,
                memo,
                created_at_time,
            } => (
                ledger,
                Account {
                    owner: Principal::anonymous(),
                    subaccount: Some(from_subaccount.to_vec()),
                },
                to,
                amount,
                fee,
                memo,
                created_at_time,
            ),
            Self::Icrc2TransferFrom {
                ledger,
                from,
                to,
                amount,
                fee,
                memo,
                created_at_time,
                ..
            } => (ledger, from.clone(), to, amount, fee, memo, created_at_time),
        };
        if *ledger == Principal::anonymous() || *ledger == Principal::management_canister() {
            return Err("transfer ledger principal is forbidden".into());
        }
        source.validate()?;
        to.validate()?;
        if *amount == 0 || *fee == 0 || *created_at_time == 0 {
            return Err("amount, fee and created_at_time must be non-zero".into());
        }
        if memo.len() > MAX_MEMO_BYTES {
            return Err("memo exceeds launch bound".into());
        }
        if matches!(self, Self::Icrc2TransferFrom { .. }) && source.effective_eq(to)? {
            return Err("transfer source and destination must differ".into());
        }
        Ok(())
    }

    pub fn ledger(&self) -> Principal {
        match self {
            Self::Icrc1 { ledger, .. } | Self::Icrc2TransferFrom { ledger, .. } => *ledger,
        }
    }

    pub fn created_at_time(&self) -> u64 {
        match self {
            Self::Icrc1 {
                created_at_time, ..
            }
            | Self::Icrc2TransferFrom {
                created_at_time, ..
            } => *created_at_time,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum TransferState {
    Prepared,
    Submitted {
        epoch: DispatchEpoch,
        first_submitted_at: u64,
        last_submitted_at: u64,
    },
    Succeeded {
        block: u128,
    },
    Stuck {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TransferAttempt {
    pub intent: OwnTransferIntent,
    pub state: TransferState,
}

impl TransferAttempt {
    pub fn prepared(intent: OwnTransferIntent) -> Result<Self, String> {
        intent.validate()?;
        Ok(Self {
            intent,
            state: TransferState::Prepared,
        })
    }

    pub fn succeeded_block(&self) -> Result<u128, String> {
        match self.state {
            TransferState::Succeeded { block } => Ok(block),
            _ => Err("transfer lacks success evidence".into()),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        self.intent.validate()?;
        match &self.state {
            TransferState::Prepared => {}
            TransferState::Submitted {
                epoch,
                first_submitted_at,
                last_submitted_at,
            } => {
                if epoch.0 == 0
                    || *first_submitted_at == 0
                    || last_submitted_at < first_submitted_at
                {
                    return Err("submitted transfer epoch/timestamps are invalid".into());
                }
            }
            TransferState::Succeeded { .. } => {}
            TransferState::Stuck { reason } if reason.is_empty() || reason.len() > 512 => {
                return Err("stuck transfer reason is invalid".into())
            }
            TransferState::Stuck { .. } => {}
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

pub enum ClassifiedResult {
    Succeeded(u128),
    NoEffect(String),
    Ambiguous(String),
}

pub fn classify_result(result: TransferResult) -> Result<ClassifiedResult, String> {
    Ok(match result {
        Ok(block) => ClassifiedResult::Succeeded(nat_to_u128(block)?),
        Err(TransferError::Duplicate { duplicate_of }) => {
            ClassifiedResult::Succeeded(nat_to_u128(duplicate_of)?)
        }
        Err(
            error @ (TransferError::BadFee { .. }
            | TransferError::BadBurn { .. }
            | TransferError::InsufficientFunds { .. }
            | TransferError::InsufficientAllowance { .. }
            | TransferError::CreatedInFuture { .. }),
        ) => ClassifiedResult::NoEffect(format!("{error:?}")),
        Err(error) => ClassifiedResult::Ambiguous(format!("{error:?}")),
    })
}

pub fn deterministic_memo(domain: &[u8], principal: Principal, nonce: u64) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(principal.as_slice());
    hasher.update(nonce.to_be_bytes());
    hasher.finalize()[..MAX_MEMO_BYTES].to_vec()
}

pub fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_duplicate_is_success_evidence() {
        assert!(matches!(
            classify_result(Err(TransferError::Duplicate {
                duplicate_of: Nat::from(9u8)
            }))
            .unwrap(),
            ClassifiedResult::Succeeded(9)
        ));
    }
}
