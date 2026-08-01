use candid::{CandidType, Principal};
use ic_cdk::call::Call;
use serde::Deserialize;

use crate::{
    api::ApiError,
    state::{Account, RewardCohort, RewardMember},
};

const PAGE_SIZE: u32 = 100;
const MAX_PAGES: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub(crate) struct NeuronId {
    pub(crate) id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub(crate) enum DissolveState {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub(crate) struct Neuron {
    pub(crate) id: Option<NeuronId>,
    pub(crate) cached_neuron_stake_e8s: u64,
    pub(crate) dissolve_state: Option<DissolveState>,
}

#[derive(Clone, Debug, CandidType)]
struct ListNeuronsRequest {
    of_principal: Option<Principal>,
    limit: u32,
    start_page_at: Option<NeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ListNeuronsResponse {
    neurons: Vec<Neuron>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, CandidType, Deserialize)]
pub(crate) struct ProposalId {
    pub(crate) id: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub(crate) struct Ballot {
    pub(crate) vote: i32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub(crate) struct Proposal {
    pub(crate) id: Option<ProposalId>,
    pub(crate) ballots: Vec<(String, Ballot)>,
    pub(crate) decided_timestamp_seconds: u64,
    pub(crate) is_eligible_for_rewards: bool,
}

#[derive(Clone, Debug, CandidType)]
struct ListProposalsRequest {
    include_reward_status: Vec<i32>,
    before_proposal: Option<ProposalId>,
    limit: u32,
    exclude_type: Vec<u64>,
    include_status: Vec<i32>,
    include_topics: Option<Vec<ReservedTopicSelector>>,
}

#[derive(Clone, Debug, CandidType)]
struct ReservedTopicSelector {
    topic: Option<SnsTopic>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, CandidType)]
enum SnsTopic {
    DaoCommunitySettings,
    SnsFrameworkManagement,
    DappCanisterManagement,
    ApplicationBusinessLogic,
    Governance,
    TreasuryAssetManagement,
    CriticalDappOperations,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ListProposalsResponse {
    proposals: Vec<Proposal>,
}

#[derive(Clone, Debug, CandidType)]
struct GetProposalRequest {
    proposal_id: Option<ProposalId>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct GetProposalResponse {
    result: Option<GetProposalResult>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum GetProposalResult {
    Error(GovernanceError),
    Proposal(Proposal),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct GovernanceError {
    error_message: String,
    error_type: i32,
}

pub(crate) async fn capture_proposal_window(
    governance: Principal,
) -> Result<(Option<u64>, Vec<u64>), ApiError> {
    let latest = list_proposal_page(governance, None, 1, Vec::new()).await?;
    let anchor = latest.first().map(required_proposal_id).transpose()?;
    let open = list_open_proposals(governance).await?;
    let mut ids = std::collections::BTreeSet::new();
    for proposal in open {
        let id = required_proposal_id(&proposal)?;
        if proposal.decided_timestamp_seconds == 0
            && proposal.is_eligible_for_rewards
            && !ids.insert(id)
        {
            return Err(ApiError::Invalid(
                "SNS open proposal evidence contains duplicate IDs".into(),
            ));
        }
    }
    if ids.len() > RewardCohort::MAX_OPEN_PROPOSALS_AT_CAPTURE {
        return Err(ApiError::Invalid(
            "open reward proposal capacity exhausted".into(),
        ));
    }
    Ok((anchor, ids.into_iter().collect()))
}

pub(crate) async fn close_proposal_window(
    governance: Principal,
    cohort: &RewardCohort,
) -> Result<Vec<Proposal>, ApiError> {
    let mut by_id = std::collections::BTreeMap::new();
    for proposal in list_proposals_after(governance, cohort.latest_proposal_id_at_capture).await? {
        let id = required_proposal_id(&proposal)?;
        if by_id.insert(id, proposal).is_some() {
            return Err(ApiError::Invalid(
                "SNS proposal pagination returned duplicate IDs".into(),
            ));
        }
    }
    for id in &cohort.open_proposal_ids_at_capture {
        let proposal = get_proposal(governance, *id).await?;
        if required_proposal_id(&proposal)? != *id || by_id.insert(*id, proposal).is_some() {
            return Err(ApiError::Invalid(
                "carried proposal evidence conflicts with cohort window".into(),
            ));
        }
    }
    if by_id.len() > RewardCohort::MAX_PROPOSALS_PER_COHORT {
        return Err(ApiError::Invalid(
            "reward proposal capacity exhausted".into(),
        ));
    }
    Ok(by_id.into_values().collect())
}

pub(crate) fn eligible_members(
    governance: Principal,
    excluded_io_accounts: &[Account],
    neurons: &[Neuron],
) -> Result<Vec<RewardMember>, ApiError> {
    let mut ids = std::collections::BTreeSet::new();
    for id in neurons.iter().filter_map(|neuron| neuron.id.as_ref()) {
        if !ids.insert(id.id.clone()) {
            return Err(ApiError::Invalid(
                "SNS list_neurons returned a duplicate neuron ID".into(),
            ));
        }
    }
    neurons
        .iter()
        .filter(|neuron| canonical_eligible(neuron))
        .filter_map(|neuron| {
            let id = match neuron.id.as_ref() {
                Some(id) => id.id.clone(),
                None => {
                    return Some(Err(ApiError::Invalid(
                        "eligible SNS neuron lacks ID".into(),
                    )))
                }
            };
            if id.len() != 32 {
                return Some(Err(ApiError::Invalid(
                    "eligible SNS neuron ID is not a canonical staking subaccount".into(),
                )));
            }
            let account = Account {
                owner: governance,
                subaccount: Some(id.clone()),
            };
            let excluded = excluded_io_accounts.iter().any(|excluded| {
                account
                    .effective_eq(excluded)
                    .expect("validated accounts have canonical subaccounts")
            });
            if excluded {
                return None;
            }
            Some(Ok(RewardMember {
                account,
                sns_neuron_id: id,
                frozen_stake_e8s: u128::from(neuron.cached_neuron_stake_e8s),
                observed_stake_e8s: u128::from(neuron.cached_neuron_stake_e8s),
                eligible_closed_proposals: 0,
                voted_closed_proposals: 0,
                destination_is_currently_eligible: true,
            }))
        })
        .collect()
}

pub(crate) fn canonical_eligible(neuron: &Neuron) -> bool {
    neuron.cached_neuron_stake_e8s > 0
        && matches!(
            neuron.dissolve_state.as_ref(),
            Some(DissolveState::DissolveDelaySeconds(
                io_core_model::TWO_WEEK_SECONDS
            ))
        )
}

pub(crate) fn participation(
    id: &[u8],
    start: u64,
    end: u64,
    proposals: &[Proposal],
) -> Result<(u64, u64), ApiError> {
    let id = crate::transfer::hex(id);
    let mut eligible = 0u64;
    let mut voted = 0u64;
    for proposal in proposals {
        let decided = proposal.decided_timestamp_seconds;
        if decided <= start || decided > end || !proposal.is_eligible_for_rewards {
            continue;
        }
        eligible = eligible
            .checked_add(1)
            .ok_or_else(|| ApiError::Invalid("eligible proposal count overflow".into()))?;
        if proposal.ballots.iter().any(|(neuron, ballot)| {
            neuron.eq_ignore_ascii_case(&id) && matches!(ballot.vote, 1 | 2)
        }) {
            voted = voted
                .checked_add(1)
                .ok_or_else(|| ApiError::Invalid("voted proposal count overflow".into()))?;
        }
    }
    Ok((eligible, voted))
}

pub(crate) async fn list_all_neurons(governance: Principal) -> Result<Vec<Neuron>, ApiError> {
    let mut neurons = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_PAGES {
        let response: ListNeuronsResponse = Call::bounded_wait(governance, "list_neurons")
            .with_arg(ListNeuronsRequest {
                of_principal: None,
                limit: PAGE_SIZE,
                start_page_at: cursor.clone(),
            })
            .await
            .map_err(|error| ApiError::Pending(format!("SNS list_neurons failed: {error:?}")))?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("SNS list_neurons decode failed: {error:?}"))
            })?;
        let count = response.neurons.len();
        if count > PAGE_SIZE as usize {
            return Err(ApiError::Invalid("SNS neuron page exceeds bound".into()));
        }
        let next = response.neurons.last().and_then(|neuron| neuron.id.clone());
        if count == PAGE_SIZE as usize && next == cursor {
            return Err(ApiError::Invalid(
                "SNS neuron pagination did not progress".into(),
            ));
        }
        neurons.extend(response.neurons);
        if count < PAGE_SIZE as usize {
            return Ok(neurons);
        }
        cursor = next;
    }
    Err(ApiError::Invalid(
        "SNS neuron evidence exceeds bounded pages".into(),
    ))
}

async fn list_open_proposals(governance: Principal) -> Result<Vec<Proposal>, ApiError> {
    let mut proposals = Vec::new();
    let mut before = None;
    for _ in 0..MAX_PAGES {
        let page = list_proposal_page(governance, before.clone(), PAGE_SIZE, vec![1]).await?;
        let count = page.len();
        let next = page.last().and_then(|proposal| proposal.id.clone());
        if count > PAGE_SIZE as usize || (count == PAGE_SIZE as usize && next == before) {
            return Err(ApiError::Invalid(
                "SNS proposal pagination is invalid".into(),
            ));
        }
        proposals.extend(page);
        if proposals.len() > RewardCohort::MAX_OPEN_PROPOSALS_AT_CAPTURE {
            return Err(ApiError::Invalid(
                "open reward proposal capacity exhausted".into(),
            ));
        }
        if count < PAGE_SIZE as usize {
            return Ok(proposals);
        }
        before = next;
    }
    Err(ApiError::Invalid(
        "open reward proposal capacity exhausted".into(),
    ))
}

async fn list_proposals_after(
    governance: Principal,
    anchor: Option<u64>,
) -> Result<Vec<Proposal>, ApiError> {
    let mut proposals = Vec::new();
    let mut before = None;
    let mut previous_id = None;
    for _ in 0..MAX_PAGES {
        let page = list_proposal_page(governance, before.clone(), PAGE_SIZE, Vec::new()).await?;
        if page.is_empty() {
            return Ok(proposals);
        }
        let reached_anchor =
            append_new_proposal_page(&page, anchor, &mut previous_id, &mut proposals)?;
        if reached_anchor || page.len() < PAGE_SIZE as usize {
            return Ok(proposals);
        }
        let next = page.last().and_then(|proposal| proposal.id.clone());
        if next == before {
            return Err(ApiError::Invalid(
                "SNS proposal pagination did not progress".into(),
            ));
        }
        before = next;
    }
    Err(ApiError::Invalid(
        "reward proposal capacity exhausted".into(),
    ))
}

fn append_new_proposal_page(
    page: &[Proposal],
    anchor: Option<u64>,
    previous_id: &mut Option<u64>,
    proposals: &mut Vec<Proposal>,
) -> Result<bool, ApiError> {
    for proposal in page {
        let id = required_proposal_id(proposal)?;
        if previous_id.is_some_and(|previous| id >= previous) {
            return Err(ApiError::Invalid(
                "SNS proposal pagination is non-descending".into(),
            ));
        }
        *previous_id = Some(id);
        if anchor.is_some_and(|anchor| id <= anchor) {
            return Ok(true);
        }
        proposals.push(proposal.clone());
        if proposals.len() > RewardCohort::MAX_PROPOSALS_PER_COHORT {
            return Err(ApiError::Invalid(
                "reward proposal capacity exhausted".into(),
            ));
        }
    }
    Ok(false)
}

async fn list_proposal_page(
    governance: Principal,
    before_proposal: Option<ProposalId>,
    limit: u32,
    include_status: Vec<i32>,
) -> Result<Vec<Proposal>, ApiError> {
    let response: ListProposalsResponse = Call::bounded_wait(governance, "list_proposals")
        .with_arg(ListProposalsRequest {
            include_reward_status: Vec::new(),
            before_proposal,
            limit,
            exclude_type: Vec::new(),
            include_status,
            include_topics: None,
        })
        .await
        .map_err(|error| ApiError::Pending(format!("SNS list_proposals failed: {error:?}")))?
        .candid()
        .map_err(|error| {
            ApiError::Invalid(format!("SNS list_proposals decode failed: {error:?}"))
        })?;
    if response.proposals.len() > limit as usize {
        return Err(ApiError::Invalid("SNS proposal page exceeds bound".into()));
    }
    Ok(response.proposals)
}

async fn get_proposal(governance: Principal, id: u64) -> Result<Proposal, ApiError> {
    let response: GetProposalResponse = Call::bounded_wait(governance, "get_proposal")
        .with_arg(GetProposalRequest {
            proposal_id: Some(ProposalId { id }),
        })
        .await
        .map_err(|error| ApiError::Pending(format!("SNS get_proposal failed: {error:?}")))?
        .candid()
        .map_err(|error| ApiError::Invalid(format!("SNS get_proposal decode failed: {error:?}")))?;
    match response.result {
        Some(GetProposalResult::Proposal(proposal)) => Ok(proposal),
        Some(GetProposalResult::Error(error)) => Err(ApiError::Invalid(format!(
            "SNS get_proposal rejected ({}): {}",
            error.error_type, error.error_message
        ))),
        None => Err(ApiError::Invalid(
            "SNS get_proposal returned no result".into(),
        )),
    }
}

fn required_proposal_id(proposal: &Proposal) -> Result<u64, ApiError> {
    proposal
        .id
        .as_ref()
        .map(|id| id.id)
        .filter(|id| *id > 0)
        .ok_or_else(|| ApiError::Invalid("SNS proposal lacks a nonzero ID".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: u8) -> Vec<u8> {
        vec![byte; 32]
    }

    fn neuron(byte: u8, stake: u64, delay: u64) -> Neuron {
        Neuron {
            id: Some(NeuronId { id: id(byte) }),
            cached_neuron_stake_e8s: stake,
            dissolve_state: Some(DissolveState::DissolveDelaySeconds(delay)),
        }
    }

    fn proposal(decided: u64, votes: &[(u8, i32)]) -> Proposal {
        Proposal {
            id: Some(ProposalId { id: decided }),
            ballots: votes
                .iter()
                .map(|(neuron, vote)| (crate::transfer::hex(&id(*neuron)), Ballot { vote: *vote }))
                .collect(),
            decided_timestamp_seconds: decided,
            is_eligible_for_rewards: true,
        }
    }

    #[test]
    fn eligibility_is_exact_two_week_non_dissolving_positive_stake() {
        assert!(canonical_eligible(&neuron(
            1,
            1,
            io_core_model::TWO_WEEK_SECONDS
        )));
        assert!(!canonical_eligible(&neuron(
            1,
            0,
            io_core_model::TWO_WEEK_SECONDS
        )));
        assert!(!canonical_eligible(&neuron(
            1,
            1,
            io_core_model::TWO_WEEK_SECONDS + 1
        )));
        let mut dissolving = neuron(1, 1, io_core_model::TWO_WEEK_SECONDS);
        dissolving.dissolve_state = Some(DissolveState::WhenDissolvedTimestampSeconds(10));
        assert!(!canonical_eligible(&dissolving));
    }

    #[test]
    fn capture_freezes_stake_and_canonical_staking_account() {
        let governance = Principal::from_slice(&[9; 29]);
        let members = eligible_members(
            governance,
            &[],
            &[neuron(1, 123, io_core_model::TWO_WEEK_SECONDS)],
        )
        .unwrap();
        assert_eq!(members[0].frozen_stake_e8s, 123);
        assert_eq!(members[0].observed_stake_e8s, 123);
        assert_eq!(members[0].account.owner, governance);
        assert_eq!(members[0].account.subaccount, Some(id(1)));
    }

    #[test]
    fn excluded_governance_staking_accounts_never_enter_a_cohort() {
        let governance = Principal::from_slice(&[9; 29]);
        let excluded = Account {
            owner: governance,
            subaccount: Some(id(1)),
        };
        let members = eligible_members(
            governance,
            &[excluded],
            &[
                neuron(1, 1_000, io_core_model::TWO_WEEK_SECONDS),
                neuron(2, 2_000, io_core_model::TWO_WEEK_SECONDS),
            ],
        )
        .unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].sns_neuron_id, id(2));
        assert_eq!(members[0].frozen_stake_e8s, 2_000);
    }

    #[test]
    fn duplicate_sns_neuron_ids_are_rejected() {
        let governance = Principal::from_slice(&[9; 29]);
        let error = eligible_members(
            governance,
            &[],
            &[
                neuron(1, 1_000, io_core_model::TWO_WEEK_SECONDS),
                neuron(1, 2_000, io_core_model::TWO_WEEK_SECONDS),
            ],
        )
        .unwrap_err();
        assert!(matches!(error, ApiError::Invalid(message) if message.contains("duplicate")));
    }

    #[test]
    fn same_second_proposal_is_not_counted_for_new_cohort() {
        let start = 100;
        let end = start + io_core_model::TWO_WEEK_SECONDS;
        let proposals = vec![
            proposal(start, &[(1, 1)]),
            proposal(start + 1, &[(1, 1)]),
            proposal(end, &[(1, 2)]),
            proposal(end + 1, &[(1, 1), (2, 1)]),
        ];
        assert_eq!(participation(&id(1), start, end, &proposals), Ok((2, 2)));
    }

    #[test]
    fn only_canonical_yes_and_no_ballots_count() {
        let start = 100;
        let end = start + io_core_model::TWO_WEEK_SECONDS;
        let proposals = vec![
            proposal(start + 1, &[(1, 1), (2, 2), (3, 3), (4, 4)]),
            proposal(end, &[(1, 2), (2, 1), (3, 4), (4, 3)]),
        ];
        assert_eq!(participation(&id(1), start, end, &proposals), Ok((2, 2)));
        assert_eq!(participation(&id(2), start, end, &proposals), Ok((2, 2)));
        assert_eq!(participation(&id(3), start, end, &proposals), Ok((2, 0)));
        assert_eq!(participation(&id(4), start, end, &proposals), Ok((2, 0)));
    }

    #[test]
    fn lifetime_history_before_capture_anchor_does_not_consume_window_capacity() {
        let mut page = (995..=1_005)
            .rev()
            .map(|id| proposal(id, &[]))
            .collect::<Vec<_>>();
        for (offset, proposal) in page.iter_mut().enumerate() {
            proposal.id = Some(ProposalId {
                id: 1_005 - offset as u64,
            });
        }
        let mut previous = None;
        let mut selected = Vec::new();
        assert_eq!(
            append_new_proposal_page(&page, Some(1_000), &mut previous, &mut selected),
            Ok(true)
        );
        assert_eq!(
            selected
                .iter()
                .map(|proposal| proposal.id.as_ref().unwrap().id)
                .collect::<Vec<_>>(),
            vec![1_005, 1_004, 1_003, 1_002, 1_001]
        );
    }

    #[test]
    fn malformed_or_duplicate_proposal_pagination_is_rejected() {
        let mut previous = None;
        let mut selected = Vec::new();
        let page = vec![proposal(2, &[]), proposal(2, &[])];
        assert!(matches!(
            append_new_proposal_page(&page, None, &mut previous, &mut selected),
            Err(ApiError::Invalid(message)) if message.contains("non-descending")
        ));
    }
}
