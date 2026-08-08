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
    let stakes = [
        200_000_000_u64,
        400_000_000,
        200_000_000,
        500_000_000,
        300_000_000,
        600_000_000,
    ];
    let neuron_ids = stakes
        .iter()
        .enumerate()
        .map(|(index, stake)| {
            let id = stake_and_claim_neuron(
                &fixture,
                *stake,
                u64::try_from(index + 1).unwrap(),
                b"reward-contract",
            )
            .expect("candidate contract neuron claim should succeed");
            if index < 5 {
                configure_increase_dissolve_delay(
                    &fixture,
                    &id,
                    u32::try_from(io_core_model::TWO_WEEK_SECONDS).unwrap(),
                );
            }
            id
        })
        .collect::<Vec<_>>();
    let alice = &neuron_ids[0];
    let bob = &neuron_ids[1];
    let carol = &neuron_ids[2];
    let follower = &neuron_ids[3];
    let non_voter = &neuron_ids[4];
    let ineligible = &neuron_ids[5];

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

    let event_1 = advance_until_reward_event(&fixture, 2, 0);
    assert_eq!(event_1.distributed_e8s_equivalent, 0);
    assert_eq!(event_1.rounds_since_last_distribution, Some(1));
    assert_eq!(event_1.settled_proposals.len(), 2);
    let event_1_end = event_1
        .end_timestamp_seconds
        .expect("candidate reward event must have an end timestamp");
    let listed_1 = list_all_neurons_paged(&fixture, 2);
    assert_eq!(listed_1.len(), 6);
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
    let expected_shares = [
        (alice, 400_000_000_u128),
        (bob, 800_000_000),
        (carol, 200_000_000),
        (follower, 1_000_000_000),
    ];
    for (id, expected) in expected_shares {
        let neuron = find_neuron(&listed_1, id);
        let participation = neuron
            .latest_reward_event_participation
            .expect("direct and followed voters must have event participation");
        assert_eq!(
            participation.reward_event_end_timestamp_seconds,
            event_1_end
        );
        assert_eq!(participation.exact_reward_shares().unwrap(), expected);
        assert_eq!(neuron.maturity_e8s_equivalent, 0);
        assert_eq!(neuron.staked_maturity_e8s_equivalent.unwrap_or(0), 0);
    }
    let alice_shares = find_neuron(&listed_1, alice)
        .latest_reward_event_participation
        .unwrap()
        .exact_reward_shares()
        .unwrap();
    let bob_shares = find_neuron(&listed_1, bob)
        .latest_reward_event_participation
        .unwrap()
        .exact_reward_shares()
        .unwrap();
    let carol_shares = find_neuron(&listed_1, carol)
        .latest_reward_event_participation
        .unwrap()
        .exact_reward_shares()
        .unwrap();
    assert_eq!(bob_shares, alice_shares * 2, "same two-proposal participation with twice the canonical voting power must produce twice the raw shares");
    assert_eq!(
        alice_shares,
        carol_shares * 2,
        "equal stake on two settled proposals versus one must produce twice the raw shares"
    );
    assert!(
        find_neuron(&listed_1, non_voter)
            .latest_reward_event_participation
            .is_none(),
        "non-voter must not be tagged to the event"
    );
    assert!(
        find_neuron(&listed_1, ineligible)
            .latest_reward_event_participation
            .is_none(),
        "the ineligible neuron must not be tagged to the event"
    );
    assert_eq!(
        listed_1
            .iter()
            .filter_map(|neuron| neuron.latest_reward_event_participation)
            .map(|participation| participation.exact_reward_shares().unwrap())
            .sum::<u128>(),
        2_400_000_000,
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

    let proposal_3 = make_motion(&fixture, carol, "candidate replacement event");
    register_vote(&fixture, bob, proposal_3, 2);
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
    assert_eq!(event_2.round, event_1.round + 1);
    assert_eq!(event_2_end, event_1_end + 86_400);
    assert_eq!(event_2.rounds_since_last_distribution, Some(1));
    let listed_2 = list_all_neurons_paged(&fixture, 2);
    assert_eq!(
        find_neuron(&listed_2, bob)
            .latest_reward_event_participation
            .as_ref()
            .unwrap()
            .reward_event_end_timestamp_seconds,
        event_2_end
    );
    assert_eq!(
        find_neuron(&listed_2, carol)
            .latest_reward_event_participation
            .as_ref()
            .unwrap()
            .reward_event_end_timestamp_seconds,
        event_2_end
    );
    for id in [alice, follower] {
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
    assert!(find_neuron(&listed_2, non_voter)
        .latest_reward_event_participation
        .is_none());

    let event_3 = advance_until_reward_event(&fixture, 0, event_2.round);
    assert!(event_3.settled_proposals.is_empty());
    assert_eq!(event_3.distributed_e8s_equivalent, 0);
    assert_eq!(event_3.round, event_2.round + 1);
    assert_eq!(event_3.end_timestamp_seconds.unwrap(), event_2_end + 86_400);
    assert_eq!(event_3.rounds_since_last_distribution, Some(1));
    let listed_3 = list_all_neurons_paged(&fixture, 2);
    for id in &neuron_ids {
        assert_eq!(
            find_neuron(&listed_3, id).latest_reward_event_participation,
            find_neuron(&listed_2, id).latest_reward_event_participation,
            "a no-proposal event must not create or refresh participation fields"
        );
        let neuron = find_neuron(&listed_3, id);
        assert_eq!(neuron.maturity_e8s_equivalent, 0);
        assert_eq!(neuron.staked_maturity_e8s_equivalent.unwrap_or(0), 0);
    }
    assert_eq!(
        find_neuron(&listed_3, alice)
            .latest_reward_event_participation
            .as_ref()
            .unwrap()
            .reward_event_end_timestamp_seconds,
        event_1_end,
        "an old direct-voter tag remains stale on a no-proposal event"
    );
    assert_eq!(
        find_neuron(&listed_3, bob)
            .latest_reward_event_participation
            .as_ref()
            .unwrap()
            .reward_event_end_timestamp_seconds,
        event_2_end,
        "the most recent participant tag remains stale on a no-proposal event"
    );
    assert!(
        find_neuron(&listed_3, non_voter)
            .latest_reward_event_participation
            .is_none(),
        "a neuron that never voted can still have no participation field"
    );

    let mut previous_event = event_3;
    let mut previous_participation = listed_3
        .iter()
        .map(|neuron| {
            (
                neuron.id.as_ref().unwrap().id.clone(),
                neuron.latest_reward_event_participation,
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    for day in 4_u64..=14 {
        let current_participants: Vec<&NeuronId> = match day % 3 {
            1 => {
                let proposal = make_motion(&fixture, alice, &format!("daily event {day}"));
                register_vote(&fixture, bob, proposal, 2);
                vec![alice, bob, follower]
            }
            2 => {
                make_motion(&fixture, carol, &format!("daily event {day}"));
                vec![carol]
            }
            _ => Vec::new(),
        };
        let event = advance_until_reward_event(
            &fixture,
            usize::from(!current_participants.is_empty()),
            previous_event.round,
        );
        assert_eq!(event.round, previous_event.round + 1);
        assert_eq!(event.rounds_since_last_distribution, Some(1));
        assert_eq!(
            event.end_timestamp_seconds.unwrap(),
            previous_event.end_timestamp_seconds.unwrap() + 86_400
        );
        assert_eq!(event.distributed_e8s_equivalent, 0);
        let neurons = list_all_neurons_paged(&fixture, 2);
        let event_end = event.end_timestamp_seconds.unwrap();
        for id in &neuron_ids {
            let neuron = find_neuron(&neurons, id);
            let is_current = current_participants.contains(&id);
            if is_current {
                assert_eq!(
                    neuron
                        .latest_reward_event_participation
                        .as_ref()
                        .map(|participation| participation.reward_event_end_timestamp_seconds),
                    Some(event_end),
                    "daily direct/followed participant must receive the new tag"
                );
            } else {
                assert_eq!(
                    neuron.latest_reward_event_participation, previous_participation[&id.id],
                    "daily inactive neuron must retain its absent or stale tag"
                );
            }
            assert_eq!(neuron.maturity_e8s_equivalent, 0);
            assert_eq!(neuron.staked_maturity_e8s_equivalent.unwrap_or(0), 0);
        }
        previous_participation = neurons
            .into_iter()
            .map(|neuron| {
                (
                    neuron.id.unwrap().id,
                    neuron.latest_reward_event_participation,
                )
            })
            .collect();
        previous_event = event;
    }

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
    fixture
        .pic
        .advance_time(Duration::from_secs(86_400 * 3 + 1));
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
        fixture.pic.advance_time(Duration::from_secs(86_401));
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
        InitArgs, Lifecycle, LiquidReceiptProgress, PrepareLiquidReceiptArgs, ReceiptKind,
        RedeemArgs, RedemptionProgress, RewardBackingProgress, RewardEventClassification,
        RewardEventObservation, Status, StreamConfig, StreamProgress,
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

    let pic = std::rc::Rc::new(pocketic_env::new_pic_with_icp_sns_features());
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
    let auxiliary_settings = CanisterSettings {
        controllers: Some(vec![root]),
        ..Default::default()
    };
    let index = pic.create_canister_on_subnet(None, Some(auxiliary_settings.clone()), sns_subnet);
    pic.add_cycles(index, 2_000_000_000_000);
    let swap = pic.create_canister_on_subnet(None, Some(auxiliary_settings), sns_subnet);
    pic.add_cycles(swap, 2_000_000_000_000);
    let stream = pocketic_env::create_empty_application_canister(&pic);
    let nns_manager = pocketic_env::create_application_canister(&pic, nns_wasm, Vec::new());
    let controller = Principal::from_slice(&[71; 29]);
    let reserve_subaccount = icrc::subaccount("candidate-reward-reserve");
    let liquid_subaccount = icrc::subaccount("candidate-reward-liquid");
    let reserve = icrc::account(stream, Some(reserve_subaccount));
    let io_ledger = pocketic_env::create_sns_canister(
        &pic,
        ledger_wasm,
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
    let icp_ledger = Principal::from_text(crate::nns_setup::install_nns_ledger().canister_id)
        .expect("official ICP ledger ID should parse");
    icrc::icrc1_transfer(
        &pic,
        icp_ledger,
        Principal::anonymous(),
        icrc::transfer_arg(
            None,
            icrc::account(stream, Some(liquid_subaccount)),
            10_000_000_000,
            Some(FEE_E8S),
            Some(b"fund-candidate-liquid-backing"),
            None,
        ),
    )
    .expect("default ICP ledger account funds candidate liquid backing");
    icrc::icrc1_transfer(
        &pic,
        icp_ledger,
        Principal::anonymous(),
        icrc::transfer_arg(
            None,
            icrc::account(nns_manager, Some(maturity_subaccount)),
            2_000_000_000,
            Some(FEE_E8S),
            Some(b"fund-candidate-maturity-source"),
            None,
        ),
    )
    .expect("default ICP ledger account funds the candidate maturity source");
    pic.install_canister(
        root,
        root_wasm,
        encode_one(SnsRootCanister {
            dapp_canister_ids: vec![stream],
            extensions: None,
            testflight: true,
            archive_canister_ids: vec![],
            governance_canister_id: Some(governance),
            index_canister_id: Some(index),
            swap_canister_id: Some(swap),
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
        Some(root),
    );
    for _ in 0..5 {
        pic.tick();
    }

    let fixture = GovernanceLedgerFixture {
        pic: pic.clone(),
        governance,
        ledger: io_ledger,
        controller,
    };
    let stakes = [
        100_000_000_u64,
        200_000_000,
        300_000_000,
        400_000_000,
        500_000_000,
    ];
    let neuron_ids = stakes
        .iter()
        .enumerate()
        .map(|(index, stake)| {
            let id = stake_and_claim_neuron(
                &fixture,
                *stake,
                u64::try_from(index + 1).unwrap(),
                b"io-reward",
            )
            .expect("candidate neuron claim succeeds");
            let delay = if index == 4 {
                io_core_model::TWO_WEEK_SECONDS - 1
            } else {
                io_core_model::TWO_WEEK_SECONDS
            };
            configure_increase_dissolve_delay(&fixture, &id, u32::try_from(delay).unwrap());
            id
        })
        .collect::<Vec<_>>();
    let excluded = StreamAccount {
        owner: governance,
        subaccount: Some(neuron_ids[3].id.clone()),
    };

    let proposal = make_motion(
        &fixture,
        &neuron_ids[0],
        "establish stale participation before no-proposal fallback",
    );
    register_vote(&fixture, &neuron_ids[1], proposal, 2);
    register_vote(&fixture, &neuron_ids[3], proposal, 1);
    let event_1 = advance_until_reward_event(&fixture, 1, 0);
    let event_1_neurons = list_all_neurons_paged(&fixture, 2);
    let event_2 = advance_until_reward_event(&fixture, 0, event_1.round);
    assert!(event_2.settled_proposals.is_empty());
    assert_eq!(event_2.distributed_e8s_equivalent, 0);
    let event_2_neurons = list_all_neurons_paged(&fixture, 2);
    for id in &neuron_ids {
        assert_eq!(
            find_neuron(&event_2_neurons, id).latest_reward_event_participation,
            find_neuron(&event_1_neurons, id).latest_reward_event_participation,
            "no-proposal event must retain every old or absent participation field"
        );
    }

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
                approved_reward_event_duration_seconds: 86_400,
                io_reserve: StreamAccount {
                    owner: stream,
                    subaccount: Some(reserve_subaccount.to_vec()),
                },
                liquid_icp: StreamAccount {
                    owner: stream,
                    subaccount: Some(liquid_subaccount.to_vec()),
                },
                excluded_io_accounts: vec![excluded.clone()],
                minimum_redemption_io_e8s: 20_000,
                expected_io_fee_e8s: FEE_E8S as u128,
                expected_icp_fee_e8s: FEE_E8S as u128,
                maximum_request_lifetime_nanos: 900_000_000_000,
                retry_delay_nanos: 1_000_000_000,
                ledger_deduplication_window_nanos: 86_400_000_000_000,
            },
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

    let observation: Result<RewardEventObservation, ApiError> = decode_one(
        &pic.update_call(
            stream,
            Principal::anonymous(),
            "resume_reward_work",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let observation = observation.expect("stream consumes the no-proposal event");
    assert_eq!(
        observation.classification,
        RewardEventClassification::NoProposalFallback
    );
    assert_eq!(observation.event.round, event_2.round);
    let observed_weights = observation
        .credits
        .iter()
        .map(|weight| (weight.sns_neuron_id.clone(), weight.event_credit))
        .collect::<std::collections::BTreeMap<_, _>>();
    for (id, expected) in neuron_ids[..3].iter().zip(stakes[..3].iter()) {
        assert_eq!(observed_weights[&id.id], u128::from(*expected));
    }
    assert!(!observed_weights.contains_key(&neuron_ids[3].id));

    let backing_step = |expected: RewardBackingProgress| {
        let progress: Result<RewardBackingProgress, ApiError> = decode_one(
            &pic.update_call(
                stream,
                Principal::anonymous(),
                "resume_reward_backing",
                encode_one(()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(progress, Ok(expected));
    };
    backing_step(RewardBackingProgress::BatchFrozen { generation: 1 });
    backing_step(RewardBackingProgress::TargetAccepted { generation: 1 });
    backing_step(RewardBackingProgress::MaturityPrepared { generation: 1 });

    let status: Status = decode_one(
        &pic.query_call(stream, controller, "get_status", encode_one(()).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        status.pending_entitlement_batch_eligible_credit,
        Some(600_000_000)
    );

    let liquid_amount = 1_000_000_000_u64;
    let permit: Result<io_stream_manager::LiquidReceiptPermit, ApiError> = decode_one(
        &pic.update_call(
            stream,
            nns_manager,
            "prepare_liquid_receipt",
            encode_one(PrepareLiquidReceiptArgs {
                receipt_sequence: 0,
                receipt_kind: ReceiptKind::TwoWeekMaturity,
                source_operation_id: b"candidate-no-proposal-1".to_vec(),
                liquid_amount_e8s: liquid_amount as u128,
                entitlement_batch_generation: Some(1),
            })
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let permit = permit.expect("two-week receipt permit is prepared");
    let before = neuron_ids
        .iter()
        .map(|id| {
            icrc::icrc1_balance_of(
                &pic,
                io_ledger,
                icrc::account(governance, Some(id.id.clone().try_into().unwrap())),
            )
        })
        .collect::<Vec<Nat>>();
    let reserve_before = icrc::icrc1_balance_of(&pic, io_ledger, reserve.clone());
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
    let mut settlement_observations = Vec::new();
    for _ in 0..24 {
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
            &progress
        {
            completed = Some(result.clone());
            break;
        }
        let after = neuron_ids[..3]
            .iter()
            .map(|id| {
                icrc::icrc1_balance_of(
                    &pic,
                    io_ledger,
                    icrc::account(governance, Some(id.id.clone().try_into().unwrap())),
                )
            })
            .collect::<Vec<Nat>>();
        settlement_observations.push(format!("{progress:?}; balances={after:?}"));
        if !upgraded_between_recipients
            && after
                .iter()
                .zip(&before[..3])
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
        "stream upgrades after exactly one recipient; observations: {settlement_observations:?}"
    );
    let result = match completed.expect("reward receipt completes") {
        CompletedReceiptResult::TwoWeek(result) => result,
        other => panic!("unexpected receipt result: {other:?}"),
    };
    assert!(result.backed_io_pool_e8s > 0);
    assert_eq!(
        result
            .distributed_io_e8s
            .checked_add(result.rounding_dust_io_e8s),
        Some(result.backed_io_pool_e8s)
    );
    let after = neuron_ids
        .iter()
        .map(|id| {
            icrc::icrc1_balance_of(
                &pic,
                io_ledger,
                icrc::account(governance, Some(id.id.clone().try_into().unwrap())),
            )
        })
        .collect::<Vec<Nat>>();
    let deltas = after[..3]
        .iter()
        .zip(&before[..3])
        .map(|(after, before)| {
            u128::try_from(after.0.clone()).unwrap() - u128::try_from(before.0.clone()).unwrap()
        })
        .collect::<Vec<_>>();
    let expected_deltas = stakes[..3]
        .iter()
        .map(|stake| result.backed_io_pool_e8s * u128::from(*stake) / 600_000_000)
        .collect::<Vec<_>>();
    assert_eq!(deltas, expected_deltas);
    assert!(deltas.iter().all(|amount| *amount > 0));
    assert_eq!(
        deltas.iter().sum::<u128>() + result.rounding_dust_io_e8s,
        result.backed_io_pool_e8s
    );
    assert_eq!(after[3], before[3], "excluded neuron receives nothing");
    assert_eq!(
        after[4], before[4],
        "one-second-short neuron receives nothing"
    );
    let reserve_after = icrc::icrc1_balance_of(&pic, io_ledger, reserve.clone());
    assert_eq!(
        u128::try_from(reserve_before.0).unwrap() - u128::try_from(reserve_after.0).unwrap(),
        result.distributed_io_e8s + 3 * u128::from(FEE_E8S)
    );

    let resumed: Result<(), ApiError> = decode_one(
        &pic.update_call(stream, governance, "set_paused", encode_one(false).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(resumed, Ok(()));

    let zero_share_proposal = make_motion(
        &fixture,
        &neuron_ids[3],
        "excluded-only proposal has zero eligible current-event shares",
    );
    let event_3 = advance_until_reward_event(&fixture, 1, event_2.round);
    assert_eq!(event_3.settled_proposals[0].id, zero_share_proposal);
    let zero_observation: Result<RewardEventObservation, ApiError> = decode_one(
        &pic.update_call(
            stream,
            Principal::anonymous(),
            "resume_reward_work",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    match zero_observation {
        Ok(observation) => {
            assert_eq!(
                observation.classification,
                RewardEventClassification::ZeroEligibleParticipation
            );
            assert!(observation.credits.is_empty());
        }
        Err(ApiError::Pending(message)) if message == "SNS reward event has not advanced" => {
            // The one-shot timer is deliberately allowed to win the race with a
            // permissionless keeper. Prove that it consumed this exact event
            // with no entitlement instead of requiring the keeper call to win.
            let status: Status = decode_one(
                &pic.query_call(stream, controller, "get_status", encode_one(()).unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                status
                    .latest_processed_reward_event
                    .map(|event| event.round),
                Some(event_3.round)
            );
            assert_eq!(
                status.latest_reward_event_classification,
                Some(RewardEventClassification::ZeroEligibleParticipation)
            );
            assert_eq!(status.accumulated_eligible_credit, 0);
        }
        other => panic!("zero-share proposal event is not consumed: {other:?}"),
    }

    backing_step(RewardBackingProgress::BatchFrozen { generation: 2 });
    backing_step(RewardBackingProgress::TargetAccepted { generation: 2 });
    backing_step(RewardBackingProgress::MaturityPrepared { generation: 2 });
    let zero_permit: Result<io_stream_manager::LiquidReceiptPermit, ApiError> = decode_one(
        &pic.update_call(
            stream,
            nns_manager,
            "prepare_liquid_receipt",
            encode_one(PrepareLiquidReceiptArgs {
                receipt_sequence: 1,
                receipt_kind: ReceiptKind::TwoWeekMaturity,
                source_operation_id: b"candidate-zero-share-2".to_vec(),
                liquid_amount_e8s: 500_000_000,
                entitlement_batch_generation: Some(2),
            })
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let zero_permit = zero_permit.expect("zero-share batch receipt permit is prepared");
    let reserve_before_zero = icrc::icrc1_balance_of(&pic, io_ledger, reserve.clone());
    let balances_before_zero = after.clone();
    let zero_block = icrc::icrc1_transfer(
        &pic,
        icp_ledger,
        nns_manager,
        icrc::transfer_arg(
            Some(maturity_subaccount),
            icrc::account(
                zero_permit.destination.owner,
                zero_permit
                    .destination
                    .subaccount
                    .clone()
                    .map(|bytes| bytes.try_into().unwrap()),
            ),
            500_000_000,
            Some(FEE_E8S),
            Some(&zero_permit.memo),
            Some(pic.get_time().as_nanos_since_unix_epoch()),
        ),
    )
    .expect("zero-share batch still receives actual ICP backing");
    let _: Result<LiquidReceiptProgress, ApiError> = decode_one(
        &pic.update_call(
            stream,
            nns_manager,
            "complete_liquid_receipt",
            encode_one(CompleteLiquidReceiptArgs {
                receipt_sequence: 1,
                block_index: u128::try_from(zero_block.0).unwrap(),
            })
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut zero_completed = None;
    for _ in 0..4 {
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
            zero_completed = Some(result);
            break;
        }
    }
    let zero_result = match zero_completed.expect("zero-share batch completes exactly") {
        CompletedReceiptResult::TwoWeek(result) => result,
        other => panic!("unexpected zero-share receipt result: {other:?}"),
    };
    assert!(zero_result.backed_io_pool_e8s > 0);
    assert_eq!(zero_result.distributed_io_e8s, 0);
    assert_eq!(
        zero_result.rounding_dust_io_e8s,
        zero_result.backed_io_pool_e8s
    );
    assert_eq!(
        icrc::icrc1_balance_of(&pic, io_ledger, reserve.clone()),
        reserve_before_zero,
        "zero-share backed IO remains in reserve"
    );
    let balances_after_zero = neuron_ids
        .iter()
        .map(|id| {
            icrc::icrc1_balance_of(
                &pic,
                io_ledger,
                icrc::account(governance, Some(id.id.clone().try_into().unwrap())),
            )
        })
        .collect::<Vec<Nat>>();
    assert_eq!(balances_after_zero, balances_before_zero);

    let set_stream_paused = |paused: bool| {
        let result: Result<(), ApiError> = decode_one(
            &pic.update_call(
                stream,
                governance,
                "set_paused",
                encode_one(paused).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(result, Ok(()));
    };
    let stream_status = || -> Status {
        decode_one(
            &pic.query_call(stream, controller, "get_status", encode_one(()).unwrap())
                .unwrap(),
        )
        .unwrap()
    };
    let entry_map = |status: &Status| {
        status
            .accumulated_entitlements
            .iter()
            .map(|entry| {
                (
                    entry.sns_neuron_id.clone(),
                    entry.accumulated_eligible_credit,
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };

    expect_manage_success(
        &fixture,
        &neuron_ids[2],
        Command::Follow(Follow {
            function_id: 1,
            followees: vec![neuron_ids[0].clone()],
        }),
        "configure installed accumulation follower",
    );
    let mut previous_round = event_3.round;
    let mut expected_live = std::collections::BTreeMap::<Vec<u8>, u128>::new();
    let mut frozen_batch_total = None;
    let mut redemption = None;
    let mut redemption_icp_before = None;
    for day in 4_u64..=15 {
        let (expected_settled, expected_weights, expected_classification) = match day {
            4 => {
                let proposal = make_motion(&fixture, &neuron_ids[0], "installed daily event 4");
                register_vote(&fixture, &neuron_ids[1], proposal, 2);
                (
                    1,
                    vec![(0_usize, stakes[0]), (1, stakes[1]), (2, stakes[2])],
                    RewardEventClassification::ProposalBearing,
                )
            }
            5 | 8 => (
                0,
                vec![(0, stakes[0]), (1, stakes[1]), (2, stakes[2])],
                RewardEventClassification::NoProposalFallback,
            ),
            6 => {
                make_motion(&fixture, &neuron_ids[0], "installed daily event 6");
                (
                    1,
                    vec![(0, stakes[0]), (2, stakes[2])],
                    RewardEventClassification::ProposalBearing,
                )
            }
            7 => {
                let proposal = make_motion(&fixture, &neuron_ids[1], "installed daily event 7");
                register_vote(&fixture, &neuron_ids[0], proposal, 2);
                (
                    1,
                    vec![(0, stakes[0]), (1, stakes[1]), (2, stakes[2])],
                    RewardEventClassification::ProposalBearing,
                )
            }
            9 | 15 => {
                make_motion(
                    &fixture,
                    &neuron_ids[3],
                    &format!("installed excluded-only daily event {day}"),
                );
                (
                    1,
                    Vec::new(),
                    RewardEventClassification::ZeroEligibleParticipation,
                )
            }
            10 => {
                configure_increase_dissolve_delay(&fixture, &neuron_ids[4], 1);
                (
                    0,
                    vec![
                        (0, stakes[0]),
                        (1, stakes[1]),
                        (2, stakes[2]),
                        (4, stakes[4]),
                    ],
                    RewardEventClassification::NoProposalFallback,
                )
            }
            11 => {
                make_motion(&fixture, &neuron_ids[4], "installed daily event 11");
                (
                    1,
                    vec![(4, stakes[4])],
                    RewardEventClassification::ProposalBearing,
                )
            }
            12 => {
                configure_start_dissolving(&fixture, &neuron_ids[1]);
                (
                    0,
                    vec![(0, stakes[0]), (2, stakes[2]), (4, stakes[4])],
                    RewardEventClassification::NoProposalFallback,
                )
            }
            13 => {
                make_motion(&fixture, &neuron_ids[0], "installed daily event 13");
                (
                    1,
                    vec![(0, stakes[0]), (2, stakes[2])],
                    RewardEventClassification::ProposalBearing,
                )
            }
            14 => (
                0,
                vec![(0, stakes[0]), (2, stakes[2]), (4, stakes[4])],
                RewardEventClassification::NoProposalFallback,
            ),
            _ => unreachable!(),
        };
        let event = advance_until_reward_event(&fixture, expected_settled, previous_round);
        assert_eq!(event.round, previous_round + 1);
        assert_eq!(event.rounds_since_last_distribution, Some(1));
        let observation: Result<RewardEventObservation, ApiError> = decode_one(
            &pic.update_call(
                stream,
                Principal::anonymous(),
                "resume_reward_work",
                encode_one(()).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let expected_event_credits = match expected_classification {
            RewardEventClassification::ProposalBearing => {
                let event_end = event
                    .end_timestamp_seconds
                    .expect("proposal-bearing event has an end timestamp");
                let listed = list_all_neurons_paged(&fixture, 2);
                expected_weights
                    .into_iter()
                    .map(|(index, _)| {
                        let participation = find_neuron(&listed, &neuron_ids[index])
                            .latest_reward_event_participation
                            .as_ref()
                            .expect("expected candidate participant has canonical event shares");
                        assert_eq!(
                            participation.reward_event_end_timestamp_seconds, event_end,
                            "proposal-bearing expectation must use only the current event tag"
                        );
                        (
                            neuron_ids[index].id.clone(),
                            participation.exact_reward_shares().unwrap(),
                        )
                    })
                    .collect::<std::collections::BTreeMap<_, _>>()
            }
            RewardEventClassification::NoProposalFallback => {
                let listed = list_all_neurons_paged(&fixture, 2);
                expected_weights
                    .into_iter()
                    .map(|(index, _)| {
                        (
                            neuron_ids[index].id.clone(),
                            u128::from(
                                find_neuron(&listed, &neuron_ids[index]).cached_neuron_stake_e8s,
                            ),
                        )
                    })
                    .collect::<std::collections::BTreeMap<_, _>>()
            }
            RewardEventClassification::ZeroEligibleParticipation => {
                assert!(expected_weights.is_empty());
                std::collections::BTreeMap::new()
            }
            RewardEventClassification::MissedSkipped => {
                unreachable!("daily normal-event loop cannot classify a skip")
            }
        };
        match observation {
            Ok(observation) => {
                assert_eq!(observation.event.round, event.round);
                assert_eq!(observation.classification, expected_classification);
                let actual_event_credits = observation
                    .credits
                    .iter()
                    .map(|weight| (weight.sns_neuron_id.clone(), weight.event_credit))
                    .collect::<std::collections::BTreeMap<_, _>>();
                assert_eq!(
                    actual_event_credits, expected_event_credits,
                    "unexpected canonical candidate event weights on installed day {day}"
                );
            }
            Err(ApiError::Pending(message)) if message == "SNS reward event has not advanced" => {
                // The single one-shot timer is allowed to consume the event before
                // the permissionless keeper. The exact accumulator delta below
                // proves that it consumed this event once with canonical weights.
            }
            other => panic!("installed daily event {day} was not consumed: {other:?}"),
        }
        for (id, weight) in expected_event_credits {
            *expected_live.entry(id).or_default() += weight;
        }
        let status = stream_status();
        assert_eq!(
            status
                .latest_processed_reward_event
                .map(|processed| processed.round),
            Some(event.round)
        );
        assert_eq!(
            status.latest_reward_event_classification,
            Some(expected_classification)
        );
        assert_eq!(
            status.processed_reward_event_count,
            day - 1,
            "stream must consume events 2 through {day} exactly once"
        );
        assert_eq!(entry_map(&status), expected_live);
        assert_eq!(
            status.pending_entitlement_batch_eligible_credit,
            if day > 4 { frozen_batch_total } else { None }
        );

        if day == 4 {
            let total = expected_live.values().copied().sum::<u128>();
            backing_step(RewardBackingProgress::BatchFrozen { generation: 3 });
            backing_step(RewardBackingProgress::TargetAccepted { generation: 3 });
            backing_step(RewardBackingProgress::MaturityPrepared { generation: 3 });
            frozen_batch_total = Some(total);
            expected_live.clear();
            let frozen = stream_status();
            assert_eq!(
                frozen.pending_entitlement_batch_eligible_credit,
                Some(total)
            );
            assert!(frozen.accumulated_entitlements.is_empty());
        }
        if day == 8 {
            let before_upgrade = stream_status();
            pocketic_env::upgrade_canister(
                &pic,
                stream,
                stream_wasm.clone(),
                encode_one(()).unwrap(),
            );
            let after_upgrade = stream_status();
            assert_eq!(after_upgrade.lifecycle, Lifecycle::Paused);
            assert_eq!(
                after_upgrade.accumulated_entitlements,
                before_upgrade.accumulated_entitlements
            );
            assert_eq!(
                after_upgrade.pending_entitlement_batch_eligible_credit,
                before_upgrade.pending_entitlement_batch_eligible_credit
            );
            set_stream_paused(false);
        }
        if day == 10 {
            let amount = 20_000_000_u64;
            let now = pic.get_time().as_nanos_since_unix_epoch();
            icrc::icrc2_approve(
                &pic,
                io_ledger,
                controller,
                icrc::ApproveArgs {
                    from_subaccount: None,
                    spender: icrc::account(stream, None),
                    amount: Nat::from(amount + FEE_E8S),
                    expected_allowance: Some(Nat::from(0_u8)),
                    expires_at: Some(now + 800_000_000_000),
                    fee: Some(Nat::from(FEE_E8S)),
                    memo: Some(b"pending-batch-redemption".to_vec()),
                    created_at_time: Some(now),
                },
            )
            .expect("controller approves redemption while backing is pending");
            let total_supply = u128::try_from(icrc::icrc1_total_supply(&pic, io_ledger).0).unwrap();
            let reserve_balance =
                u128::try_from(icrc::icrc1_balance_of(&pic, io_ledger, reserve.clone()).0).unwrap();
            let excluded_balance = u128::try_from(
                icrc::icrc1_balance_of(
                    &pic,
                    io_ledger,
                    icrc::account(
                        governance,
                        Some(neuron_ids[3].id.clone().try_into().unwrap()),
                    ),
                )
                .0,
            )
            .unwrap();
            let liquid_account = icrc::account(stream, Some(liquid_subaccount));
            let liquid_balance =
                u128::try_from(icrc::icrc1_balance_of(&pic, icp_ledger, liquid_account).0).unwrap();
            let quote = io_core_model::redemption_quote(
                u128::from(amount),
                u128::from(FEE_E8S),
                total_supply,
                reserve_balance,
                excluded_balance,
                liquid_balance,
                u128::from(FEE_E8S),
            )
            .unwrap();
            let args = RedeemArgs {
                from_subaccount: None,
                io_amount_e8s: u128::from(amount),
                min_icp_out_e8s: quote.net_icp_e8s,
                max_io_fee_e8s: u128::from(FEE_E8S),
                max_icp_fee_e8s: u128::from(FEE_E8S),
                expires_at_nanos: now + 800_000_000_000,
                nonce: 0,
            };
            redemption_icp_before = Some(icrc::icrc1_balance_of(
                &pic,
                icp_ledger,
                icrc::account(controller, None),
            ));
            let pulled: Result<RedemptionProgress, ApiError> = decode_one(
                &pic.update_call(stream, controller, "redeem", encode_one(args).unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(pulled, Ok(RedemptionProgress::IoInReserve));
            assert_eq!(
                stream_status().operation_kind.as_deref(),
                Some("Redemption")
            );
            redemption = Some(quote);
        }
        if day == 11 {
            let paid: Result<StreamProgress, ApiError> = decode_one(
                &pic.update_call(
                    stream,
                    Principal::anonymous(),
                    "resume",
                    encode_one(()).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(
                paid,
                Ok(StreamProgress::Redemption(
                    RedemptionProgress::PayoutSucceeded
                ))
            );
        }
        if day == 12 {
            let completed: Result<StreamProgress, ApiError> = decode_one(
                &pic.update_call(
                    stream,
                    Principal::anonymous(),
                    "resume",
                    encode_one(()).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
            let result = match completed {
                Ok(StreamProgress::Redemption(RedemptionProgress::Completed(result))) => result,
                other => panic!("pending-batch redemption did not complete: {other:?}"),
            };
            let quote = redemption.expect("redemption quote was captured");
            assert_eq!(result.gross_icp_e8s, quote.gross_icp_e8s);
            assert_eq!(result.net_icp_e8s, quote.net_icp_e8s);
            assert_eq!(
                icrc::icrc1_balance_of(&pic, icp_ledger, icrc::account(controller, None)),
                redemption_icp_before.as_ref().unwrap().clone() + Nat::from(quote.net_icp_e8s)
            );
            assert_eq!(
                stream_status().pending_entitlement_batch_eligible_credit,
                frozen_batch_total
            );
        }
        previous_round = event.round;
    }
    let after_fourteen = stream_status();
    assert_eq!(after_fourteen.processed_reward_event_count, 14);
    assert_eq!(entry_map(&after_fourteen), expected_live);
    backing_step(RewardBackingProgress::AwaitingReceipt { generation: 3 });

    set_stream_paused(true);
    let missed_one = advance_until_reward_event(&fixture, 0, previous_round);
    let missed_two = advance_until_reward_event(&fixture, 0, missed_one.round);
    set_stream_paused(false);
    let skipped: Result<RewardEventObservation, ApiError> = decode_one(
        &pic.update_call(
            stream,
            Principal::anonymous(),
            "resume_reward_work",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let skipped = skipped.expect("missed candidate events advance through one typed skip");
    assert_eq!(skipped.event.round, missed_two.round);
    assert_eq!(
        skipped.classification,
        RewardEventClassification::MissedSkipped
    );
    assert!(skipped.credits.is_empty());
    let after_skip = stream_status();
    assert_eq!(after_skip.processed_reward_event_count, 14);
    assert_eq!(after_skip.missed_reward_event_count, 2);
    assert_eq!(entry_map(&after_skip), expected_live);
    let replay: Result<RewardEventObservation, ApiError> = decode_one(
        &pic.update_call(
            stream,
            Principal::anonymous(),
            "resume_reward_work",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        replay,
        Err(ApiError::Pending(
            "SNS reward event has not advanced".into()
        ))
    );

    set_stream_paused(true);
    let recovered = advance_until_reward_event(&fixture, 0, missed_two.round);
    set_stream_paused(false);
    let recovered_observation: Result<RewardEventObservation, ApiError> = decode_one(
        &pic.update_call(
            stream,
            Principal::anonymous(),
            "resume_reward_work",
            encode_one(()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let recovered_observation =
        recovered_observation.expect("normal candidate event follows a typed skip");
    assert_eq!(recovered_observation.event.round, recovered.round);
    assert_eq!(
        recovered_observation.classification,
        RewardEventClassification::NoProposalFallback
    );
    let recovered_weights = recovered_observation
        .credits
        .iter()
        .map(|weight| (weight.sns_neuron_id.clone(), weight.event_credit))
        .collect::<std::collections::BTreeMap<_, _>>();
    let recovered_neurons = list_all_neurons_paged(&fixture, 2);
    assert_eq!(
        recovered_weights,
        [0_usize, 2, 4]
            .into_iter()
            .map(|index| {
                (
                    neuron_ids[index].id.clone(),
                    u128::from(
                        find_neuron(&recovered_neurons, &neuron_ids[index]).cached_neuron_stake_e8s,
                    ),
                )
            })
            .collect()
    );
    let recovered_status = stream_status();
    assert_eq!(recovered_status.processed_reward_event_count, 15);
    assert_eq!(recovered_status.missed_reward_event_count, 2);
    assert_eq!(
        recovered_status.pending_entitlement_batch_eligible_credit,
        frozen_batch_total
    );
    for neuron in list_all_neurons_paged(&fixture, 2) {
        assert_eq!(neuron.maturity_e8s_equivalent, 0);
        assert_eq!(neuron.staked_maturity_e8s_equivalent.unwrap_or(0), 0);
    }
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

fn configure_start_dissolving(fixture: &GovernanceLedgerFixture, neuron_id: &NeuronId) {
    expect_manage_success(
        fixture,
        neuron_id,
        Command::Configure(Configure {
            operation: Some(Operation::StartDissolving(EmptyRecord {})),
        }),
        "start dissolving",
    );
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
            round_duration_seconds: Some(86_400),
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
