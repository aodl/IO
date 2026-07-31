use candid::CandidType;
use serde::Deserialize;

use crate::state::Account;

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum MaturityKind {
    TwoYear,
    TwoWeek,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PendingMaturity {
    pub kind: MaturityKind,
    pub neuron_id: u64,
    pub original_maturity_e8s: u64,
    pub staked_maturity_e8s: u64,
    pub remaining_maturity_e8s: u64,
    pub amount_disbursed_e8s: Option<u64>,
    pub destination: Account,
    pub requested_at_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StakeMaturity {
    pub percentage_to_stake: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StakeMaturityResponse {
    pub maturity_e8s: u64,
    pub staked_maturity_e8s: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DisburseMaturity {
    pub percentage_to_disburse: u32,
    pub to_account: Option<NnsAccount>,
    pub to_account_identifier: Option<NnsAccountIdentifier>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsAccount {
    pub owner: Option<candid::Principal>,
    pub subaccount: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsAccountIdentifier {
    pub hash: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DisburseMaturityResponse {
    pub amount_disbursed_e8s: Option<u64>,
}

pub fn commands() -> (StakeMaturity, u32) {
    (
        StakeMaturity {
            percentage_to_stake: Some(40),
        },
        100,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn direct_policy_is_stake_40_then_disburse_all_remaining() {
        let (stake, disburse) = super::commands();
        assert_eq!(stake.percentage_to_stake, Some(40));
        assert_eq!(disburse, 100);
    }
}
