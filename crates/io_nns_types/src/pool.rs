use candid::CandidType;
use serde::Deserialize;

use crate::backing::CohortProofState;

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
    pub reconciliation_request_fingerprint: Vec<u8>,
    pub target_e8s: u128,
    pub gross_e8s: u128,
    pub split_fee_e8s: u128,
    pub committed_disbursement_fee_e8s: u128,
    pub parent_principal_before_split_e8s: u128,
    pub child_neuron_id: u64,
    pub principal_e8s: u128,
    pub child_staking_subaccount: Vec<u8>,
    pub submitted_at_seconds: u64,
    pub expected_block_index: Option<u128>,
    pub child_maturity_e8s: u128,
    pub parent_maturity_e8s: u128,
    pub parent_principal_e8s: u128,
    pub phase: UnwindPhase,
}

impl UnwindOperation {
    pub fn validate(&self, next_operation_sequence: u64) -> Result<(), String> {
        let before_child = matches!(
            self.phase,
            UnwindPhase::SplitPrepared | UnwindPhase::SplitSubmitted
        );
        let before_submission = self.phase == UnwindPhase::SplitPrepared;
        let split_lifecycle = matches!(
            self.phase,
            UnwindPhase::SplitSubmitted
                | UnwindPhase::ChildIdentified
                | UnwindPhase::SplitProved
                | UnwindPhase::StartDissolvingSubmitted
                | UnwindPhase::StartDissolvingProved
        );
        let identified = self.phase == UnwindPhase::ChildIdentified;
        let maturity_cleanup = matches!(
            self.phase,
            UnwindPhase::DelayIncreaseSubmitted
                | UnwindPhase::DelayIncreaseProved
                | UnwindPhase::MergePrepared
                | UnwindPhase::MergeSubmitted
                | UnwindPhase::MergeProved
        );
        let cleanup_with_maturity = self.phase == UnwindPhase::CleanupProved
            && self.child_maturity_e8s > 0
            && self.parent_principal_e8s > 0;
        let stuck_with_maturity = matches!(self.phase, UnwindPhase::Stuck(_))
            && self.child_maturity_e8s > 0
            && self.parent_principal_e8s > 0;
        let no_maturity_evidence = self.child_maturity_e8s == 0
            && self.parent_maturity_e8s == 0
            && self.parent_principal_e8s == 0;
        if self.operation_sequence == 0
            || self.operation_sequence >= next_operation_sequence
            || self.generation == 0
            || self.reconciliation_request_fingerprint.len() != 32
            || self.gross_e8s == 0
            || self.gross_e8s > u128::from(u64::MAX)
            || (before_submission
                && (self.split_fee_e8s != 0
                    || self.committed_disbursement_fee_e8s != 0
                    || self.parent_principal_before_split_e8s != 0))
            || (split_lifecycle
                && (self.split_fee_e8s == 0
                    || self.committed_disbursement_fee_e8s == 0
                    || self.parent_principal_before_split_e8s < self.gross_e8s))
            || self.principal_e8s > u128::from(u64::MAX)
            || (self.phase == UnwindPhase::DisbursementSubmitted && self.submitted_at_seconds == 0)
            || (before_child && (self.child_neuron_id != 0 || self.principal_e8s != 0))
            || (identified
                && (self.child_neuron_id == 0
                    || self.principal_e8s == 0
                    || !self.child_staking_subaccount.is_empty()))
            || (!before_child
                && !identified
                && (self.child_neuron_id == 0 || self.principal_e8s == 0))
            || (!before_child && !identified && self.child_staking_subaccount.len() != 32)
            || (maturity_cleanup
                && (self.child_maturity_e8s == 0 || self.parent_principal_e8s == 0))
            || (!maturity_cleanup
                && !cleanup_with_maturity
                && !stuck_with_maturity
                && !no_maturity_evidence)
        {
            return Err("unwind command evidence is inconsistent".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PassiveCohort {
    pub generation: u64,
    pub reconciliation_request_fingerprint: Vec<u8>,
    pub child_neuron_id: u64,
    pub principal_e8s: u128,
    pub committed_fee_e8s: u128,
    pub child_staking_subaccount: Vec<u8>,
    pub ready_at_seconds: u64,
    pub proof: CohortProofState,
    pub disbursement_block: Option<u128>,
}

pub fn validate_cohorts(cohorts: &[PassiveCohort]) -> Result<(), String> {
    let mut previous = None;
    for cohort in cohorts {
        if cohort.generation == 0
            || cohort.reconciliation_request_fingerprint.len() != 32
            || cohort.child_neuron_id == 0
            || cohort.principal_e8s == 0
            || cohort.committed_fee_e8s == 0
            || cohort.principal_e8s <= cohort.committed_fee_e8s
            || cohort.principal_e8s > u128::from(u64::MAX)
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
            reconciliation_request_fingerprint: vec![generation as u8; 32],
            child_neuron_id: generation + 100,
            principal_e8s: 100,
            committed_fee_e8s: 10,
            child_staking_subaccount: vec![generation as u8; 32],
            ready_at_seconds: generation + 1_000,
            proof: CohortProofState::Dissolving,
            disbursement_block: None,
        }
    }

    #[test]
    fn historical_generations_are_not_a_product_capacity_limit() {
        let mut cohorts = (1..=64).map(cohort).collect::<Vec<_>>();
        assert_eq!(validate_cohorts(&cohorts), Ok(()));
        cohorts.push(cohort(65));
        assert_eq!(validate_cohorts(&cohorts), Ok(()));
        assert!(validate_cohorts(&[cohort(2), cohort(1)]).is_err());
    }

    #[test]
    fn cleanup_contradiction_can_retain_exact_evidence_in_stuck_state() {
        let mut operation = UnwindOperation {
            operation_sequence: 1,
            generation: 1,
            reconciliation_request_fingerprint: vec![1; 32],
            target_e8s: 1_000_000,
            gross_e8s: 120_000,
            split_fee_e8s: 10_000,
            committed_disbursement_fee_e8s: 10_000,
            parent_principal_before_split_e8s: 1_120_000,
            child_neuron_id: 2,
            principal_e8s: 110_000,
            child_staking_subaccount: vec![2; 32],
            submitted_at_seconds: 1,
            expected_block_index: None,
            child_maturity_e8s: 50_000,
            parent_maturity_e8s: 20_000,
            parent_principal_e8s: 1_001_000_000,
            phase: UnwindPhase::MergeProved,
        };
        assert_eq!(operation.validate(2), Ok(()));
        operation.phase = UnwindPhase::Stuck("cleanup conservation contradicted".into());
        assert_eq!(operation.validate(2), Ok(()));

        operation.parent_principal_e8s = 0;
        assert!(operation.validate(2).is_err());
    }
}
