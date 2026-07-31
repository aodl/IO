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
