use candid::{decode_one, encode_one, Principal};
use io_governance_types::{
    SnsBallot, SnsDissolveState, SnsEligibilityPolicy, SnsGovernanceClient, SnsGovernanceError,
    SnsNeuron, SnsNeuronId, SnsNeuronPage, SnsNeuronPageRequest, SnsParticipationPolicy,
    SnsProposal, SnsProposalId, SnsProposalPage, SnsProposalPageRequest, SnsProposalRewardStatus,
    SnsProposalStatus, SnsVote,
};
use io_reward_policy::{allocate_rewards, RewardAllocation};
use io_stream_manager::governance_snapshot::{
    build_governance_reward_snapshot, GovernanceRewardSnapshotRequest,
};
use pocket_ic::PocketIc;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

const CYCLES: u128 = 2_000_000_000_000;

fn pocketic_available() -> bool {
    std::env::var_os("POCKET_IC_BIN").is_some()
}

fn required_wasm(path: &str) -> Option<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!("skipping SNS governance read PocketIC test because {path} is missing");
            None
        }
    }
}

struct PocketIcSnsGovernanceClient<'a> {
    pic: &'a PocketIc,
    canister: Principal,
}

impl SnsGovernanceClient for PocketIcSnsGovernanceClient<'_> {
    fn list_neurons<'a>(
        &'a self,
        page: SnsNeuronPageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SnsNeuronPage, SnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            let bytes = self
                .pic
                .query_call(
                    self.canister,
                    Principal::anonymous(),
                    "debug_list_governance_neurons",
                    encode_one(page).unwrap(),
                )
                .map_err(|err| SnsGovernanceError::CanisterCallFailed {
                    method: "debug_list_governance_neurons".to_string(),
                    message: format!("{err:?}"),
                })?;
            decode_one::<SnsNeuronPage>(&bytes).map_err(|err| SnsGovernanceError::DecodeError {
                message: format!("{err:?}"),
            })
        })
    }

    fn get_neuron<'a>(
        &'a self,
        id: SnsNeuronId,
    ) -> Pin<Box<dyn Future<Output = Result<SnsNeuron, SnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            let bytes = self
                .pic
                .query_call(
                    self.canister,
                    Principal::anonymous(),
                    "debug_get_governance_neuron",
                    encode_one(id).unwrap(),
                )
                .map_err(|err| SnsGovernanceError::CanisterCallFailed {
                    method: "debug_get_governance_neuron".to_string(),
                    message: format!("{err:?}"),
                })?;
            decode_one::<Result<SnsNeuron, SnsGovernanceError>>(&bytes).map_err(|err| {
                SnsGovernanceError::DecodeError {
                    message: format!("{err:?}"),
                }
            })?
        })
    }

    fn list_proposals<'a>(
        &'a self,
        request: SnsProposalPageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SnsProposalPage, SnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            let bytes = self
                .pic
                .query_call(
                    self.canister,
                    Principal::anonymous(),
                    "debug_list_proposals",
                    encode_one(request).unwrap(),
                )
                .map_err(|err| SnsGovernanceError::CanisterCallFailed {
                    method: "debug_list_proposals".to_string(),
                    message: format!("{err:?}"),
                })?;
            decode_one::<SnsProposalPage>(&bytes).map_err(|err| SnsGovernanceError::DecodeError {
                message: format!("{err:?}"),
            })
        })
    }

    fn get_proposal<'a>(
        &'a self,
        id: SnsProposalId,
    ) -> Pin<Box<dyn Future<Output = Result<SnsProposal, SnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            let bytes = self
                .pic
                .query_call(
                    self.canister,
                    Principal::anonymous(),
                    "debug_get_proposal",
                    encode_one(id).unwrap(),
                )
                .map_err(|err| SnsGovernanceError::CanisterCallFailed {
                    method: "debug_get_proposal".to_string(),
                    message: format!("{err:?}"),
                })?;
            decode_one::<Result<SnsProposal, SnsGovernanceError>>(&bytes).map_err(|err| {
                SnsGovernanceError::DecodeError {
                    message: format!("{err:?}"),
                }
            })?
        })
    }
}

#[test]
fn pocketic_live_sns_governance_reads_drive_two_week_allocation() {
    if !pocketic_available() {
        eprintln!("skipping SNS governance read PocketIC test because POCKET_IC_BIN is not set");
        return;
    }

    let Some(wasm) = required_wasm("target/wasm32-unknown-unknown/debug/mock_sns_governance.wasm")
    else {
        return;
    };
    let pic = PocketIc::new();
    let canister = pic.create_canister();
    pic.add_cycles(canister, CYCLES);
    pic.install_canister(canister, wasm, encode_one(()).unwrap(), None);

    let mut protocol = neuron(3, 10_000);
    protocol.is_io_protocol_neuron = true;
    let neurons = vec![neuron(1, 1_000), neuron(2, 1_000), protocol];
    let proposals = vec![
        proposal(
            2,
            20,
            &[
                (1, SnsVote::Yes),
                (2, SnsVote::Unspecified),
                (3, SnsVote::Yes),
            ],
        ),
        proposal(
            1,
            10,
            &[(1, SnsVote::No), (2, SnsVote::Yes), (3, SnsVote::Yes)],
        ),
    ];

    pic.update_call(
        canister,
        Principal::anonymous(),
        "debug_set_neurons",
        encode_one(neurons).unwrap(),
    )
    .expect("set governance neurons");
    pic.update_call(
        canister,
        Principal::anonymous(),
        "debug_set_proposals",
        encode_one(proposals).unwrap(),
    )
    .expect("set governance proposals");

    let client = PocketIcSnsGovernanceClient {
        pic: &pic,
        canister,
    };
    let first_page = block_on(client.list_proposals(SnsProposalPageRequest {
        limit: 1,
        before_proposal: None,
    }))
    .unwrap();
    assert_eq!(first_page.proposals[0].id, SnsProposalId(2));
    assert_eq!(first_page.next_before_proposal, Some(SnsProposalId(2)));
    assert_eq!(
        block_on(client.get_proposal(SnsProposalId(404))),
        Err(SnsGovernanceError::NotFound)
    );

    let snapshot = block_on(build_governance_reward_snapshot(
        &client,
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
            page_limit: 1,
            eligible_since_overrides: BTreeMap::new(),
        },
    ))
    .unwrap();

    assert_eq!(snapshot.fetched_neuron_count, 3);
    assert_eq!(snapshot.fetched_proposal_count, 2);
    assert_eq!(snapshot.excluded_neurons.len(), 1);
    assert_eq!(snapshot.snapshots.len(), 2);

    let out = allocate_rewards(300, &snapshot.snapshots);
    assert_eq!(
        out.allocations,
        vec![
            RewardAllocation {
                neuron_id: 1,
                io_e8s: 200
            },
            RewardAllocation {
                neuron_id: 2,
                io_e8s: 100
            }
        ]
    );
}

fn id(value: u64) -> SnsNeuronId {
    SnsNeuronId(value.to_be_bytes().to_vec())
}

fn neuron(value: u64, stake: u128) -> SnsNeuron {
    SnsNeuron {
        id: id(value),
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

fn proposal(value: u64, decided: u64, votes: &[(u64, SnsVote)]) -> SnsProposal {
    SnsProposal {
        id: SnsProposalId(value),
        topic: Some(1),
        status: SnsProposalStatus::Adopted,
        reward_status: SnsProposalRewardStatus::Settled,
        decided_timestamp_seconds: Some(decided),
        ballots: votes
            .iter()
            .map(|(neuron_id, vote)| SnsBallot {
                neuron_id: id(*neuron_id),
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
