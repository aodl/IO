//! Canonical ICRC account representation for value-moving IO canisters.

use candid::{CandidType, Principal};
use serde::Deserialize;

pub const ZERO_SUBACCOUNT: [u8; 32] = [0; 32];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, CandidType, Deserialize)]
pub struct Account {
    pub owner: Principal,
    pub subaccount: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CanonicalAccount {
    pub owner: Principal,
    pub subaccount: [u8; 32],
}

impl Account {
    pub fn canonical(&self) -> Result<CanonicalAccount, String> {
        let subaccount = match self.subaccount.as_deref() {
            None => ZERO_SUBACCOUNT,
            Some(bytes) => bytes
                .try_into()
                .map_err(|_| "subaccount must contain exactly 32 bytes")?,
        };
        Ok(CanonicalAccount {
            owner: self.owner,
            subaccount,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        self.canonical().map(|_| ())
    }

    pub fn effective_eq(&self, other: &Self) -> Result<bool, String> {
        Ok(self.canonical()? == other.canonical()?)
    }

    pub fn from_canonical(value: CanonicalAccount) -> Self {
        Self {
            owner: value.owner,
            subaccount: (value.subaccount != ZERO_SUBACCOUNT).then(|| value.subaccount.to_vec()),
        }
    }
}

impl CanonicalAccount {
    pub fn candid(self) -> Account {
        Account::from_canonical(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_and_zero_subaccounts_are_identical() {
        let owner = Principal::from_slice(&[1]);
        let null = Account {
            owner,
            subaccount: None,
        };
        let zero = Account {
            owner,
            subaccount: Some(vec![0; 32]),
        };
        assert!(null.effective_eq(&zero).unwrap());
        assert_eq!(zero.canonical().unwrap().candid().subaccount, None);
    }

    #[test]
    fn malformed_subaccount_is_rejected() {
        let account = Account {
            owner: Principal::from_slice(&[1]),
            subaccount: Some(vec![0; 31]),
        };
        assert!(account.canonical().is_err());
    }
}
