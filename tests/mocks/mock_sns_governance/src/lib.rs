use candid::CandidType;
use serde::Deserialize;
use std::cell::RefCell;

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

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MockProposal {
    pub proposal_id: u64,
    pub closed: bool,
}

#[derive(Default)]
struct SnsState {
    neurons: Vec<MockSnsNeuron>,
    proposals: Vec<MockProposal>,
}

thread_local! {
    static STATE: RefCell<SnsState> = RefCell::new(SnsState::default());
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_add_neuron(neuron: MockSnsNeuron) {
    STATE.with(|cell| cell.borrow_mut().neurons.push(neuron));
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_neuron_dissolve_state(args: (u64, bool)) -> Result<(), String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let neuron = state
            .neurons
            .iter_mut()
            .find(|n| n.neuron_id == args.0)
            .ok_or_else(|| "unknown neuron".to_string())?;
        neuron.is_dissolving = args.1;
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_add_proposal(proposal_id: u64) {
    STATE.with(|cell| {
        cell.borrow_mut().proposals.push(MockProposal {
            proposal_id,
            closed: false,
        })
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_vote(args: (u64, u64)) -> Result<(), String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let neuron = state
            .neurons
            .iter_mut()
            .find(|n| n.neuron_id == args.0)
            .ok_or_else(|| "unknown neuron".to_string())?;
        neuron.voted_closed_proposals = neuron.voted_closed_proposals.saturating_add(1);
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_close_proposal(proposal_id: u64) -> Result<(), String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let proposal = state
            .proposals
            .iter_mut()
            .find(|p| p.proposal_id == proposal_id)
            .ok_or_else(|| "unknown proposal".to_string())?;
        proposal.closed = true;
        for neuron in &mut state.neurons {
            neuron.eligible_closed_proposals = neuron.eligible_closed_proposals.saturating_add(1);
        }
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_list_neurons() -> Vec<MockSnsNeuron> {
    STATE.with(|cell| cell.borrow().neurons.clone())
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_list_closed_proposals() -> Vec<MockProposal> {
    STATE.with(|cell| {
        cell.borrow()
            .proposals
            .iter()
            .filter(|p| p.closed)
            .cloned()
            .collect()
    })
}
