//! Full-framework SNS/NNS harness preflight.
//!
//! This module deliberately does not fake a launched SNS. It verifies that the
//! complete pinned artifact set needed for the NNS + SNS-W deployment path is
//! locally available, loads those Wasms through the same SHA-checked manifest as
//! the executable ledger/index tests, and creates an application-subnet canister
//! slot that later governance/root tests can hand to SNS root.
//!
//! The actual SNS-W deployment / swap-finalization / normal staking driver still
//! requires the exact NNS and SNS init payload DTOs from the pinned artifact set.

use crate::artifacts::{resolve_from_env, ArtifactSet, ArtifactStatus};
use crate::pocketic_env;
use candid::Principal;

pub const FULL_FRAMEWORK_ARTIFACTS: &[&str] = &[
    "sns_ledger",
    "sns_index",
    "sns_governance",
    "sns_root",
    "sns_swap",
    "sns_archive",
    "sns_wasm",
    "nns_governance",
    "nns_ledger",
    "nns_root",
    "nns_lifeline",
    "nns_registry",
    "cmc",
    "icp_ledger",
    "icp_index",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameworkArtifactInventory {
    pub loaded_artifacts: Vec<String>,
    pub missing_artifacts: Vec<String>,
}

fn maybe_artifacts(required: bool) -> Option<ArtifactSet> {
    match resolve_from_env(required) {
        Ok(ArtifactStatus::Ready(set)) => Some(set),
        Ok(ArtifactStatus::Skipped(message)) => {
            eprintln!("skipping full-framework SNS/NNS preflight: {message}");
            None
        }
        Err(err) if !required => {
            panic!("real-framework artifacts are configured but invalid: {err}")
        }
        Err(err) => panic!("{err}"),
    }
}

pub fn inventory(required: bool) -> Option<FrameworkArtifactInventory> {
    let artifacts = maybe_artifacts(required)?;
    let mut loaded_artifacts = Vec::new();
    let mut missing_artifacts = Vec::new();
    for key in FULL_FRAMEWORK_ARTIFACTS {
        match artifacts.load_required(key) {
            Ok(_) => loaded_artifacts.push((*key).to_string()),
            Err(err) if required => {
                panic!("required full-framework artifact {key} is unavailable: {err}")
            }
            Err(_) => missing_artifacts.push((*key).to_string()),
        }
    }
    Some(FrameworkArtifactInventory {
        loaded_artifacts,
        missing_artifacts,
    })
}

/// Exercises only the topology and complete-artifact preflight. This is useful
/// as a strict gate before attempting the much more specific SNS-W deployment
/// and normal-staking driver.
pub fn run_full_framework_preflight(required: bool) {
    let Some(inv) = inventory(required) else {
        return;
    };
    if required && !inv.missing_artifacts.is_empty() {
        panic!(
            "full framework artifact set is incomplete: missing {:?}",
            inv.missing_artifacts
        );
    }
    if !inv.missing_artifacts.is_empty() {
        eprintln!(
            "full framework SNS/NNS driver remains blocked; missing artifacts: {:?}",
            inv.missing_artifacts
        );
        return;
    }
    if !pocketic_env::pocketic_available() {
        if required {
            panic!("POCKET_IC_BIN is required for full-framework SNS/NNS preflight");
        }
        panic!("full framework artifacts are configured but POCKET_IC_BIN is not set");
    }

    let pic = pocketic_env::new_sns_pic();
    let app_canister = pocketic_env::create_empty_application_canister(&pic);
    let app_subnet = pic
        .topology()
        .get_app_subnets()
        .into_iter()
        .next()
        .expect("application subnet should exist");
    assert!(
        app_canister != Principal::anonymous(),
        "application canister should have a concrete principal"
    );
    assert!(
        app_subnet != Principal::anonymous(),
        "application subnet should have a concrete principal"
    );

    eprintln!(
        "loaded {} pinned artifacts and created app canister {app_canister} on app subnet {app_subnet}; SNS-W deployment driver is the next step",
        inv.loaded_artifacts.len()
    );
}
