use candid::{CandidType, Principal};
use io_reward_policy::NeuronSnapshot;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MockSnsNeuron {
    pub neuron_id: u64,
    pub staked_io_e8s: u128,
    pub eligible_seconds: u64,
    pub eligible_closed_proposals: u64,
    pub voted_closed_proposals: u64,
    pub is_genesis_governance_neuron: bool,
    pub is_protocol_owned: bool,
    pub is_dissolving: bool,
}

impl From<MockSnsNeuron> for NeuronSnapshot {
    fn from(value: MockSnsNeuron) -> Self {
        Self {
            neuron_id: value.neuron_id,
            staked_io_e8s: value.staked_io_e8s,
            eligible_seconds: value.eligible_seconds,
            eligible_closed_proposals: value.eligible_closed_proposals,
            voted_closed_proposals: value.voted_closed_proposals,
            is_genesis_governance_neuron: value.is_genesis_governance_neuron,
            is_protocol_owned: value.is_protocol_owned,
            is_dissolving: value.is_dissolving,
        }
    }
}

pub async fn debug_list_neurons(canister: Principal) -> Result<Vec<NeuronSnapshot>, String> {
    let response = ic_cdk::call::Call::bounded_wait(canister, "debug_list_neurons")
        .await
        .map_err(|err| format!("sns governance neuron scan failed: {err:?}"))
        .and_then(|response| {
            response
                .candid_tuple::<(Vec<MockSnsNeuron>,)>()
                .map_err(|err| format!("sns governance neuron decode failed: {err:?}"))
        })?;
    Ok(response.0.into_iter().map(Into::into).collect())
}
