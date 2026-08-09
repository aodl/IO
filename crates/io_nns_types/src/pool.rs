use candid::CandidType;
use serde::Deserialize;
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum UnwindPhase {
    SplitPrepared,
    SplitSubmitted,
    ChildCreated,
    StartDissolvingSubmitted,
    Dissolving,
    StopDissolvingSubmitted,
    MergePrepared,
    MergeSubmitted,
    ReadyToDisburse,
    DisburseSubmitted,
    AwaitingTransferProof {
        block_index: Option<u128>,
        submitted_at_seconds: u64,
    },
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct UnwindOperation {
    pub operation_sequence: u64,
    pub target_e8s: u128,
    pub excess_e8s: u128,
    pub child_neuron_id: u64,
    pub principal_e8s: u128,
    pub child_staking_subaccount: Vec<u8>,
    pub phase: UnwindPhase,
}

impl UnwindOperation {
    pub fn validate(&self, next_operation_sequence: u64) -> Result<(), String> {
        if self.operation_sequence == 0
            || self.operation_sequence >= next_operation_sequence
            || self.excess_e8s == 0
        {
            return Err("direct unwind operation is inconsistent".into());
        }
        let before_child = matches!(
            self.phase,
            UnwindPhase::SplitPrepared | UnwindPhase::SplitSubmitted
        );
        if (before_child && (self.child_neuron_id != 0 || self.principal_e8s != 0))
            || (!before_child && (self.child_neuron_id == 0 || self.principal_e8s == 0))
            || (self.child_staking_subaccount.len() != 32
                && !(self.child_staking_subaccount.is_empty()
                    && (before_child || self.phase == UnwindPhase::ChildCreated)))
        {
            return Err("direct unwind child evidence is inconsistent".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation(phase: UnwindPhase) -> UnwindOperation {
        UnwindOperation {
            operation_sequence: 1,
            target_e8s: 1_000_000,
            excess_e8s: 300_000,
            child_neuron_id: 7,
            principal_e8s: 290_000,
            child_staking_subaccount: vec![9; 32],
            phase,
        }
    }

    #[test]
    fn split_phases_have_no_child_and_later_phases_have_exact_child_evidence() {
        let mut split = operation(UnwindPhase::SplitPrepared);
        split.child_neuron_id = 0;
        split.principal_e8s = 0;
        split.child_staking_subaccount.clear();
        assert_eq!(split.validate(2), Ok(()));
        assert_eq!(operation(UnwindPhase::Dissolving).validate(2), Ok(()));
    }

    #[test]
    fn malformed_child_evidence_is_rejected() {
        let mut missing = operation(UnwindPhase::Dissolving);
        missing.child_neuron_id = 0;
        assert!(missing.validate(2).is_err());
        let mut wrong_account = operation(UnwindPhase::ReadyToDisburse);
        wrong_account.child_staking_subaccount.pop();
        assert!(wrong_account.validate(2).is_err());
    }

    #[test]
    fn passive_child_has_only_the_canonical_dissolving_phase() {
        let child = operation(UnwindPhase::Dissolving);
        assert_eq!(child.validate(2), Ok(()));
        assert!(matches!(child.phase, UnwindPhase::Dissolving));
    }
}
