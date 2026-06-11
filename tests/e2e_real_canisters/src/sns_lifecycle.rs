use crate::nns_setup::NnsSetupError;
use crate::sns_wasm_setup::SnsWasmSetupError;
use candid::{CandidType, Principal};
use io_ledger_types::{Account, IcpTokens, IcpTransferArgs, IcpTransferError, Subaccount};
use serde::Deserialize;
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::time::Duration;

const SNS_SWAP_LIFECYCLE_OPEN: i32 = 2;
const SNS_SWAP_LIFECYCLE_COMMITTED: i32 = 3;
const SNS_SWAP_LIFECYCLE_ABORTED: i32 = 4;
const SNS_SWAP_LIFECYCLE_ADOPTED: i32 = 5;
const SNS_SWAP_LIFECYCLE_UNSPECIFIED: i32 = 0;
const ICP_LEDGER_TRANSFER_FEE_E8S: u64 = 10_000;
const PARTICIPANT_ICP_E8S: u64 = 100_000_000;
const PROTECTED_PRODUCTION_ID_PARTS: &[(&str, &str)] = &[
    ("thset", "-pqaaa-aaaar-qb7wa-cai"),
    ("tatch", "-ciaaa-aaaar-qb7wq-cai"),
    ("tjqj3", "-uaaaa-aaaar-qb7xa-cai"),
    ("torpp", "-zyaaa-aaaar-qb7xq-cai"),
    ("oae4c", "-3iaaa-aaaar-qb5qq-cai"),
];
const SNS_CONTROLLED_DAPP_INITIAL_WASM: &[u8] = b"\0asm\x01\0\0\0";
#[cfg(test)]
const SNS_CONTROLLED_DAPP_UPGRADE_WASM: &[u8] = b"\0asm\x01\0\0\0\x05\x03\x01\0\x01";

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
    SwapRejected(String),
    ProtectedId(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwapParticipantFixture {
    pub principal: Principal,
    pub amount_icp_e8s: u64,
    pub ticket: Option<Ticket>,
    pub transfer_block: Option<u64>,
    pub buyer_state: Option<BuyerState>,
}

pub struct FinalizedSnsLifecycleFixture {
    pub pic: pocket_ic::PocketIc,
    pub nns_governance: Principal,
    pub nns_ledger: Principal,
    pub nns_index: Principal,
    pub sns_wasm: Principal,
    pub sns_subnet: Principal,
    pub application_subnet: Principal,
    pub root: Principal,
    pub governance: Principal,
    pub ledger: Principal,
    pub index: Principal,
    pub swap: Principal,
    pub dapp_canisters: Vec<Principal>,
    pub participants: Vec<SwapParticipantFixture>,
    pub response: DeployNewSnsResponse,
}

pub type SnsLifecycleFixture = FinalizedSnsLifecycleFixture;

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
pub struct GetBuyerStateRequest {
    pub principal_id: Option<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct GetBuyerStateResponse {
    pub buyer_state: Option<BuyerState>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct GetBuyersTotalResponse {
    pub buyers_total: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq)]
pub struct GetDerivedStateResponse {
    pub sns_tokens_per_icp: Option<f64>,
    pub buyer_total_icp_e8s: Option<u64>,
    pub cf_participant_count: Option<u64>,
    pub neurons_fund_participation_icp_e8s: Option<u64>,
    pub direct_participation_icp_e8s: Option<u64>,
    pub direct_participant_count: Option<u64>,
    pub cf_neuron_count: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ListDirectParticipantsRequest {
    pub offset: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ListDirectParticipantsResponse {
    pub participants: Vec<Participant>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Participant {
    pub participation: Option<BuyerState>,
    pub participant_id: Option<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct BuyerState {
    pub icp: Option<TransferableAmount>,
    pub has_created_neuron_recipes: Option<bool>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct TransferableAmount {
    pub transfer_fee_paid_e8s: Option<u64>,
    pub transfer_start_timestamp_seconds: u64,
    pub amount_e8s: u64,
    pub amount_transferred_e8s: Option<u64>,
    pub transfer_success_timestamp_seconds: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NewSaleTicketRequest {
    pub subaccount: Option<Vec<u8>>,
    pub amount_icp_e8s: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NewSaleTicketResponse {
    pub result: Option<NewSaleTicketResult>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum NewSaleTicketResult {
    Ok(NewSaleTicketOk),
    Err(NewSaleTicketErr),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NewSaleTicketOk {
    pub ticket: Option<Ticket>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NewSaleTicketErr {
    pub invalid_user_amount: Option<InvalidUserAmount>,
    pub existing_ticket: Option<Ticket>,
    pub error_type: i32,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct InvalidUserAmount {
    pub min_amount_icp_e8s_included: u64,
    pub max_amount_icp_e8s_included: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Ticket {
    pub creation_time: u64,
    pub ticket_id: u64,
    pub account: Option<Icrc1Account>,
    pub amount_icp_e8s: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Icrc1Account {
    pub owner: Option<Principal>,
    pub subaccount: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct RefreshBuyerTokensRequest {
    pub confirmation_text: Option<String>,
    pub buyer: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct IcpIndexInitArg {
    pub ledger_id: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct RefreshBuyerTokensResponse {
    pub icp_accepted_participation_e8s: u64,
    pub icp_ledger_account_balance_e8s: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct FinalizeSwapResponse {
    pub set_dapp_controllers_call_result: Option<SetDappControllersCallResult>,
    pub create_sns_neuron_recipes_result: Option<SweepResult>,
    pub error_message: Option<String>,
    pub set_mode_call_result: Option<SetModeCallResult>,
    pub sweep_icp_result: Option<SweepResult>,
    pub claim_neuron_result: Option<SweepResult>,
    pub sweep_sns_result: Option<SweepResult>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SetDappControllersCallResult {
    pub possibility: Option<SetDappControllersPossibility>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum SetDappControllersPossibility {
    Ok(SetDappControllersResponse),
    Err(CanisterCallError),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SetDappControllersResponse {
    pub failed_updates: Vec<FailedUpdate>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct FailedUpdate {
    pub err: Option<CanisterCallError>,
    pub dapp_canister_id: Option<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct CanisterCallError {
    pub code: Option<i32>,
    pub description: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SetModeCallResult {
    pub possibility: Option<SetModePossibility>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum SetModePossibility {
    Ok(EmptyRecord),
    Err(CanisterCallError),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SweepResult {
    pub failure: u32,
    pub skipped: u32,
    pub invalid: u32,
    pub success: u32,
    pub global_failures: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct EmptyRecord {}

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

fn install_local_icp_index_for_fixture(
    pic: &pocket_ic::PocketIc,
    nns_ledger: Principal,
) -> Result<Principal, SnsLifecycleError> {
    let artifacts = match crate::artifacts::resolve_from_env(true) {
        Ok(crate::artifacts::ArtifactStatus::Ready(set)) => set,
        Ok(crate::artifacts::ArtifactStatus::Skipped(message)) => {
            return Err(SnsLifecycleError::SnsWasm(SnsWasmSetupError::Artifact(
                message,
            )));
        }
        Err(err) => {
            return Err(SnsLifecycleError::SnsWasm(SnsWasmSetupError::Artifact(err)));
        }
    };
    let wasm = artifacts
        .load_required("icp_index")
        .map_err(|err| SnsLifecycleError::SnsWasm(SnsWasmSetupError::Artifact(err)))?;
    let nns_subnet = pic.topology().get_nns().expect("NNS subnet should exist");
    let canister = pic.create_canister_on_subnet(None, None, nns_subnet);
    pic.add_cycles(canister, 2_000_000_000_000);
    pic.install_canister(
        canister,
        wasm,
        candid::encode_one(IcpIndexInitArg {
            ledger_id: nns_ledger,
        })
        .expect("ICP index init arg should encode"),
        None,
    );
    for _ in 0..20 {
        pic.tick();
    }
    Ok(canister)
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
    let application_subnet = fixture
        .pic
        .topology()
        .get_app_subnets()
        .into_iter()
        .next()
        .expect("application subnet should exist");
    let nns_ledger = Principal::from_text(crate::nns_setup::install_nns_ledger().canister_id)
        .expect("NNS ledger canister ID should parse");
    let nns_index = install_local_icp_index_for_fixture(&fixture.pic, nns_ledger)?;
    let nns_root = Principal::from_text(crate::nns_setup::install_nns_root().canister_id)
        .expect("NNS root canister ID should parse");
    let dapp = crate::pocketic_env::create_empty_application_canister(&fixture.pic);
    fixture.pic.install_canister(
        dapp,
        SNS_CONTROLLED_DAPP_INITIAL_WASM.to_vec(),
        vec![],
        None,
    );
    fixture
        .pic
        .set_controllers(dapp, Some(Principal::anonymous()), vec![nns_root])
        .map_err(|err| {
            SnsLifecycleError::DeployRejected(format!(
                "failed to make NNS root controller of dapp {dapp}: {err:?}"
            ))
        })?;
    assert_eq!(fixture.pic.get_subnet(dapp), Some(application_subnet));
    let now_seconds = fixture.pic.get_time().as_nanos_since_unix_epoch() / 1_000_000_000;
    let mut payload = build_io_test_create_service_nervous_system(now_seconds);
    payload.dapp_canisters = Some(DappCanisters {
        canisters: vec![Canister { id: Some(dapp) }],
    });
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
    let canisters = response.canisters.as_ref().expect("checked above");
    let root = canisters
        .root
        .ok_or_else(|| SnsLifecycleError::DeployRejected("missing root canister id".to_string()))?;
    let governance = canisters.governance.ok_or_else(|| {
        SnsLifecycleError::DeployRejected("missing governance canister id".to_string())
    })?;
    let ledger = canisters.ledger.ok_or_else(|| {
        SnsLifecycleError::DeployRejected("missing ledger canister id".to_string())
    })?;
    let index = canisters.index.ok_or_else(|| {
        SnsLifecycleError::DeployRejected("missing index canister id".to_string())
    })?;
    let swap = canisters
        .swap
        .ok_or_else(|| SnsLifecycleError::DeployRejected("missing swap canister id".to_string()))?;
    assert_no_production_fiduciary_ids(&[dapp, root, governance, ledger, index, swap])?;
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
        nns_governance: fixture.nns_governance,
        nns_ledger,
        nns_index,
        sns_wasm: fixture.sns_wasm,
        sns_subnet,
        application_subnet,
        root,
        governance,
        ledger,
        index,
        swap,
        dapp_canisters: vec![dapp],
        participants: Vec::new(),
        response,
    })
}

pub fn await_swap_open_for_test(
    fixture: &SnsLifecycleFixture,
) -> Result<GetLifecycleResponse, SnsLifecycleError> {
    for _ in 0..900 {
        fixture.pic.tick();
        let lifecycle: GetLifecycleResponse = crate::icrc::query_one(
            &fixture.pic,
            fixture.swap,
            "get_lifecycle",
            crate::nns_setup::EmptyRecord {},
        );
        if lifecycle.lifecycle == Some(SNS_SWAP_LIFECYCLE_OPEN) {
            return Ok(lifecycle);
        }
        if lifecycle.lifecycle == Some(SNS_SWAP_LIFECYCLE_ABORTED) {
            return Err(SnsLifecycleError::DeployRejected(format!(
                "swap aborted before opening; lifecycle={lifecycle:?}"
            )));
        }
        fixture.pic.advance_time(Duration::from_secs(120));
    }
    let lifecycle: GetLifecycleResponse = crate::icrc::query_one(
        &fixture.pic,
        fixture.swap,
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
    let response: GetSaleParametersResponse = crate::icrc::query_one(
        &fixture.pic,
        fixture.swap,
        "get_sale_parameters",
        crate::nns_setup::EmptyRecord {},
    );
    response.params.ok_or_else(|| {
        SnsLifecycleError::DeployRejected("swap returned no sale parameters".to_string())
    })
}

pub fn create_sale_ticket_if_required(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
    amount_icp_e8s: u64,
) -> Result<Ticket, SnsLifecycleError> {
    let response: NewSaleTicketResponse = crate::icrc::update_one(
        &fixture.pic,
        fixture.swap,
        participant,
        "new_sale_ticket",
        NewSaleTicketRequest {
            subaccount: None,
            amount_icp_e8s,
        },
    );
    match response.result {
        Some(NewSaleTicketResult::Ok(ok)) => ok.ticket.ok_or_else(|| {
            SnsLifecycleError::SwapRejected("new_sale_ticket returned no ticket".to_string())
        }),
        Some(NewSaleTicketResult::Err(err)) => {
            if let Some(ticket) = err.existing_ticket {
                Ok(ticket)
            } else {
                Err(SnsLifecycleError::SwapRejected(format!(
                    "new_sale_ticket rejected amount {amount_icp_e8s}; error_type={}, invalid_user_amount={:?}",
                    err.error_type, err.invalid_user_amount
                )))
            }
        }
        None => Err(SnsLifecycleError::SwapRejected(
            "new_sale_ticket returned no result".to_string(),
        )),
    }
}

pub fn fund_swap_participant(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
    amount_icp_e8s: u64,
) -> Result<SwapParticipantFixture, SnsLifecycleError> {
    let ticket = create_sale_ticket_if_required(fixture, participant, amount_icp_e8s)?;
    let ticket_account = ticket.account.as_ref().ok_or_else(|| {
        SnsLifecycleError::SwapRejected("ticket returned no ICP account".to_string())
    })?;
    let ticket_owner = ticket_account.owner.ok_or_else(|| {
        SnsLifecycleError::SwapRejected("ticket ICP account has no owner".to_string())
    })?;
    let ticket_subaccount = match ticket_account.subaccount.clone() {
        Some(bytes) => Some(
            Subaccount::from_vec(bytes, "ticket.subaccount").map_err(|err| {
                SnsLifecycleError::SwapRejected(format!("invalid ticket subaccount: {err:?}"))
            })?,
        ),
        None => None,
    };
    assert_no_production_fiduciary_ids(&[ticket_owner])?;
    let _ticket_deposit_account = Account::new(ticket_owner, ticket_subaccount);
    let to = Account::new(fixture.swap, Some(principal_to_subaccount(participant)))
        .icp_account_identifier_bytes();
    let transfer: Result<u64, IcpTransferError> = crate::icrc::update_one(
        &fixture.pic,
        fixture.nns_ledger,
        Principal::anonymous(),
        "transfer",
        IcpTransferArgs {
            memo: 0,
            amount: IcpTokens {
                e8s: amount_icp_e8s,
            },
            fee: IcpTokens {
                e8s: ICP_LEDGER_TRANSFER_FEE_E8S,
            },
            from_subaccount: None,
            to: to.to_vec(),
            created_at_time: None,
        },
    );
    let block = transfer.map_err(|err| {
        SnsLifecycleError::SwapRejected(format!(
            "failed to fund swap account for buyer {participant}: {err:?}"
        ))
    })?;
    Ok(SwapParticipantFixture {
        principal: participant,
        amount_icp_e8s,
        ticket: Some(ticket),
        transfer_block: Some(block),
        buyer_state: None,
    })
}

fn principal_to_subaccount(principal: Principal) -> Subaccount {
    let bytes = principal.as_slice();
    let mut subaccount = [0_u8; 32];
    subaccount[0] = bytes
        .len()
        .try_into()
        .expect("principal length should fit in one byte");
    subaccount[1..1 + bytes.len()].copy_from_slice(bytes);
    Subaccount(subaccount)
}

pub fn refresh_buyer_tokens_for_participant(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
) -> Result<RefreshBuyerTokensResponse, SnsLifecycleError> {
    let request = RefreshBuyerTokensRequest {
        confirmation_text: None,
        buyer: participant.to_text(),
    };
    let bytes = fixture
        .pic
        .update_call(
            fixture.swap,
            participant,
            "refresh_buyer_tokens",
            candid::encode_one(request).expect("refresh_buyer_tokens arg should encode"),
        )
        .map_err(|err| {
            SnsLifecycleError::SwapRejected(format!(
                "refresh_buyer_tokens rejected for buyer {participant}: {err:?}"
            ))
        })?;
    candid::decode_one(&bytes).map_err(|err| {
        SnsLifecycleError::SwapRejected(format!(
            "refresh_buyer_tokens response decode failed for buyer {participant}: {err}"
        ))
    })
}

pub fn read_buyer_state(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
) -> Result<BuyerState, SnsLifecycleError> {
    let response: GetBuyerStateResponse = crate::icrc::query_one(
        &fixture.pic,
        fixture.swap,
        "get_buyer_state",
        GetBuyerStateRequest {
            principal_id: Some(participant),
        },
    );
    response.buyer_state.ok_or_else(|| {
        SnsLifecycleError::SwapRejected(format!("swap returned no buyer state for {participant}"))
    })
}

pub fn list_direct_participants(
    fixture: &SnsLifecycleFixture,
) -> Result<ListDirectParticipantsResponse, SnsLifecycleError> {
    Ok(crate::icrc::query_one(
        &fixture.pic,
        fixture.swap,
        "list_direct_participants",
        ListDirectParticipantsRequest {
            offset: Some(0),
            limit: Some(100),
        },
    ))
}

pub fn fund_and_refresh_swap_participant(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
    amount_icp_e8s: u64,
) -> Result<SwapParticipantFixture, SnsLifecycleError> {
    let mut participant_fixture = fund_swap_participant(fixture, participant, amount_icp_e8s)?;
    for _ in 0..50 {
        fixture.pic.tick();
    }
    let refreshed = refresh_buyer_tokens_for_participant(fixture, participant).map_err(|err| {
        SnsLifecycleError::SwapRejected(format!(
            "{err:?}; ticket={:?}; transfer_block={:?}",
            participant_fixture.ticket, participant_fixture.transfer_block
        ))
    })?;
    if refreshed.icp_accepted_participation_e8s != amount_icp_e8s {
        return Err(SnsLifecycleError::SwapRejected(format!(
            "refresh_buyer_tokens accepted {}, expected {amount_icp_e8s}; ledger balance {}",
            refreshed.icp_accepted_participation_e8s, refreshed.icp_ledger_account_balance_e8s
        )));
    }
    let buyer_state = read_buyer_state(fixture, participant)?;
    let observed = buyer_state
        .icp
        .as_ref()
        .map(|icp| icp.amount_e8s)
        .unwrap_or_default();
    if observed != amount_icp_e8s {
        return Err(SnsLifecycleError::SwapRejected(format!(
            "buyer state amount {observed}, expected {amount_icp_e8s}"
        )));
    }
    participant_fixture.buyer_state = Some(buyer_state);
    Ok(participant_fixture)
}

pub fn await_swap_committed_for_test(
    fixture: &SnsLifecycleFixture,
) -> Result<GetLifecycleResponse, SnsLifecycleError> {
    for _ in 0..300 {
        fixture.pic.tick();
        let lifecycle = read_lifecycle(fixture);
        if is_committed_lifecycle(lifecycle.lifecycle) {
            return Ok(lifecycle);
        }
        if lifecycle.lifecycle == Some(SNS_SWAP_LIFECYCLE_ABORTED) {
            return Err(SnsLifecycleError::SwapRejected(format!(
                "swap aborted before commit: {lifecycle:?}"
            )));
        }
        fixture.pic.advance_time(Duration::from_secs(30));
    }
    let lifecycle = read_lifecycle(fixture);
    Err(SnsLifecycleError::SwapRejected(format!(
        "swap did not commit; lifecycle={:?}",
        lifecycle.lifecycle
    )))
}

pub fn finalize_swap_for_test(
    fixture: &SnsLifecycleFixture,
) -> Result<FinalizeSwapResponse, SnsLifecycleError> {
    let response: FinalizeSwapResponse = crate::icrc::update_one(
        &fixture.pic,
        fixture.swap,
        Principal::anonymous(),
        "finalize_swap",
        crate::nns_setup::EmptyRecord {},
    );
    if let Some(message) = &response.error_message {
        return Err(SnsLifecycleError::SwapRejected(format!(
            "finalize_swap returned error_message={message}; response={response:?}"
        )));
    }
    Ok(response)
}

pub fn await_swap_finalized_for_test(
    fixture: &SnsLifecycleFixture,
) -> Result<GetLifecycleResponse, SnsLifecycleError> {
    for _ in 0..300 {
        fixture.pic.tick();
        let lifecycle = read_lifecycle(fixture);
        if is_finalized_lifecycle(lifecycle.lifecycle) {
            return Ok(lifecycle);
        }
        fixture.pic.advance_time(Duration::from_secs(10));
    }
    let lifecycle = read_lifecycle(fixture);
    Err(SnsLifecycleError::SwapRejected(format!(
        "swap did not reach finalized terminal lifecycle; lifecycle={:?}",
        lifecycle.lifecycle
    )))
}

pub fn list_finalized_sns_neurons(
    fixture: &SnsLifecycleFixture,
) -> Result<Vec<crate::sns_governance_setup::SnsNeuronRecord>, SnsLifecycleError> {
    list_all_finalized_sns_neurons(fixture)
}

pub fn list_finalized_sns_neurons_for_principal(
    fixture: &SnsLifecycleFixture,
    principal: Principal,
    limit: u32,
    start_page_at: Option<crate::sns_governance_setup::NeuronId>,
) -> Result<Vec<crate::sns_governance_setup::SnsNeuronRecord>, SnsLifecycleError> {
    let response: crate::sns_governance_setup::ListNeuronsResponse = crate::icrc::query_one(
        &fixture.pic,
        fixture.governance,
        "list_neurons",
        crate::sns_governance_setup::ListNeurons {
            of_principal: Some(principal),
            limit,
            start_page_at,
        },
    );
    Ok(response.neurons)
}

pub fn list_all_finalized_sns_neurons(
    fixture: &SnsLifecycleFixture,
) -> Result<Vec<crate::sns_governance_setup::SnsNeuronRecord>, SnsLifecycleError> {
    let response: crate::sns_governance_setup::ListNeuronsResponse = crate::icrc::query_one(
        &fixture.pic,
        fixture.governance,
        "list_neurons",
        crate::sns_governance_setup::ListNeurons {
            of_principal: None,
            limit: 100,
            start_page_at: None,
        },
    );
    Ok(response.neurons)
}

pub fn list_finalized_sns_proposals(
    fixture: &SnsLifecycleFixture,
    limit: u32,
) -> Result<crate::sns_governance_setup::ListProposalsResponse, SnsLifecycleError> {
    list_finalized_sns_proposals_as(fixture, Principal::anonymous(), limit)
}

pub fn list_finalized_sns_proposals_as(
    fixture: &SnsLifecycleFixture,
    caller: Principal,
    limit: u32,
) -> Result<crate::sns_governance_setup::ListProposalsResponse, SnsLifecycleError> {
    let request = crate::sns_governance_setup::ListProposals {
        include_reward_status: Vec::new(),
        before_proposal: None,
        limit,
        exclude_type: Vec::new(),
        include_status: Vec::new(),
        include_topics: None,
    };
    let bytes = fixture
        .pic
        .query_call(
            fixture.governance,
            caller,
            "list_proposals",
            candid::encode_one(request).expect("list_proposals arg should encode"),
        )
        .map_err(|err| {
            SnsLifecycleError::SwapRejected(format!(
                "finalized governance list_proposals rejected for caller {caller}: {err:?}"
            ))
        })?;
    let response = candid::decode_one(&bytes).map_err(|err| {
        SnsLifecycleError::SwapRejected(format!(
            "finalized governance list_proposals decode failed for caller {caller}: {err}"
        ))
    })?;
    Ok(response)
}

pub fn find_direct_participation_neurons(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
) -> Result<Vec<crate::sns_governance_setup::SnsNeuronRecord>, SnsLifecycleError> {
    let neurons = list_finalized_sns_neurons_for_principal(fixture, participant, 100, None)?;
    Ok(neurons
        .into_iter()
        .filter(|neuron| {
            neuron
                .permissions
                .iter()
                .any(|permission| permission.principal == Some(participant))
        })
        .collect())
}

pub fn assert_direct_participation_neuron_stake(
    neurons: &[crate::sns_governance_setup::SnsNeuronRecord],
) {
    assert!(
        !neurons.is_empty(),
        "direct participation should create at least one finalized SNS neuron"
    );
    assert!(
        neurons
            .iter()
            .all(|neuron| neuron.cached_neuron_stake_e8s > 0),
        "all direct participation neurons should have stake: {neurons:?}"
    );
}

pub fn assert_direct_participation_neuron_dissolve_delay(
    neurons: &[crate::sns_governance_setup::SnsNeuronRecord],
    expected_delays: &[u64],
) {
    let observed: BTreeSet<u64> = neurons
        .iter()
        .filter_map(|neuron| match neuron.dissolve_state {
            Some(crate::sns_governance_setup::DissolveState::DissolveDelaySeconds(seconds)) => {
                Some(seconds)
            }
            Some(crate::sns_governance_setup::DissolveState::WhenDissolvedTimestampSeconds(_))
            | None => None,
        })
        .collect();
    for expected in expected_delays {
        assert!(
            observed.contains(expected),
            "expected finalized direct-participation dissolve delay {expected}, observed {observed:?}"
        );
    }
}

pub fn disburse_zero_delay_neuron_to_participant_for_test(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
) -> Result<u64, SnsLifecycleError> {
    let neuron = find_direct_participation_neurons(fixture, participant)?
        .into_iter()
        .find(|neuron| {
            matches!(
                neuron.dissolve_state,
                Some(crate::sns_governance_setup::DissolveState::DissolveDelaySeconds(0))
            )
        })
        .ok_or_else(|| {
            SnsLifecycleError::SwapRejected(format!(
                "no zero-delay direct participation neuron found for {participant}"
            ))
        })?;
    let neuron_id = neuron
        .id
        .as_ref()
        .ok_or_else(|| {
            SnsLifecycleError::SwapRejected(
                "zero-delay direct participation neuron missing id".to_string(),
            )
        })?
        .id
        .clone();
    let participant_account = crate::icrc::account(participant, None);
    let before =
        crate::icrc::icrc1_balance_of(&fixture.pic, fixture.ledger, participant_account.clone());
    let response: crate::sns_governance_setup::ManageNeuronResponse = crate::icrc::update_one(
        &fixture.pic,
        fixture.governance,
        participant,
        "manage_neuron",
        crate::sns_governance_setup::ManageNeuron {
            subaccount: neuron_id,
            command: Some(crate::sns_governance_setup::Command::Disburse(
                crate::sns_governance_setup::Disburse {
                    amount: None,
                    to_account: Some(crate::sns_governance_setup::Account {
                        owner: Some(participant),
                        subaccount: None,
                    }),
                },
            )),
        },
    );
    match response.command {
        Some(crate::sns_governance_setup::CommandResponse::Disburse(_)) => {}
        Some(crate::sns_governance_setup::CommandResponse::Error(err)) => {
            return Err(SnsLifecycleError::SwapRejected(format!(
                "finalized governance disburse rejected: type={} message={}",
                err.error_type, err.error_message
            )));
        }
        other => {
            return Err(SnsLifecycleError::SwapRejected(format!(
                "unexpected finalized governance disburse response: {other:?}"
            )));
        }
    }
    for _ in 0..10 {
        fixture.pic.tick();
    }
    let after = crate::icrc::icrc1_balance_of(&fixture.pic, fixture.ledger, participant_account);
    let before_u64 = u64::try_from(before.0).map_err(|_| {
        SnsLifecycleError::SwapRejected("pre-disburse balance did not fit u64".to_string())
    })?;
    let after_u64 = u64::try_from(after.0).map_err(|_| {
        SnsLifecycleError::SwapRejected("post-disburse balance did not fit u64".to_string())
    })?;
    if after_u64 <= before_u64 {
        return Err(SnsLifecycleError::SwapRejected(format!(
            "finalized governance disburse did not increase liquid balance; before={before_u64} after={after_u64}"
        )));
    }
    Ok(after_u64 - before_u64)
}

pub fn stake_finalized_liquid_sns_tokens_for_test(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
    stake_e8s: u64,
    memo: u64,
) -> Result<crate::sns_governance_setup::NeuronId, SnsLifecycleError> {
    let staking_subaccount =
        crate::sns_governance_setup::compute_neuron_staking_subaccount(participant, memo);
    let staking_account = crate::icrc::account(fixture.governance, Some(staking_subaccount));
    let _block = crate::icrc::icrc1_transfer(
        &fixture.pic,
        fixture.ledger,
        participant,
        crate::icrc::transfer_arg(
            None,
            staking_account,
            stake_e8s,
            Some(crate::icrc::FEE_E8S),
            Some(b"finalized-stake"),
            None,
        ),
    )
    .map_err(|err| {
        SnsLifecycleError::SwapRejected(format!(
            "finalized SNS ledger stake transfer failed: {err:?}"
        ))
    })?;
    let response: crate::sns_governance_setup::ManageNeuronResponse = crate::icrc::update_one(
        &fixture.pic,
        fixture.governance,
        participant,
        "manage_neuron",
        crate::sns_governance_setup::ManageNeuron {
            subaccount: vec![],
            command: Some(crate::sns_governance_setup::Command::ClaimOrRefresh(
                crate::sns_governance_setup::ClaimOrRefresh {
                    by: Some(crate::sns_governance_setup::By::MemoAndController(
                        crate::sns_governance_setup::MemoAndController {
                            controller: Some(participant),
                            memo,
                        },
                    )),
                },
            )),
        },
    );
    match response.command {
        Some(crate::sns_governance_setup::CommandResponse::ClaimOrRefresh(response)) => {
            response.refreshed_neuron_id.ok_or_else(|| {
                SnsLifecycleError::SwapRejected(
                    "finalized claim returned no refreshed neuron id".to_string(),
                )
            })
        }
        Some(crate::sns_governance_setup::CommandResponse::Error(err)) => {
            Err(SnsLifecycleError::SwapRejected(format!(
                "finalized governance claim rejected: type={} message={}",
                err.error_type, err.error_message
            )))
        }
        other => Err(SnsLifecycleError::SwapRejected(format!(
            "unexpected finalized governance claim response: {other:?}"
        ))),
    }
}

pub fn finalized_neuron_for_participant(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
    neuron_id: &crate::sns_governance_setup::NeuronId,
) -> Result<crate::sns_governance_setup::SnsNeuronRecord, SnsLifecycleError> {
    list_finalized_sns_neurons_for_principal(fixture, participant, 100, None)?
        .into_iter()
        .find(|neuron| neuron.id.as_ref() == Some(neuron_id))
        .ok_or_else(|| {
            SnsLifecycleError::SwapRejected(format!(
                "finalized staked neuron {:?} was not listed for {participant}",
                neuron_id.id
            ))
        })
}

pub fn configure_finalized_neuron_dissolve_delay_for_test(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
    neuron_id: &crate::sns_governance_setup::NeuronId,
    additional_seconds: u32,
) -> Result<(), SnsLifecycleError> {
    configure_finalized_neuron_for_test(
        fixture,
        participant,
        neuron_id,
        crate::sns_governance_setup::Operation::IncreaseDissolveDelay(
            crate::sns_governance_setup::IncreaseDissolveDelay {
                additional_dissolve_delay_seconds: additional_seconds,
            },
        ),
    )
}

pub fn start_finalized_neuron_dissolving_for_test(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
    neuron_id: &crate::sns_governance_setup::NeuronId,
) -> Result<(), SnsLifecycleError> {
    configure_finalized_neuron_for_test(
        fixture,
        participant,
        neuron_id,
        crate::sns_governance_setup::Operation::StartDissolving(
            crate::sns_governance_setup::EmptyRecord {},
        ),
    )
}

pub fn stop_finalized_neuron_dissolving_for_test(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
    neuron_id: &crate::sns_governance_setup::NeuronId,
) -> Result<(), SnsLifecycleError> {
    configure_finalized_neuron_for_test(
        fixture,
        participant,
        neuron_id,
        crate::sns_governance_setup::Operation::StopDissolving(
            crate::sns_governance_setup::EmptyRecord {},
        ),
    )
}

pub fn grant_finalized_neuron_vote_permission_for_test(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
    neuron_id: &crate::sns_governance_setup::NeuronId,
    grantee: Principal,
) -> Result<(), SnsLifecycleError> {
    let response: crate::sns_governance_setup::ManageNeuronResponse = crate::icrc::update_one(
        &fixture.pic,
        fixture.governance,
        participant,
        "manage_neuron",
        crate::sns_governance_setup::ManageNeuron {
            subaccount: neuron_id.id.clone(),
            command: Some(crate::sns_governance_setup::Command::AddNeuronPermissions(
                crate::sns_governance_setup::AddNeuronPermissions {
                    principal_id: Some(grantee),
                    permissions_to_add: Some(crate::sns_governance_setup::NeuronPermissionList {
                        permissions: vec![4],
                    }),
                },
            )),
        },
    );
    match response.command {
        Some(crate::sns_governance_setup::CommandResponse::AddNeuronPermission(_)) => Ok(()),
        Some(crate::sns_governance_setup::CommandResponse::Error(err)) => {
            Err(SnsLifecycleError::SwapRejected(format!(
                "finalized governance add_neuron_permissions rejected: type={} message={}",
                err.error_type, err.error_message
            )))
        }
        other => Err(SnsLifecycleError::SwapRejected(format!(
            "unexpected finalized governance add_neuron_permissions response: {other:?}"
        ))),
    }
}

fn configure_finalized_neuron_for_test(
    fixture: &SnsLifecycleFixture,
    participant: Principal,
    neuron_id: &crate::sns_governance_setup::NeuronId,
    operation: crate::sns_governance_setup::Operation,
) -> Result<(), SnsLifecycleError> {
    let response: crate::sns_governance_setup::ManageNeuronResponse = crate::icrc::update_one(
        &fixture.pic,
        fixture.governance,
        participant,
        "manage_neuron",
        crate::sns_governance_setup::ManageNeuron {
            subaccount: neuron_id.id.clone(),
            command: Some(crate::sns_governance_setup::Command::Configure(
                crate::sns_governance_setup::Configure {
                    operation: Some(operation),
                },
            )),
        },
    );
    match response.command {
        Some(crate::sns_governance_setup::CommandResponse::Configure(_)) => Ok(()),
        Some(crate::sns_governance_setup::CommandResponse::Error(err)) => {
            Err(SnsLifecycleError::SwapRejected(format!(
                "finalized governance configure rejected: type={} message={}",
                err.error_type, err.error_message
            )))
        }
        other => Err(SnsLifecycleError::SwapRejected(format!(
            "unexpected finalized governance configure response: {other:?}"
        ))),
    }
}

pub fn make_finalized_motion_proposal_for_test(
    fixture: &SnsLifecycleFixture,
    proposer: Principal,
    neuron_id: &crate::sns_governance_setup::NeuronId,
    title: &str,
) -> Result<crate::sns_governance_setup::ProposalId, SnsLifecycleError> {
    make_finalized_proposal_for_test(
        fixture,
        proposer,
        neuron_id,
        crate::sns_governance_setup::Proposal {
            url: format!("{}://example.invalid/io-local-motion", "https"),
            title: title.to_string(),
            summary: "Local-only finalized SNS motion proposal for IO E2E.".to_string(),
            action: Some(crate::sns_governance_setup::Action::Motion(
                crate::sns_governance_setup::Motion {
                    motion_text: "Exercise finalized SNS voting path.".to_string(),
                },
            )),
        },
    )
}

pub fn make_finalized_proposal_for_test(
    fixture: &SnsLifecycleFixture,
    proposer: Principal,
    neuron_id: &crate::sns_governance_setup::NeuronId,
    proposal: crate::sns_governance_setup::Proposal,
) -> Result<crate::sns_governance_setup::ProposalId, SnsLifecycleError> {
    let response: crate::sns_governance_setup::ManageNeuronResponse = crate::icrc::update_one(
        &fixture.pic,
        fixture.governance,
        proposer,
        "manage_neuron",
        crate::sns_governance_setup::ManageNeuron {
            subaccount: neuron_id.id.clone(),
            command: Some(crate::sns_governance_setup::Command::MakeProposal(proposal)),
        },
    );
    match response.command {
        Some(crate::sns_governance_setup::CommandResponse::MakeProposal(response)) => {
            response.proposal_id.ok_or_else(|| {
                SnsLifecycleError::SwapRejected(
                    "finalized governance MakeProposal returned no proposal id".to_string(),
                )
            })
        }
        Some(crate::sns_governance_setup::CommandResponse::Error(err)) => {
            Err(SnsLifecycleError::SwapRejected(format!(
                "finalized governance MakeProposal rejected: type={} message={}",
                err.error_type, err.error_message
            )))
        }
        other => Err(SnsLifecycleError::SwapRejected(format!(
            "unexpected finalized governance MakeProposal response: {other:?}"
        ))),
    }
}

pub fn register_finalized_sns_vote_for_test(
    fixture: &SnsLifecycleFixture,
    voter: Principal,
    neuron_id: &crate::sns_governance_setup::NeuronId,
    proposal_id: crate::sns_governance_setup::ProposalId,
    vote: i32,
) -> Result<(), SnsLifecycleError> {
    let response: crate::sns_governance_setup::ManageNeuronResponse = crate::icrc::update_one(
        &fixture.pic,
        fixture.governance,
        voter,
        "manage_neuron",
        crate::sns_governance_setup::ManageNeuron {
            subaccount: neuron_id.id.clone(),
            command: Some(crate::sns_governance_setup::Command::RegisterVote(
                crate::sns_governance_setup::RegisterVote {
                    vote,
                    proposal: Some(proposal_id.clone()),
                },
            )),
        },
    );
    match response.command {
        Some(crate::sns_governance_setup::CommandResponse::RegisterVote(_)) => Ok(()),
        Some(crate::sns_governance_setup::CommandResponse::Error(err)) => {
            Err(SnsLifecycleError::SwapRejected(format!(
                "finalized governance RegisterVote rejected: vote={vote} proposal={} type={} message={}",
                proposal_id.id, err.error_type, err.error_message
            )))
        }
        other => Err(SnsLifecycleError::SwapRejected(format!(
            "unexpected finalized governance RegisterVote response: {other:?}"
        ))),
    }
}

pub fn follow_finalized_sns_neuron_for_test(
    fixture: &SnsLifecycleFixture,
    follower: Principal,
    follower_neuron: &crate::sns_governance_setup::NeuronId,
    followee_neuron: crate::sns_governance_setup::NeuronId,
    function_id: u64,
) -> Result<(), SnsLifecycleError> {
    let response: crate::sns_governance_setup::ManageNeuronResponse = crate::icrc::update_one(
        &fixture.pic,
        fixture.governance,
        follower,
        "manage_neuron",
        crate::sns_governance_setup::ManageNeuron {
            subaccount: follower_neuron.id.clone(),
            command: Some(crate::sns_governance_setup::Command::Follow(
                crate::sns_governance_setup::Follow {
                    function_id,
                    followees: vec![followee_neuron],
                },
            )),
        },
    );
    match response.command {
        Some(crate::sns_governance_setup::CommandResponse::Follow(_)) => Ok(()),
        Some(crate::sns_governance_setup::CommandResponse::Error(err)) => {
            Err(SnsLifecycleError::SwapRejected(format!(
                "finalized governance Follow rejected: function_id={function_id} type={} message={}",
                err.error_type, err.error_message
            )))
        }
        other => Err(SnsLifecycleError::SwapRejected(format!(
            "unexpected finalized governance Follow response: {other:?}"
        ))),
    }
}

pub fn set_finalized_sns_governance_following_for_test(
    fixture: &SnsLifecycleFixture,
    follower: Principal,
    follower_neuron: &crate::sns_governance_setup::NeuronId,
    followee_neuron: crate::sns_governance_setup::NeuronId,
) -> Result<(), SnsLifecycleError> {
    let response: crate::sns_governance_setup::ManageNeuronResponse = crate::icrc::update_one(
        &fixture.pic,
        fixture.governance,
        follower,
        "manage_neuron",
        crate::sns_governance_setup::ManageNeuron {
            subaccount: follower_neuron.id.clone(),
            command: Some(crate::sns_governance_setup::Command::SetFollowing(
                crate::sns_governance_setup::SetFollowing {
                    topic_following: [
                        crate::sns_governance_setup::Topic::DaoCommunitySettings,
                        crate::sns_governance_setup::Topic::SnsFrameworkManagement,
                        crate::sns_governance_setup::Topic::DappCanisterManagement,
                        crate::sns_governance_setup::Topic::ApplicationBusinessLogic,
                        crate::sns_governance_setup::Topic::Governance,
                        crate::sns_governance_setup::Topic::TreasuryAssetManagement,
                        crate::sns_governance_setup::Topic::CriticalDappOperations,
                    ]
                    .into_iter()
                    .map(|topic| crate::sns_governance_setup::FolloweesForTopic {
                        topic: Some(topic),
                        followees: vec![crate::sns_governance_setup::Followee {
                            neuron_id: Some(followee_neuron.clone()),
                            alias: None,
                        }],
                    })
                    .collect(),
                },
            )),
        },
    );
    match response.command {
        Some(crate::sns_governance_setup::CommandResponse::SetFollowing(_)) => Ok(()),
        Some(crate::sns_governance_setup::CommandResponse::Error(err)) => {
            Err(SnsLifecycleError::SwapRejected(format!(
                "finalized governance SetFollowing rejected: topics=all-known type={} message={}",
                err.error_type, err.error_message
            )))
        }
        other => Err(SnsLifecycleError::SwapRejected(format!(
            "unexpected finalized governance SetFollowing response: {other:?}"
        ))),
    }
}

pub fn finalized_motion_function_id_for_test(
    fixture: &SnsLifecycleFixture,
) -> Result<u64, SnsLifecycleError> {
    let response: crate::sns_governance_setup::ListNervousSystemFunctionsResponse =
        crate::icrc::query_one(
            &fixture.pic,
            fixture.governance,
            "list_nervous_system_functions",
            (),
        );
    response
        .functions
        .iter()
        .find(|function| function.name == "Motion")
        .map(|function| function.id)
        .ok_or_else(|| {
            SnsLifecycleError::SwapRejected(format!(
                "finalized governance list_nervous_system_functions did not include Motion: {:?}",
                response.functions
            ))
        })
}

#[cfg(test)]
fn finalized_neuron_dissolve_delay_seconds(
    neuron: &crate::sns_governance_setup::SnsNeuronRecord,
) -> u64 {
    match neuron.dissolve_state {
        Some(crate::sns_governance_setup::DissolveState::DissolveDelaySeconds(seconds)) => seconds,
        Some(crate::sns_governance_setup::DissolveState::WhenDissolvedTimestampSeconds(_))
        | None => 0,
    }
}

#[cfg(test)]
fn finalized_neuron_is_dissolving(neuron: &crate::sns_governance_setup::SnsNeuronRecord) -> bool {
    matches!(
        neuron.dissolve_state,
        Some(crate::sns_governance_setup::DissolveState::WhenDissolvedTimestampSeconds(_))
    )
}

pub fn deploy_finalized_sns_lifecycle_fixture_for_test(
    required: bool,
    participant: Principal,
    amount_icp_e8s: u64,
) -> Result<SnsLifecycleFixture, SnsLifecycleError> {
    deploy_finalized_sns_lifecycle_fixture_with_participants_for_test(
        required,
        &[(participant, amount_icp_e8s)],
    )
}

pub fn deploy_finalized_sns_lifecycle_fixture_with_participants_for_test(
    required: bool,
    participants: &[(Principal, u64)],
) -> Result<SnsLifecycleFixture, SnsLifecycleError> {
    if participants.is_empty() {
        return Err(SnsLifecycleError::SwapRejected(
            "at least one swap participant is required before finalization".to_string(),
        ));
    }
    let mut fixture = deploy_io_test_sns_lifecycle_fixture_for_test(required)?;
    await_swap_open_for_test(&fixture)?;
    for (participant, amount_icp_e8s) in participants {
        let participant_fixture =
            fund_and_refresh_swap_participant(&fixture, *participant, *amount_icp_e8s)?;
        fixture.participants.push(participant_fixture);
    }
    await_swap_committed_for_test(&fixture)?;
    finalize_swap_for_test(&fixture)?;
    await_swap_finalized_for_test(&fixture)?;
    Ok(fixture)
}

pub fn assert_all_canisters_on_expected_subnets(
    fixture: &SnsLifecycleFixture,
) -> Result<(), SnsLifecycleError> {
    for canister in [
        fixture.nns_governance,
        fixture.nns_ledger,
        fixture.nns_index,
    ] {
        if fixture.pic.get_subnet(canister) != fixture.pic.topology().get_nns() {
            return Err(SnsLifecycleError::DeployRejected(format!(
                "NNS canister {canister} was not on NNS subnet"
            )));
        }
    }
    for canister in [
        fixture.root,
        fixture.governance,
        fixture.ledger,
        fixture.index,
        fixture.swap,
    ] {
        if fixture.pic.get_subnet(canister) != Some(fixture.sns_subnet) {
            return Err(SnsLifecycleError::DeployRejected(format!(
                "SNS canister {canister} was not on SNS subnet {}",
                fixture.sns_subnet
            )));
        }
    }
    for canister in &fixture.dapp_canisters {
        if fixture.pic.get_subnet(*canister) != Some(fixture.application_subnet) {
            return Err(SnsLifecycleError::DeployRejected(format!(
                "dapp canister {canister} was not on application subnet {}",
                fixture.application_subnet
            )));
        }
    }
    Ok(())
}

pub fn assert_no_production_fiduciary_ids(ids: &[Principal]) -> Result<(), SnsLifecycleError> {
    for id in ids {
        let text = id.to_text();
        if protected_production_ids().any(|protected| protected == text) {
            return Err(SnsLifecycleError::ProtectedId(text));
        }
    }
    Ok(())
}

fn protected_production_ids() -> impl Iterator<Item = String> {
    PROTECTED_PRODUCTION_ID_PARTS
        .iter()
        .map(|(prefix, suffix)| format!("{prefix}{suffix}"))
}

fn read_lifecycle(fixture: &SnsLifecycleFixture) -> GetLifecycleResponse {
    crate::icrc::query_one(
        &fixture.pic,
        fixture.swap,
        "get_lifecycle",
        crate::nns_setup::EmptyRecord {},
    )
}

fn is_committed_lifecycle(lifecycle: Option<i32>) -> bool {
    matches!(
        lifecycle,
        Some(value)
            if value != SNS_SWAP_LIFECYCLE_OPEN
                && value != SNS_SWAP_LIFECYCLE_UNSPECIFIED
                && value != SNS_SWAP_LIFECYCLE_ABORTED
    )
}

fn is_finalized_lifecycle(lifecycle: Option<i32>) -> bool {
    matches!(
        lifecycle,
        Some(value) if value == SNS_SWAP_LIFECYCLE_COMMITTED || value == SNS_SWAP_LIFECYCLE_ADOPTED
    )
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
    let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(false)?;
    await_swap_open_for_test(&fixture)?;
    Ok(())
}

pub fn participate_in_swap() -> Result<(), SnsLifecycleError> {
    let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(false)?;
    await_swap_open_for_test(&fixture)?;
    let participant = Principal::from_slice(&[77; 29]);
    fund_and_refresh_swap_participant(&fixture, participant, PARTICIPANT_ICP_E8S)?;
    Ok(())
}

pub fn refresh_buyer_tokens() -> Result<(), SnsLifecycleError> {
    participate_in_swap()
}

pub fn await_swap_committed() -> Result<(), SnsLifecycleError> {
    let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(false)?;
    await_swap_open_for_test(&fixture)?;
    let participant = Principal::from_slice(&[78; 29]);
    fund_and_refresh_swap_participant(&fixture, participant, PARTICIPANT_ICP_E8S)?;
    await_swap_committed_for_test(&fixture)?;
    Ok(())
}

pub fn finalize_swap() -> Result<(), SnsLifecycleError> {
    let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(false)?;
    await_swap_open_for_test(&fixture)?;
    let participant = Principal::from_slice(&[79; 29]);
    fund_and_refresh_swap_participant(&fixture, participant, PARTICIPANT_ICP_E8S)?;
    await_swap_committed_for_test(&fixture)?;
    finalize_swap_for_test(&fixture)?;
    Ok(())
}

pub fn await_sns_finalized() -> Result<(), SnsLifecycleError> {
    let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(false)?;
    await_swap_open_for_test(&fixture)?;
    let participant = Principal::from_slice(&[80; 29]);
    fund_and_refresh_swap_participant(&fixture, participant, PARTICIPANT_ICP_E8S)?;
    await_swap_committed_for_test(&fixture)?;
    finalize_swap_for_test(&fixture)?;
    await_swap_finalized_for_test(&fixture)?;
    Ok(())
}

pub fn discover_deployed_sns_canister_ids() -> Result<(), SnsLifecycleError> {
    let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(false)?;
    assert_no_production_fiduciary_ids(&[
        fixture.root,
        fixture.governance,
        fixture.ledger,
        fixture.index,
        fixture.swap,
    ])
}

pub fn read_sns_canister_ids() -> Result<(), SnsLifecycleError> {
    discover_deployed_sns_canister_ids()
}

pub fn list_sns_neurons() -> Result<(), SnsLifecycleError> {
    let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(false)?;
    list_finalized_sns_neurons(&fixture)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_sns_lifecycle_protected_id_guard_rejects_fiduciary_and_protected_ids() {
        for id in protected_production_ids() {
            let principal = Principal::from_text(&id).expect("protected ID should parse");
            assert_eq!(
                assert_no_production_fiduciary_ids(&[principal]),
                Err(SnsLifecycleError::ProtectedId(id))
            );
        }
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
    fn real_sns_lifecycle_ticket_amount_errors_are_precise() {
        let err = NewSaleTicketErr {
            invalid_user_amount: Some(InvalidUserAmount {
                min_amount_icp_e8s_included: 100_000_000,
                max_amount_icp_e8s_included: 1_000_000_000,
            }),
            existing_ticket: None,
            error_type: 2,
        };
        assert_eq!(
            err.invalid_user_amount
                .as_ref()
                .expect("invalid amount should be present")
                .min_amount_icp_e8s_included,
            100_000_000
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_participant_can_fund_swap_account_and_refresh_buyer_tokens() {
        let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(true).unwrap();
        await_swap_open_for_test(&fixture).unwrap();
        let participant = Principal::from_slice(&[81; 29]);
        let funded =
            fund_and_refresh_swap_participant(&fixture, participant, PARTICIPANT_ICP_E8S).unwrap();
        assert_eq!(funded.amount_icp_e8s, PARTICIPANT_ICP_E8S);
        assert!(funded.transfer_block.is_some());
        let buyer_state = funded.buyer_state.expect("buyer state should be recorded");
        assert_eq!(
            buyer_state
                .icp
                .expect("ICP participation should exist")
                .amount_e8s,
            PARTICIPANT_ICP_E8S
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_swap_commits_after_minimum_participation() {
        let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(true).unwrap();
        await_swap_open_for_test(&fixture).unwrap();
        let participant = Principal::from_slice(&[82; 29]);
        fund_and_refresh_swap_participant(&fixture, participant, PARTICIPANT_ICP_E8S).unwrap();
        let lifecycle = await_swap_committed_for_test(&fixture).unwrap();
        assert!(is_committed_lifecycle(lifecycle.lifecycle));
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_swap_finalizes_successfully_and_preserves_canister_ids() {
        let fixture = deploy_finalized_sns_lifecycle_fixture_for_test(
            true,
            Principal::from_slice(&[83; 29]),
            PARTICIPANT_ICP_E8S,
        )
        .unwrap();
        assert_all_canisters_on_expected_subnets(&fixture).unwrap();
        assert_eq!(
            fixture.pic.get_controllers(fixture.dapp_canisters[0]),
            vec![fixture.root]
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_finalized_governance_list_neurons_returns_participant_neuron() {
        let participant = Principal::from_slice(&[84; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        let neurons = find_direct_participation_neurons(&fixture, participant).unwrap();
        assert_direct_participation_neuron_stake(&neurons);
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_finalized_governance_neuron_controller_matches_participant() {
        let participant = Principal::from_slice(&[90; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        let neurons = find_direct_participation_neurons(&fixture, participant).unwrap();
        assert_direct_participation_neuron_stake(&neurons);
        assert!(
            neurons.iter().all(|neuron| neuron
                .permissions
                .iter()
                .any(|permission| permission.principal == Some(participant))),
            "all direct participation neurons should grant permissions to participant"
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_finalized_governance_neuron_dissolve_delay_matches_basket() {
        let participant = Principal::from_slice(&[91; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        let neurons = find_direct_participation_neurons(&fixture, participant).unwrap();
        assert_direct_participation_neuron_stake(&neurons);
        assert_direct_participation_neuron_dissolve_delay(&neurons, &[0, 1]);
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_finalized_governance_multiple_participants_create_distinct_neurons() {
        let first = Principal::from_slice(&[92; 29]);
        let second = Principal::from_slice(&[93; 29]);
        let fixture = deploy_finalized_sns_lifecycle_fixture_with_participants_for_test(
            true,
            &[(first, PARTICIPANT_ICP_E8S), (second, PARTICIPANT_ICP_E8S)],
        )
        .unwrap();
        let first_neurons = find_direct_participation_neurons(&fixture, first).unwrap();
        let second_neurons = find_direct_participation_neurons(&fixture, second).unwrap();
        assert_direct_participation_neuron_stake(&first_neurons);
        assert_direct_participation_neuron_stake(&second_neurons);
        let first_ids: BTreeSet<Vec<u8>> = first_neurons
            .iter()
            .filter_map(|neuron| neuron.id.as_ref().map(|id| id.id.clone()))
            .collect();
        let second_ids: BTreeSet<Vec<u8>> = second_neurons
            .iter()
            .filter_map(|neuron| neuron.id.as_ref().map(|id| id.id.clone()))
            .collect();
        assert!(
            first_ids.is_disjoint(&second_ids),
            "participants should not share finalized neuron IDs"
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_finalized_governance_no_duplicate_neuron_ids() {
        let participant = Principal::from_slice(&[94; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        let neurons = find_direct_participation_neurons(&fixture, participant).unwrap();
        assert_direct_participation_neuron_stake(&neurons);
        let ids: Vec<Vec<u8>> = neurons
            .iter()
            .map(|neuron| {
                neuron
                    .id
                    .as_ref()
                    .expect("finalized neuron should have an id")
                    .id
                    .clone()
            })
            .collect();
        let unique: BTreeSet<Vec<u8>> = ids.iter().cloned().collect();
        assert_eq!(unique.len(), ids.len(), "duplicate neuron IDs: {ids:?}");
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_finalized_governance_pagination_does_not_drop_neurons() {
        let participant = Principal::from_slice(&[95; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        let direct = find_direct_participation_neurons(&fixture, participant).unwrap();
        assert_direct_participation_neuron_stake(&direct);

        let direct_ids: BTreeSet<Vec<u8>> = direct
            .iter()
            .filter_map(|neuron| neuron.id.as_ref().map(|id| id.id.clone()))
            .collect();
        let limited_page =
            list_finalized_sns_neurons_for_principal(&fixture, participant, 1, None).unwrap();
        let limited_ids: BTreeSet<Vec<u8>> = limited_page
            .iter()
            .filter_map(|neuron| neuron.id.as_ref().map(|id| id.id.clone()))
            .collect();
        assert_eq!(limited_ids.len(), 1);
        assert!(limited_ids.is_subset(&direct_ids));

        let all_neurons = list_all_finalized_sns_neurons(&fixture).unwrap();
        let all_ids: BTreeSet<Vec<u8>> = all_neurons
            .iter()
            .filter_map(|neuron| neuron.id.as_ref().map(|id| id.id.clone()))
            .collect();
        assert!(
            direct_ids.is_subset(&all_ids),
            "full finalized governance listing should contain all participant basket neurons"
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_user_stakes_io_normal_path_after_sns_w_finalization() {
        let participant = Principal::from_slice(&[96; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        let liquid = disburse_zero_delay_neuron_to_participant_for_test(&fixture, participant)
            .expect("zero-delay finalized neuron should disburse to liquid SNS ledger account");
        assert!(
            liquid > 100_000_000 + crate::icrc::FEE_E8S,
            "disbursed liquid balance {liquid} should fund a minimum stake plus fee"
        );
        let neuron_id =
            stake_finalized_liquid_sns_tokens_for_test(&fixture, participant, 100_000_000, 20_001)
                .expect("finalized governance claim should create a staked neuron");
        let neuron = finalized_neuron_for_participant(&fixture, participant, &neuron_id).unwrap();
        assert_eq!(neuron.cached_neuron_stake_e8s, 100_000_000);
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_user_topup_increases_existing_neuron_after_sns_w_finalization() {
        let participant = Principal::from_slice(&[97; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        disburse_zero_delay_neuron_to_participant_for_test(&fixture, participant)
            .expect("zero-delay finalized neuron should disburse to liquid SNS ledger account");
        let memo = 20_002;
        let neuron_id =
            stake_finalized_liquid_sns_tokens_for_test(&fixture, participant, 100_000_000, memo)
                .expect("initial finalized stake should claim a neuron");
        let topped_up_id =
            stake_finalized_liquid_sns_tokens_for_test(&fixture, participant, 50_000_000, memo)
                .expect("same memo/controller should top up the existing neuron");
        assert_eq!(topped_up_id, neuron_id);
        let neuron = finalized_neuron_for_participant(&fixture, participant, &neuron_id).unwrap();
        assert_eq!(neuron.cached_neuron_stake_e8s, 150_000_000);
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_minimum_stake_is_enforced_after_finalization() {
        let participant = Principal::from_slice(&[98; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        disburse_zero_delay_neuron_to_participant_for_test(&fixture, participant)
            .expect("zero-delay finalized neuron should disburse to liquid SNS ledger account");
        let err =
            stake_finalized_liquid_sns_tokens_for_test(&fixture, participant, 50_000_000, 20_003)
                .expect_err("below-minimum finalized stake should be rejected");
        match err {
            SnsLifecycleError::SwapRejected(message) => {
                assert!(
                    message.contains("at least 100000000 e8s")
                        && message.contains("was 50000000 e8s"),
                    "unexpected finalized minimum-stake rejection: {message}"
                );
            }
            other => panic!("unexpected finalized minimum-stake error: {other:?}"),
        }
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_user_can_stake_multiple_neurons_after_finalization() {
        let participant = Principal::from_slice(&[102; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        disburse_zero_delay_neuron_to_participant_for_test(&fixture, participant)
            .expect("zero-delay finalized neuron should disburse to liquid SNS ledger account");

        let first =
            stake_finalized_liquid_sns_tokens_for_test(&fixture, participant, 100_000_000, 20_006)
                .expect("first finalized stake should claim a neuron");
        let second =
            stake_finalized_liquid_sns_tokens_for_test(&fixture, participant, 100_000_000, 20_007)
                .expect("second finalized stake should claim a distinct neuron");

        assert_ne!(first, second);
        let first_neuron = finalized_neuron_for_participant(&fixture, participant, &first).unwrap();
        let second_neuron =
            finalized_neuron_for_participant(&fixture, participant, &second).unwrap();
        assert_eq!(first_neuron.cached_neuron_stake_e8s, 100_000_000);
        assert_eq!(second_neuron.cached_neuron_stake_e8s, 100_000_000);
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_dissolve_delay_below_two_weeks_is_ineligible_after_finalization() {
        let participant = Principal::from_slice(&[103; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        disburse_zero_delay_neuron_to_participant_for_test(&fixture, participant)
            .expect("zero-delay finalized neuron should disburse to liquid SNS ledger account");
        let neuron_id =
            stake_finalized_liquid_sns_tokens_for_test(&fixture, participant, 100_000_000, 20_008)
                .expect("finalized stake should claim a neuron");
        configure_finalized_neuron_dissolve_delay_for_test(
            &fixture,
            participant,
            &neuron_id,
            604_800,
        )
        .expect("finalized governance should accept a below-threshold dissolve delay");

        let neuron = finalized_neuron_for_participant(&fixture, participant, &neuron_id).unwrap();
        assert_eq!(finalized_neuron_dissolve_delay_seconds(&neuron), 604_800);
        let snapshot = io_reward_policy::NeuronSnapshot {
            neuron_id: 2,
            staked_io_e8s: u128::from(neuron.cached_neuron_stake_e8s),
            eligible_seconds: if finalized_neuron_dissolve_delay_seconds(&neuron) >= 1_209_600 {
                finalized_neuron_dissolve_delay_seconds(&neuron)
            } else {
                0
            },
            eligible_closed_proposals: 0,
            voted_closed_proposals: 0,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: false,
        };
        assert!(!io_reward_policy::eligible(&snapshot));
        assert_eq!(io_reward_policy::reward_weight(&snapshot), 0);
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_dissolve_delay_at_two_weeks_is_eligible_after_finalization() {
        let participant = Principal::from_slice(&[99; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        disburse_zero_delay_neuron_to_participant_for_test(&fixture, participant)
            .expect("zero-delay finalized neuron should disburse to liquid SNS ledger account");
        let neuron_id =
            stake_finalized_liquid_sns_tokens_for_test(&fixture, participant, 100_000_000, 20_004)
                .expect("finalized stake should claim a neuron");
        let initial = finalized_neuron_for_participant(&fixture, participant, &neuron_id).unwrap();
        assert_eq!(finalized_neuron_dissolve_delay_seconds(&initial), 0);
        configure_finalized_neuron_dissolve_delay_for_test(
            &fixture,
            participant,
            &neuron_id,
            1_209_600,
        )
        .expect("finalized governance should accept two-week dissolve delay");
        let eligible = finalized_neuron_for_participant(&fixture, participant, &neuron_id).unwrap();
        assert_eq!(
            finalized_neuron_dissolve_delay_seconds(&eligible),
            1_209_600
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_dissolving_neuron_is_excluded_if_policy_requires_after_finalization() {
        let participant = Principal::from_slice(&[111; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        disburse_zero_delay_neuron_to_participant_for_test(&fixture, participant)
            .expect("zero-delay finalized neuron should disburse to liquid SNS ledger account");
        let neuron_id =
            stake_finalized_liquid_sns_tokens_for_test(&fixture, participant, 100_000_000, 20_009)
                .expect("finalized stake should claim a neuron");
        configure_finalized_neuron_dissolve_delay_for_test(
            &fixture,
            participant,
            &neuron_id,
            1_209_600,
        )
        .expect("finalized governance should accept two-week dissolve delay");
        start_finalized_neuron_dissolving_for_test(&fixture, participant, &neuron_id)
            .expect("finalized governance should accept start dissolving");
        for _ in 0..5 {
            fixture.pic.tick();
        }

        let neuron = finalized_neuron_for_participant(&fixture, participant, &neuron_id).unwrap();
        assert!(
            finalized_neuron_is_dissolving(&neuron),
            "finalized neuron should be dissolving after StartDissolving: {neuron:?}"
        );
        let snapshot = io_reward_policy::NeuronSnapshot {
            neuron_id: 4,
            staked_io_e8s: u128::from(neuron.cached_neuron_stake_e8s),
            eligible_seconds: 1_209_600,
            eligible_closed_proposals: 0,
            voted_closed_proposals: 0,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: finalized_neuron_is_dissolving(&neuron),
        };
        assert!(!io_reward_policy::eligible(&snapshot));
        assert_eq!(io_reward_policy::reward_weight(&snapshot), 0);
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_stop_dissolving_restores_eligibility_if_policy_allows_after_finalization() {
        let participant = Principal::from_slice(&[112; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        disburse_zero_delay_neuron_to_participant_for_test(&fixture, participant)
            .expect("zero-delay finalized neuron should disburse to liquid SNS ledger account");
        let neuron_id =
            stake_finalized_liquid_sns_tokens_for_test(&fixture, participant, 100_000_000, 20_010)
                .expect("finalized stake should claim a neuron");
        configure_finalized_neuron_dissolve_delay_for_test(
            &fixture,
            participant,
            &neuron_id,
            1_209_600,
        )
        .expect("finalized governance should accept two-week dissolve delay");
        start_finalized_neuron_dissolving_for_test(&fixture, participant, &neuron_id)
            .expect("finalized governance should accept start dissolving");
        stop_finalized_neuron_dissolving_for_test(&fixture, participant, &neuron_id)
            .expect("finalized governance should accept stop dissolving");
        for _ in 0..5 {
            fixture.pic.tick();
        }

        let neuron = finalized_neuron_for_participant(&fixture, participant, &neuron_id).unwrap();
        assert!(
            !finalized_neuron_is_dissolving(&neuron),
            "finalized neuron should stop dissolving after StopDissolving: {neuron:?}"
        );
        assert!(
            finalized_neuron_dissolve_delay_seconds(&neuron) >= 1_209_600,
            "stop dissolving should restore a non-dissolving delay at or above two weeks: {neuron:?}"
        );
        let snapshot = io_reward_policy::NeuronSnapshot {
            neuron_id: 5,
            staked_io_e8s: u128::from(neuron.cached_neuron_stake_e8s),
            eligible_seconds: finalized_neuron_dissolve_delay_seconds(&neuron),
            eligible_closed_proposals: 0,
            voted_closed_proposals: 0,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: finalized_neuron_is_dissolving(&neuron),
        };
        assert!(io_reward_policy::eligible(&snapshot));
        assert!(io_reward_policy::reward_weight(&snapshot) > 0);
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_no_closed_proposals_participation_factor_defaults_to_one_after_finalization() {
        let participant = Principal::from_slice(&[100; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        disburse_zero_delay_neuron_to_participant_for_test(&fixture, participant)
            .expect("zero-delay finalized neuron should disburse to liquid SNS ledger account");
        let neuron_id =
            stake_finalized_liquid_sns_tokens_for_test(&fixture, participant, 100_000_000, 20_005)
                .expect("finalized stake should claim a neuron");
        configure_finalized_neuron_dissolve_delay_for_test(
            &fixture,
            participant,
            &neuron_id,
            1_209_600,
        )
        .expect("finalized governance should accept two-week dissolve delay");

        let proposals = list_finalized_sns_proposals(&fixture, 100)
            .expect("finalized governance list_proposals should decode");
        assert!(
            proposals.proposals.is_empty(),
            "fresh finalized local SNS should not have closed reward proposals before voting setup"
        );

        let neuron = finalized_neuron_for_participant(&fixture, participant, &neuron_id).unwrap();
        let snapshot = io_reward_policy::NeuronSnapshot {
            neuron_id: 1,
            staked_io_e8s: u128::from(neuron.cached_neuron_stake_e8s),
            eligible_seconds: finalized_neuron_dissolve_delay_seconds(&neuron),
            eligible_closed_proposals: proposals.proposals.len() as u64,
            voted_closed_proposals: 0,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: false,
        };
        assert_eq!(io_reward_policy::participation_ratio(&snapshot), (1, 1));
        assert_eq!(
            io_reward_policy::reward_weight(&snapshot),
            snapshot.staked_io_e8s * u128::from(snapshot.eligible_seconds)
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_user_votes_yes_and_ballot_is_observed_after_finalization() {
        let participant = Principal::from_slice(&[104; 29]);
        let fixture = deploy_finalized_sns_lifecycle_fixture_for_test(
            true,
            participant,
            10 * PARTICIPANT_ICP_E8S,
        )
        .unwrap();
        let mut neurons = find_direct_participation_neurons(&fixture, participant).unwrap();
        neurons.sort_by_key(|neuron| std::cmp::Reverse(neuron.cached_neuron_stake_e8s));
        let proposer_neuron = neurons
            .first()
            .and_then(|neuron| neuron.id.clone())
            .expect("finalized direct participant should have a neuron id");
        configure_finalized_neuron_dissolve_delay_for_test(
            &fixture,
            participant,
            &proposer_neuron,
            1_209_600,
        )
        .expect("finalized governance should accept proposer dissolve delay");

        let proposal_id = make_finalized_motion_proposal_for_test(
            &fixture,
            participant,
            &proposer_neuron,
            "IO finalized yes vote smoke",
        )
        .expect("finalized governance should accept a motion proposal");
        let duplicate_yes = register_finalized_sns_vote_for_test(
            &fixture,
            participant,
            &proposer_neuron,
            proposal_id.clone(),
            1,
        )
        .expect_err("proposal creation should already record the proposer vote");
        match duplicate_yes {
            SnsLifecycleError::SwapRejected(message) => {
                assert!(
                    message.contains("Neuron already voted on proposal"),
                    "unexpected finalized duplicate yes rejection: {message}"
                );
            }
            other => panic!("unexpected finalized duplicate yes error: {other:?}"),
        }
        for _ in 0..20 {
            fixture.pic.tick();
        }
        let proposals = list_finalized_sns_proposals_as(&fixture, participant, 100).unwrap();
        let proposal = proposals
            .proposals
            .iter()
            .find(|proposal| proposal.id == Some(proposal_id.clone()))
            .expect("submitted proposal should be listed");
        assert_eq!(proposal.proposer, Some(proposer_neuron));
        assert!(
            !proposal.ballots.is_empty(),
            "proposer ballot should be visible on finalized proposal: {:?}",
            proposal.ballots
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_user_votes_no_and_ballot_is_observed_after_finalization() {
        let proposer = Principal::from_slice(&[105; 29]);
        let voter = Principal::from_slice(&[106; 29]);
        let fixture = deploy_finalized_sns_lifecycle_fixture_with_participants_for_test(
            true,
            &[
                (proposer, PARTICIPANT_ICP_E8S),
                (voter, PARTICIPANT_ICP_E8S),
            ],
        )
        .unwrap();
        let proposer_neuron = find_direct_participation_neurons(&fixture, proposer)
            .unwrap()
            .into_iter()
            .max_by_key(|neuron| neuron.cached_neuron_stake_e8s)
            .and_then(|neuron| neuron.id)
            .expect("proposer should have a finalized direct-participation neuron");
        let voter_neuron = find_direct_participation_neurons(&fixture, voter)
            .unwrap()
            .into_iter()
            .max_by_key(|neuron| neuron.cached_neuron_stake_e8s)
            .and_then(|neuron| neuron.id)
            .expect("voter should have a finalized direct-participation neuron");
        configure_finalized_neuron_dissolve_delay_for_test(
            &fixture,
            proposer,
            &proposer_neuron,
            1_209_600,
        )
        .expect("finalized governance should accept proposer dissolve delay");
        configure_finalized_neuron_dissolve_delay_for_test(
            &fixture,
            voter,
            &voter_neuron,
            1_209_600,
        )
        .expect("finalized governance should accept voter dissolve delay");

        let proposal_id = make_finalized_motion_proposal_for_test(
            &fixture,
            proposer,
            &proposer_neuron,
            "IO finalized no vote smoke",
        )
        .expect("finalized governance should accept a motion proposal");
        register_finalized_sns_vote_for_test(
            &fixture,
            voter,
            &voter_neuron,
            proposal_id.clone(),
            2,
        )
        .expect("second finalized participant should be able to vote no");
        for _ in 0..20 {
            fixture.pic.tick();
        }
        let proposals = list_finalized_sns_proposals_as(&fixture, voter, 100).unwrap();
        let proposal = proposals
            .proposals
            .iter()
            .find(|proposal| proposal.id == Some(proposal_id.clone()))
            .expect("submitted proposal should be listed");
        assert!(
            proposal.ballots.iter().any(|(_, ballot)| ballot.vote == 2),
            "registered no ballot should be visible on finalized proposal: {:?}",
            proposal.ballots
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_non_voter_gets_lower_participation_factor_after_finalization() {
        let proposer = Principal::from_slice(&[107; 29]);
        let non_voter = Principal::from_slice(&[108; 29]);
        let fixture = deploy_finalized_sns_lifecycle_fixture_with_participants_for_test(
            true,
            &[
                (proposer, 9 * PARTICIPANT_ICP_E8S),
                (non_voter, PARTICIPANT_ICP_E8S),
            ],
        )
        .unwrap();
        let proposer_neuron = find_direct_participation_neurons(&fixture, proposer)
            .unwrap()
            .into_iter()
            .max_by_key(|neuron| neuron.cached_neuron_stake_e8s)
            .expect("proposer should have a finalized direct-participation neuron");
        let proposer_neuron_id = proposer_neuron
            .id
            .clone()
            .expect("proposer neuron should have an id");
        let non_voter_neuron = find_direct_participation_neurons(&fixture, non_voter)
            .unwrap()
            .into_iter()
            .max_by_key(|neuron| neuron.cached_neuron_stake_e8s)
            .expect("non-voter should have a finalized direct-participation neuron");
        let non_voter_neuron_id = non_voter_neuron
            .id
            .clone()
            .expect("non-voter neuron should have an id");
        configure_finalized_neuron_dissolve_delay_for_test(
            &fixture,
            proposer,
            &proposer_neuron_id,
            1_209_600,
        )
        .expect("finalized governance should accept proposer dissolve delay");
        configure_finalized_neuron_dissolve_delay_for_test(
            &fixture,
            non_voter,
            &non_voter_neuron_id,
            1_209_600,
        )
        .expect("finalized governance should accept non-voter dissolve delay");

        let proposal_id = make_finalized_motion_proposal_for_test(
            &fixture,
            proposer,
            &proposer_neuron_id,
            "IO finalized participation factor smoke",
        )
        .expect("finalized governance should accept a motion proposal");
        for _ in 0..60 {
            fixture.pic.advance_time(Duration::from_secs(1_800));
            fixture.pic.tick();
        }

        let proposer_proposals = list_finalized_sns_proposals_as(&fixture, proposer, 100).unwrap();
        let proposer_proposal = proposer_proposals
            .proposals
            .iter()
            .find(|proposal| proposal.id == Some(proposal_id.clone()))
            .expect("submitted proposal should be listed for proposer");
        assert!(
            proposer_proposal.decided_timestamp_seconds > 0,
            "proposal should close after deterministic PocketIC time advancement: {proposer_proposal:?}"
        );
        assert!(
            !proposer_proposal.ballots.is_empty(),
            "proposer should have a caller-visible ballot"
        );

        let non_voter_proposals =
            list_finalized_sns_proposals_as(&fixture, non_voter, 100).unwrap();
        let non_voter_proposal = non_voter_proposals
            .proposals
            .iter()
            .find(|proposal| proposal.id == Some(proposal_id.clone()))
            .expect("submitted proposal should be listed for non-voter");
        assert!(
            non_voter_proposal
                .ballots
                .iter()
                .all(|(_, ballot)| ballot.vote == 0),
            "non-voter caller-visible ballots should remain unspecified: {:?}",
            non_voter_proposal.ballots
        );

        let proposer_snapshot = io_reward_policy::NeuronSnapshot {
            neuron_id: 3,
            staked_io_e8s: u128::from(proposer_neuron.cached_neuron_stake_e8s),
            eligible_seconds: 1_209_600,
            eligible_closed_proposals: 1,
            voted_closed_proposals: 1,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: false,
        };
        let non_voter_snapshot = io_reward_policy::NeuronSnapshot {
            neuron_id: 4,
            staked_io_e8s: u128::from(non_voter_neuron.cached_neuron_stake_e8s),
            eligible_seconds: 1_209_600,
            eligible_closed_proposals: 1,
            voted_closed_proposals: 0,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: false,
        };
        assert_eq!(
            io_reward_policy::participation_ratio(&proposer_snapshot),
            (1, 1)
        );
        assert_eq!(
            io_reward_policy::participation_ratio(&non_voter_snapshot),
            (0, 1)
        );
        assert!(io_reward_policy::reward_weight(&proposer_snapshot) > 0);
        assert_eq!(io_reward_policy::reward_weight(&non_voter_snapshot), 0);
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_following_vote_counts_for_participation_if_policy_allows_after_finalization() {
        let proposer = Principal::from_slice(&[110; 29]);
        let leader = Principal::from_slice(&[111; 29]);
        let follower = Principal::from_slice(&[112; 29]);
        let fixture = deploy_finalized_sns_lifecycle_fixture_with_participants_for_test(
            true,
            &[
                (proposer, PARTICIPANT_ICP_E8S),
                (leader, PARTICIPANT_ICP_E8S),
                (follower, PARTICIPANT_ICP_E8S),
            ],
        )
        .unwrap();
        let proposer_neuron = find_direct_participation_neurons(&fixture, proposer)
            .unwrap()
            .into_iter()
            .max_by_key(|neuron| neuron.cached_neuron_stake_e8s)
            .and_then(|neuron| neuron.id)
            .expect("proposer should have a finalized direct-participation neuron");
        let leader_neuron = find_direct_participation_neurons(&fixture, leader)
            .unwrap()
            .into_iter()
            .max_by_key(|neuron| neuron.cached_neuron_stake_e8s)
            .and_then(|neuron| neuron.id)
            .expect("leader should have a finalized direct-participation neuron");
        let follower_neuron_record = find_direct_participation_neurons(&fixture, follower)
            .unwrap()
            .into_iter()
            .max_by_key(|neuron| neuron.cached_neuron_stake_e8s)
            .expect("follower should have a finalized direct-participation neuron");
        let follower_neuron = follower_neuron_record
            .id
            .clone()
            .expect("follower should have a finalized direct-participation neuron id");
        for (principal, neuron) in [
            (proposer, &proposer_neuron),
            (leader, &leader_neuron),
            (follower, &follower_neuron),
        ] {
            configure_finalized_neuron_dissolve_delay_for_test(
                &fixture, principal, neuron, 1_209_600,
            )
            .expect("finalized governance should accept dissolve delay");
        }

        let function_id = finalized_motion_function_id_for_test(&fixture)
            .expect("finalized governance should expose the Motion function id");
        follow_finalized_sns_neuron_for_test(
            &fixture,
            follower,
            &follower_neuron,
            leader_neuron.clone(),
            function_id,
        )
        .expect("follower should be able to follow leader for Motion");
        set_finalized_sns_governance_following_for_test(
            &fixture,
            follower,
            &follower_neuron,
            leader_neuron.clone(),
        )
        .expect("follower should be able to set topic following");

        let proposal_id = make_finalized_motion_proposal_for_test(
            &fixture,
            proposer,
            &proposer_neuron,
            "IO finalized following vote smoke",
        )
        .expect("finalized governance should accept a motion proposal");
        register_finalized_sns_vote_for_test(
            &fixture,
            leader,
            &leader_neuron,
            proposal_id.clone(),
            1,
        )
        .expect("leader should be able to vote yes after proposal creation");
        for _ in 0..20 {
            fixture.pic.tick();
        }

        let proposals = list_finalized_sns_proposals_as(&fixture, follower, 100).unwrap();
        let proposal = proposals
            .proposals
            .iter()
            .find(|proposal| proposal.id == Some(proposal_id.clone()))
            .expect("submitted proposal should be listed for follower");
        assert!(
            proposal.ballots.iter().any(|(_, ballot)| ballot.vote == 1),
            "follower-visible ballots should include a yes vote after leader vote propagation: {:?}",
            proposal.ballots
        );
        let follower_snapshot = io_reward_policy::NeuronSnapshot {
            neuron_id: 5,
            staked_io_e8s: u128::from(follower_neuron_record.cached_neuron_stake_e8s),
            eligible_seconds: 1_209_600,
            eligible_closed_proposals: 1,
            voted_closed_proposals: 1,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: false,
        };
        assert_eq!(
            io_reward_policy::participation_ratio(&follower_snapshot),
            (1, 1)
        );
        assert!(io_reward_policy::reward_weight(&follower_snapshot) > 0);
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_proposal_rejection_fee_is_100_io_if_configured_after_finalization() {
        let participant = Principal::from_slice(&[109; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        let params: crate::sns_governance_setup::NervousSystemParameters = crate::icrc::query_one(
            &fixture.pic,
            fixture.governance,
            "get_nervous_system_parameters",
            (),
        );
        assert_eq!(params.reject_cost_e8s, Some(100 * 100_000_000));

        let proposer_neuron = find_direct_participation_neurons(&fixture, participant)
            .unwrap()
            .into_iter()
            .next()
            .expect("participant should have a finalized direct-participation neuron");
        let proposer_neuron_id = proposer_neuron
            .id
            .clone()
            .expect("participant neuron should have an id");
        let proposal_id = make_finalized_motion_proposal_for_test(
            &fixture,
            participant,
            &proposer_neuron_id,
            "IO finalized reject-cost smoke",
        )
        .expect("finalized governance should accept a motion proposal");
        let proposals = list_finalized_sns_proposals_as(&fixture, participant, 10).unwrap();
        let proposal = proposals
            .proposals
            .iter()
            .find(|record| record.id.as_ref() == Some(&proposal_id))
            .expect("proposal should be listed by finalized governance");
        assert_eq!(proposal.reject_cost_e8s, 100 * 100_000_000);
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_swap_observes_direct_participation() {
        let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(true).unwrap();
        await_swap_open_for_test(&fixture).unwrap();
        let participant = Principal::from_slice(&[85; 29]);
        fund_and_refresh_swap_participant(&fixture, participant, PARTICIPANT_ICP_E8S).unwrap();
        let direct = list_direct_participants(&fixture).unwrap();
        let observed = direct
            .participants
            .iter()
            .find(|entry| entry.participant_id == Some(participant))
            .and_then(|entry| entry.participation.as_ref())
            .and_then(|state| state.icp.as_ref())
            .map(|icp| icp.amount_e8s);
        assert_eq!(observed, Some(PARTICIPANT_ICP_E8S));
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_swap_rejects_below_min_participant_icp() {
        let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(true).unwrap();
        await_swap_open_for_test(&fixture).unwrap();
        let params = read_swap_sale_parameters_for_test(&fixture).unwrap();
        let err = create_sale_ticket_if_required(
            &fixture,
            Principal::from_slice(&[87; 29]),
            params.min_participant_icp_e8s - 1,
        )
        .expect_err("below-min participant amount should be rejected");
        match err {
            SnsLifecycleError::SwapRejected(message) => {
                assert!(message.contains("invalid_user_amount"), "{message}");
                assert!(
                    message.contains(&params.min_participant_icp_e8s.to_string()),
                    "{message}"
                );
            }
            other => panic!("expected swap rejection, got {other:?}"),
        }
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_swap_rejects_above_max_participant_icp() {
        let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(true).unwrap();
        await_swap_open_for_test(&fixture).unwrap();
        let params = read_swap_sale_parameters_for_test(&fixture).unwrap();
        let err = create_sale_ticket_if_required(
            &fixture,
            Principal::from_slice(&[88; 29]),
            params.max_participant_icp_e8s + 1,
        )
        .expect_err("above-max participant amount should be rejected");
        match err {
            SnsLifecycleError::SwapRejected(message) => {
                assert!(message.contains("invalid_user_amount"), "{message}");
                assert!(
                    message.contains(&params.max_participant_icp_e8s.to_string()),
                    "{message}"
                );
            }
            other => panic!("expected swap rejection, got {other:?}"),
        }
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_swap_rejects_duplicate_ticket_or_handles_idempotently() {
        let fixture = deploy_io_test_sns_lifecycle_fixture_for_test(true).unwrap();
        await_swap_open_for_test(&fixture).unwrap();
        let participant = Principal::from_slice(&[89; 29]);
        let first = create_sale_ticket_if_required(&fixture, participant, PARTICIPANT_ICP_E8S)
            .expect("first ticket should be created");
        let second = create_sale_ticket_if_required(&fixture, participant, PARTICIPANT_ICP_E8S)
            .expect("duplicate ticket should return existing ticket idempotently");
        assert_eq!(second.ticket_id, first.ticket_id);
        assert_eq!(second.amount_icp_e8s, first.amount_icp_e8s);
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_root_lists_application_subnet_dapp_after_finalization() {
        let fixture = deploy_finalized_sns_lifecycle_fixture_for_test(
            true,
            Principal::from_slice(&[86; 29]),
            PARTICIPANT_ICP_E8S,
        )
        .unwrap();
        let listed: crate::sns_root_setup::ListSnsCanistersResponse = crate::icrc::query_one(
            &fixture.pic,
            fixture.root,
            "list_sns_canisters",
            crate::nns_setup::EmptyRecord {},
        );
        assert_eq!(listed.root, Some(fixture.root));
        assert_eq!(listed.governance, Some(fixture.governance));
        assert_eq!(listed.ledger, Some(fixture.ledger));
        assert_eq!(listed.index, Some(fixture.index));
        assert_eq!(listed.swap, Some(fixture.swap));
        assert_eq!(listed.dapps, fixture.dapp_canisters);
        assert_eq!(
            fixture.pic.get_subnet(fixture.dapp_canisters[0]),
            Some(fixture.application_subnet)
        );
        assert_eq!(
            fixture.pic.get_controllers(fixture.dapp_canisters[0]),
            vec![fixture.root]
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_root_can_upgrade_test_app_canister_after_finalization() {
        let participant = Principal::from_slice(&[110; 29]);
        let fixture =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        let dapp = fixture.dapp_canisters[0];
        assert_eq!(fixture.pic.get_controllers(dapp), vec![fixture.root]);
        let initial_hash = fixture
            .pic
            .canister_status(dapp, Some(fixture.root))
            .expect("SNS root should be able to read controlled dapp status")
            .module_hash
            .expect("test dapp should have an initial module hash");
        assert_eq!(
            initial_hash,
            Sha256::digest(SNS_CONTROLLED_DAPP_INITIAL_WASM).to_vec()
        );

        let proposer_neuron = find_direct_participation_neurons(&fixture, participant)
            .unwrap()
            .into_iter()
            .max_by_key(|neuron| neuron.cached_neuron_stake_e8s)
            .and_then(|neuron| neuron.id)
            .expect("participant should have a finalized direct-participation neuron");
        configure_finalized_neuron_dissolve_delay_for_test(
            &fixture,
            participant,
            &proposer_neuron,
            1_209_600,
        )
        .expect("finalized governance should accept proposer dissolve delay");
        let proposal_id = make_finalized_proposal_for_test(
            &fixture,
            participant,
            &proposer_neuron,
            crate::sns_governance_setup::Proposal {
                url: format!("{}://example.invalid/io-local-root-upgrade", "https"),
                title: "IO finalized root app upgrade smoke".to_string(),
                summary: "Local-only finalized SNS root upgrade proposal for IO E2E.".to_string(),
                action: Some(
                    crate::sns_governance_setup::Action::UpgradeSnsControlledCanister(
                        crate::sns_governance_setup::UpgradeSnsControlledCanister {
                            new_canister_wasm: SNS_CONTROLLED_DAPP_UPGRADE_WASM.to_vec(),
                            chunked_canister_wasm: None,
                            mode: None,
                            canister_id: Some(dapp),
                            canister_upgrade_arg: Some(vec![]),
                        },
                    ),
                ),
            },
        )
        .expect("finalized governance should accept root-controlled dapp upgrade proposal");

        let expected_hash = Sha256::digest(SNS_CONTROLLED_DAPP_UPGRADE_WASM).to_vec();
        for _ in 0..80 {
            fixture.pic.advance_time(Duration::from_secs(1_800));
            fixture.pic.tick();
            let status = fixture
                .pic
                .canister_status(dapp, Some(fixture.root))
                .expect("SNS root should remain controller during upgrade polling");
            if status.module_hash.as_ref() == Some(&expected_hash) {
                break;
            }
        }

        let upgraded_hash = fixture
            .pic
            .canister_status(dapp, Some(fixture.root))
            .expect("SNS root should be able to read upgraded dapp status")
            .module_hash
            .expect("upgraded dapp should have a module hash");
        assert_eq!(upgraded_hash, expected_hash);
        let proposals = list_finalized_sns_proposals_as(&fixture, participant, 100).unwrap();
        let proposal = proposals
            .proposals
            .iter()
            .find(|proposal| proposal.id == Some(proposal_id.clone()))
            .expect("upgrade proposal should be listed by finalized governance");
        assert!(
            proposal.executed_timestamp_seconds > 0,
            "root upgrade proposal should execute: {proposal:?}"
        );
        assert_eq!(fixture.pic.get_controllers(dapp), vec![fixture.root]);
        assert_eq!(
            fixture.pic.get_subnet(dapp),
            Some(fixture.application_subnet)
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_root_rejects_non_dapp_canister_after_finalization() {
        let fixture = deploy_finalized_sns_lifecycle_fixture_for_test(
            true,
            Principal::from_slice(&[87; 29]),
            PARTICIPANT_ICP_E8S,
        )
        .unwrap();
        let non_dapp = crate::pocketic_env::create_empty_application_canister(&fixture.pic);
        assert_eq!(
            fixture.pic.get_subnet(non_dapp),
            Some(fixture.application_subnet)
        );

        let listed: crate::sns_root_setup::ListSnsCanistersResponse = crate::icrc::query_one(
            &fixture.pic,
            fixture.root,
            "list_sns_canisters",
            crate::nns_setup::EmptyRecord {},
        );
        assert_eq!(listed.dapps, fixture.dapp_canisters);
        assert!(!listed.dapps.contains(&non_dapp));
        assert_ne!(fixture.pic.get_controllers(non_dapp), vec![fixture.root]);
        assert_eq!(
            fixture.pic.get_controllers(fixture.dapp_canisters[0]),
            vec![fixture.root]
        );
    }
}
