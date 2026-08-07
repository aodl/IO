use crate::artifacts::{resolve_from_env, ArtifactStatus};
use crate::icrc::{self, FEE_E8S};
use crate::pocketic_env;
use candid::{CandidType, Principal};
use io_governance_types::{SnsRewardEvent, SnsRewardEventParticipation};
use pocket_ic::PocketIc;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::Duration;

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Governance {
    pub root_canister_id: Option<Principal>,
    pub id_to_nervous_system_functions: Vec<(u64, NervousSystemFunction)>,
    pub metrics: Option<EmptyRecord>,
    pub maturity_modulation: Option<EmptyRecord>,
    pub mode: i32,
    pub parameters: Option<NervousSystemParameters>,
    pub is_finalizing_disburse_maturity: Option<bool>,
    pub deployed_version: Option<Version>,
    pub cached_upgrade_steps: Option<EmptyRecord>,
    pub sns_initialization_parameters: String,
    pub latest_reward_event: Option<EmptyRecord>,
    pub pending_version: Option<EmptyRecord>,
    pub swap_canister_id: Option<Principal>,
    pub ledger_canister_id: Option<Principal>,
    pub proposals: Vec<(u64, EmptyRecord)>,
    pub in_flight_commands: Vec<(String, EmptyRecord)>,
    pub sns_metadata: Option<ManageSnsMetadata>,
    pub neurons: Vec<(String, EmptyRecord)>,
    pub genesis_timestamp_seconds: u64,
    pub target_version: Option<Version>,
    pub timers: Option<Timers>,
    pub upgrade_journal: Option<EmptyRecord>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct EmptyRecord {}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NervousSystemFunction {
    pub id: u64,
    pub name: String,
    pub description: Option<String>,
    pub function_type: Option<EmptyRecord>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ListNervousSystemFunctionsResponse {
    pub reserved_ids: Vec<u64>,
    pub functions: Vec<ListedNervousSystemFunction>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ListedNervousSystemFunction {
    pub id: u64,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NervousSystemParameters {
    pub default_followees: Option<DefaultFollowees>,
    pub max_dissolve_delay_seconds: Option<u64>,
    pub max_dissolve_delay_bonus_percentage: Option<u64>,
    pub max_followees_per_function: Option<u64>,
    pub neuron_claimer_permissions: Option<NeuronPermissionList>,
    pub neuron_minimum_stake_e8s: Option<u64>,
    pub max_neuron_age_for_age_bonus: Option<u64>,
    pub initial_voting_period_seconds: Option<u64>,
    pub neuron_minimum_dissolve_delay_to_vote_seconds: Option<u64>,
    pub reject_cost_e8s: Option<u64>,
    pub max_proposals_to_keep_per_action: Option<u32>,
    pub wait_for_quiet_deadline_increase_seconds: Option<u64>,
    pub max_number_of_neurons: Option<u64>,
    pub transaction_fee_e8s: Option<u64>,
    pub max_number_of_proposals_with_ballots: Option<u64>,
    pub max_age_bonus_percentage: Option<u64>,
    pub neuron_grantable_permissions: Option<NeuronPermissionList>,
    pub voting_rewards_parameters: Option<VotingRewardsParameters>,
    pub maturity_modulation_disabled: Option<bool>,
    pub max_number_of_principals_per_neuron: Option<u64>,
    pub automatically_advance_target_version: Option<bool>,
    pub custom_proposal_criticality: Option<EmptyRecord>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NeuronPermissionList {
    pub permissions: Vec<i32>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct DefaultFollowees {
    pub followees: Vec<(u64, Followees)>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Followees {
    pub followees: Vec<NeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct VotingRewardsParameters {
    pub final_reward_rate_basis_points: Option<u64>,
    pub initial_reward_rate_basis_points: Option<u64>,
    pub reward_rate_transition_duration_seconds: Option<u64>,
    pub round_duration_seconds: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Version {
    pub archive_wasm_hash: Vec<u8>,
    pub root_wasm_hash: Vec<u8>,
    pub swap_wasm_hash: Vec<u8>,
    pub ledger_wasm_hash: Vec<u8>,
    pub governance_wasm_hash: Vec<u8>,
    pub index_wasm_hash: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ManageSnsMetadata {
    pub url: Option<String>,
    pub logo: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Timers {
    pub requires_periodic_tasks: Option<bool>,
    pub last_reset_timestamp_seconds: Option<u64>,
    pub last_spawned_timestamp_seconds: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ListNeurons {
    pub of_principal: Option<Principal>,
    pub limit: u32,
    pub start_page_at: Option<NeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NeuronId {
    pub id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ListNeuronsResponse {
    pub neurons: Vec<SnsNeuronRecord>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct GetNeuron {
    pub neuron_id: Option<NeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct GetNeuronResponse {
    pub result: Option<GetNeuronResult>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum GetNeuronResult {
    Error(GovernanceError),
    Neuron(Box<SnsNeuronRecord>),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SnsNeuronRecord {
    pub id: Option<NeuronId>,
    pub staked_maturity_e8s_equivalent: Option<u64>,
    pub permissions: Vec<NeuronPermission>,
    pub maturity_e8s_equivalent: u64,
    pub cached_neuron_stake_e8s: u64,
    pub created_timestamp_seconds: u64,
    pub source_nns_neuron_id: Option<u64>,
    pub auto_stake_maturity: Option<bool>,
    pub aging_since_timestamp_seconds: u64,
    pub dissolve_state: Option<DissolveState>,
    pub voting_power_percentage_multiplier: u64,
    pub vesting_period_seconds: Option<u64>,
    pub disburse_maturity_in_progress: Vec<EmptyRecord>,
    pub followees: Vec<(u64, Followees)>,
    pub topic_followees: Option<EmptyRecord>,
    pub neuron_fees_e8s: u64,
    pub latest_reward_event_participation: Option<SnsRewardEventParticipation>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NeuronPermission {
    pub principal: Option<Principal>,
    pub permission_type: Vec<i32>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum DissolveState {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ListProposals {
    pub include_reward_status: Vec<i32>,
    pub before_proposal: Option<ProposalId>,
    pub limit: u32,
    pub exclude_type: Vec<u64>,
    pub include_status: Vec<i32>,
    pub include_topics: Option<Vec<EmptyRecord>>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ProposalId {
    pub id: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ListProposalsResponse {
    pub include_ballots_by_caller: Option<bool>,
    pub include_topic_filtering: Option<bool>,
    pub proposals: Vec<SnsProposalRecord>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SnsProposalRecord {
    pub id: Option<ProposalId>,
    pub ballots: Vec<(String, SnsBallotRecord)>,
    pub decided_timestamp_seconds: u64,
    pub executed_timestamp_seconds: u64,
    pub failed_timestamp_seconds: u64,
    pub reject_cost_e8s: u64,
    pub proposal: Option<SnsProposalPayload>,
    pub proposer: Option<NeuronId>,
    pub is_eligible_for_rewards: bool,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SnsBallotRecord {
    pub vote: i32,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SnsProposalPayload {
    pub url: String,
    pub title: String,
    pub summary: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ManageNeuron {
    pub subaccount: Vec<u8>,
    pub command: Option<Command>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum Command {
    ClaimOrRefresh(ClaimOrRefresh),
    Configure(Configure),
    Disburse(Disburse),
    Follow(Follow),
    MakeProposal(Proposal),
    RegisterVote(RegisterVote),
    SetFollowing(SetFollowing),
    AddNeuronPermissions(AddNeuronPermissions),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Follow {
    pub function_id: u64,
    pub followees: Vec<NeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SetFollowing {
    pub topic_following: Vec<FolloweesForTopic>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct AddNeuronPermissions {
    pub principal_id: Option<Principal>,
    pub permissions_to_add: Option<NeuronPermissionList>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct FolloweesForTopic {
    pub topic: Option<Topic>,
    pub followees: Vec<Followee>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Followee {
    pub neuron_id: Option<NeuronId>,
    pub alias: Option<String>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum Topic {
    DaoCommunitySettings,
    SnsFrameworkManagement,
    DappCanisterManagement,
    ApplicationBusinessLogic,
    Governance,
    TreasuryAssetManagement,
    CriticalDappOperations,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Proposal {
    pub url: String,
    pub title: String,
    pub summary: String,
    pub action: Option<Action>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum Action {
    Motion(Motion),
    UpgradeSnsControlledCanister(UpgradeSnsControlledCanister),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Motion {
    pub motion_text: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct UpgradeSnsControlledCanister {
    pub new_canister_wasm: Vec<u8>,
    pub chunked_canister_wasm: Option<ChunkedCanisterWasm>,
    pub mode: Option<i32>,
    pub canister_id: Option<Principal>,
    pub canister_upgrade_arg: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ChunkedCanisterWasm {
    pub wasm_module_hash: Vec<u8>,
    pub store_canister_id: Option<Principal>,
    pub chunk_hashes_list: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct RegisterVote {
    pub vote: i32,
    pub proposal: Option<ProposalId>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ClaimOrRefresh {
    pub by: Option<By>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum By {
    MemoAndController(MemoAndController),
    NeuronId(EmptyRecord),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct MemoAndController {
    pub controller: Option<Principal>,
    pub memo: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Configure {
    pub operation: Option<Operation>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Disburse {
    pub amount: Option<Tokens>,
    pub to_account: Option<Account>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Tokens {
    pub e8s: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub owner: Option<Principal>,
    pub subaccount: Option<Subaccount>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Subaccount {
    pub subaccount: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum Operation {
    IncreaseDissolveDelay(IncreaseDissolveDelay),
    StartDissolving(EmptyRecord),
    StopDissolving(EmptyRecord),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct IncreaseDissolveDelay {
    pub additional_dissolve_delay_seconds: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ManageNeuronResponse {
    pub command: Option<CommandResponse>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum CommandResponse {
    Error(GovernanceError),
    ClaimOrRefresh(ClaimOrRefreshResponse),
    Configure(EmptyRecord),
    Disburse(DisburseResponse),
    Follow(EmptyRecord),
    MakeProposal(MakeProposalResponse),
    RegisterVote(EmptyRecord),
    SetFollowing(EmptyRecord),
    AddNeuronPermission(EmptyRecord),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct GovernanceError {
    pub error_message: String,
    pub error_type: i32,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ClaimOrRefreshResponse {
    pub refreshed_neuron_id: Option<NeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct DisburseResponse {
    pub transfer_block_height: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct MakeProposalResponse {
    pub proposal_id: Option<ProposalId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnsGovernanceSetupError {
    Artifact(String),
    PocketIcMissing,
}

struct GovernanceLedgerFixture {
    pic: std::rc::Rc<PocketIc>,
    governance: Principal,
    ledger: Principal,
    controller: Principal,
}

pub fn install_real_sns_governance_empty_state(
    required: bool,
) -> Result<(), SnsGovernanceSetupError> {
    let artifacts = match resolve_from_env(required) {
        Ok(ArtifactStatus::Ready(set)) => set,
        Ok(ArtifactStatus::Skipped(message)) => {
            return Err(SnsGovernanceSetupError::Artifact(message));
        }
        Err(err) => return Err(SnsGovernanceSetupError::Artifact(err)),
    };
    if !pocketic_env::pocketic_available() {
        return Err(SnsGovernanceSetupError::PocketIcMissing);
    }
    let governance_wasm = artifacts
        .load_required("sns_governance")
        .map_err(SnsGovernanceSetupError::Artifact)?;
    let pic = std::rc::Rc::new(pocketic_env::new_sns_pic());
    let governance = pocketic_env::create_sns_canister(
        &pic,
        governance_wasm,
        candid::encode_one(Governance {
            root_canister_id: Some(Principal::from_slice(&[21; 29])),
            id_to_nervous_system_functions: vec![],
            metrics: None,
            maturity_modulation: None,
            mode: 1,
            parameters: Some(NervousSystemParameters {
                default_followees: Some(DefaultFollowees { followees: vec![] }),
                max_dissolve_delay_seconds: Some(io_core_model::TWO_WEEK_SECONDS),
                max_dissolve_delay_bonus_percentage: Some(0),
                max_followees_per_function: Some(15),
                neuron_claimer_permissions: Some(NeuronPermissionList {
                    permissions: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
                }),
                neuron_minimum_stake_e8s: Some(100_000_000),
                max_neuron_age_for_age_bonus: Some(0),
                initial_voting_period_seconds: Some(86_400),
                neuron_minimum_dissolve_delay_to_vote_seconds: Some(
                    io_core_model::TWO_WEEK_SECONDS - 1,
                ),
                reject_cost_e8s: Some(10_000_000_000),
                max_proposals_to_keep_per_action: Some(100),
                wait_for_quiet_deadline_increase_seconds: Some(1),
                max_number_of_neurons: Some(100_000),
                transaction_fee_e8s: Some(10_000),
                max_number_of_proposals_with_ballots: Some(700),
                max_age_bonus_percentage: Some(0),
                neuron_grantable_permissions: Some(NeuronPermissionList {
                    permissions: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
                }),
                voting_rewards_parameters: Some(VotingRewardsParameters {
                    final_reward_rate_basis_points: Some(0),
                    initial_reward_rate_basis_points: Some(0),
                    reward_rate_transition_duration_seconds: Some(1),
                    round_duration_seconds: Some(86_400),
                }),
                maturity_modulation_disabled: Some(true),
                max_number_of_principals_per_neuron: Some(10),
                automatically_advance_target_version: Some(false),
                custom_proposal_criticality: None,
            }),
            is_finalizing_disburse_maturity: Some(false),
            deployed_version: None,
            cached_upgrade_steps: None,
            sns_initialization_parameters: "direct-empty-governance-smoke".to_string(),
            latest_reward_event: None,
            pending_version: None,
            swap_canister_id: Some(Principal::from_slice(&[22; 29])),
            ledger_canister_id: Some(Principal::from_slice(&[23; 29])),
            proposals: vec![],
            in_flight_commands: vec![],
            sns_metadata: Some(ManageSnsMetadata {
                url: Some(format!("{}://example.invalid", "https")),
                logo: None,
                name: Some("IO Test".to_string()),
                description: Some("Direct governance smoke only".to_string()),
            }),
            neurons: vec![],
            genesis_timestamp_seconds: 1,
            target_version: None,
            timers: None,
            upgrade_journal: None,
        })
        .expect("SNS governance init should encode"),
    );
    for _ in 0..5 {
        pic.tick();
    }
    let neurons: ListNeuronsResponse = icrc::query_one(
        &pic,
        governance,
        "list_neurons",
        ListNeurons {
            of_principal: None,
            limit: 10,
            start_page_at: None,
        },
    );
    assert!(neurons.neurons.is_empty());
    let proposals: ListProposalsResponse = icrc::query_one(
        &pic,
        governance,
        "list_proposals",
        ListProposals {
            include_reward_status: vec![],
            before_proposal: None,
            limit: 10,
            exclude_type: vec![],
            include_status: vec![],
            include_topics: None,
        },
    );
    assert!(proposals.proposals.is_empty());
    let params: NervousSystemParameters =
        icrc::query_one(&pic, governance, "get_nervous_system_parameters", ());
    assert_eq!(params.neuron_minimum_stake_e8s, Some(100_000_000));
    Ok(())
}

pub fn install_real_sns_governance_and_stake_neuron(
    required: bool,
) -> Result<(), SnsGovernanceSetupError> {
    let fixture = setup_real_sns_governance_with_ledger(required, 500_000_000)?;
    let stake_e8s = 200_000_000_u64;
    let memo = 77_u64;
    let neuron_id =
        stake_and_claim_neuron(&fixture, stake_e8s, memo, b"stake").expect("claim should succeed");
    let neuron = listed_neuron(&fixture, &neuron_id);
    assert_eq!(neuron.cached_neuron_stake_e8s, stake_e8s);
    Ok(())
}

pub fn install_real_sns_governance_and_topup_neuron(
    required: bool,
) -> Result<(), SnsGovernanceSetupError> {
    let fixture = setup_real_sns_governance_with_ledger(required, 700_000_000)?;
    let memo = 88_u64;
    let first_stake_e8s = 200_000_000_u64;
    let topup_e8s = 150_000_000_u64;
    let neuron_id = stake_and_claim_neuron(&fixture, first_stake_e8s, memo, b"stake")
        .expect("initial claim should succeed");
    let topped_up_id = stake_and_claim_neuron(&fixture, topup_e8s, memo, b"topup")
        .expect("top-up refresh should succeed");
    assert_eq!(topped_up_id, neuron_id);
    let neuron = listed_neuron(&fixture, &neuron_id);
    assert_eq!(neuron.cached_neuron_stake_e8s, first_stake_e8s + topup_e8s);
    Ok(())
}

pub fn install_real_sns_governance_and_reject_below_minimum_stake(
    required: bool,
) -> Result<(), SnsGovernanceSetupError> {
    let fixture = setup_real_sns_governance_with_ledger(required, 200_000_000)?;
    let err = stake_and_claim_neuron(&fixture, 50_000_000, 99, b"too-small")
        .expect_err("below-minimum stake should return a governance error");
    assert_eq!(err.error_type, 13, "unexpected error type: {err:?}");
    assert!(
        err.error_message.contains("at least 100000000 e8s")
            && err.error_message.contains("was 50000000 e8s"),
        "unexpected minimum-stake error: {err:?}"
    );
    Ok(())
}

pub fn install_real_sns_governance_and_observe_dissolve_delay_boundaries(
    required: bool,
) -> Result<(), SnsGovernanceSetupError> {
    let fixture = setup_real_sns_governance_with_ledger(required, 500_000_000)?;
    let neuron_id = stake_and_claim_neuron(&fixture, 200_000_000, 111, b"dissolve")
        .expect("claim should succeed");
    let initial_neuron = listed_neuron(&fixture, &neuron_id);
    assert_eq!(dissolve_delay_seconds(&initial_neuron), 0);

    configure_increase_dissolve_delay(&fixture, &neuron_id, 1_209_600);
    let eligible_neuron = listed_neuron(&fixture, &neuron_id);
    assert_eq!(dissolve_delay_seconds(&eligible_neuron), 1_209_600);
    Ok(())
}

pub fn run_candidate_reward_event_participation_contract(
    required: bool,
) -> Result<(), SnsGovernanceSetupError> {
    let fixture = setup_real_sns_governance_with_ledger(required, 5_000_000_000)?;
    let neuron_ids = (1_u64..=5)
        .map(|memo| {
            let id = stake_and_claim_neuron(&fixture, 500_000_000, memo, b"reward-contract")
                .expect("candidate contract neuron claim should succeed");
            configure_increase_dissolve_delay(
                &fixture,
                &id,
                u32::try_from(io_core_model::TWO_WEEK_SECONDS).unwrap(),
            );
            id
        })
        .collect::<Vec<_>>();
    let alice = &neuron_ids[0];
    let bob = &neuron_ids[1];
    let carol = &neuron_ids[2];
    let follower = &neuron_ids[3];
    let non_voter = &neuron_ids[4];

    expect_manage_success(
        &fixture,
        follower,
        Command::Follow(Follow {
            function_id: 1,
            followees: vec![alice.clone()],
        }),
        "follow",
    );
    let proposal_1 = make_motion(&fixture, alice, "candidate reward event one");
    register_vote(&fixture, bob, proposal_1, 2);
    register_vote(&fixture, carol, proposal_1, 1);
    let proposal_2 = make_motion(&fixture, bob, "candidate reward event two");
    register_vote(&fixture, alice, proposal_2, 2);
    register_vote(&fixture, carol, proposal_2, 2);

    let event_1 = advance_until_reward_event(&fixture, 2, 0);
    assert_eq!(event_1.distributed_e8s_equivalent, 0);
    assert_eq!(event_1.settled_proposals.len(), 2);
    let event_1_end = event_1
        .end_timestamp_seconds
        .expect("candidate reward event must have an end timestamp");
    let listed_1 = list_all_neurons_paged(&fixture, 2);
    assert_eq!(listed_1.len(), 5);
    let listed_ids = listed_1
        .iter()
        .filter_map(|neuron| neuron.id.as_ref())
        .map(|id| id.id.clone())
        .collect::<Vec<_>>();
    let mut sorted_ids = listed_ids.clone();
    sorted_ids.sort();
    assert_eq!(
        listed_ids, sorted_ids,
        "list_neurons pagination must be deterministic by neuron ID"
    );
    let expected_ids = neuron_ids
        .iter()
        .map(|id| id.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        listed_ids
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        expected_ids
    );
    let expected_two_proposal_shares = 1_000_000_000_u128;
    for id in [alice, bob, carol, follower] {
        let neuron = find_neuron(&listed_1, id);
        let participation = neuron
            .latest_reward_event_participation
            .clone()
            .expect("direct and followed voters must have event participation");
        assert_eq!(
            participation.reward_event_end_timestamp_seconds,
            event_1_end
        );
        assert_eq!(
            participation.exact_reward_shares().unwrap(),
            expected_two_proposal_shares
        );
        assert_eq!(neuron.maturity_e8s_equivalent, 0);
        assert_eq!(neuron.staked_maturity_e8s_equivalent.unwrap_or(0), 0);
    }
    assert!(
        find_neuron(&listed_1, non_voter)
            .latest_reward_event_participation
            .is_none(),
        "non-voter must not be tagged to the event"
    );
    assert_eq!(
        listed_1
            .iter()
            .filter_map(|neuron| neuron.latest_reward_event_participation.clone())
            .map(|participation| participation.exact_reward_shares().unwrap())
            .sum::<u128>(),
        4_000_000_000,
        "multiple-proposal voting powers must sum exactly"
    );
    let direct: GetNeuronResponse = icrc::query_one(
        &fixture.pic,
        fixture.governance,
        "get_neuron",
        GetNeuron {
            neuron_id: Some(alice.clone()),
        },
    );
    let direct = match direct.result {
        Some(GetNeuronResult::Neuron(neuron)) => neuron,
        other => panic!("candidate get_neuron did not return the requested neuron: {other:?}"),
    };
    assert_eq!(
        direct
            .latest_reward_event_participation
            .as_ref()
            .map(|participation| participation.reward_event_end_timestamp_seconds),
        Some(event_1_end),
        "get_neuron and paginated list_neurons must expose the same event tag"
    );

    let proposal_3 = make_motion(&fixture, bob, "candidate replacement event");
    let event_2 = advance_until_reward_event(&fixture, 1, event_1.round);
    assert_eq!(
        event_2
            .settled_proposals
            .iter()
            .map(|proposal| proposal.id)
            .collect::<Vec<_>>(),
        vec![proposal_3]
    );
    let event_2_end = event_2.end_timestamp_seconds.unwrap();
    let listed_2 = list_all_neurons_paged(&fixture, 2);
    assert_eq!(
        find_neuron(&listed_2, bob)
            .latest_reward_event_participation
            .as_ref()
            .unwrap()
            .reward_event_end_timestamp_seconds,
        event_2_end
    );
    for id in [alice, carol, follower] {
        assert_eq!(
            find_neuron(&listed_2, id)
                .latest_reward_event_participation
                .as_ref()
                .unwrap()
                .reward_event_end_timestamp_seconds,
            event_1_end,
            "non-participant must retain its older tag, which clients ignore for the new event"
        );
    }

    let event_3 = advance_until_reward_event(&fixture, 0, event_2.round);
    assert!(event_3.settled_proposals.is_empty());

    let consistency_e1 = latest_reward_event(&fixture);
    let first_page = list_neurons_page(&fixture, 2, None);
    assert_eq!(first_page.neurons.len(), 2);
    let consistency_e2 = advance_until_reward_event(&fixture, 0, consistency_e1.round);
    let _mixed_second_page = list_neurons_page(
        &fixture,
        2,
        first_page
            .neurons
            .last()
            .and_then(|neuron| neuron.id.clone()),
    );
    assert_ne!(
        (consistency_e1.end_timestamp_seconds, consistency_e1.round),
        (consistency_e2.end_timestamp_seconds, consistency_e2.round),
        "E1/pages/E2 client must discard pages when an event occurs between page reads"
    );
    let delayed_before = latest_reward_event(&fixture);
    fixture.pic.advance_time(Duration::from_secs(
        io_core_model::TWO_WEEK_SECONDS
            .checked_mul(3)
            .unwrap()
            .saturating_add(1),
    ));
    for _ in 0..30 {
        fixture.pic.tick();
    }
    let delayed_after = latest_reward_event(&fixture);
    let delta = delayed_after
        .round
        .checked_sub(delayed_before.round)
        .expect("candidate reward-event round must not regress");
    assert!(
        delta >= 3,
        "delayed periodic work should catch up multiple rounds"
    );
    assert_eq!(
        delayed_after.rounds_since_last_distribution,
        Some(delta),
        "one actual catch-up event must report the complete elapsed round span"
    );
    Ok(())
}

fn expect_manage_success(
    fixture: &GovernanceLedgerFixture,
    neuron_id: &NeuronId,
    command: Command,
    operation: &str,
) -> ManageNeuronResponse {
    let response: ManageNeuronResponse = icrc::update_one(
        &fixture.pic,
        fixture.governance,
        fixture.controller,
        "manage_neuron",
        ManageNeuron {
            subaccount: neuron_id.id.clone(),
            command: Some(command),
        },
    );
    match &response.command {
        Some(CommandResponse::Error(error)) => panic!("{operation} failed: {error:?}"),
        None => panic!("{operation} returned no command response"),
        Some(_) => response,
    }
}

fn make_motion(fixture: &GovernanceLedgerFixture, neuron_id: &NeuronId, title: &str) -> u64 {
    let response = expect_manage_success(
        fixture,
        neuron_id,
        Command::MakeProposal(Proposal {
            url: String::new(),
            title: title.to_string(),
            summary: title.to_string(),
            action: Some(Action::Motion(Motion {
                motion_text: title.to_string(),
            })),
        }),
        "make proposal",
    );
    match response.command {
        Some(CommandResponse::MakeProposal(response)) => {
            response
                .proposal_id
                .expect("proposal response must contain an id")
                .id
        }
        other => panic!("unexpected make proposal response: {other:?}"),
    }
}

fn register_vote(
    fixture: &GovernanceLedgerFixture,
    neuron_id: &NeuronId,
    proposal_id: u64,
    vote: i32,
) {
    expect_manage_success(
        fixture,
        neuron_id,
        Command::RegisterVote(RegisterVote {
            vote,
            proposal: Some(ProposalId { id: proposal_id }),
        }),
        "register vote",
    );
}

fn latest_reward_event(fixture: &GovernanceLedgerFixture) -> SnsRewardEvent {
    icrc::query_one(
        &fixture.pic,
        fixture.governance,
        "get_latest_reward_event",
        (),
    )
}

fn advance_until_reward_event(
    fixture: &GovernanceLedgerFixture,
    expected_settled: usize,
    after_round: u64,
) -> SnsRewardEvent {
    for _ in 0..8 {
        fixture
            .pic
            .advance_time(Duration::from_secs(io_core_model::TWO_WEEK_SECONDS + 1));
        for _ in 0..20 {
            fixture.pic.tick();
        }
        let event = latest_reward_event(fixture);
        if event.round > after_round && event.settled_proposals.len() == expected_settled {
            return event;
        }
    }
    panic!(
        "candidate Governance did not produce expected reward event after round {after_round} with {expected_settled} settled proposals"
    )
}

fn list_neurons_page(
    fixture: &GovernanceLedgerFixture,
    limit: u32,
    start_page_at: Option<NeuronId>,
) -> ListNeuronsResponse {
    icrc::query_one(
        &fixture.pic,
        fixture.governance,
        "list_neurons",
        ListNeurons {
            of_principal: None,
            limit,
            start_page_at,
        },
    )
}

fn list_all_neurons_paged(fixture: &GovernanceLedgerFixture, limit: u32) -> Vec<SnsNeuronRecord> {
    let mut all = Vec::new();
    let mut cursor = None;
    loop {
        let page = list_neurons_page(fixture, limit, cursor);
        if page.neurons.is_empty() {
            return all;
        }
        cursor = page.neurons.last().and_then(|neuron| neuron.id.clone());
        all.extend(page.neurons);
    }
}

fn find_neuron<'a>(neurons: &'a [SnsNeuronRecord], id: &NeuronId) -> &'a SnsNeuronRecord {
    neurons
        .iter()
        .find(|neuron| neuron.id.as_ref() == Some(id))
        .expect("expected neuron in paginated result")
}

pub fn run_candidate_reward_shares_drive_io_rewards(
    required: bool,
) -> Result<(), SnsGovernanceSetupError> {
    use crate::sns_root_setup::SnsRootCanister;
    use candid::{decode_one, encode_one, Nat};
    use io_stream_manager::{
        Account as StreamAccount, ApiError, CompleteLiquidReceiptArgs, CompletedReceiptResult,
        InitArgs, LiquidReceiptProgress, PrepareLiquidReceiptArgs, ReceiptKind, RewardCohort,
        Status, StreamConfig, StreamProgress,
    };
    use pocket_ic::CanisterSettings;

    let artifacts = match resolve_from_env(required) {
        Ok(ArtifactStatus::Ready(set)) => set,
        Ok(ArtifactStatus::Skipped(message)) => {
            return Err(SnsGovernanceSetupError::Artifact(message));
        }
        Err(error) => return Err(SnsGovernanceSetupError::Artifact(error)),
    };
    if !pocketic_env::pocketic_available() {
        return Err(SnsGovernanceSetupError::PocketIcMissing);
    }
    let candidate_governance_wasm = artifacts
        .load_required("sns_governance")
        .map_err(SnsGovernanceSetupError::Artifact)?;
    let root_wasm = artifacts
        .load_required("sns_root")
        .map_err(SnsGovernanceSetupError::Artifact)?;
    let ledger_wasm = artifacts
        .load_required("sns_ledger")
        .map_err(SnsGovernanceSetupError::Artifact)?;
    let stream_wasm = local_debug_wasm("io_stream_manager")?;
    let nns_wasm = local_debug_wasm("mock_nns_governance")?;
    let governance_hash = Sha256::digest(&candidate_governance_wasm).to_vec();

    let pic = std::rc::Rc::new(pocketic_env::new_sns_pic());
    let sns_subnet = pic.topology().get_sns().expect("SNS subnet exists");
    let root = pic.create_canister_on_subnet(None, None, sns_subnet);
    pic.add_cycles(root, 2_000_000_000_000);
    let governance = pic.create_canister_on_subnet(
        None,
        Some(CanisterSettings {
            controllers: Some(vec![root]),
            ..Default::default()
        }),
        sns_subnet,
    );
    pic.add_cycles(governance, 2_000_000_000_000);
    let stream = pocketic_env::create_empty_application_canister(&pic);
    let nns_manager = pocketic_env::create_application_canister(&pic, nns_wasm, Vec::new());
    let controller = Principal::from_slice(&[71; 29]);
    let reserve_subaccount = icrc::subaccount("candidate-reward-reserve");
    let liquid_subaccount = icrc::subaccount("candidate-reward-liquid");
    let reserve = icrc::account(stream, Some(reserve_subaccount));
    let liquid = icrc::account(stream, Some(liquid_subaccount));
    let io_ledger = pocketic_env::create_sns_canister(
        &pic,
        ledger_wasm.clone(),
        icrc::ledger_init_arg(
            Principal::anonymous(),
            icrc::account(Principal::from_slice(&[72; 29]), None),
            vec![
                (icrc::account(controller, None), 5_000_000_000),
                (reserve.clone(), 20_000_000_000),
            ],
        ),
    );
    let maturity_subaccount = [9_u8; 32];
    let icp_ledger = pocketic_env::create_sns_canister(
        &pic,
        ledger_wasm,
        icrc::ledger_init_arg(
            Principal::anonymous(),
            icrc::account(Principal::from_slice(&[73; 29]), None),
            vec![
                (liquid.clone(), 10_000_000_000),
                (
                    icrc::account(nns_manager, Some(maturity_subaccount)),
                    2_000_000_000,
                ),
            ],
        ),
    );
    pic.install_canister(
        root,
        root_wasm,
        encode_one(SnsRootCanister {
            dapp_canister_ids: vec![stream],
            extensions: None,
            testflight: true,
            archive_canister_ids: vec![],
            governance_canister_id: Some(governance),
            index_canister_id: None,
            swap_canister_id: None,
            ledger_canister_id: Some(io_ledger),
            timers: None,
        })
        .unwrap(),
        None,
    );
    pic.install_canister(
        governance,
        candidate_governance_wasm,
        governance_init_arg(Some(io_ledger), Some(root)),
        None,
    );
    for _ in 0..5 {
        pic.tick();
    }
    let neuron_ids = (1_u64..=3)
        .map(|memo| {
            let fixture = GovernanceLedgerFixture {
                pic: pic.clone(),
                governance,
                ledger: io_ledger,
                controller,
            };
            let id = stake_and_claim_neuron(&fixture, 500_000_000, memo, b"io-reward")
                .expect("candidate neuron claim succeeds");
            configure_increase_dissolve_delay(
                &fixture,
                &id,
                u32::try_from(io_core_model::TWO_WEEK_SECONDS).unwrap(),
            );
            id
        })
        .collect::<Vec<_>>();
    let excluded = StreamAccount {
        owner: governance,
        subaccount: Some(neuron_ids[2].id.clone()),
    };
    pic.install_canister(
        stream,
        stream_wasm.clone(),
        encode_one(InitArgs {
            config: StreamConfig {
                io_ledger,
                icp_ledger,
                nns_manager,
                jupiter_receipt_source: StreamAccount {
                    owner: nns_manager,
                    subaccount: None,
                },
                two_week_receipt_source: StreamAccount {
                    owner: nns_manager,
                    subaccount: Some(maturity_subaccount.to_vec()),
                },
                jupiter_io_account: StreamAccount {
                    owner: controller,
                    subaccount: Some(vec![10; 32]),
                },
                sns_governance: governance,
                sns_root: root,
                expected_sns_governance_module_hash: governance_hash,
                approved_reward_event_duration_seconds: io_core_model::TWO_WEEK_SECONDS,
                io_reserve: StreamAccount {
                    owner: stream,
                    subaccount: Some(reserve_subaccount.to_vec()),
                },
                liquid_icp: StreamAccount {
                    owner: stream,
                    subaccount: Some(liquid_subaccount.to_vec()),
                },
                excluded_io_accounts: vec![excluded],
                minimum_redemption_io_e8s: 20_000,
                expected_io_fee_e8s: FEE_E8S as u128,
                expected_icp_fee_e8s: FEE_E8S as u128,
                maximum_request_lifetime_nanos: 900_000_000_000,
                retry_delay_nanos: 1_000_000_000,
                ledger_deduplication_window_nanos: 86_400_000_000_000,
            },
            next_cohort_timestamp_seconds: 0,
        })
        .unwrap(),
        None,
    );
    let ready: Result<(), ApiError> = decode_one(
        &pic.update_call(stream, governance, "set_paused", encode_one(false).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(ready, Ok(()));
    let cohort: Result<RewardCohort, ApiError> = decode_one(
        &pic.update_call(
            stream,
            Principal::anonymous(),
            "capture_reward_cohort",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let cohort = cohort.expect("installed stream captures the candidate cohort");
    assert_eq!(cohort.members.len(), 2, "configured neuron is excluded");
    let fixture = GovernanceLedgerFixture {
        pic: pic.clone(),
        governance,
        ledger: io_ledger,
        controller,
    };
    let proposal = make_motion(&fixture, &neuron_ids[0], "installed IO reward shares");
    register_vote(&fixture, &neuron_ids[1], proposal, 2);
    register_vote(&fixture, &neuron_ids[2], proposal, 1);
    let event =
        advance_until_reward_event(&fixture, 1, cohort.reward_event_at_capture.unwrap().round);
    assert_eq!(event.settled_proposals.len(), 1);
    let status: Status = decode_one(
        &pic.query_call(stream, controller, "get_status", encode_one(()).unwrap())
            .unwrap(),
    )
    .unwrap();
    if status.has_active_reward_cohort {
        let closed: Result<RewardCohort, ApiError> = decode_one(
            &pic.update_call(
                stream,
                Principal::anonymous(),
                "close_reward_cohort",
                encode_one(()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        closed.expect("installed stream closes from the candidate event");
    }
    let status: Status = decode_one(
        &pic.query_call(stream, controller, "get_status", encode_one(()).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(status.has_pending_reward_cohort);
    let liquid_amount = 1_000_000_000_u64;
    let permit: Result<io_stream_manager::LiquidReceiptPermit, ApiError> = decode_one(
        &pic.update_call(
            stream,
            nns_manager,
            "prepare_liquid_receipt",
            encode_one(PrepareLiquidReceiptArgs {
                receipt_sequence: 0,
                receipt_kind: ReceiptKind::TwoWeekMaturity,
                source_operation_id: b"candidate-event-1".to_vec(),
                liquid_amount_e8s: liquid_amount as u128,
                cohort_generation: Some(cohort.generation),
            })
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let permit = permit.expect("two-week receipt permit is prepared");
    let before = neuron_ids[..2]
        .iter()
        .map(|id| {
            icrc::icrc1_balance_of(
                &pic,
                io_ledger,
                icrc::account(governance, Some(id.id.clone().try_into().unwrap())),
            )
        })
        .collect::<Vec<Nat>>();
    let receipt_block = icrc::icrc1_transfer(
        &pic,
        icp_ledger,
        nns_manager,
        icrc::transfer_arg(
            Some(maturity_subaccount),
            icrc::account(
                permit.destination.owner,
                permit
                    .destination
                    .subaccount
                    .clone()
                    .map(|bytes| bytes.try_into().unwrap()),
            ),
            liquid_amount,
            Some(FEE_E8S),
            Some(&permit.memo),
            Some(pic.get_time().as_nanos_since_unix_epoch()),
        ),
    )
    .expect("maturity source delivers exact liquid receipt");
    let proved: Result<LiquidReceiptProgress, ApiError> = decode_one(
        &pic.update_call(
            stream,
            nns_manager,
            "complete_liquid_receipt",
            encode_one(CompleteLiquidReceiptArgs {
                receipt_sequence: 0,
                block_index: u128::try_from(receipt_block.0).unwrap(),
            })
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(proved, Ok(LiquidReceiptProgress::ReceiptProved));
    let mut completed = None;
    let mut upgraded_between_recipients = false;
    for _ in 0..12 {
        let progress: Result<StreamProgress, ApiError> = decode_one(
            &pic.update_call(
                stream,
                Principal::anonymous(),
                "resume",
                encode_one(()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        if let Ok(StreamProgress::LiquidReceipt(LiquidReceiptProgress::Completed(result))) =
            progress
        {
            completed = Some(result);
            break;
        }
        let after = neuron_ids[..2]
            .iter()
            .map(|id| {
                icrc::icrc1_balance_of(
                    &pic,
                    io_ledger,
                    icrc::account(governance, Some(id.id.clone().try_into().unwrap())),
                )
            })
            .collect::<Vec<Nat>>();
        if !upgraded_between_recipients
            && after
                .iter()
                .zip(&before)
                .filter(|(after, before)| after > before)
                .count()
                == 1
        {
            pocketic_env::upgrade_canister(
                &pic,
                stream,
                stream_wasm.clone(),
                encode_one(()).unwrap(),
            );
            upgraded_between_recipients = true;
        }
    }
    assert!(
        upgraded_between_recipients,
        "stream upgrades after exactly one recipient"
    );
    let result = match completed.expect("reward receipt completes") {
        CompletedReceiptResult::TwoWeek(result) => result,
        other => panic!("unexpected receipt result: {other:?}"),
    };
    assert!(result.backed_io_pool_e8s > 0);
    assert!(result.distributed_io_e8s > 0);
    assert_eq!(
        result
            .distributed_io_e8s
            .checked_add(result.total_dust_io_e8s),
        Some(result.backed_io_pool_e8s)
    );
    let after = neuron_ids[..2]
        .iter()
        .map(|id| {
            icrc::icrc1_balance_of(
                &pic,
                io_ledger,
                icrc::account(governance, Some(id.id.clone().try_into().unwrap())),
            )
        })
        .collect::<Vec<Nat>>();
    assert!(after
        .iter()
        .zip(before)
        .all(|(after, before)| after > &before));
    Ok(())
}

pub fn run_official_to_candidate_reward_participation_upgrade(
    required: bool,
) -> Result<(), SnsGovernanceSetupError> {
    let artifacts = match resolve_from_env(required) {
        Ok(ArtifactStatus::Ready(set)) => set,
        Ok(ArtifactStatus::Skipped(message)) => {
            return Err(SnsGovernanceSetupError::Artifact(message));
        }
        Err(error) => return Err(SnsGovernanceSetupError::Artifact(error)),
    };
    let baseline_name = artifacts
        .manifest
        .value("baseline", "sns_governance_wasm")
        .ok_or_else(|| {
            SnsGovernanceSetupError::Artifact("bundle lacks official Governance baseline".into())
        })?;
    let baseline = std::fs::read(artifacts.wasm_dir.join(baseline_name))
        .map_err(|error| SnsGovernanceSetupError::Artifact(error.to_string()))?;
    let candidate = artifacts
        .load_required("sns_governance")
        .map_err(SnsGovernanceSetupError::Artifact)?;
    let ledger_wasm = artifacts
        .load_required("sns_ledger")
        .map_err(SnsGovernanceSetupError::Artifact)?;
    if !pocketic_env::pocketic_available() {
        return Err(SnsGovernanceSetupError::PocketIcMissing);
    }
    let pic = std::rc::Rc::new(pocketic_env::new_sns_pic());
    let sns_subnet = pic.topology().get_sns().expect("SNS subnet exists");
    let governance = pic.create_canister_on_subnet(None, None, sns_subnet);
    pic.add_cycles(governance, 2_000_000_000_000);
    let controller = Principal::from_slice(&[81; 29]);
    let ledger = pocketic_env::create_sns_canister(
        &pic,
        ledger_wasm,
        icrc::ledger_init_arg(
            Principal::anonymous(),
            icrc::account(Principal::from_slice(&[82; 29]), None),
            vec![(icrc::account(controller, None), 2_000_000_000)],
        ),
    );
    pic.install_canister(
        governance,
        baseline,
        governance_init_arg(Some(ledger), Some(Principal::from_slice(&[83; 29]))),
        None,
    );
    for _ in 0..5 {
        pic.tick();
    }
    let fixture = GovernanceLedgerFixture {
        pic,
        governance,
        ledger,
        controller,
    };
    let neuron_id = stake_and_claim_neuron(&fixture, 500_000_000, 1, b"old-state")
        .expect("official Governance creates the old neuron state");
    configure_increase_dissolve_delay(
        &fixture,
        &neuron_id,
        u32::try_from(io_core_model::TWO_WEEK_SECONDS).unwrap(),
    );
    assert_eq!(
        listed_neuron(&fixture, &neuron_id).latest_reward_event_participation,
        None,
        "official old Governance must decode without the additive field"
    );
    fixture
        .pic
        .upgrade_canister(
            governance,
            candidate.clone(),
            candid::encode_one(()).unwrap(),
            None,
        )
        .expect("official-to-candidate Governance upgrade succeeds");
    for _ in 0..5 {
        fixture.pic.tick();
    }
    assert_eq!(
        listed_neuron(&fixture, &neuron_id).latest_reward_event_participation,
        None,
        "old neuron state upgrades with None"
    );
    let proposal = make_motion(&fixture, &neuron_id, "candidate upgrade reward event");
    let event = advance_until_reward_event(&fixture, 1, 0);
    assert_eq!(event.settled_proposals[0].id, proposal);
    let populated = listed_neuron(&fixture, &neuron_id)
        .latest_reward_event_participation
        .expect("first candidate reward event populates the additive field");
    assert_eq!(
        populated.reward_event_end_timestamp_seconds,
        event.end_timestamp_seconds.unwrap()
    );
    assert!(populated.exact_reward_shares().unwrap() > 0);
    fixture
        .pic
        .upgrade_canister(governance, candidate, candid::encode_one(()).unwrap(), None)
        .expect("candidate same-Wasm upgrade succeeds");
    for _ in 0..5 {
        fixture.pic.tick();
    }
    assert_eq!(
        listed_neuron(&fixture, &neuron_id).latest_reward_event_participation,
        Some(populated),
        "candidate same-Wasm upgrade preserves reward participation"
    );
    run_candidate_reward_shares_drive_io_rewards(required)
}

fn local_debug_wasm(name: &str) -> Result<Vec<u8>, SnsGovernanceSetupError> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/debug")
        .join(format!("{name}.wasm"));
    std::fs::read(&path).map_err(|error| {
        SnsGovernanceSetupError::Artifact(format!(
            "build local debug Wasm {} before the profile: {error}",
            path.display()
        ))
    })
}

fn setup_real_sns_governance_with_ledger(
    required: bool,
    initial_user_balance_e8s: u64,
) -> Result<GovernanceLedgerFixture, SnsGovernanceSetupError> {
    let artifacts = match resolve_from_env(required) {
        Ok(ArtifactStatus::Ready(set)) => set,
        Ok(ArtifactStatus::Skipped(message)) => {
            return Err(SnsGovernanceSetupError::Artifact(message));
        }
        Err(err) => return Err(SnsGovernanceSetupError::Artifact(err)),
    };
    if !pocketic_env::pocketic_available() {
        return Err(SnsGovernanceSetupError::PocketIcMissing);
    }

    let ledger_wasm = artifacts
        .load_required("sns_ledger")
        .map_err(SnsGovernanceSetupError::Artifact)?;
    let governance_wasm = artifacts
        .load_required("sns_governance")
        .map_err(SnsGovernanceSetupError::Artifact)?;
    let pic = pocketic_env::new_sns_pic();
    let sns_subnet = pic.topology().get_sns().expect("SNS subnet should exist");
    let governance = pic.create_canister_on_subnet(None, None, sns_subnet);
    pic.add_cycles(governance, 2_000_000_000_000);
    let controller = Principal::from_slice(&[61; 29]);
    let minting = icrc::account(Principal::from_slice(&[62; 29]), None);
    let user = icrc::account(controller, None);
    let ledger = pocketic_env::create_sns_canister(
        &pic,
        ledger_wasm,
        icrc::ledger_init_arg(
            Principal::anonymous(),
            minting,
            vec![(user.clone(), initial_user_balance_e8s)],
        ),
    );
    pic.install_canister(
        governance,
        governance_wasm,
        governance_init_arg(Some(ledger), Some(Principal::from_slice(&[63; 29]))),
        None,
    );
    for _ in 0..5 {
        pic.tick();
    }
    Ok(GovernanceLedgerFixture {
        pic: std::rc::Rc::new(pic),
        governance,
        ledger,
        controller,
    })
}

fn stake_and_claim_neuron(
    fixture: &GovernanceLedgerFixture,
    stake_e8s: u64,
    memo: u64,
    memo_bytes: &[u8],
) -> Result<NeuronId, GovernanceError> {
    let staking_subaccount = compute_neuron_staking_subaccount(fixture.controller, memo);
    let staking_account = icrc::account(fixture.governance, Some(staking_subaccount));
    let _block = icrc::icrc1_transfer(
        &fixture.pic,
        fixture.ledger,
        fixture.controller,
        icrc::transfer_arg(
            None,
            staking_account,
            stake_e8s,
            Some(FEE_E8S),
            Some(memo_bytes),
            None,
        ),
    )
    .expect("stake transfer should succeed");

    let claim: ManageNeuronResponse = icrc::update_one(
        &fixture.pic,
        fixture.governance,
        fixture.controller,
        "manage_neuron",
        ManageNeuron {
            subaccount: vec![],
            command: Some(Command::ClaimOrRefresh(ClaimOrRefresh {
                by: Some(By::MemoAndController(MemoAndController {
                    controller: Some(fixture.controller),
                    memo,
                })),
            })),
        },
    );
    match claim.command {
        Some(CommandResponse::ClaimOrRefresh(response)) => Ok(response
            .refreshed_neuron_id
            .expect("claim should return a neuron id")),
        Some(CommandResponse::Error(err)) => Err(err),
        other => panic!("unexpected claim response: {other:?}"),
    }
}

fn listed_neuron(fixture: &GovernanceLedgerFixture, neuron_id: &NeuronId) -> SnsNeuronRecord {
    let neurons: ListNeuronsResponse = icrc::query_one(
        &fixture.pic,
        fixture.governance,
        "list_neurons",
        ListNeurons {
            of_principal: Some(fixture.controller),
            limit: 10,
            start_page_at: None,
        },
    );
    neurons
        .neurons
        .into_iter()
        .find(|neuron| neuron.id.as_ref() == Some(neuron_id))
        .expect("claimed neuron should be listed")
}

fn configure_increase_dissolve_delay(
    fixture: &GovernanceLedgerFixture,
    neuron_id: &NeuronId,
    additional_seconds: u32,
) {
    let response: ManageNeuronResponse = icrc::update_one(
        &fixture.pic,
        fixture.governance,
        fixture.controller,
        "manage_neuron",
        ManageNeuron {
            subaccount: neuron_id.id.clone(),
            command: Some(Command::Configure(Configure {
                operation: Some(Operation::IncreaseDissolveDelay(IncreaseDissolveDelay {
                    additional_dissolve_delay_seconds: additional_seconds,
                })),
            })),
        },
    );
    match response.command {
        Some(CommandResponse::Configure(_)) => {}
        Some(CommandResponse::Error(err)) => panic!("configure failed: {err:?}"),
        other => panic!("unexpected configure response: {other:?}"),
    }
}

fn dissolve_delay_seconds(neuron: &SnsNeuronRecord) -> u64 {
    match neuron.dissolve_state {
        Some(DissolveState::DissolveDelaySeconds(seconds)) => seconds,
        Some(DissolveState::WhenDissolvedTimestampSeconds(_)) | None => 0,
    }
}

pub fn governance_init_arg(ledger: Option<Principal>, root: Option<Principal>) -> Vec<u8> {
    candid::encode_one(Governance {
        root_canister_id: root,
        id_to_nervous_system_functions: vec![],
        metrics: None,
        maturity_modulation: None,
        mode: 1,
        parameters: Some(test_nervous_system_parameters()),
        is_finalizing_disburse_maturity: Some(false),
        deployed_version: None,
        cached_upgrade_steps: None,
        sns_initialization_parameters: "direct-governance-smoke".to_string(),
        latest_reward_event: None,
        pending_version: None,
        swap_canister_id: Some(Principal::from_slice(&[22; 29])),
        ledger_canister_id: ledger,
        proposals: vec![],
        in_flight_commands: vec![],
        sns_metadata: Some(ManageSnsMetadata {
            url: Some(format!("{}://example.invalid", "https")),
            logo: None,
            name: Some("IO Test".to_string()),
            description: Some("Direct governance smoke only".to_string()),
        }),
        neurons: vec![],
        genesis_timestamp_seconds: 1,
        target_version: None,
        timers: None,
        upgrade_journal: None,
    })
    .expect("SNS governance init should encode")
}

pub fn test_nervous_system_parameters() -> NervousSystemParameters {
    NervousSystemParameters {
        default_followees: Some(DefaultFollowees { followees: vec![] }),
        max_dissolve_delay_seconds: Some(io_core_model::TWO_WEEK_SECONDS),
        max_dissolve_delay_bonus_percentage: Some(0),
        max_followees_per_function: Some(15),
        neuron_claimer_permissions: Some(NeuronPermissionList {
            permissions: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
        }),
        neuron_minimum_stake_e8s: Some(100_000_000),
        max_neuron_age_for_age_bonus: Some(0),
        initial_voting_period_seconds: Some(86_400),
        neuron_minimum_dissolve_delay_to_vote_seconds: Some(io_core_model::TWO_WEEK_SECONDS - 1),
        reject_cost_e8s: Some(100_000_000),
        max_proposals_to_keep_per_action: Some(100),
        wait_for_quiet_deadline_increase_seconds: Some(1),
        max_number_of_neurons: Some(100_000),
        transaction_fee_e8s: Some(10_000),
        max_number_of_proposals_with_ballots: Some(700),
        max_age_bonus_percentage: Some(0),
        neuron_grantable_permissions: Some(NeuronPermissionList {
            permissions: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
        }),
        voting_rewards_parameters: Some(VotingRewardsParameters {
            final_reward_rate_basis_points: Some(0),
            initial_reward_rate_basis_points: Some(0),
            reward_rate_transition_duration_seconds: Some(1),
            round_duration_seconds: Some(io_core_model::TWO_WEEK_SECONDS),
        }),
        maturity_modulation_disabled: Some(true),
        max_number_of_principals_per_neuron: Some(10),
        automatically_advance_target_version: Some(false),
        custom_proposal_criticality: None,
    }
}

pub fn compute_neuron_staking_subaccount(controller: Principal, nonce: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([0x0c]);
    hasher.update(b"neuron-stake");
    hasher.update(controller.as_slice());
    hasher.update(nonce.to_be_bytes());
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires pinned real SNS governance artifact and POCKET_IC_BIN"]
    fn real_sns_governance_direct_empty_state_lists_no_neurons_or_proposals() {
        install_real_sns_governance_empty_state(true).unwrap();
    }

    #[test]
    #[ignore = "requires pinned real SNS governance/ledger artifacts and POCKET_IC_BIN"]
    fn real_sns_user_stakes_io_normal_path_and_list_neurons_observes_it_direct_governance_path() {
        install_real_sns_governance_and_stake_neuron(true).unwrap();
    }

    #[test]
    #[ignore = "requires pinned real SNS governance/ledger artifacts and POCKET_IC_BIN"]
    fn real_sns_user_topup_increases_existing_neuron_stake_direct_governance_path() {
        install_real_sns_governance_and_topup_neuron(true).unwrap();
    }

    #[test]
    #[ignore = "requires pinned real SNS governance/ledger artifacts and POCKET_IC_BIN"]
    fn real_sns_minimum_stake_is_enforced_direct_governance_path() {
        install_real_sns_governance_and_reject_below_minimum_stake(true).unwrap();
    }

    #[test]
    #[ignore = "requires pinned real SNS governance/ledger artifacts and POCKET_IC_BIN"]
    fn real_sns_dissolve_delay_boundaries_are_visible_direct_governance_path() {
        install_real_sns_governance_and_observe_dissolve_delay_boundaries(true).unwrap();
    }
}
