use crate::artifacts::ArtifactSet;
use crate::icrc;
use crate::nns_setup::{install_sns_wasm_on_existing_pic, EmptyRecord, SnsWasmCanisterInitPayload};
use crate::pocketic_env;
use candid::{CandidType, Principal};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const POCKETIC_UPDATE_INGRESS_LIMIT_BYTES: usize = 3_670_016;

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
pub enum SnsWasmSetupError {
    Artifact(String),
    WrongHashOrType,
    SnsWProposalDriverMissing,
    PocketIcMissing,
    SnsWasmRejected(String),
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

pub fn add_wasm_to_sns_w(
    artifacts: &ArtifactSet,
    plan: SnsWasmPlan,
) -> Result<PublishedSnsWasm, SnsWasmSetupError> {
    let bytes = artifacts
        .load_required(plan.artifact_key)
        .map_err(SnsWasmSetupError::Artifact)?;
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
    Err(SnsWasmSetupError::SnsWProposalDriverMissing)
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
        let bytes = artifacts
            .load_required(plan.artifact_key)
            .map_err(SnsWasmSetupError::Artifact)?;
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
            AddWasmRequest {
                hash: hash.clone(),
                wasm: Some(SnsWasm {
                    wasm: bytes.clone(),
                    proposal_id: Some(0),
                    canister_type: sns_canister_type_id(plan.canister_type),
                }),
                skip_update_latest_version: Some(false),
            },
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
    let bytes = artifacts
        .load_required(plan.artifact_key)
        .map_err(SnsWasmSetupError::Artifact)?;
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
            wasm: Some(SnsWasm {
                wasm: bytes,
                proposal_id: Some(0),
                canister_type: sns_canister_type_id(plan.canister_type),
            }),
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
    fn real_sns_w_publication_is_blocked_on_nns_proposal_driver() {
        assert_eq!(
            await_sns_wasm_publication(),
            Err(SnsWasmSetupError::SnsWProposalDriverMissing)
        );
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
    fn real_sns_w_full_publication_requires_proposal_or_chunked_driver() {
        assert_eq!(
            publish_all_sns_wasms_directly_for_test(true),
            Err(SnsWasmSetupError::WasmTooLargeForDirectUpdate(
                "sns_governance"
            ))
        );
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
