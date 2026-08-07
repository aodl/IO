use candid::CandidType;
use candid::Nat;
use candid::Principal;
use io_governance_types::{
    SnsDisburseMaturityInProgress, SnsDissolveStateRecord, SnsFollowees, SnsGetNeuronResult,
    SnsGovernanceError, SnsManageNeuronCommand, SnsManageNeuronCommandResponse, SnsNeuron,
    SnsNeuronId, SnsNeuronIdRecord, SnsNeuronPage, SnsNeuronPageRequest, SnsNeuronPermissionRecord,
    SnsNeuronRecord, SnsProductionGetNeuronRequest, SnsProductionGetNeuronResponse,
    SnsProductionListNeuronsRequest, SnsProductionListNeuronsResponse,
    SnsProductionManageNeuronRequest, SnsProductionManageNeuronResponse, SnsProposal,
    SnsProposalId, SnsProposalIdRecord, SnsProposalPage, SnsProposalPageRequest, SnsRewardEvent,
    SnsRewardEventParticipation, SnsTopicFollowees, SnsUint128,
};
use io_ledger_types::{Account, IcrcAccount, Subaccount};
use io_sns_lifecycle::{
    RootUpgradeIntent, RootUpgradeRequest, UpgradeProposal, UpgradeProposalRequest,
    UpgradeProposalStatus, UpgradeVote,
};
use serde::Deserialize;
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MockSnsNeuron {
    pub neuron_id: u64,
    pub staked_io_e8s: u128,
    pub dissolve_delay_seconds: u64,
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

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LatestRewardEventFixture {
    pub round: u64,
    pub rounds_since_last_distribution: u64,
    pub end_timestamp_seconds: u64,
    pub settled_proposal_ids: Vec<u64>,
    pub neuron_reward_shares: Vec<(u64, SnsUint128)>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct VotingRewardsParameters {
    pub final_reward_rate_basis_points: Option<u64>,
    pub initial_reward_rate_basis_points: Option<u64>,
    pub reward_rate_transition_duration_seconds: Option<u64>,
    pub round_duration_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NervousSystemParameters {
    pub voting_rewards_parameters: Option<VotingRewardsParameters>,
}

#[derive(Default)]
struct SnsState {
    neurons: Vec<MockSnsNeuron>,
    proposals: Vec<MockProposal>,
    governance_neurons: Vec<SnsNeuron>,
    governance_proposals: Vec<SnsProposal>,
    root_principal: Option<Principal>,
    upgrade_proposals: Vec<UpgradeProposal>,
    next_upgrade_proposal_id: u64,
    now: u64,
    io_ledger: Option<Principal>,
    available: bool,
    latest_reward_event: SnsRewardEvent,
    latest_reward_shares: BTreeMap<u64, SnsUint128>,
    reward_round_duration_seconds: u64,
}

thread_local! {
    static STATE: RefCell<SnsState> = RefCell::new(SnsState {
        available: true,
        latest_reward_event: SnsRewardEvent {
            end_timestamp_seconds: Some(1),
            actual_timestamp_seconds: 1,
            round: 1,
            ..SnsRewardEvent::default()
        },
        reward_round_duration_seconds: io_core_model::TWO_WEEK_SECONDS,
        ..SnsState::default()
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_add_neuron(neuron: MockSnsNeuron) {
    let (reward_event, reward_shares) = STATE.with(|cell| {
        let state = cell.borrow();
        (
            state.latest_reward_event.clone(),
            state.latest_reward_shares.get(&neuron.neuron_id).copied(),
        )
    });
    let production: SnsNeuron = mock_to_production_neuron(&neuron, &reward_event, reward_shares)
        .try_into()
        .expect("mock neuron should convert to production-shaped domain neuron");
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state
            .neurons
            .retain(|existing| existing.neuron_id != neuron.neuron_id);
        state.neurons.push(neuron);
        state
            .governance_neurons
            .retain(|existing| existing.id != production.id);
        state.governance_neurons.push(production);
    });
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
pub fn debug_set_root_principal(root: Principal) {
    STATE.with(|cell| cell.borrow_mut().root_principal = Some(root));
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_io_ledger_principal(ledger: Principal) {
    STATE.with(|cell| cell.borrow_mut().io_ledger = Some(ledger));
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_available(available: bool) {
    STATE.with(|cell| cell.borrow_mut().available = available);
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_submit_upgrade_proposal(request: UpgradeProposalRequest) -> UpgradeProposal {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.next_upgrade_proposal_id = state.next_upgrade_proposal_id.saturating_add(1);
        state.now = state.now.saturating_add(1);
        let proposal = UpgradeProposal {
            proposal_id: state.next_upgrade_proposal_id,
            target_canister: request.target_canister,
            wasm_sha256: request.wasm_sha256,
            wasm_gz_sha256: request.wasm_gz_sha256,
            artifact_name: request.artifact_name,
            artifact_path: request.artifact_path,
            expected_module_hash: request.expected_module_hash,
            status: UpgradeProposalStatus::Open,
            yes_votes: 0,
            no_votes: 0,
            created_at: state.now,
            decided_at: None,
            failure_reason: None,
        };
        state.upgrade_proposals.push(proposal.clone());
        proposal
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_vote_proposal(args: (u64, UpgradeVote)) -> Result<UpgradeProposal, String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let proposal = state
            .upgrade_proposals
            .iter_mut()
            .find(|proposal| proposal.proposal_id == args.0)
            .ok_or_else(|| "unknown upgrade proposal".to_string())?;
        if proposal.status != UpgradeProposalStatus::Open {
            return Err("proposal is not open".to_string());
        }
        match args.1 {
            UpgradeVote::Yes => proposal.yes_votes = proposal.yes_votes.saturating_add(1),
            UpgradeVote::No => proposal.no_votes = proposal.no_votes.saturating_add(1),
        }
        Ok(proposal.clone())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_reject_upgrade_proposal(proposal_id: u64) -> Result<UpgradeProposal, String> {
    decide_upgrade_proposal(proposal_id, UpgradeProposalStatus::Rejected)
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_adopt_upgrade_proposal(proposal_id: u64) -> Result<UpgradeProposal, String> {
    decide_upgrade_proposal(proposal_id, UpgradeProposalStatus::Adopted)
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn debug_finalize_proposal(proposal_id: u64) -> Result<RootUpgradeIntent, String> {
    let (root, request) = STATE.with(|cell| {
        let state = cell.borrow();
        let root = state
            .root_principal
            .ok_or_else(|| "root principal is not configured".to_string())?;
        let proposal = state
            .upgrade_proposals
            .iter()
            .find(|proposal| proposal.proposal_id == proposal_id)
            .ok_or_else(|| "unknown upgrade proposal".to_string())?;
        match proposal.status {
            UpgradeProposalStatus::Adopted => Ok((
                root,
                RootUpgradeRequest {
                    proposal_id: proposal.proposal_id,
                    target_canister: proposal.target_canister,
                    wasm_sha256: proposal.wasm_sha256.clone(),
                    wasm_gz_sha256: proposal.wasm_gz_sha256.clone(),
                    artifact_name: proposal.artifact_name.clone(),
                    artifact_path: proposal.artifact_path.clone(),
                    expected_module_hash: proposal.expected_module_hash.clone(),
                },
            )),
            UpgradeProposalStatus::Open => Err("cannot execute open proposal".to_string()),
            UpgradeProposalStatus::Rejected => Err("cannot execute rejected proposal".to_string()),
            UpgradeProposalStatus::Executed => Err("proposal already executed".to_string()),
            UpgradeProposalStatus::Failed => Err("proposal already failed".to_string()),
        }
    })?;

    let result = call_root_upgrade(root, request).await;
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.now = state.now.saturating_add(1);
        let decided_at = state.now;
        let proposal = state
            .upgrade_proposals
            .iter_mut()
            .find(|proposal| proposal.proposal_id == proposal_id)
            .ok_or_else(|| "unknown upgrade proposal".to_string())?;
        match result {
            Ok(intent) => {
                proposal.status = UpgradeProposalStatus::Executed;
                proposal.decided_at = Some(decided_at);
                proposal.failure_reason = None;
                Ok(intent)
            }
            Err(err) => {
                proposal.status = UpgradeProposalStatus::Failed;
                proposal.decided_at = Some(decided_at);
                proposal.failure_reason = Some(err.clone());
                Err(err)
            }
        }
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_upgrade_proposal(proposal_id: u64) -> Option<UpgradeProposal> {
    STATE.with(|cell| {
        cell.borrow()
            .upgrade_proposals
            .iter()
            .find(|proposal| proposal.proposal_id == proposal_id)
            .cloned()
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_list_upgrade_proposals() -> Vec<UpgradeProposal> {
    STATE.with(|cell| cell.borrow().upgrade_proposals.clone())
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
        state.latest_reward_event.round = state.latest_reward_event.round.saturating_add(1);
        let end = state
            .latest_reward_event
            .end_timestamp_seconds
            .unwrap_or(1)
            .saturating_add(io_core_model::TWO_WEEK_SECONDS);
        state.latest_reward_event.end_timestamp_seconds = Some(end);
        state.latest_reward_event.actual_timestamp_seconds = end;
        state.latest_reward_event.settled_proposals = vec![SnsProposalIdRecord { id: proposal_id }];
        state.latest_reward_event.rounds_since_last_distribution = Some(1);
        state.latest_reward_shares.clear();
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_advance_reward_event(settled_proposal_ids: Vec<u64>) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.latest_reward_event.round = state.latest_reward_event.round.saturating_add(1);
        let end = state
            .latest_reward_event
            .end_timestamp_seconds
            .unwrap_or(1)
            .saturating_add(io_core_model::TWO_WEEK_SECONDS);
        state.latest_reward_event.end_timestamp_seconds = Some(end);
        state.latest_reward_event.actual_timestamp_seconds = end;
        state.latest_reward_event.settled_proposals = settled_proposal_ids
            .into_iter()
            .map(|id| SnsProposalIdRecord { id })
            .collect();
        state.latest_reward_event.rounds_since_last_distribution = Some(1);
        state.latest_reward_shares.clear();
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_latest_reward_event(fixture: LatestRewardEventFixture) -> Result<(), String> {
    if fixture.round == 0
        || fixture.rounds_since_last_distribution == 0
        || fixture.end_timestamp_seconds == 0
    {
        return Err("reward-event fixture identifiers must be nonzero".into());
    }
    let mut shares = BTreeMap::new();
    for (neuron_id, reward_shares) in fixture.neuron_reward_shares {
        if shares.insert(neuron_id, reward_shares).is_some() {
            return Err("reward-event fixture contains duplicate neuron shares".into());
        }
    }
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.latest_reward_event = SnsRewardEvent {
            rounds_since_last_distribution: Some(fixture.rounds_since_last_distribution),
            actual_timestamp_seconds: fixture.end_timestamp_seconds,
            end_timestamp_seconds: Some(fixture.end_timestamp_seconds),
            round: fixture.round,
            settled_proposals: fixture
                .settled_proposal_ids
                .into_iter()
                .map(|id| SnsProposalIdRecord { id })
                .collect(),
            ..SnsRewardEvent::default()
        };
        state.latest_reward_shares = shares;
    });
    Ok(())
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_reward_round_duration_seconds(duration: u64) -> Result<(), String> {
    if duration == 0 {
        return Err("reward round duration must be nonzero".into());
    }
    STATE.with(|cell| cell.borrow_mut().reward_round_duration_seconds = duration);
    Ok(())
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_nervous_system_parameters() -> NervousSystemParameters {
    NervousSystemParameters {
        voting_rewards_parameters: Some(VotingRewardsParameters {
            final_reward_rate_basis_points: Some(0),
            initial_reward_rate_basis_points: Some(0),
            reward_rate_transition_duration_seconds: Some(0),
            round_duration_seconds: Some(
                STATE.with(|cell| cell.borrow().reward_round_duration_seconds),
            ),
        }),
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_list_neurons() -> Vec<MockSnsNeuron> {
    STATE.with(|cell| {
        let state = cell.borrow();
        assert!(state.available, "mock SNS governance unavailable");
        state.neurons.clone()
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_list_governance_neurons(request: SnsNeuronPageRequest) -> SnsNeuronPage {
    STATE.with(|cell| {
        let state = cell.borrow();
        assert!(state.available, "mock SNS governance unavailable");
        let mut neurons = state.governance_neurons.clone();
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

fn mock_neuron_id(id: u64) -> Vec<u8> {
    let mut bytes = [0_u8; 32];
    bytes[24..].copy_from_slice(&id.to_be_bytes());
    bytes.to_vec()
}

fn mock_to_production_neuron(
    neuron: &MockSnsNeuron,
    latest_reward_event: &SnsRewardEvent,
    reward_shares: Option<SnsUint128>,
) -> SnsNeuronRecord {
    SnsNeuronRecord {
        id: Some(SnsNeuronIdRecord {
            id: mock_neuron_id(neuron.neuron_id),
        }),
        staked_maturity_e8s_equivalent: None,
        cached_neuron_stake_e8s: u64::try_from(neuron.staked_io_e8s).unwrap_or(u64::MAX),
        maturity_e8s_equivalent: 0,
        created_timestamp_seconds: 0,
        source_nns_neuron_id: None,
        auto_stake_maturity: None,
        aging_since_timestamp_seconds: 0,
        dissolve_state: Some(if neuron.is_dissolving {
            SnsDissolveStateRecord::WhenDissolvedTimestampSeconds(0)
        } else {
            SnsDissolveStateRecord::DissolveDelaySeconds(neuron.dissolve_delay_seconds)
        }),
        voting_power_percentage_multiplier: 100,
        vesting_period_seconds: None,
        disburse_maturity_in_progress: Vec::<SnsDisburseMaturityInProgress>::new(),
        followees: Vec::<(u64, SnsFollowees)>::new(),
        neuron_fees_e8s: 0,
        permissions: Vec::<SnsNeuronPermissionRecord>::new(),
        topic_followees: None::<SnsTopicFollowees>,
        latest_reward_event_participation: reward_shares.map(|reward_shares| {
            SnsRewardEventParticipation {
                reward_event_end_timestamp_seconds: latest_reward_event
                    .end_timestamp_seconds
                    .unwrap_or_default(),
                reward_shares: Some(reward_shares),
            }
        }),
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_latest_reward_event() -> SnsRewardEvent {
    STATE.with(|cell| cell.borrow().latest_reward_event.clone())
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn list_neurons(request: SnsProductionListNeuronsRequest) -> SnsProductionListNeuronsResponse {
    STATE.with(|cell| {
        let state = cell.borrow();
        assert!(state.available, "mock SNS governance unavailable");
        let mut neurons = state.neurons.iter().collect::<Vec<_>>();
        neurons.sort_by_key(|neuron| mock_neuron_id(neuron.neuron_id));
        let cursor = request.start_page_at.map(|id| id.id);
        let neurons = neurons
            .into_iter()
            .filter(|neuron| {
                cursor
                    .as_ref()
                    .is_none_or(|cursor| mock_neuron_id(neuron.neuron_id) > *cursor)
            })
            .take(request.limit as usize)
            .map(|neuron| {
                mock_to_production_neuron(
                    neuron,
                    &state.latest_reward_event,
                    state.latest_reward_shares.get(&neuron.neuron_id).copied(),
                )
            })
            .collect();
        SnsProductionListNeuronsResponse { neurons }
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn get_neuron(request: SnsProductionGetNeuronRequest) -> SnsProductionGetNeuronResponse {
    let id = request.neuron_id.map(|id| id.id);
    let result = STATE.with(|cell| {
        let state = cell.borrow();
        state
            .neurons
            .iter()
            .find(|neuron| Some(mock_neuron_id(neuron.neuron_id)) == id)
            .map(|neuron| {
                SnsGetNeuronResult::Neuron(Box::new(mock_to_production_neuron(
                    neuron,
                    &state.latest_reward_event,
                    state.latest_reward_shares.get(&neuron.neuron_id).copied(),
                )))
            })
    });
    SnsProductionGetNeuronResponse { result }
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn manage_neuron(
    request: SnsProductionManageNeuronRequest,
) -> SnsProductionManageNeuronResponse {
    let id = request.subaccount;
    if matches!(
        request.command,
        Some(SnsManageNeuronCommand::ClaimOrRefresh(_))
    ) {
        let known_neuron = STATE.with(|cell| {
            cell.borrow()
                .neurons
                .iter()
                .any(|neuron| mock_neuron_id(neuron.neuron_id) == id)
        });
        if !known_neuron {
            return SnsProductionManageNeuronResponse {
                command: Some(SnsManageNeuronCommandResponse::Error(
                    io_governance_types::SnsGovernanceErrorRecord {
                        error_type: 2,
                        error_message: "mock SNS governance neuron not found".to_string(),
                    },
                )),
            };
        }
        let Some(balance) = exact_staking_balance(id.clone()).await else {
            return SnsProductionManageNeuronResponse {
                command: Some(SnsManageNeuronCommandResponse::Error(
                    io_governance_types::SnsGovernanceErrorRecord {
                        error_type: 1,
                        error_message: "mock SNS governance IO ledger is not configured"
                            .to_string(),
                    },
                )),
            };
        };
        STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            let reward_event = state.latest_reward_event.clone();
            let reward_shares = state.latest_reward_shares.clone();
            let updated: Option<SnsNeuron> = state
                .neurons
                .iter_mut()
                .find(|neuron| mock_neuron_id(neuron.neuron_id) == id)
                .map(|neuron| {
                    neuron.staked_io_e8s = balance;
                    mock_to_production_neuron(
                        neuron,
                        &reward_event,
                        reward_shares.get(&neuron.neuron_id).copied(),
                    )
                    .try_into()
                    .expect("mock neuron should convert to production-shaped domain neuron")
                });
            if let Some(updated) = updated {
                state
                    .governance_neurons
                    .retain(|existing| existing.id != updated.id);
                state.governance_neurons.push(updated);
            }
        });
        return SnsProductionManageNeuronResponse {
            command: Some(SnsManageNeuronCommandResponse::ClaimOrRefresh(
                io_governance_types::SnsClaimOrRefreshResponse {
                    refreshed_neuron_id: Some(SnsNeuronIdRecord { id }),
                },
            )),
        };
    }
    SnsProductionManageNeuronResponse { command: None }
}

async fn exact_staking_balance(neuron_id: Vec<u8>) -> Option<u128> {
    let ledger = STATE.with(|cell| cell.borrow().io_ledger)?;
    let subaccount = <[u8; 32]>::try_from(neuron_id).ok()?;
    let account = IcrcAccount::from(Account::new(
        current_canister_id(),
        Some(Subaccount(subaccount)),
    ));
    let balance = call_ledger_balance(ledger, account).await.ok()?;
    balance.0.to_str_radix(10).parse::<u128>().ok()
}

fn current_canister_id() -> Principal {
    #[cfg(target_family = "wasm")]
    {
        ic_cdk::api::canister_self()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        Principal::anonymous()
    }
}

async fn call_ledger_balance(ledger: Principal, account: IcrcAccount) -> Result<Nat, String> {
    #[cfg(target_family = "wasm")]
    {
        let response = ic_cdk::call::Call::bounded_wait(ledger, "icrc1_balance_of")
            .with_arg(account)
            .await
            .map_err(|err| format!("{err:?}"))?;
        let (balance,) = response
            .candid_tuple::<(Nat,)>()
            .map_err(|err| format!("{err:?}"))?;
        Ok(balance)
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = (ledger, account);
        Ok(Nat::from(0_u64))
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_list_proposals(request: SnsProposalPageRequest) -> SnsProposalPage {
    STATE.with(|cell| {
        let state = cell.borrow();
        assert!(state.available, "mock SNS governance unavailable");
        let mut proposals = state.governance_proposals.clone();
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
    STATE.with(|cell| {
        *cell.borrow_mut() = SnsState {
            available: true,
            ..SnsState::default()
        }
    });
}

fn decide_upgrade_proposal(
    proposal_id: u64,
    status: UpgradeProposalStatus,
) -> Result<UpgradeProposal, String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.now = state.now.saturating_add(1);
        let decided_at = state.now;
        let proposal = state
            .upgrade_proposals
            .iter_mut()
            .find(|proposal| proposal.proposal_id == proposal_id)
            .ok_or_else(|| "unknown upgrade proposal".to_string())?;
        if proposal.status != UpgradeProposalStatus::Open {
            return Err("proposal is not open".to_string());
        }
        if status == UpgradeProposalStatus::Adopted && proposal.yes_votes <= proposal.no_votes {
            return Err("proposal does not have enough yes votes".to_string());
        }
        proposal.status = status;
        proposal.decided_at = Some(decided_at);
        Ok(proposal.clone())
    })
}

async fn call_root_upgrade(
    root: Principal,
    request: RootUpgradeRequest,
) -> Result<RootUpgradeIntent, String> {
    #[cfg(target_family = "wasm")]
    {
        ic_cdk::call::Call::bounded_wait(root, "debug_upgrade_dapp_canister")
            .with_arg(request)
            .await
            .map_err(|err| format!("root call failed: {err:?}"))?
            .candid::<Result<RootUpgradeIntent, String>>()
            .map_err(|err| format!("root response decode failed: {err:?}"))?
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = (root, request);
        Err("root calls require wasm/PocketIC execution".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use io_governance_types::{
        EmptyRecord, SnsBallot, SnsClaimOrRefresh, SnsClaimOrRefreshBy, SnsDissolveState,
        SnsProposalRewardStatus, SnsProposalStatus, SnsVote,
    };
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn block_on_ready<F: Future>(future: F) -> F::Output {
        fn raw_waker() -> RawWaker {
            fn clone(_: *const ()) -> RawWaker {
                raw_waker()
            }
            fn no_op(_: *const ()) {}
            RawWaker::new(
                std::ptr::null(),
                &RawWakerVTable::new(clone, no_op, no_op, no_op),
            )
        }
        let waker = unsafe { Waker::from_raw(raw_waker()) };
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        match Pin::new(&mut future).poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("mock future unexpectedly pending in host test"),
        }
    }

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

    #[test]
    fn upgrade_proposal_adopt_reject_and_open_guardrails() {
        debug_clear();
        let request = upgrade_request(Principal::anonymous());
        let proposal = debug_submit_upgrade_proposal(request);
        assert_eq!(proposal.status, UpgradeProposalStatus::Open);
        assert!(debug_adopt_upgrade_proposal(proposal.proposal_id)
            .unwrap_err()
            .contains("enough yes"));
        debug_vote_proposal((proposal.proposal_id, UpgradeVote::Yes)).unwrap();
        let adopted = debug_adopt_upgrade_proposal(proposal.proposal_id).unwrap();
        assert_eq!(adopted.status, UpgradeProposalStatus::Adopted);
        assert!(debug_reject_upgrade_proposal(proposal.proposal_id)
            .unwrap_err()
            .contains("not open"));

        let rejected = debug_submit_upgrade_proposal(upgrade_request(Principal::from_slice(&[1])));
        debug_vote_proposal((rejected.proposal_id, UpgradeVote::No)).unwrap();
        assert_eq!(
            debug_reject_upgrade_proposal(rejected.proposal_id)
                .unwrap()
                .status,
            UpgradeProposalStatus::Rejected
        );
    }

    fn claim_or_refresh_request(neuron_id: Vec<u8>) -> SnsProductionManageNeuronRequest {
        SnsProductionManageNeuronRequest {
            subaccount: neuron_id,
            command: Some(SnsManageNeuronCommand::ClaimOrRefresh(SnsClaimOrRefresh {
                by: Some(SnsClaimOrRefreshBy::NeuronId(EmptyRecord {})),
            })),
        }
    }

    #[test]
    fn mock_claim_or_refresh_unknown_neuron_fails() {
        debug_clear();
        debug_add_neuron(MockSnsNeuron {
            neuron_id: 1,
            staked_io_e8s: 100,
            dissolve_delay_seconds: io_core_model::TWO_WEEK_SECONDS,
            eligible_closed_proposals: 0,
            voted_closed_proposals: 0,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: false,
        });
        debug_set_io_ledger_principal(Principal::from_slice(&[1]));

        let response = block_on_ready(manage_neuron(claim_or_refresh_request(vec![9; 32])));

        assert!(matches!(
            response.command,
            Some(SnsManageNeuronCommandResponse::Error(_))
        ));
    }

    #[test]
    fn mock_claim_or_refresh_unknown_neuron_does_not_mutate_any_stake() {
        debug_clear();
        debug_add_neuron(MockSnsNeuron {
            neuron_id: 1,
            staked_io_e8s: 100,
            dissolve_delay_seconds: io_core_model::TWO_WEEK_SECONDS,
            eligible_closed_proposals: 0,
            voted_closed_proposals: 0,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: false,
        });
        debug_set_io_ledger_principal(Principal::from_slice(&[1]));

        let before = debug_list_neurons();
        let _ = block_on_ready(manage_neuron(claim_or_refresh_request(vec![9; 32])));
        let after = debug_list_neurons();

        assert_eq!(after, before);
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
            latest_reward_event_participation: None,
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

    fn upgrade_request(target: Principal) -> UpgradeProposalRequest {
        UpgradeProposalRequest {
            target_canister: target,
            wasm_sha256: "raw".to_string(),
            wasm_gz_sha256: "gz".to_string(),
            artifact_name: "io_stream_manager".to_string(),
            artifact_path: "release-artifacts/io_stream_manager.wasm".to_string(),
            expected_module_hash: None,
        }
    }
}
