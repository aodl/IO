use candid::Principal;
use io_sns_lifecycle::{
    DappCanisterRecord, ExpectedModuleHashRequest, RegisterDappCanisterRequest, RootUpgradeAttempt,
    RootUpgradeAttemptStatus, RootUpgradeIntent, RootUpgradeOutcomeRequest, RootUpgradeRequest,
};
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Default)]
struct RootState {
    governance_principal: Option<Principal>,
    dapp_canisters: BTreeMap<Principal, String>,
    expected_hashes: BTreeMap<Principal, String>,
    history: Vec<RootUpgradeAttempt>,
    next_attempt_id: u64,
    now: u64,
}

thread_local! {
    static STATE: RefCell<RootState> = RefCell::new(RootState::default());
}

fn caller() -> Principal {
    #[cfg(target_family = "wasm")]
    {
        ic_cdk::api::msg_caller()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        Principal::anonymous()
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_governance_principal(governance: Principal) {
    STATE.with(|cell| cell.borrow_mut().governance_principal = Some(governance));
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_register_dapp_canister(request: RegisterDappCanisterRequest) {
    STATE.with(|cell| {
        cell.borrow_mut()
            .dapp_canisters
            .insert(request.principal, request.name);
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_list_dapp_canisters() -> Vec<DappCanisterRecord> {
    STATE.with(|cell| {
        cell.borrow()
            .dapp_canisters
            .iter()
            .map(|(principal, name)| DappCanisterRecord {
                name: name.clone(),
                principal: *principal,
            })
            .collect()
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_record_expected_module_hash(request: ExpectedModuleHashRequest) {
    STATE.with(|cell| {
        cell.borrow_mut()
            .expected_hashes
            .insert(request.target_canister, request.expected_module_hash);
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_upgrade_dapp_canister(
    request: RootUpgradeRequest,
) -> Result<RootUpgradeIntent, String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let request_caller = caller();
        if state.governance_principal != Some(request_caller) {
            record_failure(
                &mut state,
                &request,
                request_caller,
                "unauthorized caller".to_string(),
            );
            return Err("unauthorized caller".to_string());
        }
        if !state.dapp_canisters.contains_key(&request.target_canister) {
            record_failure(
                &mut state,
                &request,
                request_caller,
                "unknown dapp canister".to_string(),
            );
            return Err("unknown dapp canister".to_string());
        }
        if let Some(recorded) = state.expected_hashes.get(&request.target_canister) {
            if request.expected_module_hash.as_deref() != Some(recorded.as_str()) {
                record_failure(
                    &mut state,
                    &request,
                    request_caller,
                    "expected module hash mismatch".to_string(),
                );
                return Err("expected module hash mismatch".to_string());
            }
        }
        let attempt_id = next_attempt_id(&mut state);
        let timestamp = next_timestamp(&mut state);
        state.history.push(RootUpgradeAttempt {
            attempt_id,
            proposal_id: request.proposal_id,
            caller: request_caller,
            target_canister: request.target_canister,
            wasm_sha256: request.wasm_sha256.clone(),
            wasm_gz_sha256: request.wasm_gz_sha256.clone(),
            artifact_name: request.artifact_name.clone(),
            artifact_path: request.artifact_path.clone(),
            expected_module_hash: request.expected_module_hash.clone(),
            status: RootUpgradeAttemptStatus::ApprovedIntent,
            failure_reason: None,
            timestamp,
        });
        Ok(RootUpgradeIntent {
            attempt_id,
            proposal_id: request.proposal_id,
            target_canister: request.target_canister,
            wasm_sha256: request.wasm_sha256,
            wasm_gz_sha256: request.wasm_gz_sha256,
            artifact_name: request.artifact_name,
            artifact_path: request.artifact_path,
            expected_module_hash: request.expected_module_hash,
        })
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_record_upgrade_outcome(request: RootUpgradeOutcomeRequest) -> Result<(), String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let attempt = state
            .history
            .iter_mut()
            .find(|attempt| attempt.attempt_id == request.attempt_id)
            .ok_or_else(|| "unknown upgrade attempt".to_string())?;
        attempt.status = if request.success {
            RootUpgradeAttemptStatus::Succeeded
        } else {
            RootUpgradeAttemptStatus::Failed
        };
        attempt.failure_reason = request.failure_reason;
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_upgrade_history() -> Vec<RootUpgradeAttempt> {
    STATE.with(|cell| cell.borrow().history.clone())
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_clear() {
    STATE.with(|cell| *cell.borrow_mut() = RootState::default());
}

fn next_attempt_id(state: &mut RootState) -> u64 {
    state.next_attempt_id = state.next_attempt_id.saturating_add(1);
    state.next_attempt_id
}

fn next_timestamp(state: &mut RootState) -> u64 {
    state.now = state.now.saturating_add(1);
    state.now
}

fn record_failure(
    state: &mut RootState,
    request: &RootUpgradeRequest,
    request_caller: Principal,
    failure_reason: String,
) {
    let attempt_id = next_attempt_id(state);
    let timestamp = next_timestamp(state);
    state.history.push(RootUpgradeAttempt {
        attempt_id,
        proposal_id: request.proposal_id,
        caller: request_caller,
        target_canister: request.target_canister,
        wasm_sha256: request.wasm_sha256.clone(),
        wasm_gz_sha256: request.wasm_gz_sha256.clone(),
        artifact_name: request.artifact_name.clone(),
        artifact_path: request.artifact_path.clone(),
        expected_module_hash: request.expected_module_hash.clone(),
        status: RootUpgradeAttemptStatus::Failed,
        failure_reason: Some(failure_reason),
        timestamp,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;

    #[test]
    fn root_tracks_dapps_and_enforces_expected_hash() {
        debug_clear();
        let target = Principal::from_slice(&[1]);
        debug_set_governance_principal(Principal::anonymous());
        debug_register_dapp_canister(RegisterDappCanisterRequest {
            name: "io_stream_manager".to_string(),
            principal: target,
        });
        debug_record_expected_module_hash(ExpectedModuleHashRequest {
            target_canister: target,
            expected_module_hash: "hash".to_string(),
        });

        let bad = request(target, Some("wrong"));
        assert!(debug_upgrade_dapp_canister(bad)
            .unwrap_err()
            .contains("hash mismatch"));

        let ok = debug_upgrade_dapp_canister(request(target, Some("hash"))).unwrap();
        assert_eq!(ok.target_canister, target);
        assert_eq!(debug_get_upgrade_history().len(), 2);
    }

    fn request(target: Principal, expected_module_hash: Option<&str>) -> RootUpgradeRequest {
        RootUpgradeRequest {
            proposal_id: 1,
            target_canister: target,
            wasm_sha256: "raw".to_string(),
            wasm_gz_sha256: "gz".to_string(),
            artifact_name: "io_stream_manager".to_string(),
            artifact_path: "release-artifacts/io_stream_manager.wasm".to_string(),
            expected_module_hash: expected_module_hash.map(str::to_string),
        }
    }
}
