//! Canonical ICRC account representation for value-moving IO canisters.

use candid::{CandidType, Principal};
use serde::Deserialize;

pub const ZERO_SUBACCOUNT: [u8; 32] = [0; 32];

/// SHA-256("io-maturity-two-week-v1").
pub const TWO_WEEK_MATURITY_SUBACCOUNT: [u8; 32] = [
    0xf2, 0xa8, 0xf5, 0x95, 0xdf, 0xb1, 0x05, 0xf2, 0xc3, 0x13, 0x4b, 0x46, 0x6f, 0x6e, 0x8b, 0x51,
    0x02, 0x75, 0x2b, 0xa2, 0x50, 0x5b, 0x0e, 0xe5, 0xcb, 0x7d, 0x7e, 0x3d, 0xe1, 0xd5, 0x72, 0x66,
];

/// SHA-256("io-maturity-two-year-v1").
pub const TWO_YEAR_MATURITY_SUBACCOUNT: [u8; 32] = [
    0xda, 0xa2, 0xf7, 0x49, 0xbb, 0x8f, 0x89, 0x98, 0xf9, 0x4e, 0xe6, 0x5e, 0x18, 0x71, 0xdc, 0x84,
    0x44, 0x4d, 0xb3, 0x89, 0x65, 0x12, 0x38, 0x89, 0xf3, 0xca, 0xd6, 0xbc, 0x52, 0x1b, 0xa5, 0xa1,
];

pub fn two_week_maturity_staging(owner: Principal) -> Account {
    semantic_account(owner, TWO_WEEK_MATURITY_SUBACCOUNT)
}

pub fn two_year_maturity_staging(owner: Principal) -> Account {
    semantic_account(owner, TWO_YEAR_MATURITY_SUBACCOUNT)
}

fn semantic_account(owner: Principal, subaccount: [u8; 32]) -> Account {
    Account {
        owner,
        subaccount: Some(subaccount.to_vec()),
    }
}

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

    #[test]
    fn maturity_accounts_are_fixed_distinct_and_owned_by_the_manager() {
        let owner = Principal::from_slice(&[1]);
        let two_week = two_week_maturity_staging(owner);
        let two_year = two_year_maturity_staging(owner);
        assert_eq!(two_week.owner, owner);
        assert_eq!(two_year.owner, owner);
        assert_eq!(
            two_week.canonical().unwrap().subaccount,
            TWO_WEEK_MATURITY_SUBACCOUNT
        );
        assert_eq!(
            two_year.canonical().unwrap().subaccount,
            TWO_YEAR_MATURITY_SUBACCOUNT
        );
        assert_ne!(two_week, two_year);
    }
}
