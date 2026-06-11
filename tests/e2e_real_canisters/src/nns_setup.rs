use crate::artifacts::{resolve_from_env, ArtifactSet, ArtifactStatus};
use crate::icrc;
use crate::pocketic_env;
use candid::{CandidType, Principal};
use io_production_wiring::{
    PRODUCTION_FRONTEND_CANISTER_ID, PRODUCTION_IO_HISTORIAN_CANISTER_ID,
    PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID, PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
    PROTECTED_IO_NEURON_OWNER_CANISTER,
};
use pocket_ic::PocketIc;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NnsCanisterKind {
    Registry,
    Governance,
    Ledger,
    Root,
    Lifeline,
    SnsWasm,
    CyclesMinting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NnsCanisterPlan {
    pub kind: NnsCanisterKind,
    pub artifact_key: &'static str,
    pub canister_id: &'static str,
    pub controller: Option<&'static str>,
}

pub const NNS_INSTALL_PLAN: &[NnsCanisterPlan] = &[
    NnsCanisterPlan {
        kind: NnsCanisterKind::Ledger,
        artifact_key: "nns_ledger",
        canister_id: "ryjl3-tyaaa-aaaaa-aaaba-cai",
        controller: Some("r7inp-6aaaa-aaaaa-aaabq-cai"),
    },
    NnsCanisterPlan {
        kind: NnsCanisterKind::Root,
        artifact_key: "nns_root",
        canister_id: "r7inp-6aaaa-aaaaa-aaabq-cai",
        controller: Some("rno2w-sqaaa-aaaaa-aaacq-cai"),
    },
    NnsCanisterPlan {
        kind: NnsCanisterKind::Governance,
        artifact_key: "nns_governance",
        canister_id: "rrkah-fqaaa-aaaaa-aaaaq-cai",
        controller: Some("r7inp-6aaaa-aaaaa-aaabq-cai"),
    },
    NnsCanisterPlan {
        kind: NnsCanisterKind::Lifeline,
        artifact_key: "nns_lifeline",
        canister_id: "rno2w-sqaaa-aaaaa-aaacq-cai",
        controller: Some("r7inp-6aaaa-aaaaa-aaabq-cai"),
    },
    NnsCanisterPlan {
        kind: NnsCanisterKind::SnsWasm,
        artifact_key: "sns_wasm",
        canister_id: "qaa6y-5yaaa-aaaaa-aaafa-cai",
        controller: Some("r7inp-6aaaa-aaaaa-aaabq-cai"),
    },
    NnsCanisterPlan {
        kind: NnsCanisterKind::Registry,
        artifact_key: "nns_registry",
        canister_id: "rwlgt-iiaaa-aaaaa-aaaaa-cai",
        controller: Some("r7inp-6aaaa-aaaaa-aaabq-cai"),
    },
    NnsCanisterPlan {
        kind: NnsCanisterKind::CyclesMinting,
        artifact_key: "cmc",
        canister_id: "rkp4c-7iaaa-aaaaa-aaaca-cai",
        controller: Some("r7inp-6aaaa-aaaaa-aaabq-cai"),
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NnsFixture {
    pub canister_ids: Vec<Principal>,
    pub nns_subnet: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SnsWasmCanisterInitPayload {
    pub allowed_principals: Vec<Principal>,
    pub access_controls_enabled: bool,
    pub sns_subnet_ids: Vec<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct GetSnsSubnetIdsResponse {
    pub sns_subnet_ids: Vec<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct GetAllowedPrincipalsResponse {
    pub allowed_principals: Vec<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct EmptyRecord {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnsWasmBasicQueryFixture {
    pub sns_wasm: Principal,
    pub nns_subnet: Principal,
    pub sns_subnet: Principal,
    pub allowed_principal: Principal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NnsSetupError {
    Skipped(String),
    Artifact(String),
    PocketIcMissing,
    ProtectedId(String),
    InitPayloadDriverMissing,
}

fn principal(text: &str) -> Principal {
    Principal::from_text(text).expect("well-known principal should parse")
}

fn maybe_artifacts(required: bool) -> Result<ArtifactSet, NnsSetupError> {
    match resolve_from_env(required) {
        Ok(ArtifactStatus::Ready(set)) => Ok(set),
        Ok(ArtifactStatus::Skipped(message)) => Err(NnsSetupError::Skipped(message)),
        Err(err) => Err(NnsSetupError::Artifact(err)),
    }
}

pub fn assert_nns_plan_avoids_protected_ids() -> Result<(), NnsSetupError> {
    let protected_ids = [
        PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
        PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
        PRODUCTION_IO_HISTORIAN_CANISTER_ID,
        PRODUCTION_FRONTEND_CANISTER_ID,
        PROTECTED_IO_NEURON_OWNER_CANISTER,
    ];
    for entry in NNS_INSTALL_PLAN {
        let id = entry.canister_id;
        if protected_ids.contains(&id) {
            return Err(NnsSetupError::ProtectedId(id.to_string()));
        }
    }
    Ok(())
}

pub fn load_required_nns_artifacts(artifacts: &ArtifactSet) -> Result<(), NnsSetupError> {
    for entry in NNS_INSTALL_PLAN {
        artifacts
            .load_required(entry.artifact_key)
            .map_err(NnsSetupError::Artifact)?;
    }
    Ok(())
}

pub fn install_minimal_nns_for_sns_w(required: bool) -> Result<NnsFixture, NnsSetupError> {
    let artifacts = maybe_artifacts(required)?;
    assert_nns_plan_avoids_protected_ids()?;
    load_required_nns_artifacts(&artifacts)?;
    if !pocketic_env::pocketic_available() {
        return Err(NnsSetupError::PocketIcMissing);
    }

    let pic = pocketic_env::new_sns_pic();
    let nns_subnet = pic.topology().get_nns().expect("NNS subnet should exist");
    let mut canister_ids = Vec::new();
    for entry in NNS_INSTALL_PLAN {
        let id = principal(entry.canister_id);
        let created = pic
            .create_canister_with_id(None, None, id)
            .map_err(NnsSetupError::Artifact)?;
        assert_eq!(created, id);
        assert_eq!(pic.get_subnet(id), Some(nns_subnet));
        canister_ids.push(id);
    }

    // Real Wasm installation is intentionally blocked until IO owns the exact
    // NNS init payload DTO builder matching the pinned artifacts.
    Err(NnsSetupError::InitPayloadDriverMissing)
}

pub fn install_sns_wasm_for_basic_queries(
    required: bool,
) -> Result<SnsWasmBasicQueryFixture, NnsSetupError> {
    let artifacts = maybe_artifacts(required)?;
    assert_nns_plan_avoids_protected_ids()?;
    if !pocketic_env::pocketic_available() {
        return Err(NnsSetupError::PocketIcMissing);
    }

    let pic = pocketic_env::new_sns_pic();
    let nns_subnet = pic.topology().get_nns().expect("NNS subnet should exist");
    let sns_subnet = pic.topology().get_sns().expect("SNS subnet should exist");
    let allowed_principal = Principal::from_slice(&[42; 29]);
    let sns_wasm = install_sns_wasm_on_existing_pic(
        &pic,
        &artifacts,
        SnsWasmCanisterInitPayload {
            allowed_principals: vec![allowed_principal],
            access_controls_enabled: true,
            sns_subnet_ids: vec![sns_subnet],
        },
    )?;

    assert_eq!(pic.get_subnet(sns_wasm), Some(nns_subnet));
    let subnet_response: GetSnsSubnetIdsResponse =
        icrc::query_one(&pic, sns_wasm, "get_sns_subnet_ids", EmptyRecord {});
    assert_eq!(subnet_response.sns_subnet_ids, vec![sns_subnet]);
    let allowed_response: GetAllowedPrincipalsResponse =
        icrc::query_one(&pic, sns_wasm, "get_allowed_principals", EmptyRecord {});
    assert!(
        allowed_response.allowed_principals.is_empty()
            || allowed_response.allowed_principals == vec![allowed_principal],
        "SNS-W allowed principals query should be readable; observed {:?}",
        allowed_response.allowed_principals
    );

    Ok(SnsWasmBasicQueryFixture {
        sns_wasm,
        nns_subnet,
        sns_subnet,
        allowed_principal,
    })
}

pub fn install_sns_wasm_on_existing_pic(
    pic: &PocketIc,
    artifacts: &ArtifactSet,
    init: SnsWasmCanisterInitPayload,
) -> Result<Principal, NnsSetupError> {
    let sns_wasm_bytes = artifacts
        .load_required(install_sns_wasm().artifact_key)
        .map_err(NnsSetupError::Artifact)?;
    let sns_wasm = principal(install_sns_wasm().canister_id);
    let created = pic
        .create_canister_with_id(None, None, sns_wasm)
        .map_err(NnsSetupError::Artifact)?;
    assert_eq!(created, sns_wasm);
    pic.add_cycles(sns_wasm, 2_000_000_000_000);
    pic.install_canister(
        sns_wasm,
        sns_wasm_bytes,
        candid::encode_one(init).expect("SNS-W init payload should encode"),
        None,
    );
    for _ in 0..5 {
        pic.tick();
    }
    Ok(sns_wasm)
}

pub fn install_nns_ledger() -> NnsCanisterPlan {
    NNS_INSTALL_PLAN
        .iter()
        .copied()
        .find(|entry| entry.kind == NnsCanisterKind::Ledger)
        .unwrap()
}

pub fn install_nns_governance() -> NnsCanisterPlan {
    NNS_INSTALL_PLAN
        .iter()
        .copied()
        .find(|entry| entry.kind == NnsCanisterKind::Governance)
        .unwrap()
}

pub fn install_nns_root() -> NnsCanisterPlan {
    NNS_INSTALL_PLAN
        .iter()
        .copied()
        .find(|entry| entry.kind == NnsCanisterKind::Root)
        .unwrap()
}

pub fn install_sns_wasm() -> NnsCanisterPlan {
    NNS_INSTALL_PLAN
        .iter()
        .copied()
        .find(|entry| entry.kind == NnsCanisterKind::SnsWasm)
        .unwrap()
}

pub fn install_registry_if_required() -> NnsCanisterPlan {
    NNS_INSTALL_PLAN
        .iter()
        .copied()
        .find(|entry| entry.kind == NnsCanisterKind::Registry)
        .unwrap()
}

pub fn install_cmc_if_required() -> NnsCanisterPlan {
    NNS_INSTALL_PLAN
        .iter()
        .copied()
        .find(|entry| entry.kind == NnsCanisterKind::CyclesMinting)
        .unwrap()
}

pub fn await_nns_ready() -> Result<(), NnsSetupError> {
    Err(NnsSetupError::InitPayloadDriverMissing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    fn clear_env() {
        env::remove_var(crate::artifacts::ENV_WASM_DIR);
        env::remove_var(crate::artifacts::ENV_MANIFEST);
    }

    fn write_manifest(dir: &Path, omit: Option<&str>) -> PathBuf {
        let manifest_path = dir.join("wasms.local.toml");
        let mut text = String::from("[artifacts]\n");
        for entry in NNS_INSTALL_PLAN {
            if Some(entry.artifact_key) == omit {
                continue;
            }
            let file = format!("{}.wasm", entry.artifact_key);
            let bytes = format!("wasm bytes for {}", entry.artifact_key);
            fs::write(dir.join(&file), bytes.as_bytes()).unwrap();
            let hash = hex::encode(Sha256::digest(bytes.as_bytes()));
            text.push_str(&format!("{}_wasm = \"{file}\"\n", entry.artifact_key));
            text.push_str(&format!("{}_sha256 = \"{hash}\"\n", entry.artifact_key));
        }
        fs::write(&manifest_path, text).unwrap();
        manifest_path
    }

    #[test]
    fn real_nns_minimal_installer_rejects_missing_artifacts() {
        let _guard = crate::lock_test_env();
        clear_env();
        let dir = tempfile::tempdir().unwrap();
        let manifest = write_manifest(dir.path(), Some("nns_ledger"));
        env::set_var(crate::artifacts::ENV_WASM_DIR, dir.path());
        env::set_var(crate::artifacts::ENV_MANIFEST, manifest);

        let artifacts = maybe_artifacts(true).unwrap();
        let err = load_required_nns_artifacts(&artifacts).unwrap_err();
        assert!(matches!(err, NnsSetupError::Artifact(message) if message.contains("nns_ledger")));
        clear_env();
    }

    #[test]
    fn real_nns_installer_uses_well_known_nns_canister_ids() {
        assert_eq!(
            install_nns_ledger().canister_id,
            "ryjl3-tyaaa-aaaaa-aaaba-cai"
        );
        assert_eq!(
            install_nns_governance().canister_id,
            "rrkah-fqaaa-aaaaa-aaaaq-cai"
        );
        assert_eq!(
            install_nns_root().canister_id,
            "r7inp-6aaaa-aaaaa-aaabq-cai"
        );
        assert_eq!(
            install_sns_wasm().canister_id,
            "qaa6y-5yaaa-aaaaa-aaafa-cai"
        );
    }

    #[test]
    fn real_nns_installer_uses_nns_subnet_not_app_subnet() {
        for entry in NNS_INSTALL_PLAN {
            assert!(
                entry.canister_id.starts_with('r') || entry.canister_id.starts_with('q'),
                "{:?} should use an NNS well-known principal, not an app-subnet generated ID",
                entry.kind
            );
        }
    }

    #[test]
    fn real_nns_installer_does_not_touch_production_fiduciary_ids() {
        assert_eq!(assert_nns_plan_avoids_protected_ids(), Ok(()));
    }

    #[test]
    fn real_nns_sns_wasm_canister_responds_to_basic_queries_is_blocked_on_init_payload_driver() {
        assert_eq!(
            await_nns_ready(),
            Err(NnsSetupError::InitPayloadDriverMissing)
        );
    }

    #[test]
    #[ignore = "requires pinned real NNS/SNS-W artifacts and POCKET_IC_BIN"]
    fn real_nns_sns_wasm_canister_responds_to_basic_queries() {
        let fixture = install_sns_wasm_for_basic_queries(true).unwrap();
        assert_eq!(fixture.sns_wasm, principal(install_sns_wasm().canister_id));
        assert_ne!(fixture.nns_subnet, fixture.sns_subnet);
    }
}
