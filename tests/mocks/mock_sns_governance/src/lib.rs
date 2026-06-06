use candid::CandidType;
use io_governance_types::{
    SnsGovernanceError, SnsNeuron, SnsNeuronId, SnsNeuronPage, SnsNeuronPageRequest, SnsProposal,
    SnsProposalId, SnsProposalPage, SnsProposalPageRequest,
};
use serde::Deserialize;
use std::cell::RefCell;
use std::cmp::Reverse;

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
    governance_neurons: Vec<SnsNeuron>,
    governance_proposals: Vec<SnsProposal>,
}

thread_local! {
    static STATE: RefCell<SnsState> = RefCell::new(SnsState::default());
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_add_neuron(neuron: MockSnsNeuron) {
    STATE.with(|cell| cell.borrow_mut().neurons.push(neuron));
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_neurons(neurons: Vec<SnsNeuron>) {
    STATE.with(|cell| cell.borrow_mut().governance_neurons = neurons);
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
pub fn debug_set_proposals(proposals: Vec<SnsProposal>) {
    STATE.with(|cell| cell.borrow_mut().governance_proposals = proposals);
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
pub fn debug_list_governance_neurons(request: SnsNeuronPageRequest) -> SnsNeuronPage {
    STATE.with(|cell| {
        let mut neurons = cell.borrow().governance_neurons.clone();
        neurons.sort_by(|a, b| a.id.cmp(&b.id));
        let start = request
            .start_page_at
            .as_ref()
            .and_then(|cursor| neurons.iter().position(|neuron| neuron.id >= *cursor))
            .unwrap_or(0);
        let limit = request.limit as usize;
        let page = neurons
            .iter()
            .skip(start)
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        let next_page_at = neurons
            .get(start.saturating_add(limit))
            .map(|neuron| neuron.id.clone());
        SnsNeuronPage {
            neurons: page,
            next_page_at,
        }
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_governance_neuron(id: SnsNeuronId) -> Result<SnsNeuron, SnsGovernanceError> {
    STATE.with(|cell| {
        cell.borrow()
            .governance_neurons
            .iter()
            .find(|neuron| neuron.id == id)
            .cloned()
            .ok_or(SnsGovernanceError::NotFound)
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_list_proposals(request: SnsProposalPageRequest) -> SnsProposalPage {
    STATE.with(|cell| {
        let mut proposals = cell.borrow().governance_proposals.clone();
        proposals.sort_by_key(|proposal| Reverse(proposal.id));
        let filtered = proposals
            .into_iter()
            .filter(|proposal| {
                request
                    .before_proposal
                    .is_none_or(|cursor| proposal.id < cursor)
            })
            .collect::<Vec<_>>();
        let limit = request.limit as usize;
        let page = filtered.iter().take(limit).cloned().collect::<Vec<_>>();
        let next_before_proposal = (filtered.len() > limit)
            .then(|| page.last().map(|proposal| proposal.id))
            .flatten();
        SnsProposalPage {
            proposals: page,
            next_before_proposal,
        }
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_proposal(id: SnsProposalId) -> Result<SnsProposal, SnsGovernanceError> {
    STATE.with(|cell| {
        cell.borrow()
            .governance_proposals
            .iter()
            .find(|proposal| proposal.id == id)
            .cloned()
            .ok_or(SnsGovernanceError::NotFound)
    })
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

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_clear() {
    STATE.with(|cell| *cell.borrow_mut() = SnsState::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use io_governance_types::{
        SnsBallot, SnsDissolveState, SnsProposalRewardStatus, SnsProposalStatus, SnsVote,
    };

    #[test]
    fn governance_neurons_page_deterministically() {
        debug_clear();
        debug_set_neurons(vec![neuron(3), neuron(1), neuron(2)]);

        let page = debug_list_governance_neurons(SnsNeuronPageRequest {
            limit: 2,
            start_page_at: None,
        });
        assert_eq!(ids(&page.neurons), vec![1, 2]);
        assert_eq!(
            page.next_page_at,
            Some(SnsNeuronId(3u64.to_be_bytes().to_vec()))
        );

        let page = debug_list_governance_neurons(SnsNeuronPageRequest {
            limit: 2,
            start_page_at: page.next_page_at,
        });
        assert_eq!(ids(&page.neurons), vec![3]);
        assert_eq!(page.next_page_at, None);
    }

    #[test]
    fn governance_proposals_page_before_cursor_descending() {
        debug_clear();
        debug_set_proposals(vec![proposal(10), proposal(30), proposal(20)]);

        let page = debug_list_proposals(SnsProposalPageRequest {
            limit: 2,
            before_proposal: None,
        });
        assert_eq!(
            page.proposals.iter().map(|p| p.id.0).collect::<Vec<_>>(),
            vec![30, 20]
        );
        assert_eq!(page.next_before_proposal, Some(SnsProposalId(20)));

        let page = debug_list_proposals(SnsProposalPageRequest {
            limit: 2,
            before_proposal: Some(SnsProposalId(20)),
        });
        assert_eq!(
            page.proposals.iter().map(|p| p.id.0).collect::<Vec<_>>(),
            vec![10]
        );
    }

    #[test]
    fn get_proposal_reports_not_found() {
        debug_clear();
        debug_set_proposals(vec![proposal(1)]);
        assert_eq!(
            debug_get_proposal(SnsProposalId(1)).unwrap().id,
            SnsProposalId(1)
        );
        assert_eq!(
            debug_get_proposal(SnsProposalId(2)),
            Err(SnsGovernanceError::NotFound)
        );
    }

    fn ids(neurons: &[SnsNeuron]) -> Vec<u64> {
        neurons
            .iter()
            .map(|neuron| u64::from_be_bytes(neuron.id.0.as_slice().try_into().unwrap()))
            .collect()
    }

    fn neuron(id: u64) -> SnsNeuron {
        SnsNeuron {
            id: SnsNeuronId(id.to_be_bytes().to_vec()),
            controller: None,
            stake_e8s: 100,
            dissolve_delay_seconds: 1_209_600,
            dissolve_state: SnsDissolveState::NotDissolving {
                dissolve_delay_seconds: 1_209_600,
            },
            cached_neuron_stake_e8s: 100,
            voting_power: 100,
            permissions: Vec::new(),
            is_io_protocol_neuron: false,
            is_jupiter_governance_neuron: false,
        }
    }

    fn proposal(id: u64) -> SnsProposal {
        SnsProposal {
            id: SnsProposalId(id),
            topic: Some(1),
            status: SnsProposalStatus::Adopted,
            reward_status: SnsProposalRewardStatus::Settled,
            decided_timestamp_seconds: Some(10),
            ballots: vec![SnsBallot {
                neuron_id: SnsNeuronId(1u64.to_be_bytes().to_vec()),
                vote: SnsVote::Yes,
            }],
        }
    }
}
