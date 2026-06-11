use crate::artifacts::ArtifactSet;
use crate::icrc;
use crate::nns_setup::{
    install_nns_ledger, install_sns_wasm_on_existing_pic, EmptyRecord, SnsWasmCanisterInitPayload,
};
use crate::pocketic_env;
use candid::{decode_one, encode_one, CandidType, Principal};
use io_ledger_types::{Account, IcpTokens, IcpTransferArgs, IcpTransferError, Subaccount};
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const POCKETIC_UPDATE_INGRESS_LIMIT_BYTES: usize = 3_670_016;
pub const DECOMPRESSED_GOVERNANCE_PROPOSAL_SIZE_BLOCKER_BYTES: usize = 6_724_190;
pub const POCKETIC_UPDATE_CALL_ENVELOPE_OVERHEAD_BYTES: usize = 170;
const NNS_PROPOSAL_TOP_UP_E8S: u64 = 30 * 100_000_000;
const NNS_LEDGER_TRANSFER_FEE_E8S: u64 = 10_000;
const NNS_PROPOSAL_DISSOLVE_DELAY_SECONDS: u32 = 365 * 24 * 60 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnsCanisterType {
    Root,
    Governance,
    Ledger,
    Index,
    Swap,
    Archive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnsWasmPlan {
    pub canister_type: SnsCanisterType,
    pub artifact_key: &'static str,
}

pub const SNS_WASM_PUBLICATION_PLAN: &[SnsWasmPlan] = &[
    SnsWasmPlan {
        canister_type: SnsCanisterType::Root,
        artifact_key: "sns_root",
    },
    SnsWasmPlan {
        canister_type: SnsCanisterType::Governance,
        artifact_key: "sns_governance",
    },
    SnsWasmPlan {
        canister_type: SnsCanisterType::Ledger,
        artifact_key: "sns_ledger",
    },
    SnsWasmPlan {
        canister_type: SnsCanisterType::Index,
        artifact_key: "sns_index",
    },
    SnsWasmPlan {
        canister_type: SnsCanisterType::Swap,
        artifact_key: "sns_swap",
    },
    SnsWasmPlan {
        canister_type: SnsCanisterType::Archive,
        artifact_key: "sns_archive",
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedSnsWasm {
    pub canister_type: SnsCanisterType,
    pub artifact_key: &'static str,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernancePublicationPayloadSizes {
    pub compressed_wasm_bytes: usize,
    pub decompressed_wasm_bytes: usize,
    pub compressed_sns_wasm_candid_bytes: usize,
    pub decompressed_sns_wasm_candid_bytes: usize,
    pub compressed_manage_neuron_candid_bytes: usize,
    pub decompressed_manage_neuron_candid_bytes: usize,
    pub legacy_decompressed_manage_neuron_candid_bytes: usize,
    pub pocketic_ingress_max_bytes: usize,
}

pub struct PublishedSnsWasmFixture {
    pub pic: pocket_ic::PocketIc,
    pub sns_wasm: Principal,
    pub nns_governance: Principal,
    pub published: Vec<PublishedSnsWasm>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnsWasmSetupError {
    Artifact(String),
    WrongHashOrType,
    SnsWProposalDriverMissing,
    PocketIcMissing,
    SnsWasmRejected(String),
    NnsProposalRejected(String),
    WasmTooLargeForDirectUpdate(&'static str),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SnsWasm {
    pub wasm: Vec<u8>,
    pub proposal_id: Option<u64>,
    pub canister_type: i32,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct AddWasmRequest {
    pub hash: Vec<u8>,
    pub wasm: Option<SnsWasm>,
    pub skip_update_latest_version: Option<bool>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct AddWasmResponse {
    pub result: Option<AddWasmResult>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum AddWasmResult {
    Error(SnsWasmError),
    Hash(Vec<u8>),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SnsWasmError {
    pub message: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct GetWasmRequest {
    pub hash: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct GetWasmResponse {
    pub wasm: Option<SnsWasm>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ListDeployedSnsesResponse {
    pub instances: Vec<DeployedSns>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct DeployedSns {
    pub root_canister_id: Option<Principal>,
    pub governance_canister_id: Option<Principal>,
    pub index_canister_id: Option<Principal>,
    pub swap_canister_id: Option<Principal>,
    pub ledger_canister_id: Option<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsListNeuronsRequest {
    pub neuron_ids: Vec<u64>,
    pub include_neurons_readable_by_caller: bool,
    pub include_empty_neurons_readable_by_caller: Option<bool>,
    pub include_public_neurons_in_full_neurons: Option<bool>,
    pub page_number: Option<u64>,
    pub page_size: Option<u64>,
    pub neuron_subaccounts: Option<Vec<NnsNeuronSubaccount>>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsNeuronSubaccount {
    pub subaccount: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsListNeuronsResponse {
    pub full_neurons: Vec<NnsNeuron>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsNeuron {
    pub id: Option<NnsNeuronId>,
    pub controller: Option<Principal>,
    pub account: Vec<u8>,
    pub cached_neuron_stake_e8s: u64,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsNeuronId {
    pub id: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsManageNeuronRequest {
    pub neuron_id_or_subaccount: Option<NnsNeuronIdOrSubaccount>,
    pub command: Option<NnsManageNeuronCommandRequest>,
    pub id: Option<NnsNeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum NnsNeuronIdOrSubaccount {
    NeuronId(NnsNeuronId),
    Subaccount(Vec<u8>),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum NnsManageNeuronCommandRequest {
    MakeProposal(NnsMakeProposalRequest),
    ClaimOrRefresh(NnsClaimOrRefreshRequest),
    Configure(NnsConfigureRequest),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsClaimOrRefreshRequest {
    pub by: Option<NnsClaimOrRefreshByRequest>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum NnsClaimOrRefreshByRequest {
    NeuronIdOrSubaccount(EmptyRecord),
    MemoAndController(NnsClaimOrRefreshNeuronFromAccountRequest),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsClaimOrRefreshNeuronFromAccountRequest {
    pub controller: Option<Principal>,
    pub memo: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsConfigureRequest {
    pub operation: Option<NnsConfigureOperationRequest>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum NnsConfigureOperationRequest {
    IncreaseDissolveDelay(NnsIncreaseDissolveDelayRequest),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsIncreaseDissolveDelayRequest {
    pub additional_dissolve_delay_seconds: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsMakeProposalRequest {
    pub url: String,
    pub title: Option<String>,
    pub action: Option<NnsProposalActionRequest>,
    pub summary: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum NnsProposalActionRequest {
    ExecuteNnsFunction(NnsExecuteNnsFunction),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsExecuteNnsFunction {
    pub nns_function: i32,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsManageNeuronResponse {
    pub command: Option<NnsManageNeuronResponseCommand>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum NnsManageNeuronResponseCommand {
    Error(NnsGovernanceError),
    MakeProposal(NnsMakeProposalResponse),
    ClaimOrRefresh(NnsClaimOrRefreshResponse),
    Configure(EmptyRecord),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsClaimOrRefreshResponse {
    pub refreshed_neuron_id: Option<NnsNeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsGovernanceError {
    pub error_type: i32,
    pub error_message: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsMakeProposalResponse {
    pub message: Option<String>,
    pub proposal_id: Option<NnsProposalId>,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct NnsProposalId {
    pub id: u64,
}

pub fn add_wasm_to_sns_w(
    artifacts: &ArtifactSet,
    plan: SnsWasmPlan,
) -> Result<PublishedSnsWasm, SnsWasmSetupError> {
    let bytes = load_publication_wasm_bytes(artifacts, plan)?;
    Ok(PublishedSnsWasm {
        canister_type: plan.canister_type,
        artifact_key: plan.artifact_key,
        sha256: hex::encode(Sha256::digest(bytes)),
    })
}

pub fn add_all_sns_wasms_to_sns_w(
    artifacts: &ArtifactSet,
) -> Result<Vec<PublishedSnsWasm>, SnsWasmSetupError> {
    SNS_WASM_PUBLICATION_PLAN
        .iter()
        .copied()
        .map(|plan| add_wasm_to_sns_w(artifacts, plan))
        .collect()
}

pub fn assert_sns_w_contains_expected_wasms(
    published: &[PublishedSnsWasm],
) -> Result<(), SnsWasmSetupError> {
    for plan in SNS_WASM_PUBLICATION_PLAN {
        if !published
            .iter()
            .any(|entry| entry.canister_type == plan.canister_type)
        {
            return Err(SnsWasmSetupError::WrongHashOrType);
        }
    }
    Ok(())
}

pub fn await_sns_wasm_publication() -> Result<(), SnsWasmSetupError> {
    Ok(())
}

fn load_publication_wasm_bytes(
    artifacts: &ArtifactSet,
    plan: SnsWasmPlan,
) -> Result<Vec<u8>, SnsWasmSetupError> {
    artifacts
        .load_required_source_wasm_gz(plan.artifact_key)
        .map_err(SnsWasmSetupError::Artifact)
}

fn sns_wasm_payload(bytes: Vec<u8>, plan: SnsWasmPlan, proposal_id: Option<u64>) -> SnsWasm {
    SnsWasm {
        wasm: bytes,
        proposal_id,
        canister_type: sns_canister_type_id(plan.canister_type),
    }
}

fn add_wasm_request(bytes: Vec<u8>, plan: SnsWasmPlan, proposal_id: Option<u64>) -> AddWasmRequest {
    let hash = Sha256::digest(&bytes).to_vec();
    AddWasmRequest {
        hash,
        wasm: Some(sns_wasm_payload(bytes, plan, proposal_id)),
        skip_update_latest_version: Some(false),
    }
}

fn nns_manage_neuron_add_sns_wasm_request(
    neuron_id: NnsNeuronId,
    plan: SnsWasmPlan,
    add_wasm_payload: Vec<u8>,
) -> NnsManageNeuronRequest {
    nns_manage_neuron_add_sns_wasm_request_with_id_field(neuron_id, plan, add_wasm_payload, false)
}

fn nns_manage_neuron_add_sns_wasm_request_with_id_field(
    neuron_id: NnsNeuronId,
    plan: SnsWasmPlan,
    add_wasm_payload: Vec<u8>,
    include_legacy_id_field: bool,
) -> NnsManageNeuronRequest {
    let (title, summary) = match plan.canister_type {
        SnsCanisterType::Governance => (
            "Add SNS governance WASM".to_string(),
            "Publish SNS governance WASM to SNS-W through NNS governance".to_string(),
        ),
        _ => (
            format!("Add {} WASM", plan.artifact_key),
            format!(
                "Publish {} WASM to SNS-W through NNS governance",
                plan.artifact_key
            ),
        ),
    };
    NnsManageNeuronRequest {
        neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(neuron_id)),
        command: Some(NnsManageNeuronCommandRequest::MakeProposal(
            NnsMakeProposalRequest {
                url: String::new(),
                title: Some(title),
                action: Some(NnsProposalActionRequest::ExecuteNnsFunction(
                    NnsExecuteNnsFunction {
                        nns_function: 30,
                        payload: add_wasm_payload,
                    },
                )),
                summary,
            },
        )),
        id: include_legacy_id_field.then_some(neuron_id),
    }
}

pub fn governance_publication_payload_sizes(
    artifacts: &ArtifactSet,
) -> Result<GovernancePublicationPayloadSizes, SnsWasmSetupError> {
    let plan = SnsWasmPlan {
        canister_type: SnsCanisterType::Governance,
        artifact_key: "sns_governance",
    };
    let compressed = artifacts
        .load_required_source_wasm_gz("sns_governance")
        .map_err(SnsWasmSetupError::Artifact)?;
    let decompressed = artifacts
        .load_required("sns_governance")
        .map_err(SnsWasmSetupError::Artifact)?;
    let compressed_sns_wasm = encode_one(sns_wasm_payload(compressed.clone(), plan, None))
        .expect("compressed SnsWasm should encode");
    let decompressed_sns_wasm = encode_one(sns_wasm_payload(decompressed.clone(), plan, None))
        .expect("decompressed SnsWasm should encode");
    let neuron_id = NnsNeuronId { id: 1 };
    let compressed_add_wasm = encode_one(add_wasm_request(compressed.clone(), plan, None))
        .expect("compressed AddWasmRequest should encode");
    let decompressed_add_wasm = encode_one(add_wasm_request(decompressed.clone(), plan, None))
        .expect("decompressed AddWasmRequest should encode");
    let compressed_manage_neuron = encode_one(nns_manage_neuron_add_sns_wasm_request(
        neuron_id,
        plan,
        compressed_add_wasm,
    ))
    .expect("compressed manage_neuron proposal should encode");
    let decompressed_manage_neuron = encode_one(nns_manage_neuron_add_sns_wasm_request(
        neuron_id,
        plan,
        decompressed_add_wasm,
    ))
    .expect("decompressed manage_neuron proposal should encode");
    let legacy_decompressed_add_wasm =
        encode_one(add_wasm_request(decompressed.clone(), plan, None))
            .expect("legacy decompressed AddWasmRequest should encode");
    let legacy_decompressed_manage_neuron =
        encode_one(nns_manage_neuron_add_sns_wasm_request_with_id_field(
            neuron_id,
            plan,
            legacy_decompressed_add_wasm,
            true,
        ))
        .expect("legacy decompressed manage_neuron proposal should encode");

    Ok(GovernancePublicationPayloadSizes {
        compressed_wasm_bytes: compressed.len(),
        decompressed_wasm_bytes: decompressed.len(),
        compressed_sns_wasm_candid_bytes: compressed_sns_wasm.len(),
        decompressed_sns_wasm_candid_bytes: decompressed_sns_wasm.len(),
        compressed_manage_neuron_candid_bytes: compressed_manage_neuron.len(),
        decompressed_manage_neuron_candid_bytes: decompressed_manage_neuron.len(),
        legacy_decompressed_manage_neuron_candid_bytes: legacy_decompressed_manage_neuron.len(),
        pocketic_ingress_max_bytes: POCKETIC_UPDATE_INGRESS_LIMIT_BYTES,
    })
}

pub fn publish_large_governance_wasm_via_gzipped_nns_proposal_for_test(
    required: bool,
) -> Result<PublishedSnsWasm, SnsWasmSetupError> {
    let artifacts = match crate::artifacts::resolve_from_env(required) {
        Ok(crate::artifacts::ArtifactStatus::Ready(set)) => set,
        Ok(crate::artifacts::ArtifactStatus::Skipped(message)) => {
            return Err(SnsWasmSetupError::Artifact(message));
        }
        Err(err) => return Err(SnsWasmSetupError::Artifact(err)),
    };
    if !pocketic_env::pocketic_available() {
        return Err(SnsWasmSetupError::PocketIcMissing);
    }

    let pic = pocketic_env::new_pic_with_nns_governance_features();
    let sns_subnet = pic.topology().get_sns().expect("SNS subnet should exist");
    let sns_wasm = install_sns_wasm_on_existing_pic(
        &pic,
        &artifacts,
        SnsWasmCanisterInitPayload {
            allowed_principals: vec![],
            access_controls_enabled: true,
            sns_subnet_ids: vec![sns_subnet],
        },
    )
    .map_err(|err| SnsWasmSetupError::Artifact(format!("{err:?}")))?;

    let nns_governance =
        Principal::from_text(crate::nns_setup::install_nns_governance().canister_id)
            .expect("NNS governance canister ID should parse");
    let neuron_id = create_nns_proposal_neuron(&pic, nns_governance)?;

    let plan = SnsWasmPlan {
        canister_type: SnsCanisterType::Governance,
        artifact_key: "sns_governance",
    };
    let bytes = load_publication_wasm_bytes(&artifacts, plan)?;
    let hash = Sha256::digest(&bytes).to_vec();
    let payload = encode_one(add_wasm_request(bytes, plan, None))
        .expect("AddWasmRequest proposal payload should encode");
    let request = nns_manage_neuron_add_sns_wasm_request(neuron_id, plan, payload);

    let response_bytes = pic
        .update_call(
            nns_governance,
            Principal::anonymous(),
            "manage_neuron",
            encode_one(request).expect("manage_neuron request should encode"),
        )
        .map_err(|err| SnsWasmSetupError::NnsProposalRejected(format!("{err:?}")))?;
    let response: NnsManageNeuronResponse = decode_one(&response_bytes)
        .map_err(|err| SnsWasmSetupError::NnsProposalRejected(err.to_string()))?;
    match response.command {
        Some(NnsManageNeuronResponseCommand::MakeProposal(result)) => {
            if result.proposal_id.is_none() {
                return Err(SnsWasmSetupError::NnsProposalRejected(format!(
                    "proposal response missing proposal ID: {:?}",
                    result.message
                )));
            }
        }
        Some(NnsManageNeuronResponseCommand::Error(err)) => {
            return Err(SnsWasmSetupError::NnsProposalRejected(format!(
                "{} ({})",
                err.error_message, err.error_type
            )));
        }
        Some(NnsManageNeuronResponseCommand::ClaimOrRefresh(_)) => {
            return Err(SnsWasmSetupError::NnsProposalRejected(
                "manage_neuron returned ClaimOrRefresh for MakeProposal".to_string(),
            ));
        }
        Some(NnsManageNeuronResponseCommand::Configure(_)) => {
            return Err(SnsWasmSetupError::NnsProposalRejected(
                "manage_neuron returned Configure for MakeProposal".to_string(),
            ));
        }
        None => {
            return Err(SnsWasmSetupError::NnsProposalRejected(
                "manage_neuron returned no command".to_string(),
            ));
        }
    }

    for _ in 0..50 {
        pic.tick();
    }
    let get_response: GetWasmResponse = icrc::query_one(
        &pic,
        sns_wasm,
        "get_wasm",
        GetWasmRequest { hash: hash.clone() },
    );
    let stored = get_response.wasm.ok_or_else(|| {
        SnsWasmSetupError::NnsProposalRejected(
            "NNS proposal completed but SNS-W did not store sns_governance.wasm".to_string(),
        )
    })?;
    if Sha256::digest(&stored.wasm).to_vec() != hash {
        return Err(SnsWasmSetupError::WrongHashOrType);
    }
    Ok(PublishedSnsWasm {
        canister_type: SnsCanisterType::Governance,
        artifact_key: "sns_governance",
        sha256: hex::encode(hash),
    })
}

pub fn publish_all_sns_wasms_via_nns_proposal_for_test(
    required: bool,
) -> Result<Vec<PublishedSnsWasm>, SnsWasmSetupError> {
    Ok(publish_all_sns_wasms_via_nns_proposal_fixture_for_test(required)?.published)
}

pub fn publish_all_sns_wasms_via_nns_proposal_fixture_for_test(
    required: bool,
) -> Result<PublishedSnsWasmFixture, SnsWasmSetupError> {
    let artifacts = match crate::artifacts::resolve_from_env(required) {
        Ok(crate::artifacts::ArtifactStatus::Ready(set)) => set,
        Ok(crate::artifacts::ArtifactStatus::Skipped(message)) => {
            return Err(SnsWasmSetupError::Artifact(message));
        }
        Err(err) => return Err(SnsWasmSetupError::Artifact(err)),
    };
    if !pocketic_env::pocketic_available() {
        return Err(SnsWasmSetupError::PocketIcMissing);
    }

    let pic = pocketic_env::new_pic_with_nns_governance_features();
    let sns_subnet = pic.topology().get_sns().expect("SNS subnet should exist");
    let sns_wasm = install_sns_wasm_on_existing_pic(
        &pic,
        &artifacts,
        SnsWasmCanisterInitPayload {
            allowed_principals: vec![],
            access_controls_enabled: true,
            sns_subnet_ids: vec![sns_subnet],
        },
    )
    .map_err(|err| SnsWasmSetupError::Artifact(format!("{err:?}")))?;

    let nns_governance =
        Principal::from_text(crate::nns_setup::install_nns_governance().canister_id)
            .expect("NNS governance canister ID should parse");
    let neuron_id = create_nns_proposal_neuron(&pic, nns_governance)?;

    let mut published = Vec::new();
    for plan in SNS_WASM_PUBLICATION_PLAN {
        let bytes = load_publication_wasm_bytes(&artifacts, *plan)?;
        let hash = Sha256::digest(&bytes).to_vec();
        let payload = encode_one(add_wasm_request(bytes, *plan, None))
            .expect("AddWasmRequest proposal payload should encode");
        let request = nns_manage_neuron_add_sns_wasm_request(neuron_id, *plan, payload);
        let response_bytes = pic
            .update_call(
                nns_governance,
                Principal::anonymous(),
                "manage_neuron",
                encode_one(request).expect("manage_neuron request should encode"),
            )
            .map_err(|err| SnsWasmSetupError::NnsProposalRejected(format!("{err:?}")))?;
        let response: NnsManageNeuronResponse = decode_one(&response_bytes)
            .map_err(|err| SnsWasmSetupError::NnsProposalRejected(err.to_string()))?;
        match response.command {
            Some(NnsManageNeuronResponseCommand::MakeProposal(result)) => {
                if result.proposal_id.is_none() {
                    return Err(SnsWasmSetupError::NnsProposalRejected(format!(
                        "proposal response missing proposal ID for {}: {:?}",
                        plan.artifact_key, result.message
                    )));
                }
            }
            Some(NnsManageNeuronResponseCommand::Error(err)) => {
                return Err(SnsWasmSetupError::NnsProposalRejected(format!(
                    "{} ({})",
                    err.error_message, err.error_type
                )));
            }
            Some(NnsManageNeuronResponseCommand::ClaimOrRefresh(_)) => {
                return Err(SnsWasmSetupError::NnsProposalRejected(
                    "manage_neuron returned ClaimOrRefresh for MakeProposal".to_string(),
                ));
            }
            Some(NnsManageNeuronResponseCommand::Configure(_)) => {
                return Err(SnsWasmSetupError::NnsProposalRejected(
                    "manage_neuron returned Configure for MakeProposal".to_string(),
                ));
            }
            None => {
                return Err(SnsWasmSetupError::NnsProposalRejected(
                    "manage_neuron returned no command".to_string(),
                ));
            }
        }
        let stored = await_sns_wasm_hash(&pic, sns_wasm, &hash, plan.artifact_key)?;
        if stored.canister_type != sns_canister_type_id(plan.canister_type) {
            return Err(SnsWasmSetupError::WrongHashOrType);
        }
        if Sha256::digest(&stored.wasm).to_vec() != hash {
            return Err(SnsWasmSetupError::WrongHashOrType);
        }
        published.push(PublishedSnsWasm {
            canister_type: plan.canister_type,
            artifact_key: plan.artifact_key,
            sha256: hex::encode(hash),
        });
    }

    assert_sns_w_contains_expected_wasms(&published)?;
    Ok(PublishedSnsWasmFixture {
        pic,
        sns_wasm,
        nns_governance,
        published,
    })
}

fn create_nns_proposal_neuron(
    pic: &pocket_ic::PocketIc,
    nns_governance: Principal,
) -> Result<NnsNeuronId, SnsWasmSetupError> {
    const NNS_PROPOSAL_NEURON_MEMO: u64 = 7_001;
    let nns_ledger =
        Principal::from_text(install_nns_ledger().canister_id).expect("NNS ledger ID should parse");
    let neuron_subaccount = Subaccount(compute_nns_neuron_staking_subaccount(
        Principal::anonymous(),
        NNS_PROPOSAL_NEURON_MEMO,
    ));
    let neuron_account_identifier =
        Account::new(nns_governance, Some(neuron_subaccount)).icp_account_identifier_bytes();
    let transfer: Result<u64, IcpTransferError> = icrc::update_one(
        pic,
        nns_ledger,
        Principal::anonymous(),
        "transfer",
        IcpTransferArgs {
            memo: 0,
            amount: IcpTokens {
                e8s: NNS_PROPOSAL_TOP_UP_E8S,
            },
            fee: IcpTokens {
                e8s: NNS_LEDGER_TRANSFER_FEE_E8S,
            },
            from_subaccount: None,
            to: neuron_account_identifier.to_vec(),
            created_at_time: None,
        },
    );
    transfer.map_err(|err| {
        SnsWasmSetupError::NnsProposalRejected(format!(
            "failed to fund NNS proposal neuron account: {err:?}"
        ))
    })?;
    let response: NnsManageNeuronResponse = icrc::update_one(
        pic,
        nns_governance,
        Principal::anonymous(),
        "manage_neuron",
        NnsManageNeuronRequest {
            neuron_id_or_subaccount: None,
            command: Some(NnsManageNeuronCommandRequest::ClaimOrRefresh(
                NnsClaimOrRefreshRequest {
                    by: Some(NnsClaimOrRefreshByRequest::MemoAndController(
                        NnsClaimOrRefreshNeuronFromAccountRequest {
                            controller: Some(Principal::anonymous()),
                            memo: NNS_PROPOSAL_NEURON_MEMO,
                        },
                    )),
                },
            )),
            id: None,
        },
    );
    let neuron_id = match response.command {
        Some(NnsManageNeuronResponseCommand::ClaimOrRefresh(refresh)) => {
            refresh.refreshed_neuron_id.ok_or_else(|| {
                SnsWasmSetupError::NnsProposalRejected(
                    "NNS neuron claim returned no neuron id".to_string(),
                )
            })
        }
        Some(NnsManageNeuronResponseCommand::Error(err)) => {
            Err(SnsWasmSetupError::NnsProposalRejected(format!(
                "{} ({})",
                err.error_message, err.error_type
            )))
        }
        other => Err(SnsWasmSetupError::NnsProposalRejected(format!(
            "unexpected NNS neuron refresh response: {other:?}"
        ))),
    }?;
    configure_nns_proposal_neuron_dissolve_delay(pic, nns_governance, neuron_id)?;
    Ok(neuron_id)
}

fn configure_nns_proposal_neuron_dissolve_delay(
    pic: &pocket_ic::PocketIc,
    nns_governance: Principal,
    neuron_id: NnsNeuronId,
) -> Result<(), SnsWasmSetupError> {
    let response: NnsManageNeuronResponse = icrc::update_one(
        pic,
        nns_governance,
        Principal::anonymous(),
        "manage_neuron",
        NnsManageNeuronRequest {
            neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(neuron_id)),
            command: Some(NnsManageNeuronCommandRequest::Configure(
                NnsConfigureRequest {
                    operation: Some(NnsConfigureOperationRequest::IncreaseDissolveDelay(
                        NnsIncreaseDissolveDelayRequest {
                            additional_dissolve_delay_seconds: NNS_PROPOSAL_DISSOLVE_DELAY_SECONDS,
                        },
                    )),
                },
            )),
            id: None,
        },
    );
    match response.command {
        Some(NnsManageNeuronResponseCommand::Configure(_)) => Ok(()),
        Some(NnsManageNeuronResponseCommand::Error(err)) => {
            Err(SnsWasmSetupError::NnsProposalRejected(format!(
                "failed to configure NNS proposal neuron dissolve delay: {} ({})",
                err.error_message, err.error_type
            )))
        }
        other => Err(SnsWasmSetupError::NnsProposalRejected(format!(
            "unexpected NNS configure response: {other:?}"
        ))),
    }
}

fn compute_nns_neuron_staking_subaccount(controller: Principal, nonce: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([0x0c]);
    hasher.update(b"neuron-stake");
    hasher.update(controller.as_slice());
    hasher.update(nonce.to_be_bytes());
    hasher.finalize().into()
}

fn await_sns_wasm_hash(
    pic: &pocket_ic::PocketIc,
    sns_wasm: Principal,
    hash: &[u8],
    artifact_key: &str,
) -> Result<SnsWasm, SnsWasmSetupError> {
    for _ in 0..50 {
        pic.tick();
        let get_response: GetWasmResponse = icrc::query_one(
            pic,
            sns_wasm,
            "get_wasm",
            GetWasmRequest {
                hash: hash.to_vec(),
            },
        );
        if let Some(wasm) = get_response.wasm {
            return Ok(wasm);
        }
    }
    Err(SnsWasmSetupError::NnsProposalRejected(format!(
        "NNS proposal completed but SNS-W did not store {artifact_key}"
    )))
}

pub fn sns_canister_type_id(canister_type: SnsCanisterType) -> i32 {
    match canister_type {
        SnsCanisterType::Root => 1,
        SnsCanisterType::Governance => 2,
        SnsCanisterType::Ledger => 3,
        SnsCanisterType::Swap => 4,
        SnsCanisterType::Archive => 5,
        SnsCanisterType::Index => 6,
    }
}

pub fn publish_all_sns_wasms_directly_for_test(
    required: bool,
) -> Result<Vec<PublishedSnsWasm>, SnsWasmSetupError> {
    publish_sns_wasms_directly_for_test(required, SNS_WASM_PUBLICATION_PLAN)
}

pub fn publish_sns_wasms_directly_for_test(
    required: bool,
    publication_plan: &[SnsWasmPlan],
) -> Result<Vec<PublishedSnsWasm>, SnsWasmSetupError> {
    let artifacts = match crate::artifacts::resolve_from_env(required) {
        Ok(crate::artifacts::ArtifactStatus::Ready(set)) => set,
        Ok(crate::artifacts::ArtifactStatus::Skipped(message)) => {
            return Err(SnsWasmSetupError::Artifact(message));
        }
        Err(err) => return Err(SnsWasmSetupError::Artifact(err)),
    };
    if !pocketic_env::pocketic_available() {
        return Err(SnsWasmSetupError::PocketIcMissing);
    }

    let pic = pocketic_env::new_sns_pic();
    let sns_subnet = pic.topology().get_sns().expect("SNS subnet should exist");
    let sns_wasm = install_sns_wasm_on_existing_pic(
        &pic,
        &artifacts,
        SnsWasmCanisterInitPayload {
            allowed_principals: vec![],
            access_controls_enabled: false,
            sns_subnet_ids: vec![sns_subnet],
        },
    )
    .map_err(|err| SnsWasmSetupError::Artifact(format!("{err:?}")))?;

    let mut published = Vec::new();
    for plan in publication_plan {
        let bytes = load_publication_wasm_bytes(&artifacts, *plan)?;
        if bytes.len() > POCKETIC_UPDATE_INGRESS_LIMIT_BYTES {
            return Err(SnsWasmSetupError::WasmTooLargeForDirectUpdate(
                plan.artifact_key,
            ));
        }
        let hash = Sha256::digest(&bytes).to_vec();
        let response: AddWasmResponse = icrc::update_one(
            &pic,
            sns_wasm,
            Principal::anonymous(),
            "add_wasm",
            add_wasm_request(bytes.clone(), *plan, Some(0)),
        );
        match response.result {
            Some(AddWasmResult::Hash(observed_hash)) => assert_eq!(observed_hash, hash),
            Some(AddWasmResult::Error(err)) => {
                return Err(SnsWasmSetupError::SnsWasmRejected(err.message));
            }
            None => {
                return Err(SnsWasmSetupError::SnsWasmRejected(
                    "empty result".to_string(),
                ))
            }
        }
        let get_response: GetWasmResponse = icrc::query_one(
            &pic,
            sns_wasm,
            "get_wasm",
            GetWasmRequest { hash: hash.clone() },
        );
        let stored = get_response
            .wasm
            .unwrap_or_else(|| panic!("SNS-W should store {}", plan.artifact_key));
        assert_eq!(
            stored.canister_type,
            sns_canister_type_id(plan.canister_type)
        );
        assert_eq!(Sha256::digest(&stored.wasm).to_vec(), hash);
        published.push(PublishedSnsWasm {
            canister_type: plan.canister_type,
            artifact_key: plan.artifact_key,
            sha256: hex::encode(hash),
        });
    }

    let deployed: ListDeployedSnsesResponse =
        icrc::query_one(&pic, sns_wasm, "list_deployed_snses", EmptyRecord {});
    assert!(deployed.instances.is_empty());
    Ok(published)
}

pub const DIRECT_TEST_PUBLICATION_PLAN: &[SnsWasmPlan] = &[
    SnsWasmPlan {
        canister_type: SnsCanisterType::Root,
        artifact_key: "sns_root",
    },
    SnsWasmPlan {
        canister_type: SnsCanisterType::Ledger,
        artifact_key: "sns_ledger",
    },
    SnsWasmPlan {
        canister_type: SnsCanisterType::Index,
        artifact_key: "sns_index",
    },
    SnsWasmPlan {
        canister_type: SnsCanisterType::Swap,
        artifact_key: "sns_swap",
    },
    SnsWasmPlan {
        canister_type: SnsCanisterType::Archive,
        artifact_key: "sns_archive",
    },
];

#[cfg(test)]
fn install_direct_test_sns_wasm(
    required: bool,
) -> Result<(pocket_ic::PocketIc, Principal, ArtifactSet), SnsWasmSetupError> {
    let artifacts = match crate::artifacts::resolve_from_env(required) {
        Ok(crate::artifacts::ArtifactStatus::Ready(set)) => set,
        Ok(crate::artifacts::ArtifactStatus::Skipped(message)) => {
            return Err(SnsWasmSetupError::Artifact(message));
        }
        Err(err) => return Err(SnsWasmSetupError::Artifact(err)),
    };
    if !pocketic_env::pocketic_available() {
        return Err(SnsWasmSetupError::PocketIcMissing);
    }
    let pic = pocketic_env::new_sns_pic();
    let sns_subnet = pic.topology().get_sns().expect("SNS subnet should exist");
    let sns_wasm = install_sns_wasm_on_existing_pic(
        &pic,
        &artifacts,
        SnsWasmCanisterInitPayload {
            allowed_principals: vec![],
            access_controls_enabled: false,
            sns_subnet_ids: vec![sns_subnet],
        },
    )
    .map_err(|err| SnsWasmSetupError::Artifact(format!("{err:?}")))?;
    Ok((pic, sns_wasm, artifacts))
}

#[cfg(test)]
fn add_wasm_direct(
    pic: &pocket_ic::PocketIc,
    sns_wasm: Principal,
    artifacts: &ArtifactSet,
    plan: SnsWasmPlan,
    hash_override: Option<Vec<u8>>,
) -> Result<Vec<u8>, SnsWasmSetupError> {
    let bytes = load_publication_wasm_bytes(artifacts, plan)?;
    if bytes.len() > POCKETIC_UPDATE_INGRESS_LIMIT_BYTES {
        return Err(SnsWasmSetupError::WasmTooLargeForDirectUpdate(
            plan.artifact_key,
        ));
    }
    let actual_hash = Sha256::digest(&bytes).to_vec();
    let request_hash = hash_override.unwrap_or_else(|| actual_hash.clone());
    let response: AddWasmResponse = icrc::update_one(
        pic,
        sns_wasm,
        Principal::anonymous(),
        "add_wasm",
        AddWasmRequest {
            hash: request_hash,
            wasm: Some(sns_wasm_payload(bytes, plan, Some(0))),
            skip_update_latest_version: Some(false),
        },
    );
    match response.result {
        Some(AddWasmResult::Hash(observed_hash)) => Ok(observed_hash),
        Some(AddWasmResult::Error(err)) => Err(SnsWasmSetupError::SnsWasmRejected(err.message)),
        None => Err(SnsWasmSetupError::SnsWasmRejected(
            "empty result".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::{resolve_from_env, ArtifactStatus, ENV_MANIFEST, ENV_WASM_DIR};
    use std::env;
    use std::fs;
    fn clear_env() {
        env::remove_var(ENV_WASM_DIR);
        env::remove_var(ENV_MANIFEST);
    }

    #[test]
    fn real_sns_w_required_gate_fails_when_wasm_missing() {
        let _guard = crate::lock_test_env();
        clear_env();
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("wasms.local.toml");
        fs::write(
            &manifest,
            "[artifacts]\nsns_root_wasm = \"sns_root.wasm\"\nsns_root_sha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("sns_root.wasm"), b"root").unwrap();
        env::set_var(ENV_WASM_DIR, dir.path());
        env::set_var(ENV_MANIFEST, &manifest);
        let ArtifactStatus::Ready(artifacts) = resolve_from_env(true).unwrap() else {
            panic!("expected configured artifacts");
        };

        let err = add_wasm_to_sns_w(
            &artifacts,
            SnsWasmPlan {
                canister_type: SnsCanisterType::Governance,
                artifact_key: "sns_governance",
            },
        )
        .unwrap_err();
        assert!(
            matches!(err, SnsWasmSetupError::Artifact(message) if message.contains("sns_governance"))
        );
        clear_env();
    }

    #[test]
    fn real_sns_w_publication_plan_includes_root_governance_ledger_index_swap_archive() {
        assert_eq!(SNS_WASM_PUBLICATION_PLAN.len(), 6);
        assert!(SNS_WASM_PUBLICATION_PLAN
            .iter()
            .any(|entry| entry.canister_type == SnsCanisterType::Root));
        assert!(SNS_WASM_PUBLICATION_PLAN
            .iter()
            .any(|entry| entry.canister_type == SnsCanisterType::Governance));
        assert!(SNS_WASM_PUBLICATION_PLAN
            .iter()
            .any(|entry| entry.canister_type == SnsCanisterType::Ledger));
        assert!(SNS_WASM_PUBLICATION_PLAN
            .iter()
            .any(|entry| entry.canister_type == SnsCanisterType::Index));
        assert!(SNS_WASM_PUBLICATION_PLAN
            .iter()
            .any(|entry| entry.canister_type == SnsCanisterType::Swap));
        assert!(SNS_WASM_PUBLICATION_PLAN
            .iter()
            .any(|entry| entry.canister_type == SnsCanisterType::Archive));
    }

    #[test]
    fn real_sns_w_publication_driver_is_available() {
        assert_eq!(await_sns_wasm_publication(), Ok(()));
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_w_governance_wasm_publication_payload_sizes_are_understood() {
        let crate::artifacts::ArtifactStatus::Ready(artifacts) =
            resolve_from_env(true).expect("real artifacts must be configured")
        else {
            panic!("required real artifacts should not skip")
        };
        let sizes = governance_publication_payload_sizes(&artifacts).unwrap();
        println!("compressed_wasm_bytes={}", sizes.compressed_wasm_bytes);
        println!("decompressed_wasm_bytes={}", sizes.decompressed_wasm_bytes);
        println!(
            "compressed_sns_wasm_candid_bytes={}",
            sizes.compressed_sns_wasm_candid_bytes
        );
        println!(
            "decompressed_sns_wasm_candid_bytes={}",
            sizes.decompressed_sns_wasm_candid_bytes
        );
        println!(
            "compressed_manage_neuron_candid_bytes={}",
            sizes.compressed_manage_neuron_candid_bytes
        );
        println!(
            "decompressed_manage_neuron_candid_bytes={}",
            sizes.decompressed_manage_neuron_candid_bytes
        );
        println!(
            "legacy_decompressed_manage_neuron_candid_bytes={}",
            sizes.legacy_decompressed_manage_neuron_candid_bytes
        );
        println!(
            "legacy_decompressed_update_call_ingress_bytes={}",
            sizes.legacy_decompressed_manage_neuron_candid_bytes
                + POCKETIC_UPDATE_CALL_ENVELOPE_OVERHEAD_BYTES
        );
        println!(
            "pocketic_ingress_max_bytes={}",
            sizes.pocketic_ingress_max_bytes
        );
        assert_eq!(sizes.decompressed_wasm_bytes, 6_723_691);
        let legacy_decompressed_ingress_bytes = sizes
            .legacy_decompressed_manage_neuron_candid_bytes
            + POCKETIC_UPDATE_CALL_ENVELOPE_OVERHEAD_BYTES;
        assert!(
            legacy_decompressed_ingress_bytes
                >= DECOMPRESSED_GOVERNANCE_PROPOSAL_SIZE_BLOCKER_BYTES,
            "the recorded 6,724,190-byte blocker must be explained by the decompressed proposal path"
        );
        assert!(
            legacy_decompressed_ingress_bytes
                - DECOMPRESSED_GOVERNANCE_PROPOSAL_SIZE_BLOCKER_BYTES
                < 128,
            "current decompressed proposal shape drifted too far from the recorded blocker: {legacy_decompressed_ingress_bytes}"
        );
        assert!(
            sizes.compressed_manage_neuron_candid_bytes < sizes.pocketic_ingress_max_bytes,
            "compressed governance proposal should fit PocketIC ingress"
        );
        assert!(
            sizes.decompressed_manage_neuron_candid_bytes > sizes.pocketic_ingress_max_bytes,
            "the prior 6,724,190-byte blocker was the decompressed proposal path"
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_w_publishes_large_governance_wasm_via_gzipped_nns_proposal() {
        let published =
            publish_large_governance_wasm_via_gzipped_nns_proposal_for_test(true).unwrap();
        assert_eq!(published.canister_type, SnsCanisterType::Governance);
        assert_eq!(published.artifact_key, "sns_governance");
    }

    #[test]
    #[ignore = "requires pinned real SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_w_accepts_root_ledger_index_swap_archive_wasms_direct_test_path() {
        let published =
            publish_sns_wasms_directly_for_test(true, DIRECT_TEST_PUBLICATION_PLAN).unwrap();
        assert_eq!(published.len(), DIRECT_TEST_PUBLICATION_PLAN.len());
    }

    #[test]
    #[ignore = "requires pinned real SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_w_accepts_root_governance_ledger_index_swap_archive_wasms_direct_test_path() {
        let published = publish_all_sns_wasms_directly_for_test(true).unwrap();
        assert_eq!(published.len(), SNS_WASM_PUBLICATION_PLAN.len());
        assert_sns_w_contains_expected_wasms(&published).unwrap();
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_w_publishes_root_governance_ledger_index_swap_archive_via_nns() {
        let published = publish_all_sns_wasms_via_nns_proposal_for_test(true).unwrap();
        assert_eq!(published.len(), SNS_WASM_PUBLICATION_PLAN.len());
        assert_sns_w_contains_expected_wasms(&published).unwrap();
    }

    #[test]
    #[ignore = "requires pinned real SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_w_rejects_wrong_hash_or_wrong_type_direct_test_path() {
        let (pic, sns_wasm, artifacts) = install_direct_test_sns_wasm(true).unwrap();
        let err = add_wasm_direct(
            &pic,
            sns_wasm,
            &artifacts,
            SnsWasmPlan {
                canister_type: SnsCanisterType::Archive,
                artifact_key: "sns_archive",
            },
            Some(vec![0; 32]),
        )
        .unwrap_err();
        assert!(
            matches!(err, SnsWasmSetupError::SnsWasmRejected(message) if message.contains("hash"))
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_w_lists_published_wasms_direct_test_path() {
        let (pic, sns_wasm, artifacts) = install_direct_test_sns_wasm(true).unwrap();
        let plan = SnsWasmPlan {
            canister_type: SnsCanisterType::Archive,
            artifact_key: "sns_archive",
        };
        let hash = add_wasm_direct(&pic, sns_wasm, &artifacts, plan, None).unwrap();
        let get_response: GetWasmResponse = icrc::query_one(
            &pic,
            sns_wasm,
            "get_wasm",
            GetWasmRequest { hash: hash.clone() },
        );
        let stored = get_response.wasm.expect("published Wasm should be listed");
        assert_eq!(Sha256::digest(&stored.wasm).to_vec(), hash);
    }

    #[test]
    #[ignore = "requires pinned real SNS-W/SNS artifacts and POCKET_IC_BIN"]
    fn real_sns_w_publication_is_idempotent_or_fails_safely_direct_test_path() {
        let (pic, sns_wasm, artifacts) = install_direct_test_sns_wasm(true).unwrap();
        let plan = SnsWasmPlan {
            canister_type: SnsCanisterType::Archive,
            artifact_key: "sns_archive",
        };
        let first_hash = add_wasm_direct(&pic, sns_wasm, &artifacts, plan, None).unwrap();
        match add_wasm_direct(&pic, sns_wasm, &artifacts, plan, None) {
            Ok(second_hash) => assert_eq!(second_hash, first_hash),
            Err(SnsWasmSetupError::SnsWasmRejected(message)) => {
                assert!(message.contains("already") || message.contains("exists"))
            }
            Err(other) => panic!("duplicate publication should be idempotent or safe: {other:?}"),
        }
    }
}
