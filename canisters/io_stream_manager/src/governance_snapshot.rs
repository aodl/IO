use io_governance_types::{
    snapshot_sns_eligibility, SnsEligibilityPolicy, SnsGovernanceClient, SnsGovernanceError,
    SnsNeuron, SnsNeuronEligibility, SnsNeuronId, SnsNeuronPageRequest, SnsProposal,
    SnsProposalPageRequest,
};
use io_reward_policy::{
    sns_neuron_id_is_canonical_staking_subaccount, sns_neuron_id_to_u64, SnsNeuronIdConversionError,
};
#[cfg(test)]
use std::cmp::Reverse;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceRewardSnapshotRequest {
    pub eligibility_policy: SnsEligibilityPolicy,
    pub max_neuron_pages: u64,
    pub max_proposal_pages: u64,
    pub page_limit: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceRewardObservation {
    pub eligibilities: Vec<SnsNeuronEligibility>,
    pub proposals: Vec<SnsProposal>,
    pub excluded_neurons: Vec<ExcludedGovernanceNeuron>,
    pub conversion_errors: Vec<SnsNeuronIdConversionError>,
    pub fetched_neuron_count: u64,
    pub fetched_proposal_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceActiveStakeSnapshotRequest {
    pub eligibility_policy: SnsEligibilityPolicy,
    pub max_neuron_pages: u64,
    pub page_limit: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceActiveStakeSnapshot {
    pub eligibilities: Vec<SnsNeuronEligibility>,
    pub active_staked_io_e8s: u128,
    pub excluded_neurons: Vec<ExcludedGovernanceNeuron>,
    pub fetched_neuron_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExcludedGovernanceNeuron {
    pub neuron_id: SnsNeuronId,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GovernanceSnapshotError {
    SnsGovernance(SnsGovernanceError),
    PaginationLimitExceeded,
    EmptyPageWithNextCursor,
    DuplicateNeuronId,
    DuplicateRewardNeuronId,
    DuplicateProposalId,
    InvalidPageLimit,
}

impl From<SnsGovernanceError> for GovernanceSnapshotError {
    fn from(value: SnsGovernanceError) -> Self {
        Self::SnsGovernance(value)
    }
}

pub async fn build_governance_reward_snapshot<C: SnsGovernanceClient>(
    client: &C,
    request: GovernanceRewardSnapshotRequest,
) -> Result<GovernanceRewardObservation, GovernanceSnapshotError> {
    if request.page_limit == 0 {
        return Err(GovernanceSnapshotError::InvalidPageLimit);
    }

    let neurons = fetch_all_neurons(client, request.page_limit, request.max_neuron_pages).await?;
    let proposals =
        fetch_all_proposals(client, request.page_limit, request.max_proposal_pages).await?;
    reject_duplicate_neurons(&neurons)?;
    reject_duplicate_proposals(&proposals)?;

    let fetched_neuron_count = neurons.len() as u64;
    let fetched_proposal_count = proposals.len() as u64;
    let mut eligibilities = snapshot_sns_eligibility(&neurons, &request.eligibility_policy);
    let (excluded_neurons, conversion_errors) =
        resolve_reward_eligibility_records(&mut eligibilities)?;

    Ok(GovernanceRewardObservation {
        eligibilities,
        proposals,
        excluded_neurons,
        conversion_errors,
        fetched_neuron_count,
        fetched_proposal_count,
    })
}

pub async fn build_governance_active_stake_snapshot<C: SnsGovernanceClient>(
    client: &C,
    request: GovernanceActiveStakeSnapshotRequest,
) -> Result<GovernanceActiveStakeSnapshot, GovernanceSnapshotError> {
    if request.page_limit == 0 {
        return Err(GovernanceSnapshotError::InvalidPageLimit);
    }

    let neurons = fetch_all_neurons(client, request.page_limit, request.max_neuron_pages).await?;
    reject_duplicate_neurons(&neurons)?;

    let fetched_neuron_count = neurons.len() as u64;
    let mut eligibilities = snapshot_sns_eligibility(&neurons, &request.eligibility_policy);
    let (excluded_neurons, _) = resolve_reward_eligibility_records(&mut eligibilities)?;
    let active_staked_io_e8s = eligibilities
        .iter()
        .filter(|eligibility| eligibility.excluded_reason.is_none())
        .map(|eligibility| eligibility.eligible_stake_e8s)
        .sum();

    Ok(GovernanceActiveStakeSnapshot {
        eligibilities,
        active_staked_io_e8s,
        excluded_neurons,
        fetched_neuron_count,
    })
}

fn resolve_reward_eligibility_records(
    eligibilities: &mut [SnsNeuronEligibility],
) -> Result<
    (
        Vec<ExcludedGovernanceNeuron>,
        Vec<SnsNeuronIdConversionError>,
    ),
    GovernanceSnapshotError,
> {
    let mut excluded_neurons = Vec::new();
    let mut conversion_errors = Vec::new();
    let mut seen_reward_ids = BTreeSet::new();
    for eligibility in eligibilities {
        if eligibility.excluded_reason.is_none() {
            if let Err(err) = sns_neuron_id_to_u64(&eligibility.neuron_id) {
                conversion_errors.push(err);
                eligibility.excluded_reason = Some("invalid SNS neuron id".to_string());
            } else if !sns_neuron_id_is_canonical_staking_subaccount(&eligibility.neuron_id) {
                eligibility.excluded_reason = Some("non-canonical SNS neuron id".to_string());
            } else {
                let reward_id = sns_neuron_id_to_u64(&eligibility.neuron_id)
                    .map_err(|_| GovernanceSnapshotError::DuplicateRewardNeuronId)?;
                if !seen_reward_ids.insert(reward_id) {
                    return Err(GovernanceSnapshotError::DuplicateRewardNeuronId);
                }
            }
        }
        if let Some(reason) = &eligibility.excluded_reason {
            excluded_neurons.push(ExcludedGovernanceNeuron {
                neuron_id: eligibility.neuron_id.clone(),
                reason: reason.clone(),
            });
        }
    }
    Ok((excluded_neurons, conversion_errors))
}

async fn fetch_all_neurons<C: SnsGovernanceClient>(
    client: &C,
    page_limit: u64,
    max_pages: u64,
) -> Result<Vec<SnsNeuron>, GovernanceSnapshotError> {
    let mut neurons = Vec::new();
    let mut start_page_at = None;
    let mut seen_cursors = BTreeSet::new();
    for _ in 0..max_pages {
        let page = client
            .list_neurons(SnsNeuronPageRequest {
                limit: page_limit,
                start_page_at: start_page_at.clone(),
            })
            .await?;
        if page.neurons.is_empty() && page.next_page_at.is_some() {
            return Err(GovernanceSnapshotError::EmptyPageWithNextCursor);
        }
        neurons.extend(page.neurons);
        let Some(next) = page.next_page_at else {
            return Ok(neurons);
        };
        if !seen_cursors.insert(next.clone()) {
            return Err(GovernanceSnapshotError::PaginationLimitExceeded);
        }
        start_page_at = Some(next);
    }
    Err(GovernanceSnapshotError::PaginationLimitExceeded)
}

async fn fetch_all_proposals<C: SnsGovernanceClient>(
    client: &C,
    page_limit: u64,
    max_pages: u64,
) -> Result<Vec<SnsProposal>, GovernanceSnapshotError> {
    let mut proposals = Vec::new();
    let mut before_proposal = None;
    let mut seen_cursors = BTreeSet::new();
    for _ in 0..max_pages {
        let page = client
            .list_proposals(SnsProposalPageRequest {
                limit: page_limit,
                before_proposal,
            })
            .await?;
        if page.proposals.is_empty() && page.next_before_proposal.is_some() {
            return Err(GovernanceSnapshotError::EmptyPageWithNextCursor);
        }
        proposals.extend(page.proposals);
        let Some(next) = page.next_before_proposal else {
            return Ok(proposals);
        };
        if !seen_cursors.insert(next) {
            return Err(GovernanceSnapshotError::PaginationLimitExceeded);
        }
        before_proposal = Some(next);
    }
    Err(GovernanceSnapshotError::PaginationLimitExceeded)
}

fn reject_duplicate_neurons(neurons: &[SnsNeuron]) -> Result<(), GovernanceSnapshotError> {
    let mut seen = BTreeSet::new();
    for neuron in neurons {
        if !seen.insert(neuron.id.clone()) {
            return Err(GovernanceSnapshotError::DuplicateNeuronId);
        }
    }
    Ok(())
}

fn reject_duplicate_proposals(proposals: &[SnsProposal]) -> Result<(), GovernanceSnapshotError> {
    let mut seen = BTreeSet::new();
    for proposal in proposals {
        if !seen.insert(proposal.id) {
            return Err(GovernanceSnapshotError::DuplicateProposalId);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use io_governance_types::{
        SnsBallot, SnsDissolveState, SnsProposalId, SnsProposalPage, SnsProposalRewardStatus,
        SnsProposalStatus, SnsVote,
    };
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    #[derive(Clone, Default)]
    struct InMemoryClient {
        neurons: Vec<SnsNeuron>,
        proposals: Vec<SnsProposal>,
        duplicate_neuron_cursor: bool,
        empty_neuron_page_with_cursor: bool,
    }

    impl SnsGovernanceClient for InMemoryClient {
        fn list_neurons<'a>(
            &'a self,
            page: SnsNeuronPageRequest,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<io_governance_types::SnsNeuronPage, SnsGovernanceError>>
                    + 'a,
            >,
        > {
            Box::pin(async move {
                if self.empty_neuron_page_with_cursor {
                    return Ok(io_governance_types::SnsNeuronPage {
                        neurons: Vec::new(),
                        next_page_at: Some(id(1)),
                    });
                }
                let mut neurons = self.neurons.clone();
                neurons.sort_by(|a, b| a.id.cmp(&b.id));
                let start = page
                    .start_page_at
                    .as_ref()
                    .and_then(|cursor| neurons.iter().position(|neuron| neuron.id >= *cursor))
                    .unwrap_or(0);
                let limit = page.limit as usize;
                let values = neurons
                    .iter()
                    .skip(start)
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>();
                let next_page_at = if self.duplicate_neuron_cursor {
                    Some(id(1))
                } else {
                    neurons
                        .get(start.saturating_add(limit))
                        .map(|neuron| neuron.id.clone())
                };
                Ok(io_governance_types::SnsNeuronPage {
                    neurons: values,
                    next_page_at,
                })
            })
        }

        fn get_neuron<'a>(
            &'a self,
            id: SnsNeuronId,
        ) -> Pin<Box<dyn Future<Output = Result<SnsNeuron, SnsGovernanceError>> + 'a>> {
            Box::pin(async move {
                self.neurons
                    .iter()
                    .find(|neuron| neuron.id == id)
                    .cloned()
                    .ok_or(SnsGovernanceError::NotFound)
            })
        }

        fn list_proposals<'a>(
            &'a self,
            page: SnsProposalPageRequest,
        ) -> Pin<Box<dyn Future<Output = Result<SnsProposalPage, SnsGovernanceError>> + 'a>>
        {
            Box::pin(async move {
                let mut proposals = self.proposals.clone();
                proposals.sort_by_key(|proposal| Reverse(proposal.id));
                let filtered = proposals
                    .into_iter()
                    .filter(|proposal| {
                        page.before_proposal
                            .is_none_or(|cursor| proposal.id < cursor)
                    })
                    .collect::<Vec<_>>();
                let limit = page.limit as usize;
                Ok(SnsProposalPage {
                    proposals: filtered.iter().take(limit).cloned().collect(),
                    next_before_proposal: (filtered.len() > limit)
                        .then(|| {
                            filtered
                                .get(limit.saturating_sub(1))
                                .map(|proposal| proposal.id)
                        })
                        .flatten(),
                })
            })
        }

        fn get_proposal<'a>(
            &'a self,
            id: SnsProposalId,
        ) -> Pin<Box<dyn Future<Output = Result<SnsProposal, SnsGovernanceError>> + 'a>> {
            Box::pin(async move {
                self.proposals
                    .iter()
                    .find(|proposal| proposal.id == id)
                    .cloned()
                    .ok_or(SnsGovernanceError::NotFound)
            })
        }
    }

    #[test]
    fn reward_observation_contains_raw_eligibilities_and_proposals() {
        let result = block_on(build_governance_reward_snapshot(
            &InMemoryClient {
                neurons: vec![neuron(1, 1_000), neuron(2, 1_000)],
                proposals: vec![
                    proposal(2, 75, &[(1, SnsVote::Yes), (2, SnsVote::Yes)]),
                    proposal(1, 25, &[(1, SnsVote::Yes)]),
                ],
                ..Default::default()
            },
            request(1),
        ))
        .unwrap();
        assert_eq!(result.fetched_neuron_count, 2);
        assert_eq!(result.fetched_proposal_count, 2);
        assert_eq!(
            result
                .eligibilities
                .iter()
                .map(|eligibility| eligibility.neuron_id.clone())
                .collect::<Vec<_>>(),
            vec![id(1), id(2)]
        );
        assert_eq!(
            result
                .proposals
                .iter()
                .map(|proposal| proposal.id.0)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
    }

    #[test]
    fn governance_exclusions_and_empty_ids_are_reported() {
        let mut jupiter = neuron(1, 10_000);
        jupiter.is_jupiter_governance_neuron = true;
        let mut protocol = neuron(2, 10_000);
        protocol.is_io_protocol_neuron = true;
        let mut dissolving = neuron(3, 10_000);
        dissolving.dissolve_state = SnsDissolveState::Dissolving {
            when_dissolved_timestamp_seconds: 1,
        };
        let mut short_delay = neuron(4, 10_000);
        short_delay.dissolve_delay_seconds = 1;
        let zero = neuron(5, 0);
        let mut invalid = neuron(0, 1_000);
        invalid.id = SnsNeuronId(Vec::new());
        let mut real_shaped = neuron(0, 1_000);
        real_shaped.id = SnsNeuronId(vec![1, 2, 3]);
        let result = block_on(build_governance_reward_snapshot(
            &InMemoryClient {
                neurons: vec![
                    jupiter,
                    protocol,
                    dissolving,
                    short_delay,
                    zero,
                    invalid,
                    real_shaped,
                    neuron(7, 1_000),
                ],
                proposals: Vec::new(),
                ..Default::default()
            },
            request(10),
        ))
        .unwrap();
        assert_eq!(result.eligibilities.len(), 8);
        assert!(result
            .eligibilities
            .iter()
            .any(|eligibility| eligibility.neuron_id == id(7)
                && eligibility.excluded_reason.is_none()));
        assert_eq!(
            result.conversion_errors,
            vec![SnsNeuronIdConversionError::Empty]
        );
        assert_eq!(
            result
                .excluded_neurons
                .iter()
                .filter(|n| n.reason == "invalid SNS neuron id")
                .count(),
            1
        );
        assert_eq!(
            result
                .excluded_neurons
                .iter()
                .filter(|n| n.reason == "non-canonical SNS neuron id")
                .count(),
            1
        );
    }

    #[test]
    fn governance_active_stake_snapshot_excludes_invalid_ids_from_active_stake() {
        let mut empty_id = neuron(0, 1_000);
        empty_id.id = SnsNeuronId(Vec::new());
        let real_shaped = neuron(1, 2_000);
        let result = block_on(build_governance_active_stake_snapshot(
            &InMemoryClient {
                neurons: vec![empty_id, real_shaped],
                ..Default::default()
            },
            GovernanceActiveStakeSnapshotRequest {
                eligibility_policy: request(10).eligibility_policy,
                max_neuron_pages: 10,
                page_limit: 10,
            },
        ))
        .unwrap();

        assert_eq!(result.active_staked_io_e8s, 2_000);
        assert_eq!(result.fetched_neuron_count, 2);
        assert_eq!(
            result
                .excluded_neurons
                .iter()
                .filter(|n| n.reason == "invalid SNS neuron id")
                .count(),
            1
        );
    }

    #[test]
    fn governance_reward_snapshot_reports_empty_id_as_excluded() {
        let mut invalid = neuron(0, 1_000);
        invalid.id = SnsNeuronId(Vec::new());
        let result = block_on(build_governance_reward_snapshot(
            &InMemoryClient {
                neurons: vec![invalid],
                ..Default::default()
            },
            request(10),
        ))
        .unwrap();

        assert_eq!(result.eligibilities.len(), 1);
        assert_eq!(
            result.conversion_errors,
            vec![SnsNeuronIdConversionError::Empty]
        );
        assert_eq!(result.excluded_neurons.len(), 1);
        assert_eq!(
            result.excluded_neurons[0].neuron_id,
            SnsNeuronId(Vec::new())
        );
        assert_eq!(result.excluded_neurons[0].reason, "invalid SNS neuron id");
    }

    #[test]
    fn pagination_guardrails_reject_bad_inputs() {
        assert_eq!(
            block_on(build_governance_reward_snapshot(
                &InMemoryClient::default(),
                request(0),
            )),
            Err(GovernanceSnapshotError::InvalidPageLimit)
        );
        assert_eq!(
            block_on(build_governance_reward_snapshot(
                &InMemoryClient {
                    neurons: vec![neuron(1, 1), neuron(2, 1)],
                    ..Default::default()
                },
                GovernanceRewardSnapshotRequest {
                    max_neuron_pages: 1,
                    ..request(1)
                },
            )),
            Err(GovernanceSnapshotError::PaginationLimitExceeded)
        );
        assert_eq!(
            block_on(build_governance_reward_snapshot(
                &InMemoryClient {
                    duplicate_neuron_cursor: true,
                    neurons: vec![neuron(1, 1), neuron(2, 1), neuron(3, 1)],
                    ..Default::default()
                },
                request(1),
            )),
            Err(GovernanceSnapshotError::PaginationLimitExceeded)
        );
        assert_eq!(
            block_on(build_governance_reward_snapshot(
                &InMemoryClient {
                    empty_neuron_page_with_cursor: true,
                    ..Default::default()
                },
                request(1),
            )),
            Err(GovernanceSnapshotError::EmptyPageWithNextCursor)
        );
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        assert_eq!(
            block_on(build_governance_reward_snapshot(
                &InMemoryClient {
                    neurons: vec![neuron(1, 1), neuron(1, 2)],
                    ..Default::default()
                },
                request(10),
            )),
            Err(GovernanceSnapshotError::DuplicateNeuronId)
        );
        assert_eq!(
            block_on(build_governance_reward_snapshot(
                &InMemoryClient {
                    neurons: vec![neuron(1, 1)],
                    proposals: vec![proposal(1, 1, &[]), proposal(1, 2, &[])],
                    ..Default::default()
                },
                request(10),
            )),
            Err(GovernanceSnapshotError::DuplicateProposalId)
        );
    }

    fn request(page_limit: u64) -> GovernanceRewardSnapshotRequest {
        GovernanceRewardSnapshotRequest {
            eligibility_policy: SnsEligibilityPolicy {
                protocol_neuron_ids: BTreeSet::new(),
                jupiter_governance_neuron_ids: BTreeSet::new(),
                required_dissolve_delay_seconds: io_core_model::TWO_WEEK_SECONDS,
            },
            max_neuron_pages: 10,
            max_proposal_pages: 10,
            page_limit,
        }
    }

    fn id(value: u64) -> SnsNeuronId {
        let mut bytes = [0_u8; 32];
        bytes[24..].copy_from_slice(&value.to_be_bytes());
        SnsNeuronId(bytes.to_vec())
    }

    fn neuron(id_value: u64, stake: u128) -> SnsNeuron {
        SnsNeuron {
            id: id(id_value),
            controller: None,
            stake_e8s: stake,
            dissolve_delay_seconds: 1_209_600,
            dissolve_state: SnsDissolveState::NotDissolving {
                dissolve_delay_seconds: 1_209_600,
            },
            cached_neuron_stake_e8s: stake,
            voting_power: stake,
            permissions: Vec::new(),
            is_io_protocol_neuron: false,
            is_jupiter_governance_neuron: false,
        }
    }

    fn proposal(id: u64, decided: u64, votes: &[(u64, SnsVote)]) -> SnsProposal {
        SnsProposal {
            id: SnsProposalId(id),
            topic: Some(1),
            status: SnsProposalStatus::Adopted,
            reward_status: SnsProposalRewardStatus::Settled,
            decided_timestamp_seconds: Some(decided),
            ballots: votes
                .iter()
                .map(|(neuron_id, vote)| SnsBallot {
                    neuron_id: self::id(*neuron_id),
                    vote: *vote,
                })
                .collect(),
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut context = Context::from_waker(std::task::Waker::noop());
        let mut future = Box::pin(future);
        loop {
            match Future::poll(future.as_mut(), &mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }
}
