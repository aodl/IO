use candid::CandidType;
use serde::Deserialize;

use crate::backing::{CohortProofState, MAX_LIVE_UNWIND_COHORTS};

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum UnwindPhase {
    SplitPrepared,
    SplitSubmitted,
    ChildIdentified,
    SplitProved,
    StartDissolvingSubmitted,
    StartDissolvingProved,
    DisbursementPrepared,
    DisbursementSubmitted,
    PrincipalReturned,
    DelayIncreaseSubmitted,
    DelayIncreaseProved,
    MergePrepared,
    MergeSubmitted,
    MergeProved,
    CleanupProved,
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct UnwindOperation {
    pub operation_sequence: u64,
    pub generation: u64,
    pub target_e8s: u128,
    pub gross_e8s: u128,
    pub child_neuron_id: u64,
    pub principal_e8s: u128,
    pub child_staking_subaccount: Vec<u8>,
    pub submitted_at_seconds: u64,
    pub expected_block_index: Option<u128>,
    pub child_maturity_e8s: u128,
    pub parent_maturity_e8s: u128,
    pub phase: UnwindPhase,
}

impl UnwindOperation {
    pub fn validate(&self, next_operation_sequence: u64) -> Result<(), String> {
        let before_child = matches!(
            self.phase,
            UnwindPhase::SplitPrepared | UnwindPhase::SplitSubmitted
        );
        let identified = self.phase == UnwindPhase::ChildIdentified;
        if self.operation_sequence == 0
            || self.operation_sequence >= next_operation_sequence
            || self.generation == 0
            || self.gross_e8s == 0
            || (self.phase == UnwindPhase::DisbursementSubmitted && self.submitted_at_seconds == 0)
            || (before_child && (self.child_neuron_id != 0 || self.principal_e8s != 0))
            || (identified && (self.child_neuron_id == 0 || self.principal_e8s != 0))
            || (!before_child
                && !identified
                && (self.child_neuron_id == 0 || self.principal_e8s == 0))
            || (!before_child && !identified && self.child_staking_subaccount.len() != 32)
        {
            return Err("unwind command evidence is inconsistent".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PassiveCohort {
    pub generation: u64,
    pub child_neuron_id: u64,
    pub principal_e8s: u128,
    pub child_staking_subaccount: Vec<u8>,
    pub ready_at_seconds: u64,
    pub proof: CohortProofState,
    pub disbursement_block: Option<u128>,
}

pub fn validate_cohorts(cohorts: &[PassiveCohort]) -> Result<(), String> {
    if cohorts.len() > MAX_LIVE_UNWIND_COHORTS {
        return Err("live unwind cohort capacity exceeded".into());
    }
    let mut previous = None;
    for cohort in cohorts {
        if cohort.generation == 0
            || cohort.child_neuron_id == 0
            || cohort.principal_e8s == 0
            || cohort.child_staking_subaccount.len() != 32
            || previous
                .replace(cohort.generation)
                .is_some_and(|value| value >= cohort.generation)
        {
            return Err("live unwind cohorts are malformed or unsorted".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cohort(generation: u64) -> PassiveCohort {
        PassiveCohort {
            generation,
            child_neuron_id: generation + 100,
            principal_e8s: 100,
            child_staking_subaccount: vec![generation as u8; 32],
            ready_at_seconds: generation + 1_000,
            proof: CohortProofState::Dissolving,
            disbursement_block: None,
        }
    }

    #[test]
    fn live_cohorts_are_sorted_unique_and_bounded() {
        let mut cohorts = (1..=MAX_LIVE_UNWIND_COHORTS as u64)
            .map(cohort)
            .collect::<Vec<_>>();
        assert_eq!(validate_cohorts(&cohorts), Ok(()));
        cohorts.push(cohort(33));
        assert!(validate_cohorts(&cohorts).is_err());
        assert!(validate_cohorts(&[cohort(2), cohort(1)]).is_err());
    }
}
