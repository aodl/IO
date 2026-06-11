use crate::artifacts::{resolve_from_env, ArtifactStatus};
use crate::icrc::{self, FEE_E8S};
use crate::pocketic_env;
use candid::{CandidType, Principal};
use pocket_ic::PocketIc;
use serde::Deserialize;
use sha2::{Digest, Sha256};

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
    pub proposals: Vec<EmptyRecord>,
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
pub enum Operation {
    IncreaseDissolveDelay(IncreaseDissolveDelay),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnsGovernanceSetupError {
    Artifact(String),
    PocketIcMissing,
}

struct GovernanceLedgerFixture {
    pic: PocketIc,
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
    let pic = pocketic_env::new_sns_pic();
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
                max_dissolve_delay_seconds: Some(252_288_000),
                max_dissolve_delay_bonus_percentage: Some(0),
                max_followees_per_function: Some(15),
                neuron_claimer_permissions: Some(NeuronPermissionList {
                    permissions: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
                }),
                neuron_minimum_stake_e8s: Some(100_000_000),
                max_neuron_age_for_age_bonus: Some(0),
                initial_voting_period_seconds: Some(86_400),
                neuron_minimum_dissolve_delay_to_vote_seconds: Some(1_209_600),
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
        pic,
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
        max_dissolve_delay_seconds: Some(252_288_000),
        max_dissolve_delay_bonus_percentage: Some(0),
        max_followees_per_function: Some(15),
        neuron_claimer_permissions: Some(NeuronPermissionList {
            permissions: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
        }),
        neuron_minimum_stake_e8s: Some(100_000_000),
        max_neuron_age_for_age_bonus: Some(0),
        initial_voting_period_seconds: Some(86_400),
        neuron_minimum_dissolve_delay_to_vote_seconds: Some(1_209_600),
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
