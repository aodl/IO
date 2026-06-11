use io_governance_types::{
    snapshot_sns_eligibility, summarize_sns_participation, SnsEligibilityPolicy,
    SnsGovernanceClient, SnsGovernanceError, SnsNeuron, SnsNeuronEligibility, SnsNeuronId,
    SnsNeuronPageRequest, SnsParticipationPolicy, SnsProposal, SnsProposalPageRequest,
};
use io_reward_policy::{sns_neuron_id_to_u64, NeuronSnapshot, SnsNeuronIdConversionError};
#[cfg(test)]
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceRewardSnapshotRequest {
    pub eligibility_policy: SnsEligibilityPolicy,
    pub participation_policy: SnsParticipationPolicy,
    pub max_neuron_pages: u64,
    pub max_proposal_pages: u64,
    pub page_limit: u64,
    pub eligible_since_overrides: BTreeMap<SnsNeuronId, u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernanceRewardSnapshot {
    pub snapshots: Vec<NeuronSnapshot>,
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
) -> Result<GovernanceRewardSnapshot, GovernanceSnapshotError> {
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
    for eligibility in &mut eligibilities {
        if let Some(eligible_since) = request
            .eligible_since_overrides
            .get(&eligibility.neuron_id)
            .copied()
        {
            eligibility.eligible_since_seconds = eligible_since;
        }
    }
    let summaries =
        summarize_sns_participation(&eligibilities, &proposals, &request.participation_policy);
    let summary_by_id = summaries
        .iter()
        .map(|summary| (summary.neuron_id.clone(), summary))
        .collect::<BTreeMap<_, _>>();

    let mut snapshots = Vec::new();
    let mut excluded_neurons = Vec::new();
    let mut conversion_errors = Vec::new();
    for eligibility in &eligibilities {
        if let Some(reason) = &eligibility.excluded_reason {
            excluded_neurons.push(ExcludedGovernanceNeuron {
                neuron_id: eligibility.neuron_id.clone(),
                reason: reason.clone(),
            });
            continue;
        }
        let Some(summary) = summary_by_id.get(&eligibility.neuron_id) else {
            continue;
        };
        let neuron_id = match sns_neuron_id_to_u64(&eligibility.neuron_id) {
            Ok(id) => id,
            Err(err) => {
                conversion_errors.push(err);
                excluded_neurons.push(ExcludedGovernanceNeuron {
                    neuron_id: eligibility.neuron_id.clone(),
                    reason: "invalid SNS neuron id".to_string(),
                });
                continue;
            }
        };
        snapshots.push(NeuronSnapshot {
            neuron_id,
            staked_io_e8s: eligibility.eligible_stake_e8s,
            eligible_seconds: request
                .participation_policy
                .epoch_end_seconds
                .saturating_sub(
                    eligibility
                        .eligible_since_seconds
                        .min(request.participation_policy.epoch_end_seconds),
                ),
            eligible_closed_proposals: summary.eligible_closed_proposals_total,
            voted_closed_proposals: summary.voted_proposals,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: !eligibility.is_non_dissolving,
        });
    }
    reject_duplicate_reward_neuron_ids(&snapshots)?;

    Ok(GovernanceRewardSnapshot {
        snapshots,
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
    let eligibilities = snapshot_sns_eligibility(&neurons, &request.eligibility_policy);
    let active_staked_io_e8s = eligibilities
        .iter()
        .filter(|eligibility| eligibility.excluded_reason.is_none())
        .map(|eligibility| eligibility.eligible_stake_e8s)
        .sum();
    let excluded_neurons = eligibilities
        .iter()
        .filter_map(|eligibility| {
            eligibility
                .excluded_reason
                .as_ref()
                .map(|reason| ExcludedGovernanceNeuron {
                    neuron_id: eligibility.neuron_id.clone(),
                    reason: reason.clone(),
                })
        })
        .collect();

    Ok(GovernanceActiveStakeSnapshot {
        eligibilities,
        active_staked_io_e8s,
        excluded_neurons,
        fetched_neuron_count,
    })
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

fn reject_duplicate_reward_neuron_ids(
    snapshots: &[NeuronSnapshot],
) -> Result<(), GovernanceSnapshotError> {
    let mut seen = BTreeSet::new();
    for snapshot in snapshots {
        if !seen.insert(snapshot.neuron_id) {
            return Err(GovernanceSnapshotError::DuplicateRewardNeuronId);
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
    use io_reward_policy::{allocate_rewards, RewardAllocation};
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
    fn governance_source_drives_equal_two_week_allocation() {
        let result = block_on(build_governance_reward_snapshot(
            &InMemoryClient {
                neurons: vec![neuron(1, 1_000), neuron(2, 1_000)],
                proposals: vec![proposal(1, 10, &[(1, SnsVote::Yes), (2, SnsVote::No)])],
                ..Default::default()
            },
            request(2),
        ))
        .unwrap();
        let out = allocate_rewards(200, &result.snapshots);
        assert_eq!(
            out.allocations,
            vec![
                RewardAllocation {
                    neuron_id: 1,
                    io_e8s: 100
                },
                RewardAllocation {
                    neuron_id: 2,
                    io_e8s: 100
                }
            ]
        );
    }

    #[test]
    fn participation_and_stake_time_weighting_flow_into_allocation() {
        let mut req = request(1);
        req.eligible_since_overrides.insert(id(2), 50);
        let result = block_on(build_governance_reward_snapshot(
            &InMemoryClient {
                neurons: vec![neuron(1, 1_000), neuron(2, 1_000)],
                proposals: vec![
                    proposal(2, 75, &[(1, SnsVote::Yes), (2, SnsVote::Yes)]),
                    proposal(1, 25, &[(1, SnsVote::Yes)]),
                ],
                ..Default::default()
            },
            req,
        ))
        .unwrap();
        assert_eq!(result.fetched_neuron_count, 2);
        assert_eq!(result.fetched_proposal_count, 2);
        let out = allocate_rewards(400, &result.snapshots);
        assert_eq!(out.allocations[0].io_e8s, 266);
        assert_eq!(out.allocations[1].io_e8s, 133);
        assert_eq!(out.dust_e8s, 1);
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
        assert_eq!(result.snapshots.len(), 2);
        assert!(result
            .snapshots
            .iter()
            .any(|snapshot| snapshot.neuron_id == 7));
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
    }

    #[test]
    fn no_closed_proposals_gives_full_participation_and_dust_stays_unissued() {
        let result = block_on(build_governance_reward_snapshot(
            &InMemoryClient {
                neurons: vec![neuron(1, 1), neuron(2, 1), neuron(3, 1)],
                proposals: Vec::new(),
                ..Default::default()
            },
            request(10),
        ))
        .unwrap();
        let out = allocate_rewards(100, &result.snapshots);
        assert_eq!(out.allocations.iter().map(|a| a.io_e8s).sum::<u128>(), 99);
        assert_eq!(out.dust_e8s, 1);
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
        assert_eq!(
            reject_duplicate_reward_neuron_ids(&[
                NeuronSnapshot {
                    neuron_id: 1,
                    staked_io_e8s: 1,
                    eligible_seconds: 1,
                    eligible_closed_proposals: 0,
                    voted_closed_proposals: 0,
                    is_genesis_governance_neuron: false,
                    is_protocol_owned: false,
                    is_dissolving: false,
                },
                NeuronSnapshot {
                    neuron_id: 1,
                    staked_io_e8s: 2,
                    eligible_seconds: 1,
                    eligible_closed_proposals: 0,
                    voted_closed_proposals: 0,
                    is_genesis_governance_neuron: false,
                    is_protocol_owned: false,
                    is_dissolving: false,
                },
            ]),
            Err(GovernanceSnapshotError::DuplicateRewardNeuronId)
        );
    }

    fn request(page_limit: u64) -> GovernanceRewardSnapshotRequest {
        GovernanceRewardSnapshotRequest {
            eligibility_policy: SnsEligibilityPolicy {
                protocol_neuron_ids: BTreeSet::new(),
                jupiter_governance_neuron_ids: BTreeSet::new(),
                minimum_dissolve_delay_seconds: 1_209_600,
                require_non_dissolving: true,
                current_timestamp_seconds: 0,
            },
            participation_policy: SnsParticipationPolicy {
                count_direct_votes: true,
                count_followed_votes: true,
                excluded_topics: BTreeSet::new(),
                epoch_start_seconds: 0,
                epoch_end_seconds: 100,
            },
            max_neuron_pages: 10,
            max_proposal_pages: 10,
            page_limit,
            eligible_since_overrides: BTreeMap::new(),
        }
    }

    fn id(value: u64) -> SnsNeuronId {
        SnsNeuronId(value.to_be_bytes().to_vec())
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
