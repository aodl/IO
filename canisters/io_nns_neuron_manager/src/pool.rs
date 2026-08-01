use candid::CandidType;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum UnwindPhase {
    Split,
    Dissolving,
    Ready,
    Disbursing,
    Stuck,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PendingUnwind {
    pub generation: u64,
    pub child_neuron_id: u64,
    pub principal_e8s: u128,
    pub phase: UnwindPhase,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct UnwindOperation {
    pub operation_sequence: u64,
    pub dispatch_epoch: u64,
    pub generation: u64,
    pub child_neuron_id: u64,
    pub phase: UnwindPhase,
}

impl UnwindOperation {
    pub fn validate(
        &self,
        next_operation_sequence: u64,
        parent_neuron_id: u64,
    ) -> Result<(), String> {
        if self.operation_sequence == 0
            || self.operation_sequence >= next_operation_sequence
            || self.generation == 0
            || self.child_neuron_id == 0
            || self.child_neuron_id == parent_neuron_id
        {
            return Err("pool rebalance operation is inconsistent".into());
        }
        Ok(())
    }
}
