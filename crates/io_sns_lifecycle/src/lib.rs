use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactManifest {
    pub schema_version: u32,
    pub build_profile: String,
    pub target: String,
    pub git_commit: Option<String>,
    pub artifacts: Vec<ArtifactManifestEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactManifestEntry {
    pub canister: String,
    pub raw_wasm_path: String,
    pub raw_wasm_sha256: String,
    pub raw_wasm_bytes: u64,
    pub gz_wasm_path: String,
    pub gz_wasm_sha256: String,
    pub gz_wasm_bytes: u64,
    pub build_profile: String,
    pub target: String,
    pub git_commit: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, CandidType, Serialize)]
pub struct UpgradeProposalRequest {
    pub target_canister: Principal,
    pub wasm_sha256: String,
    pub wasm_gz_sha256: String,
    pub artifact_name: String,
    pub artifact_path: String,
    pub expected_module_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, CandidType, Serialize)]
pub enum UpgradeVote {
    Yes,
    No,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, CandidType, Serialize)]
pub enum UpgradeProposalStatus {
    Open,
    Adopted,
    Rejected,
    Executed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, CandidType, Serialize)]
pub struct UpgradeProposal {
    pub proposal_id: u64,
    pub target_canister: Principal,
    pub wasm_sha256: String,
    pub wasm_gz_sha256: String,
    pub artifact_name: String,
    pub artifact_path: String,
    pub expected_module_hash: Option<String>,
    pub status: UpgradeProposalStatus,
    pub yes_votes: u64,
    pub no_votes: u64,
    pub created_at: u64,
    pub decided_at: Option<u64>,
    pub failure_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, CandidType, Serialize)]
pub struct DappCanisterRecord {
    pub name: String,
    pub principal: Principal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, CandidType, Serialize)]
pub struct RegisterDappCanisterRequest {
    pub name: String,
    pub principal: Principal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, CandidType, Serialize)]
pub struct ExpectedModuleHashRequest {
    pub target_canister: Principal,
    pub expected_module_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, CandidType, Serialize)]
pub struct RootUpgradeRequest {
    pub proposal_id: u64,
    pub target_canister: Principal,
    pub wasm_sha256: String,
    pub wasm_gz_sha256: String,
    pub artifact_name: String,
    pub artifact_path: String,
    pub expected_module_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, CandidType, Serialize)]
pub struct RootUpgradeIntent {
    pub attempt_id: u64,
    pub proposal_id: u64,
    pub target_canister: Principal,
    pub wasm_sha256: String,
    pub wasm_gz_sha256: String,
    pub artifact_name: String,
    pub artifact_path: String,
    pub expected_module_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, CandidType, Serialize)]
pub struct RootUpgradeOutcomeRequest {
    pub attempt_id: u64,
    pub success: bool,
    pub failure_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, CandidType, Serialize)]
pub enum RootUpgradeAttemptStatus {
    ApprovedIntent,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, CandidType, Serialize)]
pub struct RootUpgradeAttempt {
    pub attempt_id: u64,
    pub proposal_id: u64,
    pub caller: Principal,
    pub target_canister: Principal,
    pub wasm_sha256: String,
    pub wasm_gz_sha256: String,
    pub artifact_name: String,
    pub artifact_path: String,
    pub expected_module_hash: Option<String>,
    pub status: RootUpgradeAttemptStatus,
    pub failure_reason: Option<String>,
    pub timestamp: u64,
}

pub fn read_manifest(path: impl AsRef<Path>) -> Result<ArtifactManifest, String> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
    serde_json::from_str(&text).map_err(|err| format!("{}: {err}", path.display()))
}

pub fn resolve_manifest_entry<'a>(
    manifest: &'a ArtifactManifest,
    canister: &str,
) -> Result<&'a ArtifactManifestEntry, String> {
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported manifest schema_version {}",
            manifest.schema_version
        ));
    }
    manifest
        .artifacts
        .iter()
        .find(|entry| entry.canister == canister)
        .ok_or_else(|| format!("missing artifact manifest entry for {canister}"))
}

