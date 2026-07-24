use candid::CandidType;
use candid::Nat;
use candid::Principal;
use io_governance_types::{
    SnsDisburseMaturityInProgress, SnsDissolveStateRecord, SnsFollowees, SnsGetNeuronResult,
    SnsGovernanceError, SnsManageNeuronCommand, SnsManageNeuronCommandResponse, SnsNeuron,
    SnsNeuronId, SnsNeuronIdRecord, SnsNeuronPage, SnsNeuronPageRequest, SnsNeuronPermissionRecord,
    SnsNeuronRecord, SnsProductionGetNeuronRequest, SnsProductionGetNeuronResponse,
    SnsProductionManageNeuronRequest, SnsProductionManageNeuronResponse, SnsProposal,
    SnsProposalId, SnsProposalPage, SnsProposalPageRequest, SnsTopicFollowees,
};
use io_ledger_types::{Account, IcrcAccount, Subaccount};
use io_sns_lifecycle::{
    RootUpgradeIntent, RootUpgradeRequest, UpgradeProposal, UpgradeProposalRequest,
    UpgradeProposalStatus, UpgradeVote,
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
    root_principal: Option<Principal>,
    upgrade_proposals: Vec<UpgradeProposal>,
    next_upgrade_proposal_id: u64,
    now: u64,
    io_ledger: Option<Principal>,
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
pub fn debug_set_root_principal(root: Principal) {
    STATE.with(|cell| cell.borrow_mut().root_principal = Some(root));
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_io_ledger_principal(ledger: Principal) {
    STATE.with(|cell| cell.borrow_mut().io_ledger = Some(ledger));
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

fn mock_neuron_id(id: u64) -> Vec<u8> {
    let mut bytes = [0_u8; 32];
    bytes[24..].copy_from_slice(&id.to_be_bytes());
    bytes.to_vec()
}

fn mock_to_production_neuron(neuron: &MockSnsNeuron) -> SnsNeuronRecord {
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
            SnsDissolveStateRecord::DissolveDelaySeconds(neuron.eligible_seconds)
        }),
        voting_power_percentage_multiplier: 100,
        vesting_period_seconds: None,
        disburse_maturity_in_progress: Vec::<SnsDisburseMaturityInProgress>::new(),
        followees: Vec::<(u64, SnsFollowees)>::new(),
        neuron_fees_e8s: 0,
        permissions: Vec::<SnsNeuronPermissionRecord>::new(),
        topic_followees: None::<SnsTopicFollowees>,
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn get_neuron(request: SnsProductionGetNeuronRequest) -> SnsProductionGetNeuronResponse {
    let id = request.neuron_id.map(|id| id.id);
    let result = STATE.with(|cell| {
        cell.borrow()
            .neurons
            .iter()
            .find(|neuron| Some(mock_neuron_id(neuron.neuron_id)) == id)
            .map(|neuron| SnsGetNeuronResult::Neuron(Box::new(mock_to_production_neuron(neuron))))
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
            if let Some(neuron) = cell
                .borrow_mut()
                .neurons
                .iter_mut()
                .find(|neuron| mock_neuron_id(neuron.neuron_id) == id)
            {
                neuron.staked_io_e8s = balance;
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
            eligible_seconds: 1_209_600,
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
            eligible_seconds: 1_209_600,
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
