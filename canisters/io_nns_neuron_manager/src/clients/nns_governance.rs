use candid::{CandidType, Principal};
use io_governance_types::{
    NnsCommandResult, NnsDissolveState, NnsGovernanceClient, NnsGovernanceError, NnsNeuron,
    NnsNeuronCommand, NnsNeuronId,
};
use io_ledger_types::Account;
use serde::Deserialize;
use std::future::Future;
use std::pin::Pin;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NeuronIdArgs {
    pub neuron_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NeuronAmountArgs {
    pub neuron_id: u64,
    pub amount_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MockNeuron {
    pub neuron_id: u64,
    pub principal_e8s: u128,
    pub maturity_e8s: u128,
    pub dissolve_delay_seconds: u64,
    pub is_dissolving: bool,
    pub dissolve_started_at_seconds: Option<u64>,
}

impl From<MockNeuron> for NnsNeuron {
    fn from(value: MockNeuron) -> Self {
        let dissolve_state = if value.is_dissolving {
            NnsDissolveState::Dissolving {
                when_dissolved_timestamp_seconds: value
                    .dissolve_started_at_seconds
                    .unwrap_or(0)
                    .saturating_add(value.dissolve_delay_seconds),
            }
        } else {
            NnsDissolveState::NotDissolving {
                dissolve_delay_seconds: value.dissolve_delay_seconds,
            }
        };
        Self {
            id: NnsNeuronId(value.neuron_id),
            controller: None,
            stake_e8s: value.principal_e8s,
            maturity_e8s_equivalent: value.maturity_e8s,
            dissolve_delay_seconds: value.dissolve_delay_seconds,
            dissolve_state,
            known_neuron_name: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MockNnsGovernanceClient {
    pub canister: Principal,
}

fn mock_err(method: &str, message: String) -> NnsGovernanceError {
    if message.contains("unknown neuron") {
        NnsGovernanceError::NeuronNotFound
    } else if message.contains("split exceeds") {
        NnsGovernanceError::InsufficientStake
    } else if message.contains("not ready") {
        NnsGovernanceError::InvalidCommand { message }
    } else if message.contains("temporar") {
        NnsGovernanceError::TemporarilyUnavailable
    } else {
        NnsGovernanceError::CanisterCallFailed {
            method: method.to_string(),
            message,
        }
    }
}

fn command_result(
    command: NnsNeuronCommand,
    neuron_id: u64,
    amount_e8s: Option<u128>,
) -> NnsCommandResult {
    NnsCommandResult {
        command,
        neuron_id: NnsNeuronId(neuron_id),
        amount_e8s,
        child_neuron_id: None,
    }
}

impl NnsGovernanceClient for MockNnsGovernanceClient {
    fn get_neuron<'a>(
        &'a self,
        id: NnsNeuronId,
    ) -> Pin<Box<dyn Future<Output = Result<NnsNeuron, NnsGovernanceError>> + 'a>> {
        Box::pin(async move { debug_get_neuron(self.canister, id.0).await })
    }

    fn disburse_maturity<'a>(
        &'a self,
        id: NnsNeuronId,
        _percentage_to_disburse: u32,
        _to: Account,
    ) -> Pin<Box<dyn Future<Output = Result<NnsCommandResult, NnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            let amount = debug_disburse_maturity(self.canister, id.0)
                .await
                .map_err(|err| mock_err("debug_disburse_maturity", err))?;
            Ok(command_result(
                NnsNeuronCommand::DisburseMaturity,
                id.0,
                Some(amount),
            ))
        })
    }

    fn split_neuron<'a>(
        &'a self,
        id: NnsNeuronId,
        amount_e8s: u128,
    ) -> Pin<Box<dyn Future<Output = Result<NnsNeuronId, NnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            debug_split(self.canister, id.0, amount_e8s)
                .await
                .map(NnsNeuronId)
                .map_err(|err| mock_err("debug_split", err))
        })
    }

    fn start_dissolving<'a>(
        &'a self,
        id: NnsNeuronId,
    ) -> Pin<Box<dyn Future<Output = Result<NnsCommandResult, NnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            debug_start_dissolving(self.canister, id.0)
                .await
                .map_err(|err| mock_err("debug_start_dissolving", err))?;
            Ok(command_result(
                NnsNeuronCommand::StartDissolving,
                id.0,
                None,
            ))
        })
    }

    fn stop_dissolving<'a>(
        &'a self,
        id: NnsNeuronId,
    ) -> Pin<Box<dyn Future<Output = Result<NnsCommandResult, NnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            debug_stop_dissolving(self.canister, id.0)
                .await
                .map_err(|err| mock_err("debug_stop_dissolving", err))?;
            Ok(command_result(NnsNeuronCommand::StopDissolving, id.0, None))
        })
    }

    fn disburse_neuron<'a>(
        &'a self,
        id: NnsNeuronId,
        _to: Account,
    ) -> Pin<Box<dyn Future<Output = Result<NnsCommandResult, NnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            let amount = debug_disburse_principal(self.canister, id.0)
                .await
                .map_err(|err| mock_err("debug_disburse_principal", err))?;
            Ok(command_result(
                NnsNeuronCommand::Disburse,
                id.0,
                Some(amount),
            ))
        })
    }
}

pub async fn debug_get_neuron(
    canister: Principal,
    neuron_id: u64,
) -> Result<NnsNeuron, NnsGovernanceError> {
    let response = ic_cdk::call::Call::bounded_wait(canister, "debug_get_neuron")
        .with_arg(NeuronIdArgs { neuron_id })
        .await
        .map_err(|err| NnsGovernanceError::CanisterCallFailed {
            method: "debug_get_neuron".to_string(),
            message: format!("{err:?}"),
        })
        .and_then(|response| {
            response
                .candid_tuple::<(Option<MockNeuron>,)>()
                .map_err(|err| NnsGovernanceError::DecodeError {
                    message: format!("{err:?}"),
                })
        })?;
    response
        .0
        .map(Into::into)
        .ok_or(NnsGovernanceError::NeuronNotFound)
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
