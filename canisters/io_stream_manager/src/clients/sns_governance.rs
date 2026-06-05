use candid::{CandidType, Principal};
use io_governance_types::{
    SnsDissolveState, SnsGovernanceClient, SnsGovernanceError, SnsNeuron, SnsNeuronId,
    SnsNeuronPage, SnsNeuronPageRequest, SnsParticipationSummary, SnsProposal, SnsProposalId,
    SnsProposalPage, SnsProposalPageRequest,
};
use io_reward_policy::{sns_neuron_id_to_u64, NeuronSnapshot, SnsNeuronIdConversionError};
use serde::Deserialize;
use std::future::Future;
use std::pin::Pin;

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

impl From<MockSnsNeuron> for SnsNeuron {
    fn from(value: MockSnsNeuron) -> Self {
        let dissolve_state = if value.is_dissolving {
            SnsDissolveState::Dissolving {
                when_dissolved_timestamp_seconds: 0,
            }
        } else {
            SnsDissolveState::NotDissolving {
                dissolve_delay_seconds: value.eligible_seconds,
            }
        };
        Self {
            id: SnsNeuronId(value.neuron_id.to_be_bytes().to_vec()),
            controller: None,
            stake_e8s: value.staked_io_e8s,
            dissolve_delay_seconds: value.eligible_seconds,
            dissolve_state,
            cached_neuron_stake_e8s: value.staked_io_e8s,
            voting_power: value.staked_io_e8s,
            permissions: Vec::new(),
            is_io_protocol_neuron: value.is_protocol_owned,
            is_jupiter_governance_neuron: value.is_genesis_governance_neuron,
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MockSnsGovernanceClient {
    pub canister: Principal,
}

impl SnsGovernanceClient for MockSnsGovernanceClient {
    fn list_neurons<'a>(
        &'a self,
        _page: SnsNeuronPageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SnsNeuronPage, SnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            let neurons = debug_list_mock_neurons(self.canister)
                .await?
                .into_iter()
                .map(Into::into)
                .collect();
            Ok(SnsNeuronPage {
                neurons,
                next_page_at: None,
            })
        })
    }

    fn get_neuron<'a>(
        &'a self,
        id: SnsNeuronId,
    ) -> Pin<Box<dyn Future<Output = Result<SnsNeuron, SnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            self.list_neurons(SnsNeuronPageRequest {
                limit: 1_000,
                start_page_at: None,
            })
            .await?
            .neurons
            .into_iter()
            .find(|neuron| neuron.id == id)
            .ok_or(SnsGovernanceError::NotFound)
        })
    }

    fn list_proposals<'a>(
        &'a self,
        _request: SnsProposalPageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SnsProposalPage, SnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            Ok(SnsProposalPage {
                proposals: Vec::new(),
                next_before_proposal: None,
            })
        })
    }

    fn get_proposal<'a>(
        &'a self,
        _id: SnsProposalId,
    ) -> Pin<Box<dyn Future<Output = Result<SnsProposal, SnsGovernanceError>> + 'a>> {
        Box::pin(async move { Err(SnsGovernanceError::Unsupported) })
    }
}

pub fn participation_summary_to_snapshot(
    eligibility: &io_governance_types::SnsNeuronEligibility,
    summary: &SnsParticipationSummary,
) -> Result<Option<NeuronSnapshot>, SnsNeuronIdConversionError> {
    if eligibility.excluded_reason.is_some() {
        return Ok(None);
    }
    Ok(Some(NeuronSnapshot {
        neuron_id: sns_neuron_id_to_u64(&eligibility.neuron_id)?,
        staked_io_e8s: eligibility.eligible_stake_e8s,
        eligible_seconds: 1,
        eligible_closed_proposals: summary.eligible_closed_proposals_total,
        voted_closed_proposals: summary.voted_proposals,
        is_genesis_governance_neuron: false,
        is_protocol_owned: false,
        is_dissolving: !eligibility.is_non_dissolving,
    }))
}

async fn debug_list_mock_neurons(
    canister: Principal,
) -> Result<Vec<MockSnsNeuron>, SnsGovernanceError> {
    let response = ic_cdk::call::Call::bounded_wait(canister, "debug_list_neurons")
        .await
        .map_err(|err| SnsGovernanceError::CanisterCallFailed {
            method: "debug_list_neurons".to_string(),
            message: format!("{err:?}"),
        })
        .and_then(|response| {
            response
                .candid_tuple::<(Vec<MockSnsNeuron>,)>()
                .map_err(|err| SnsGovernanceError::DecodeError {
                    message: format!("{err:?}"),
                })
        })?;
    Ok(response.0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use io_governance_types::SnsNeuronEligibility;

    fn eligibility(id: SnsNeuronId) -> SnsNeuronEligibility {
        SnsNeuronEligibility {
            neuron_id: id,
            owner: None,
            eligible_stake_e8s: 1_000,
            eligible_since_seconds: 0,
            dissolve_delay_seconds: 14 * 24 * 60 * 60,
            is_non_dissolving: true,
            excluded_reason: None,
        }
    }

    fn summary(id: SnsNeuronId) -> SnsParticipationSummary {
        SnsParticipationSummary {
            neuron_id: id,
            eligible_closed_proposals_total: 2,
            voted_proposals: 2,
            participation_bps: 10_000,
        }
    }

    #[test]
    fn participation_summary_snapshot_converts_eight_byte_sns_neuron_id() {
        let id = SnsNeuronId(42u64.to_be_bytes().to_vec());
        let snapshot = participation_summary_to_snapshot(&eligibility(id.clone()), &summary(id))
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.neuron_id, 42);
    }

    #[test]
    fn participation_summary_snapshot_rejects_non_eight_byte_sns_neuron_id() {
        let id = SnsNeuronId(vec![0]);
        assert_eq!(
            participation_summary_to_snapshot(&eligibility(id.clone()), &summary(id)),
            Err(SnsNeuronIdConversionError::InvalidLength { actual_len: 1 })
        );
    }

    #[test]
    fn invalid_id_neuron_is_excluded_before_reward_allocation() {
        let valid_id = SnsNeuronId(7u64.to_be_bytes().to_vec());
        let invalid_id = SnsNeuronId(vec![1, 2, 3]);
        let inputs = [
            (eligibility(valid_id.clone()), summary(valid_id)),
            (eligibility(invalid_id.clone()), summary(invalid_id)),
        ];

        let mut conversion_errors = Vec::new();
        let snapshots: Vec<NeuronSnapshot> = inputs
            .iter()
            .filter_map(|(eligibility, summary)| {
                match participation_summary_to_snapshot(eligibility, summary) {
                    Ok(snapshot) => snapshot,
                    Err(err) => {
                        conversion_errors.push(err);
                        None
                    }
                }
            })
            .collect();

        assert_eq!(
            conversion_errors,
            vec![SnsNeuronIdConversionError::InvalidLength { actual_len: 3 }]
        );
        let out = io_reward_policy::allocate_rewards(100, &snapshots);
        assert_eq!(out.allocations.len(), 1);
        assert_eq!(out.allocations[0].neuron_id, 7);
        assert_eq!(out.allocations[0].io_e8s, 100);
    }
}
