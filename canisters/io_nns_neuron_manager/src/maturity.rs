use candid::CandidType;
use serde::Deserialize;

use crate::{jupiter::NeuronSnapshot, state::Account};

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum MaturityKind {
    TwoYear,
    TwoWeek,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MaturityPlan {
    pub neuron: NeuronSnapshot,
    pub original_maturity_e8s: u64,
    pub original_staked_maturity_e8s: u64,
    pub stake_maturity_e8s: u64,
    pub destination: Account,
    pub requested_at_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StakeMaturitySucceeded {
    pub plan: MaturityPlan,
    pub remaining_maturity_e8s: u64,
    pub staked_maturity_e8s: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct AwaitingMintProof {
    pub stake: StakeMaturitySucceeded,
    pub amount_disbursed_e8s: u64,
    pub expected_finalization_timestamp_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DisburseMaturitySucceeded {
    pub stake: StakeMaturitySucceeded,
    pub amount_disbursed_e8s: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum MaturityPhase {
    Observed(MaturityPlan),
    StakeMaturitySubmitted(MaturityPlan),
    StakeMaturitySucceeded(StakeMaturitySucceeded),
    DisburseMaturitySubmitted(StakeMaturitySucceeded),
    DisburseMaturitySucceeded(DisburseMaturitySucceeded),
    AwaitingMintProof(AwaitingMintProof),
    MintProved {
        proof: AwaitingMintProof,
        mint_block: u128,
        actual_minted_e8s: u128,
    },
    DeliveringTwoWeekReceipt {
        proof: AwaitingMintProof,
        mint_block: u128,
        actual_minted_e8s: u128,
    },
    Stuck {
        reason: String,
        plan: Box<MaturityPlan>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PendingMaturity {
    pub operation_sequence: u64,
    pub dispatch_epoch: u64,
    pub kind: MaturityKind,
    pub phase: MaturityPhase,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CompletedMaturity {
    pub kind: MaturityKind,
    pub neuron_id: u64,
    pub mint_block: u128,
    pub actual_minted_e8s: u128,
    pub destination: Account,
    pub completed_at_nanos: u64,
}

impl PendingMaturity {
    pub fn plan(&self) -> &MaturityPlan {
        match &self.phase {
            MaturityPhase::Observed(plan) | MaturityPhase::StakeMaturitySubmitted(plan) => plan,
            MaturityPhase::StakeMaturitySucceeded(value)
            | MaturityPhase::DisburseMaturitySubmitted(value) => &value.plan,
            MaturityPhase::DisburseMaturitySucceeded(value) => &value.stake.plan,
            MaturityPhase::AwaitingMintProof(value)
            | MaturityPhase::MintProved { proof: value, .. }
            | MaturityPhase::DeliveringTwoWeekReceipt { proof: value, .. } => &value.stake.plan,
            MaturityPhase::Stuck { plan, .. } => plan,
        }
    }

    pub fn validate(
        &self,
        expected_kind: MaturityKind,
        expected_neuron_id: u64,
        expected_destination: &Account,
        next_operation_sequence: u64,
    ) -> Result<(), String> {
        if self.operation_sequence == 0
            || self.operation_sequence >= next_operation_sequence
            || self.kind != expected_kind
        {
            return Err("pending maturity identity is malformed".into());
        }
        let plan = self.plan();
        let expected_stake = plan
            .original_maturity_e8s
            .checked_mul(40)
            .ok_or("maturity stake calculation overflow")?
            / 100;
        if plan.neuron.neuron_id != expected_neuron_id
            || plan.original_maturity_e8s == 0
            || plan.stake_maturity_e8s != expected_stake
            || plan.requested_at_seconds == 0
            || !plan.destination.effective_eq(expected_destination)?
        {
            return Err("pending maturity plan is inconsistent".into());
        }
        if let MaturityPhase::Stuck { reason, .. } = &self.phase {
            return if reason.is_empty() || reason.len() > 512 {
                Err("pending maturity Stuck reason is malformed".into())
            } else {
                Ok(())
            };
        }
        if let MaturityPhase::AwaitingMintProof(value)
        | MaturityPhase::MintProved { proof: value, .. }
        | MaturityPhase::DeliveringTwoWeekReceipt { proof: value, .. } = &self.phase
        {
            if value.amount_disbursed_e8s == 0
                || value.expected_finalization_timestamp_seconds <= plan.requested_at_seconds
            {
                return Err("maturity finalization evidence is malformed".into());
            }
        }
        Ok(())
    }
}

pub fn commands() -> (u32, u32) {
    (40, 100)
}

pub fn progress(pending: &PendingMaturity) -> crate::api::MaturityProgress {
    use crate::api::MaturityProgress;
    match &pending.phase {
        MaturityPhase::Observed(_) => MaturityProgress::Observed,
        MaturityPhase::StakeMaturitySubmitted(_) => MaturityProgress::StakeMaturitySubmitted,
        MaturityPhase::StakeMaturitySucceeded(_) => MaturityProgress::StakeMaturitySucceeded,
        MaturityPhase::DisburseMaturitySubmitted(_) => MaturityProgress::DisburseMaturitySubmitted,
        MaturityPhase::DisburseMaturitySucceeded(_) => MaturityProgress::DisburseMaturitySucceeded,
        MaturityPhase::AwaitingMintProof(_) => MaturityProgress::AwaitingMintProof,
        MaturityPhase::MintProved { .. } => MaturityProgress::MintProved,
        MaturityPhase::DeliveringTwoWeekReceipt { .. } => {
            MaturityProgress::DeliveringTwoWeekReceipt
        }
        MaturityPhase::Stuck { reason, .. } => MaturityProgress::Stuck(reason.clone()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn direct_policy_is_stake_40_then_disburse_all_remaining() {
        assert_eq!(super::commands(), (40, 100));
    }
}
