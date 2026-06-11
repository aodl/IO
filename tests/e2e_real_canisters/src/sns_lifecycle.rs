use crate::nns_setup::NnsSetupError;
use crate::sns_wasm_setup::SnsWasmSetupError;
use candid::{CandidType, Principal};
use serde::Deserialize;
use std::time::Duration;

const SNS_SWAP_LIFECYCLE_OPEN: i32 = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IoTestSnsInitPayloadPlan {
    pub token_name: &'static str,
    pub token_symbol: &'static str,
    pub minimum_participants: u32,
    pub dapp_canisters: Vec<Principal>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnsLifecycleError {
    Nns(NnsSetupError),
    SnsWasm(SnsWasmSetupError),
    CreateServiceNervousSystemDtoMissing,
    DeployRejected(String),
}

pub struct SnsLifecycleFixture {
    pub pic: pocket_ic::PocketIc,
    pub sns_wasm: Principal,
    pub response: DeployNewSnsResponse,
}

impl From<SnsWasmSetupError> for SnsLifecycleError {
    fn from(err: SnsWasmSetupError) -> Self {
        Self::SnsWasm(err)
    }
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct DeployNewSnsRequest {
    pub sns_init_payload: Option<SnsInitPayload>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct DeployNewSnsResponse {
    pub dapp_canisters_transfer_result: Option<DappCanistersTransferResult>,
    pub subnet_id: Option<Principal>,
    pub error: Option<SnsWasmError>,
    pub canisters: Option<SnsCanisterIds>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct GetLifecycleResponse {
    pub decentralization_sale_open_timestamp_seconds: Option<u64>,
    pub lifecycle: Option<i32>,
    pub decentralization_swap_termination_timestamp_seconds: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct GetSaleParametersResponse {
    pub params: Option<SwapParams>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SwapParams {
    pub min_participant_icp_e8s: u64,
    pub neuron_basket_construction_parameters: Option<NeuronBasketConstructionParameters>,
    pub max_icp_e8s: u64,
    pub swap_due_timestamp_seconds: u64,
    pub min_participants: u32,
    pub sns_token_e8s: u64,
    pub sale_delay_seconds: Option<u64>,
    pub max_participant_icp_e8s: u64,
    pub min_direct_participation_icp_e8s: Option<u64>,
    pub min_icp_e8s: u64,
    pub max_direct_participation_icp_e8s: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SnsWasmError {
    pub message: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct DappCanistersTransferResult {
    pub restored_dapp_canisters: Vec<Canister>,
    pub nns_controlled_dapp_canisters: Vec<Canister>,
    pub sns_controlled_dapp_canisters: Vec<Canister>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SnsCanisterIds {
    pub root: Option<Principal>,
    pub swap: Option<Principal>,
    pub ledger: Option<Principal>,
    pub index: Option<Principal>,
    pub governance: Option<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Canister {
    pub id: Option<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct DappCanisters {
    pub canisters: Vec<Canister>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Countries {
    pub iso_codes: Vec<String>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NeuronBasketConstructionParameters {
    pub dissolve_delay_interval_seconds: u64,
    pub count: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct CustomProposalCriticality {
    pub additional_critical_native_action_ids: Vec<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum InitialTokenDistribution {
    FractionalDeveloperVotingPower(FractionalDeveloperVotingPower),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct FractionalDeveloperVotingPower {
    pub treasury_distribution: Option<TreasuryDistribution>,
    pub developer_distribution: Option<DeveloperDistribution>,
    pub swap_distribution: Option<SwapDistribution>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct TreasuryDistribution {
    pub total_e8s: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct DeveloperDistribution {
    pub developer_neurons: Vec<NeuronDistribution>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SwapDistribution {
    pub total_e8s: u64,
    pub initial_swap_amount_e8s: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NeuronDistribution {
    pub controller: Option<Principal>,
    pub dissolve_delay_seconds: u64,
    pub memo: u64,
    pub stake_e8s: u64,
    pub vesting_period_seconds: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NeuronsFundParticipationConstraints {
    pub coefficient_intervals: Vec<LinearScalingCoefficient>,
    pub max_neurons_fund_participation_icp_e8s: Option<u64>,
    pub min_direct_participation_threshold_icp_e8s: Option<u64>,
    pub ideal_matched_participation_function: Option<IdealMatchedParticipationFunction>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct LinearScalingCoefficient {
    pub slope_numerator: Option<u64>,
    pub intercept_icp_e8s: Option<u64>,
    pub from_direct_participation_icp_e8s: Option<u64>,
    pub slope_denominator: Option<u64>,
    pub to_direct_participation_icp_e8s: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct IdealMatchedParticipationFunction {
    pub serialized_representation: Option<String>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SnsInitPayload {
    pub url: Option<String>,
    pub max_dissolve_delay_seconds: Option<u64>,
    pub max_dissolve_delay_bonus_percentage: Option<u64>,
    pub nns_proposal_id: Option<u64>,
    pub neurons_fund_participation: Option<bool>,
    pub min_participant_icp_e8s: Option<u64>,
    pub neuron_basket_construction_parameters: Option<NeuronBasketConstructionParameters>,
    pub fallback_controller_principal_ids: Vec<String>,
    pub token_symbol: Option<String>,
    pub final_reward_rate_basis_points: Option<u64>,
    pub max_icp_e8s: Option<u64>,
    pub neuron_minimum_stake_e8s: Option<u64>,
    pub confirmation_text: Option<String>,
    pub logo: Option<String>,
    pub name: Option<String>,
    pub swap_start_timestamp_seconds: Option<u64>,
    pub swap_due_timestamp_seconds: Option<u64>,
    pub initial_voting_period_seconds: Option<u64>,
    pub neuron_minimum_dissolve_delay_to_vote_seconds: Option<u64>,
    pub description: Option<String>,
    pub max_neuron_age_seconds_for_age_bonus: Option<u64>,
    pub min_participants: Option<u64>,
    pub initial_reward_rate_basis_points: Option<u64>,
    pub wait_for_quiet_deadline_increase_seconds: Option<u64>,
    pub transaction_fee_e8s: Option<u64>,
    pub dapp_canisters: Option<DappCanisters>,
    pub neurons_fund_participation_constraints: Option<NeuronsFundParticipationConstraints>,
    pub max_age_bonus_percentage: Option<u64>,
    pub initial_token_distribution: Option<InitialTokenDistribution>,
    pub reward_rate_transition_duration_seconds: Option<u64>,
    pub token_logo: Option<String>,
    pub token_name: Option<String>,
    pub max_participant_icp_e8s: Option<u64>,
    pub min_direct_participation_icp_e8s: Option<u64>,
    pub proposal_reject_cost_e8s: Option<u64>,
    pub restricted_countries: Option<Countries>,
    pub min_icp_e8s: Option<u64>,
    pub max_direct_participation_icp_e8s: Option<u64>,
    pub custom_proposal_criticality: Option<CustomProposalCriticality>,
}

pub fn build_io_test_sns_init_payload(
    dapp_canisters: Vec<Principal>,
) -> Result<IoTestSnsInitPayloadPlan, SnsLifecycleError> {
    if dapp_canisters.is_empty() {
        return Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing);
    }
    Ok(IoTestSnsInitPayloadPlan {
        token_name: "Internet Olympiad Test",
        token_symbol: "IOT",
        minimum_participants: 1,
        dapp_canisters,
    })
}

pub fn deploy_io_test_sns_through_sns_w() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn deploy_io_test_sns_through_sns_w_for_test(
    required: bool,
) -> Result<DeployNewSnsResponse, SnsLifecycleError> {
    Ok(deploy_io_test_sns_lifecycle_fixture_for_test(required)?.response)
}

pub fn deploy_io_test_sns_lifecycle_fixture_for_test(
    required: bool,
) -> Result<SnsLifecycleFixture, SnsLifecycleError> {
    let fixture =
        crate::sns_wasm_setup::publish_all_sns_wasms_via_nns_proposal_fixture_for_test(required)?;
    fixture
        .pic
        .add_cycles(fixture.sns_wasm, 1_000_000_000_000_000);
    let sns_subnet = fixture
        .pic
        .topology()
        .get_sns()
        .expect("SNS subnet should exist");
    let now_seconds = fixture.pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000;
    let payload = build_io_test_create_service_nervous_system(now_seconds);
    let response: DeployNewSnsResponse = crate::icrc::update_one(
        &fixture.pic,
        fixture.sns_wasm,
        fixture.nns_governance,
        "deploy_new_sns",
        DeployNewSnsRequest {
            sns_init_payload: Some(payload),
        },
    );
    if let Some(err) = &response.error {
        return Err(SnsLifecycleError::DeployRejected(err.message.clone()));
    }
    if response.subnet_id != Some(sns_subnet) {
        return Err(SnsLifecycleError::DeployRejected(format!(
            "expected SNS subnet {sns_subnet}, got {:?}",
            response.subnet_id
        )));
    }
    if response.canisters.is_none() {
        return Err(SnsLifecycleError::DeployRejected(
            "deploy_new_sns returned no SNS canister IDs".to_string(),
        ));
    }
    let deployed: crate::sns_wasm_setup::ListDeployedSnsesResponse = crate::icrc::query_one(
        &fixture.pic,
        fixture.sns_wasm,
        "list_deployed_snses",
        crate::nns_setup::EmptyRecord {},
    );
    if deployed.instances.is_empty() {
        return Err(SnsLifecycleError::DeployRejected(
            "SNS-W list_deployed_snses returned no instances after deploy".to_string(),
        ));
    }
    Ok(SnsLifecycleFixture {
        pic: fixture.pic,
        sns_wasm: fixture.sns_wasm,
        response,
    })
}

pub fn await_swap_open_for_test(
    fixture: &SnsLifecycleFixture,
) -> Result<GetLifecycleResponse, SnsLifecycleError> {
    let swap = fixture
        .response
        .canisters
        .as_ref()
        .and_then(|ids| ids.swap)
        .ok_or_else(|| SnsLifecycleError::DeployRejected("missing swap canister id".to_string()))?;
    for _ in 0..10 {
        fixture.pic.tick();
        let lifecycle: GetLifecycleResponse = crate::icrc::query_one(
            &fixture.pic,
            swap,
            "get_lifecycle",
            crate::nns_setup::EmptyRecord {},
        );
        if lifecycle.lifecycle == Some(SNS_SWAP_LIFECYCLE_OPEN) {
            return Ok(lifecycle);
        }
        fixture.pic.advance_time(Duration::from_secs(1));
    }
    let lifecycle: GetLifecycleResponse = crate::icrc::query_one(
        &fixture.pic,
        swap,
        "get_lifecycle",
        crate::nns_setup::EmptyRecord {},
    );
    Err(SnsLifecycleError::DeployRejected(format!(
        "swap did not open; lifecycle={:?}",
        lifecycle.lifecycle
    )))
}

pub fn read_swap_sale_parameters_for_test(
    fixture: &SnsLifecycleFixture,
) -> Result<SwapParams, SnsLifecycleError> {
    let swap = fixture
        .response
        .canisters
        .as_ref()
        .and_then(|ids| ids.swap)
        .ok_or_else(|| SnsLifecycleError::DeployRejected("missing swap canister id".to_string()))?;
    let response: GetSaleParametersResponse = crate::icrc::query_one(
        &fixture.pic,
        swap,
        "get_sale_parameters",
        crate::nns_setup::EmptyRecord {},
    );
    response.params.ok_or_else(|| {
        SnsLifecycleError::DeployRejected("swap returned no sale parameters".to_string())
    })
}

pub fn build_io_test_create_service_nervous_system(now_seconds: u64) -> SnsInitPayload {
    const E8S: u64 = 100_000_000;
    const YEAR_SECONDS: u64 = 365 * 24 * 60 * 60;
    SnsInitPayload {
        url: Some(format!("{}://example.invalid/io-local-sns", "https")),
        max_dissolve_delay_seconds: Some(8 * YEAR_SECONDS),
        max_dissolve_delay_bonus_percentage: Some(0),
        nns_proposal_id: Some(1),
        neurons_fund_participation: Some(false),
        min_participant_icp_e8s: Some(E8S),
        neuron_basket_construction_parameters: Some(NeuronBasketConstructionParameters {
            dissolve_delay_interval_seconds: 1,
            count: 2,
        }),
        fallback_controller_principal_ids: vec![Principal::anonymous().to_text()],
        token_symbol: Some("IOT".to_string()),
        final_reward_rate_basis_points: Some(0),
        max_icp_e8s: None,
        neuron_minimum_stake_e8s: Some(E8S),
        confirmation_text: None,
        logo: Some("data:image/png;base64,".to_string()),
        name: Some("Internet Olympiad Test".to_string()),
        swap_start_timestamp_seconds: Some(now_seconds.saturating_add(1)),
        swap_due_timestamp_seconds: Some(now_seconds.saturating_add(3_600)),
        initial_voting_period_seconds: Some(86_401),
        neuron_minimum_dissolve_delay_to_vote_seconds: Some(0),
        description: Some("Local-only IO SNS lifecycle test.".to_string()),
        max_neuron_age_seconds_for_age_bonus: Some(0),
        min_participants: Some(1),
        initial_reward_rate_basis_points: Some(0),
        wait_for_quiet_deadline_increase_seconds: Some(1),
        transaction_fee_e8s: Some(10_000),
        dapp_canisters: Some(DappCanisters { canisters: vec![] }),
        neurons_fund_participation_constraints: None,
        max_age_bonus_percentage: Some(0),
        initial_token_distribution: Some(InitialTokenDistribution::FractionalDeveloperVotingPower(
            FractionalDeveloperVotingPower {
                treasury_distribution: Some(TreasuryDistribution {
                    total_e8s: 1_000_000 * E8S,
                }),
                developer_distribution: Some(DeveloperDistribution {
                    developer_neurons: vec![NeuronDistribution {
                        controller: Some(Principal::anonymous()),
                        dissolve_delay_seconds: YEAR_SECONDS,
                        memo: 10_001,
                        stake_e8s: 1_000 * E8S,
                        vesting_period_seconds: Some(0),
                    }],
                }),
                swap_distribution: Some(SwapDistribution {
                    total_e8s: 10_000 * E8S,
                    initial_swap_amount_e8s: 10_000 * E8S,
                }),
            },
        )),
        reward_rate_transition_duration_seconds: Some(0),
        token_logo: Some("data:image/png;base64,".to_string()),
        token_name: Some("Internet Olympiad Test".to_string()),
        max_participant_icp_e8s: Some(10 * E8S),
        min_direct_participation_icp_e8s: Some(E8S),
        proposal_reject_cost_e8s: Some(100 * E8S),
        restricted_countries: None,
        min_icp_e8s: None,
        max_direct_participation_icp_e8s: Some(10 * E8S),
        custom_proposal_criticality: Some(CustomProposalCriticality {
            additional_critical_native_action_ids: vec![],
        }),
    }
}

pub fn await_swap_open() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn participate_in_swap() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn refresh_buyer_tokens() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn await_swap_committed() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn finalize_swap() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn await_sns_finalized() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn discover_deployed_sns_canister_ids() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn read_sns_canister_ids() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn list_sns_neurons() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_sns_swap_opens_with_expected_parameters_is_blocked_on_sns_init_dto() {
        assert_eq!(
            await_swap_open(),
            Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
        );
    }

    #[test]
    fn real_sns_lifecycle_deploys_sns_via_sns_w_is_blocked_on_sns_init_dto() {
        assert_eq!(
            deploy_io_test_sns_through_sns_w(),
            Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_lifecycle_deploys_sns_via_sns_w() {
        let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(true).unwrap();
        let response = fixture.response;
        let canisters = response
            .canisters
            .expect("SNS canister IDs should be present");
        assert!(canisters.root.is_some());
        assert!(canisters.governance.is_some());
        assert!(canisters.ledger.is_some());
        assert!(canisters.index.is_some());
        assert!(canisters.swap.is_some());
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_swap_opens_with_expected_parameters() {
        let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(true).unwrap();
        let lifecycle = await_swap_open_for_test(&fixture).unwrap();
        assert_eq!(lifecycle.lifecycle, Some(SNS_SWAP_LIFECYCLE_OPEN));
        assert!(lifecycle
            .decentralization_sale_open_timestamp_seconds
            .is_some());

        let params = read_swap_sale_parameters_for_test(&fixture).unwrap();
        assert_eq!(params.min_participants, 1);
        assert_eq!(params.min_participant_icp_e8s, 100_000_000);
        assert_eq!(params.min_direct_participation_icp_e8s, Some(100_000_000));
        assert_eq!(
            params.neuron_basket_construction_parameters,
            Some(NeuronBasketConstructionParameters {
                dissolve_delay_interval_seconds: 1,
                count: 2,
            })
        );
    }

    #[test]
    fn real_sns_finalized_swap_creates_direct_participation_neurons_is_blocked() {
        assert_eq!(
            list_sns_neurons(),
            Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
        );
    }
}
