use crate::nns_setup::NnsSetupError;
use crate::sns_wasm_setup::SnsWasmSetupError;
use candid::Principal;

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

pub fn await_swap_open() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn participate_in_swap() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn refresh_buyer_tokens() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn await_swap_committed() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn finalize_swap() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn await_sns_finalized() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn discover_deployed_sns_canister_ids() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn read_sns_canister_ids() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

pub fn list_sns_neurons() -> Result<(), SnsLifecycleError> {
    Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_sns_swap_opens_with_expected_parameters_is_blocked_on_sns_init_dto() {
        assert_eq!(
            await_swap_open(),
            Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
        );
    }

    #[test]
    fn real_sns_lifecycle_deploys_sns_via_sns_w_is_blocked_on_sns_init_dto() {
        assert_eq!(
            deploy_io_test_sns_through_sns_w(),
            Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
        );
    }

    #[test]
    fn real_sns_finalized_swap_creates_direct_participation_neurons_is_blocked() {
        assert_eq!(
            list_sns_neurons(),
            Err(SnsLifecycleError::CreateServiceNervousSystemDtoMissing)
        );
    }
}
