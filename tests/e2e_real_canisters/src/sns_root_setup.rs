use crate::artifacts::{resolve_from_env, ArtifactStatus};
use crate::icrc;
use crate::nns_setup::EmptyRecord;
use crate::pocketic_env;
use candid::{CandidType, Principal};
use pocket_ic::CanisterSettings;
use serde::Deserialize;

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Extensions {
    pub extension_canister_ids: Vec<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct SnsRootCanister {
    pub dapp_canister_ids: Vec<Principal>,
    pub extensions: Option<Extensions>,
    pub testflight: bool,
    pub archive_canister_ids: Vec<Principal>,
    pub governance_canister_id: Option<Principal>,
    pub index_canister_id: Option<Principal>,
    pub swap_canister_id: Option<Principal>,
    pub ledger_canister_id: Option<Principal>,
    pub timers: Option<Timers>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct Timers {
    pub requires_periodic_tasks: Option<bool>,
    pub last_reset_timestamp_seconds: Option<u64>,
    pub last_spawned_timestamp_seconds: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct ListSnsCanistersResponse {
    pub root: Option<Principal>,
    pub swap: Option<Principal>,
    pub ledger: Option<Principal>,
    pub index: Option<Principal>,
    pub governance: Option<Principal>,
    pub dapps: Vec<Principal>,
    pub extensions: Option<Extensions>,
    pub archives: Vec<Principal>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnsRootSetupError {
    Artifact(String),
    PocketIcMissing,
}

pub fn install_real_sns_root_with_local_dapp(
    required: bool,
) -> Result<(Principal, Principal, Principal), SnsRootSetupError> {
    let artifacts = match resolve_from_env(required) {
        Ok(ArtifactStatus::Ready(set)) => set,
        Ok(ArtifactStatus::Skipped(message)) => return Err(SnsRootSetupError::Artifact(message)),
        Err(err) => return Err(SnsRootSetupError::Artifact(err)),
    };
    if !pocketic_env::pocketic_available() {
        return Err(SnsRootSetupError::PocketIcMissing);
    }

    let root_wasm = artifacts
        .load_required("sns_root")
        .map_err(SnsRootSetupError::Artifact)?;
    let pic = pocketic_env::new_sns_pic();
    let sns_subnet = pic.topology().get_sns().expect("SNS subnet should exist");
    let app_subnet = pic
        .topology()
        .get_app_subnets()
        .into_iter()
        .next()
        .expect("application subnet should exist");
    let root = pic.create_canister_on_subnet(None, None, sns_subnet);
    pic.add_cycles(root, 2_000_000_000_000);
    let dapp = pic.create_canister_on_subnet(
        None,
        Some(CanisterSettings {
            controllers: Some(vec![root]),
            ..Default::default()
        }),
        app_subnet,
    );
    pic.add_cycles(dapp, 2_000_000_000_000);
    pic.install_canister(
        root,
        root_wasm,
        candid::encode_one(SnsRootCanister {
            dapp_canister_ids: vec![dapp],
            extensions: None,
            testflight: true,
            archive_canister_ids: vec![],
            governance_canister_id: Some(Principal::from_slice(&[10; 29])),
            index_canister_id: Some(Principal::from_slice(&[11; 29])),
            swap_canister_id: Some(Principal::from_slice(&[12; 29])),
            ledger_canister_id: Some(Principal::from_slice(&[13; 29])),
            timers: None,
        })
        .expect("SNS root init payload should encode"),
        None,
    );
    for _ in 0..5 {
        pic.tick();
    }

    assert_eq!(pic.get_subnet(root), Some(sns_subnet));
    assert_eq!(pic.get_subnet(dapp), Some(app_subnet));
    let listed: ListSnsCanistersResponse =
        icrc::query_one(&pic, root, "list_sns_canisters", EmptyRecord {});
    assert_eq!(listed.root, Some(root));
    assert_eq!(listed.dapps, vec![dapp]);
    assert!(pic.cycle_balance(dapp) >= 2_000_000_000_000);
    Ok((root, dapp, app_subnet))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires pinned real SNS root artifact and POCKET_IC_BIN"]
    fn real_sns_root_control_uses_application_subnet_canister_direct_root_path() {
        let (root, dapp, app_subnet) = install_real_sns_root_with_local_dapp(true).unwrap();
        assert_ne!(root, dapp);
        assert_ne!(app_subnet, Principal::anonymous());
    }
}
