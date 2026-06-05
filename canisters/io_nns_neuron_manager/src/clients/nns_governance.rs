use candid::{CandidType, Principal};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NeuronIdArgs {
    pub neuron_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NeuronAmountArgs {
    pub neuron_id: u64,
    pub amount_e8s: u128,
}

pub async fn debug_disburse_maturity(canister: Principal, neuron_id: u64) -> Result<u128, String> {
    let response = ic_cdk::call::Call::bounded_wait(canister, "debug_disburse_maturity")
        .with_arg(NeuronIdArgs { neuron_id })
        .await
        .map_err(|err| format!("nns governance call failed: {err:?}"))
        .and_then(|response| {
            response
                .candid_tuple::<(Result<u128, String>,)>()
                .map_err(|err| format!("nns governance decode failed: {err:?}"))
        })?;
    response.0
}

pub async fn debug_split(
    canister: Principal,
    neuron_id: u64,
    amount_e8s: u128,
) -> Result<u64, String> {
    let response = ic_cdk::call::Call::bounded_wait(canister, "debug_split")
        .with_arg(NeuronAmountArgs {
            neuron_id,
            amount_e8s,
        })
        .await
        .map_err(|err| format!("nns governance split call failed: {err:?}"))
        .and_then(|response| {
            response
                .candid_tuple::<(Result<u64, String>,)>()
                .map_err(|err| format!("nns governance split decode failed: {err:?}"))
        })?;
    response.0
}

pub async fn debug_start_dissolving(canister: Principal, neuron_id: u64) -> Result<(), String> {
    let response = ic_cdk::call::Call::bounded_wait(canister, "debug_start_dissolving")
        .with_arg(NeuronIdArgs { neuron_id })
        .await
        .map_err(|err| format!("nns governance start dissolve call failed: {err:?}"))
        .and_then(|response| {
            response
                .candid_tuple::<(Result<(), String>,)>()
                .map_err(|err| format!("nns governance start dissolve decode failed: {err:?}"))
        })?;
    response.0
}

pub async fn debug_stop_dissolving(canister: Principal, neuron_id: u64) -> Result<(), String> {
    let response = ic_cdk::call::Call::bounded_wait(canister, "debug_stop_dissolving")
        .with_arg(NeuronIdArgs { neuron_id })
        .await
        .map_err(|err| format!("nns governance stop dissolve call failed: {err:?}"))
        .and_then(|response| {
            response
                .candid_tuple::<(Result<(), String>,)>()
                .map_err(|err| format!("nns governance stop dissolve decode failed: {err:?}"))
        })?;
    response.0
}

pub async fn debug_merge(
    canister: Principal,
    neuron_id: u64,
    amount_e8s: u128,
) -> Result<(), String> {
    let response = ic_cdk::call::Call::bounded_wait(canister, "debug_merge")
        .with_arg(NeuronAmountArgs {
            neuron_id,
            amount_e8s,
        })
        .await
        .map_err(|err| format!("nns governance merge call failed: {err:?}"))
        .and_then(|response| {
            response
                .candid_tuple::<(Result<(), String>,)>()
                .map_err(|err| format!("nns governance merge decode failed: {err:?}"))
        })?;
    response.0
}

pub async fn debug_disburse_principal(canister: Principal, neuron_id: u64) -> Result<u128, String> {
    let response = ic_cdk::call::Call::bounded_wait(canister, "debug_disburse_principal")
        .with_arg(NeuronIdArgs { neuron_id })
        .await
        .map_err(|err| format!("nns governance principal disburse call failed: {err:?}"))
        .and_then(|response| {
            response
                .candid_tuple::<(Result<u128, String>,)>()
                .map_err(|err| format!("nns governance principal disburse decode failed: {err:?}"))
        })?;
    response.0
}