pub fn verify_manifest_entry_paths(
    root: &Path,
    entry: &ArtifactManifestEntry,
) -> Result<(), String> {
    let raw = root.join(&entry.raw_wasm_path);
    let gz = root.join(&entry.gz_wasm_path);
    let raw_len = fs::metadata(&raw)
        .map_err(|err| format!("{}: {err}", entry.raw_wasm_path))?
        .len();
    let gz_len = fs::metadata(&gz)
        .map_err(|err| format!("{}: {err}", entry.gz_wasm_path))?
        .len();
    if raw_len != entry.raw_wasm_bytes {
        return Err(format!(
            "{} stale size: manifest {}, actual {}",
            entry.raw_wasm_path, entry.raw_wasm_bytes, raw_len
        ));
    }
    if gz_len != entry.gz_wasm_bytes {
        return Err(format!(
            "{} stale size: manifest {}, actual {}",
            entry.gz_wasm_path, entry.gz_wasm_bytes, gz_len
        ));
    }
    Ok(())
}

pub fn verify_upgrade_proposal_against_manifest(
    manifest: &ArtifactManifest,
    canister: &str,
    proposal: &UpgradeProposalRequest,
) -> Result<ArtifactManifestEntry, String> {
    let entry = resolve_manifest_entry(manifest, canister)?;
    if proposal.wasm_sha256 != entry.raw_wasm_sha256 {
        return Err(format!(
            "{canister} raw wasm hash mismatch: proposal {}, manifest {}",
            proposal.wasm_sha256, entry.raw_wasm_sha256
        ));
    }
    if proposal.wasm_gz_sha256 != entry.gz_wasm_sha256 {
        return Err(format!(
            "{canister} gz wasm hash mismatch: proposal {}, manifest {}",
            proposal.wasm_gz_sha256, entry.gz_wasm_sha256
        ));
    }
    if proposal.artifact_path != entry.raw_wasm_path {
        return Err(format!(
            "{canister} artifact path mismatch: proposal {}, manifest {}",
            proposal.artifact_path, entry.raw_wasm_path
        ));
    }
    Ok(entry.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> ArtifactManifest {
        ArtifactManifest {
            schema_version: 1,
            build_profile: "release".to_string(),
            target: "wasm32-unknown-unknown".to_string(),
            git_commit: Some("abc".to_string()),
            artifacts: vec![ArtifactManifestEntry {
                canister: "io_stream_manager".to_string(),
                raw_wasm_path: "release-artifacts/io_stream_manager.wasm".to_string(),
                raw_wasm_sha256: "raw".to_string(),
                raw_wasm_bytes: 10,
                gz_wasm_path: "release-artifacts/io_stream_manager.wasm.gz".to_string(),
                gz_wasm_sha256: "gz".to_string(),
                gz_wasm_bytes: 5,
                build_profile: "release".to_string(),
                target: "wasm32-unknown-unknown".to_string(),
                git_commit: Some("abc".to_string()),
            }],
        }
    }

    #[test]
    fn proposal_hash_must_match_manifest() {
        let manifest = manifest();
        let request = UpgradeProposalRequest {
            target_canister: Principal::anonymous(),
            wasm_sha256: "raw".to_string(),
            wasm_gz_sha256: "gz".to_string(),
            artifact_name: "io_stream_manager".to_string(),
            artifact_path: "release-artifacts/io_stream_manager.wasm".to_string(),
            expected_module_hash: None,
        };
        verify_upgrade_proposal_against_manifest(&manifest, "io_stream_manager", &request).unwrap();

        let mut bad = request;
        bad.wasm_sha256 = "wrong".to_string();
        assert!(
            verify_upgrade_proposal_against_manifest(&manifest, "io_stream_manager", &bad)
                .unwrap_err()
                .contains("raw wasm hash mismatch")
        );
    }

    #[test]
    fn missing_manifest_entry_fails() {
        assert!(resolve_manifest_entry(&manifest(), "io_nns_neuron_manager")
            .unwrap_err()
            .contains("missing artifact"));
    }
}
