use candid::{encode_one, CandidType, Principal};
#[cfg(test)]
use io_ledger_types::{Account, IcpTokens, IcpTransferArgs, IcpTransferError, IcrcAccount};
use io_production_wiring::{
    ProductionWiringConfig, DEV_MAINNET_FRONTEND_CANISTER_ID, DEV_MAINNET_HISTORIAN_CANISTER_ID,
    PRODUCTION_FRONTEND_CANISTER_ID, PRODUCTION_IO_HISTORIAN_CANISTER_ID,
    PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID, PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
    PROTECTED_IO_NEURON_OWNER_CANISTER, PROTECTED_IO_NNS_NEURON_ID,
};
use pocket_ic::PocketIc;
use serde::Deserialize;
use std::path::Path;
#[cfg(test)]
use std::time::Duration;

use crate::sns_lifecycle::{
    assert_all_canisters_on_expected_subnets, assert_no_production_fiduciary_ids,
    deploy_finalized_sns_lifecycle_fixture_for_test,
    grant_finalized_neuron_vote_permission_for_test, FinalizedSnsLifecycleFixture,
    SnsLifecycleError,
};

const PARTICIPANT_ICP_E8S: u64 = 100_000_000;
#[cfg(test)]
const JUPITER_DEPOSIT_ICP_E8S: u64 = 10_000_000_000;
#[cfg(test)]
const JUPITER_EXPECTED_IO_E8S: u128 = 6_000_000_000;
#[cfg(test)]
const JUPITER_REDEMPTION_IO_E8S: u64 = 1_000_000_000;
#[cfg(test)]
const JUPITER_EXPECTED_REDEMPTION_ICP_E8S: u128 = 1_000_000_000;
#[cfg(test)]
const TWO_WEEK_MATURITY_ICP_E8S: u64 = 500_000_000;
const APP_CANISTER_CYCLES: u128 = 2_000_000_000_000;
#[cfg(test)]
const ICP_LEDGER_TRANSFER_FEE_E8S: u64 = 10_000;
const LOCAL_TWO_YEAR_NEURON_ID: u64 = 42;
#[cfg(test)]
const TWO_WEEK_DISSOLVE_DELAY_SECONDS: u64 = 14 * 24 * 60 * 60;
#[cfg(test)]
const FINALIZED_SNS_PROPOSAL_REJECT_COST_E8S: u64 = 10_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StreamManagerInitArgs {
    pub initial_total_io_supply_e8s: u128,
    pub initial_protocol_reserve_io_e8s: u128,
    pub non_redeemable_governance_io_e8s: u128,
    pub two_week_pool_backing_bps: u128,
    pub jupiter_faucet_principal_text: Option<String>,
    pub io_nns_neuron_manager_principal_text: Option<String>,
    pub icp_ledger_principal_text: Option<String>,
    pub icp_index_principal_text: Option<String>,
    pub io_ledger_principal_text: Option<String>,
    pub io_index_principal_text: Option<String>,
    pub io_sns_ledger_principal_text: Option<String>,
    pub io_sns_index_principal_text: Option<String>,
    pub sns_governance_principal_text: Option<String>,
    pub production_wiring: Option<ProductionWiringConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsNeuronManagerInitArgs {
    pub controller_canister_principal_text: String,
    pub two_year_nns_neuron_id: u64,
    pub two_week_dissolve_seconds: u64,
    pub initial_two_year_principal_e8s: u128,
    pub initial_two_week_principal_e8s: u128,
    pub model_annual_bps: u128,
    pub io_stream_manager_principal_text: Option<String>,
    pub two_year_maturity_memo: Option<u64>,
    pub two_week_maturity_memo: Option<u64>,
    pub principal_unwind_memo: Option<u64>,
    pub nns_governance_principal_text: Option<String>,
    pub icp_ledger_principal_text: Option<String>,
    pub icp_index_principal_text: Option<String>,
    pub production_wiring: Option<ProductionWiringConfig>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IcpAccountBalanceArgs {
    pub account: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct IoRealStackInstallArgs {
    pub stream_manager: StreamManagerInitArgs,
    pub nns_neuron_manager: NnsNeuronManagerInitArgs,
}

#[derive(Clone, Debug)]
pub struct FinalizedSnsCanisterIds {
    pub nns_governance: Principal,
    pub nns_ledger: Principal,
    pub nns_index: Principal,
    pub governance: Principal,
    pub ledger: Principal,
    pub index: Principal,
}

impl From<&FinalizedSnsLifecycleFixture> for FinalizedSnsCanisterIds {
    fn from(value: &FinalizedSnsLifecycleFixture) -> Self {
        Self {
            nns_governance: value.nns_governance,
            nns_ledger: value.nns_ledger,
            nns_index: value.nns_index,
            governance: value.governance,
            ledger: value.ledger,
            index: value.index,
        }
    }
}

pub struct IoRealStackFixture {
    pub sns: FinalizedSnsLifecycleFixture,
    pub stream_manager: Principal,
    pub nns_neuron_manager: Principal,
    pub historian: Principal,
    pub install_args: IoRealStackInstallArgs,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IoRealStackError {
    MissingWasm {
        name: &'static str,
        tried: Vec<String>,
    },
    ForbiddenPrincipal {
        field: &'static str,
        value: String,
        reason: &'static str,
    },
    ForbiddenNeuron {
        field: &'static str,
        value: u64,
        reason: &'static str,
    },
    SnsLifecycle(String),
}

impl From<SnsLifecycleError> for IoRealStackError {
    fn from(value: SnsLifecycleError) -> Self {
        Self::SnsLifecycle(format!("{value:?}"))
    }
}

pub fn deploy_finalized_sns_with_io_real_stack_for_test(
    required: bool,
) -> Result<IoRealStackFixture, IoRealStackError> {
    let participant = Principal::from_slice(&[105; 29]);
    let sns = deploy_finalized_sns_lifecycle_fixture_for_test(
        required,
        participant,
        PARTICIPANT_ICP_E8S,
    )?;
    deploy_io_real_stack_on_fixture(sns)
}

pub fn deploy_io_real_stack_on_fixture(
    sns: FinalizedSnsLifecycleFixture,
) -> Result<IoRealStackFixture, IoRealStackError> {
    deploy_io_real_stack_on_fixture_configured(sns, |_, _, _| Ok(()))
}

fn deploy_io_real_stack_on_fixture_configured<F>(
    sns: FinalizedSnsLifecycleFixture,
    configure_stream_manager: F,
) -> Result<IoRealStackFixture, IoRealStackError>
where
    F: FnOnce(
        &FinalizedSnsLifecycleFixture,
        Principal,
        &mut StreamManagerInitArgs,
    ) -> Result<(), IoRealStackError>,
{
    let stream_wasm = required_io_wasm(
        "io_stream_manager",
        "IO_STREAM_MANAGER_WASM",
        &[
            "target/wasm32-unknown-unknown/debug/io_stream_manager.wasm",
            "release-artifacts/io_stream_manager.wasm",
        ],
    )?;
    let nns_manager_wasm = required_io_wasm(
        "io_nns_neuron_manager",
        "IO_NNS_NEURON_MANAGER_WASM",
        &[
            "target/wasm32-unknown-unknown/debug/io_nns_neuron_manager.wasm",
            "release-artifacts/io_nns_neuron_manager.wasm",
        ],
    )?;
    let historian_wasm = required_io_wasm(
        "io_historian",
        "IO_HISTORIAN_WASM",
        &[
            "target/wasm32-unknown-unknown/debug/io_historian.wasm",
            "release-artifacts/io_historian.wasm",
        ],
    )?;

    let stream_manager =
        create_funded_application_canister_on_subnet(&sns.pic, sns.application_subnet);
    let mut install_args = build_io_real_stack_install_args(&sns, Some(stream_manager));
    configure_stream_manager(&sns, stream_manager, &mut install_args.stream_manager)?;
    validate_io_real_stack_install_args(&install_args)?;

    sns.pic.install_canister(
        stream_manager,
        stream_wasm,
        encode_one(&install_args.stream_manager).expect("stream-manager init args encode"),
        None,
    );
    grant_stream_manager_governance_visibility(&sns, stream_manager)?;

    validate_io_real_stack_install_args(&install_args)?;

    let nns_neuron_manager = create_application_canister_on_subnet(
        &sns.pic,
        sns.application_subnet,
        nns_manager_wasm,
        encode_one(&install_args.nns_neuron_manager).expect("nns-manager init args encode"),
    );

    let historian = create_application_canister_on_subnet(
        &sns.pic,
        sns.application_subnet,
        historian_wasm,
        encode_one(()).expect("historian init args encode"),
    );

    assert_no_production_fiduciary_ids(&[stream_manager, nns_neuron_manager, historian])?;

    let stack = IoRealStackFixture {
        sns,
        stream_manager,
        nns_neuron_manager,
        historian,
        install_args,
    };
    assert_io_real_stack_on_application_subnet(&stack);
    Ok(stack)
}

fn grant_stream_manager_governance_visibility(
    sns: &FinalizedSnsLifecycleFixture,
    stream_manager: Principal,
) -> Result<(), IoRealStackError> {
    for participant in &sns.participants {
        let neurons = crate::sns_lifecycle::list_finalized_sns_neurons_for_principal(
            sns,
            participant.principal,
            100,
            None,
        )?;
        for neuron in neurons {
            let Some(neuron_id) = neuron.id else {
                continue;
            };
            grant_finalized_neuron_vote_permission_for_test(
                sns,
                participant.principal,
                &neuron_id,
                stream_manager,
            )?;
        }
    }
    Ok(())
}

pub fn build_io_real_stack_install_args(
    sns: &FinalizedSnsLifecycleFixture,
    stream_manager: Option<Principal>,
) -> IoRealStackInstallArgs {
    build_io_real_stack_install_args_from_ids(&FinalizedSnsCanisterIds::from(sns), stream_manager)
}

pub fn build_io_real_stack_install_args_from_ids(
    sns: &FinalizedSnsCanisterIds,
    stream_manager: Option<Principal>,
) -> IoRealStackInstallArgs {
    IoRealStackInstallArgs {
        stream_manager: StreamManagerInitArgs {
            initial_total_io_supply_e8s: 100_000_000_000_000,
            initial_protocol_reserve_io_e8s: 90_000_000_000_000,
            non_redeemable_governance_io_e8s: 10_000_000_000_000,
            two_week_pool_backing_bps: 10_000,
            jupiter_faucet_principal_text: Some(Principal::from_slice(&[106; 29]).to_text()),
            io_nns_neuron_manager_principal_text: None,
            icp_ledger_principal_text: Some(sns.nns_ledger.to_text()),
            icp_index_principal_text: Some(sns.nns_index.to_text()),
            io_ledger_principal_text: Some(sns.ledger.to_text()),
            io_index_principal_text: Some(sns.index.to_text()),
            io_sns_ledger_principal_text: Some(sns.ledger.to_text()),
            io_sns_index_principal_text: Some(sns.index.to_text()),
            sns_governance_principal_text: Some(sns.governance.to_text()),
            production_wiring: None,
        },
        nns_neuron_manager: NnsNeuronManagerInitArgs {
            controller_canister_principal_text: sns.governance.to_text(),
            two_year_nns_neuron_id: LOCAL_TWO_YEAR_NEURON_ID,
            two_week_dissolve_seconds: 14 * 24 * 60 * 60,
            initial_two_year_principal_e8s: 0,
            initial_two_week_principal_e8s: 0,
            model_annual_bps: 0,
            io_stream_manager_principal_text: stream_manager.map(|principal| principal.to_text()),
            two_year_maturity_memo: Some(2_000_001),
            two_week_maturity_memo: Some(2_000_002),
            principal_unwind_memo: Some(2_000_003),
            nns_governance_principal_text: Some(sns.nns_governance.to_text()),
            icp_ledger_principal_text: Some(sns.nns_ledger.to_text()),
            icp_index_principal_text: Some(sns.nns_index.to_text()),
            production_wiring: None,
        },
    }
}

pub fn validate_io_real_stack_install_args(
    args: &IoRealStackInstallArgs,
) -> Result<(), IoRealStackError> {
    let mut principal_fields = vec![
        (
            "stream_manager.jupiter_faucet_principal_text",
            args.stream_manager.jupiter_faucet_principal_text.as_deref(),
        ),
        (
            "stream_manager.io_nns_neuron_manager_principal_text",
            args.stream_manager
                .io_nns_neuron_manager_principal_text
                .as_deref(),
        ),
        (
            "stream_manager.icp_ledger_principal_text",
            args.stream_manager.icp_ledger_principal_text.as_deref(),
        ),
        (
            "stream_manager.icp_index_principal_text",
            args.stream_manager.icp_index_principal_text.as_deref(),
        ),
        (
            "stream_manager.io_ledger_principal_text",
            args.stream_manager.io_ledger_principal_text.as_deref(),
        ),
        (
            "stream_manager.io_index_principal_text",
            args.stream_manager.io_index_principal_text.as_deref(),
        ),
        (
            "stream_manager.io_sns_ledger_principal_text",
            args.stream_manager.io_sns_ledger_principal_text.as_deref(),
        ),
        (
            "stream_manager.io_sns_index_principal_text",
            args.stream_manager.io_sns_index_principal_text.as_deref(),
        ),
        (
            "stream_manager.sns_governance_principal_text",
            args.stream_manager.sns_governance_principal_text.as_deref(),
        ),
        (
            "nns_neuron_manager.controller_canister_principal_text",
            Some(
                args.nns_neuron_manager
                    .controller_canister_principal_text
                    .as_str(),
            ),
        ),
        (
            "nns_neuron_manager.io_stream_manager_principal_text",
            args.nns_neuron_manager
                .io_stream_manager_principal_text
                .as_deref(),
        ),
        (
            "nns_neuron_manager.nns_governance_principal_text",
            args.nns_neuron_manager
                .nns_governance_principal_text
                .as_deref(),
        ),
        (
            "nns_neuron_manager.icp_ledger_principal_text",
            args.nns_neuron_manager.icp_ledger_principal_text.as_deref(),
        ),
        (
            "nns_neuron_manager.icp_index_principal_text",
            args.nns_neuron_manager.icp_index_principal_text.as_deref(),
        ),
    ];

    for (field, value) in principal_fields.drain(..) {
        if let Some(value) = value {
            validate_local_principal(field, value)?;
        }
    }

    if args.nns_neuron_manager.two_year_nns_neuron_id == PROTECTED_IO_NNS_NEURON_ID {
        return Err(IoRealStackError::ForbiddenNeuron {
            field: "nns_neuron_manager.two_year_nns_neuron_id",
            value: args.nns_neuron_manager.two_year_nns_neuron_id,
            reason: "protected IO NNS neuron",
        });
    }

    Ok(())
}

pub fn assert_io_real_stack_uses_finalized_sns_ids(stack: &IoRealStackFixture) {
    assert_eq!(
        stack
            .install_args
            .stream_manager
            .io_sns_ledger_principal_text
            .as_deref(),
        Some(stack.sns.ledger.to_text().as_str())
    );
    assert_eq!(
        stack
            .install_args
            .stream_manager
            .io_sns_index_principal_text
            .as_deref(),
        Some(stack.sns.index.to_text().as_str())
    );
    assert_eq!(
        stack
            .install_args
            .stream_manager
            .sns_governance_principal_text
            .as_deref(),
        Some(stack.sns.governance.to_text().as_str())
    );
    assert_eq!(
        stack
            .install_args
            .nns_neuron_manager
            .nns_governance_principal_text
            .as_deref(),
        Some(stack.sns.nns_governance.to_text().as_str())
    );
    assert_eq!(
        stack
            .install_args
            .nns_neuron_manager
            .icp_ledger_principal_text
            .as_deref(),
        Some(stack.sns.nns_ledger.to_text().as_str())
    );
}

fn validate_local_principal(field: &'static str, value: &str) -> Result<(), IoRealStackError> {
    if value == PROTECTED_IO_NEURON_OWNER_CANISTER {
        return Err(IoRealStackError::ForbiddenPrincipal {
            field,
            value: value.to_string(),
            reason: "protected IO neuron-owner canister",
        });
    }
    if [
        PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
        PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
        PRODUCTION_IO_HISTORIAN_CANISTER_ID,
        PRODUCTION_FRONTEND_CANISTER_ID,
    ]
    .contains(&value)
    {
        return Err(IoRealStackError::ForbiddenPrincipal {
            field,
            value: value.to_string(),
            reason: "production fiduciary canister",
        });
    }
    if [
        DEV_MAINNET_FRONTEND_CANISTER_ID,
        DEV_MAINNET_HISTORIAN_CANISTER_ID,
    ]
    .contains(&value)
    {
        return Err(IoRealStackError::ForbiddenPrincipal {
            field,
            value: value.to_string(),
            reason: "DevMainnet canister",
        });
    }
    Ok(())
}

fn assert_io_real_stack_on_application_subnet(stack: &IoRealStackFixture) {
    assert_all_canisters_on_expected_subnets(&stack.sns).expect("SNS fixture subnets are valid");
    for canister in [
        stack.stream_manager,
        stack.nns_neuron_manager,
        stack.historian,
    ] {
        assert_eq!(
            stack.sns.pic.get_subnet(canister),
            Some(stack.sns.application_subnet),
            "IO canister {canister} should be installed on application subnet"
        );
    }
}

fn create_application_canister_on_subnet(
    pic: &PocketIc,
    application_subnet: Principal,
    wasm: Vec<u8>,
    arg: Vec<u8>,
) -> Principal {
    let canister = create_funded_application_canister_on_subnet(pic, application_subnet);
    pic.install_canister(canister, wasm, arg, None);
    canister
}

fn create_funded_application_canister_on_subnet(
    pic: &PocketIc,
    application_subnet: Principal,
) -> Principal {
    let canister = pic.create_canister_on_subnet(None, None, application_subnet);
    pic.add_cycles(canister, APP_CANISTER_CYCLES);
    canister
}

fn required_io_wasm(
    name: &'static str,
    env_var: &'static str,
    default_paths: &[&str],
) -> Result<Vec<u8>, IoRealStackError> {
    let mut tried = Vec::new();
    if let Some(path) = std::env::var_os(env_var) {
        let path = path.to_string_lossy().into_owned();
        if let Some(bytes) = read_wasm_candidate(&path, &mut tried) {
            return Ok(bytes);
        }
    }
    for path in default_paths {
        if let Some(bytes) = read_wasm_candidate(path, &mut tried) {
            return Ok(bytes);
        }
    }
    Err(IoRealStackError::MissingWasm { name, tried })
}

fn read_wasm_candidate(path: &str, tried: &mut Vec<String>) -> Option<Vec<u8>> {
    let path = Path::new(path);
    tried.push(path.display().to_string());
    if let Ok(bytes) = std::fs::read(path) {
        return Some(bytes);
    }
    if path.is_relative() {
        let workspace_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path);
        tried.push(workspace_path.display().to_string());
        if let Ok(bytes) = std::fs::read(workspace_path) {
            return Some(bytes);
        }
    }
    None
}

#[cfg(test)]
fn finalized_governance_expected_active_stake_e8s(stack: &IoRealStackFixture) -> u128 {
    crate::sns_lifecycle::list_finalized_sns_neurons(&stack.sns)
        .expect("finalized SNS governance should list neurons")
        .into_iter()
        .filter(|neuron| neuron.cached_neuron_stake_e8s > 0)
        .filter(|neuron| {
            matches!(
                neuron.dissolve_state,
                Some(crate::sns_governance_setup::DissolveState::DissolveDelaySeconds(seconds))
                    if seconds >= TWO_WEEK_DISSOLVE_DELAY_SECONDS
            )
        })
        .map(|neuron| u128::from(neuron.cached_neuron_stake_e8s))
        .sum()
}

#[cfg(test)]
fn finalized_governance_expected_reward_neuron_ids(
    stack: &IoRealStackFixture,
) -> Vec<crate::sns_governance_setup::NeuronId> {
    crate::sns_lifecycle::list_finalized_sns_neurons(&stack.sns)
        .expect("finalized SNS governance should list neurons")
        .into_iter()
        .filter(|neuron| neuron.cached_neuron_stake_e8s > 0)
        .filter(|neuron| {
            matches!(
                neuron.dissolve_state,
                Some(crate::sns_governance_setup::DissolveState::DissolveDelaySeconds(seconds))
                    if seconds >= TWO_WEEK_DISSOLVE_DELAY_SECONDS
            )
        })
        .map(|neuron| {
            neuron
                .id
                .expect("eligible finalized SNS neuron should have an id")
        })
        .collect()
}

#[cfg(test)]
fn reward_id_for_sns_neuron_id(neuron_id: &crate::sns_governance_setup::NeuronId) -> u64 {
    io_reward_policy::sns_neuron_id_to_u64(&io_governance_types::SnsNeuronId(neuron_id.id.clone()))
        .expect("finalized SNS neuron id should map to reward key")
}

#[cfg(test)]
fn finalized_neuron_cached_stake_e8s(
    stack: &IoRealStackFixture,
    neuron_id: &crate::sns_governance_setup::NeuronId,
) -> u64 {
    crate::sns_lifecycle::list_finalized_sns_neurons(&stack.sns)
        .expect("finalized SNS governance should list neurons")
        .into_iter()
        .find(|neuron| neuron.id.as_ref() == Some(neuron_id))
        .expect("finalized SNS neuron should be listed")
        .cached_neuron_stake_e8s
}

#[cfg(test)]
fn stream_manager_tick(stack: &IoRealStackFixture) -> io_stream_manager::DebugTickOutcome {
    let bytes = stack
        .sns
        .pic
        .update_call(
            stack.stream_manager,
            Principal::anonymous(),
            "debug_tick",
            candid::encode_one(()).expect("debug_tick arg encode"),
        )
        .expect("stream-manager debug_tick should succeed");
    candid::decode_one(&bytes).expect("stream-manager debug_tick response decode")
}

#[cfg(test)]
fn stream_manager_tick_traps(stack: &IoRealStackFixture) -> bool {
    stack
        .sns
        .pic
        .update_call(
            stack.stream_manager,
            Principal::anonymous(),
            "debug_tick",
            candid::encode_one(()).expect("debug_tick arg encode"),
        )
        .is_err()
}

#[cfg(test)]
fn stream_manager_set_failpoint(
    stack: &IoRealStackFixture,
    failpoint: Option<io_stream_manager::DebugFailpoint>,
) {
    stack
        .sns
        .pic
        .update_call(
            stack.stream_manager,
            Principal::anonymous(),
            "debug_set_failpoint",
            candid::encode_one(failpoint).expect("debug_set_failpoint arg encode"),
        )
        .expect("stream-manager debug_set_failpoint should succeed");
}

#[cfg(test)]
fn stream_manager_state(stack: &IoRealStackFixture) -> io_stream_manager::ApiState {
    let bytes = stack
        .sns
        .pic
        .query_call(
            stack.stream_manager,
            Principal::anonymous(),
            "debug_get_state",
            candid::encode_one(()).expect("debug_get_state arg encode"),
        )
        .expect("stream-manager debug_get_state should succeed");
    candid::decode_one(&bytes).expect("stream-manager debug_get_state response decode")
}

#[cfg(test)]
fn stream_manager_stable_state(stack: &IoRealStackFixture) -> io_stream_manager::StableState {
    let bytes = stack
        .sns
        .pic
        .query_call(
            stack.stream_manager,
            Principal::anonymous(),
            "debug_get_stable_state",
            candid::encode_one(()).expect("debug_get_stable_state arg encode"),
        )
        .expect("stream-manager debug_get_stable_state should succeed");
    candid::decode_one(&bytes).expect("stream-manager debug_get_stable_state response decode")
}

#[cfg(test)]
fn stream_manager_redemption_operation(
    stack: &IoRealStackFixture,
    block: &candid::Nat,
) -> io_stream_manager::StreamOperation {
    let block = block
        .0
        .to_str_radix(10)
        .parse::<u64>()
        .expect("redemption block should fit in u64");
    let stable = stream_manager_stable_state(stack);
    stable
        .operation_journal
        .into_iter()
        .find(|op| op.operation_id == format!("io:{block}"))
        .expect("redemption operation should be journaled")
}

#[cfg(test)]
fn upgrade_stream_manager_same_wasm(stack: &IoRealStackFixture) {
    let wasm = required_io_wasm(
        "io_stream_manager",
        "IO_STREAM_MANAGER_WASM",
        &[
            "target/wasm32-unknown-unknown/debug/io_stream_manager.wasm",
            "release-artifacts/io_stream_manager.wasm",
        ],
    )
    .expect("stream-manager Wasm should be available for same-Wasm upgrade");
    crate::pocketic_env::upgrade_canister(&stack.sns.pic, stack.stream_manager, wasm, vec![]);
}

#[cfg(test)]
fn api_redeemable_io_e8s(protocol: &io_stream_manager::ApiProtocolState) -> u128 {
    protocol
        .total_io_supply_e8s
        .saturating_sub(protocol.protocol_reserve_io_e8s)
        .saturating_sub(protocol.non_redeemable_governance_io_e8s)
}

#[cfg(test)]
fn nat_to_u128(value: &candid::Nat, field: &str) -> u128 {
    value
        .0
        .to_str_radix(10)
        .parse::<u128>()
        .unwrap_or_else(|err| panic!("{field} should fit in u128: {err}"))
}

#[cfg(test)]
fn reserve_account_for_stack(stack: &IoRealStackFixture) -> IcrcAccount {
    reserve_account_for_stream_manager(stack.stream_manager)
}

#[cfg(test)]
fn reserve_account_for_stream_manager(stream_manager: Principal) -> IcrcAccount {
    crate::icrc::account(
        stream_manager,
        Some(crate::icrc::subaccount(
            io_stream_manager::scheduler::PROTOCOL_RESERVE_ACCOUNT,
        )),
    )
}

#[cfg(test)]
fn jupiter_io_recipient_account() -> IcrcAccount {
    io_stream_manager::clients::io_ledger::mock_account(
        io_stream_manager::state::JUPITER_FAUCET_SOURCE,
    )
    .to_icrc_account()
}

#[cfg(test)]
fn redemption_io_account_for_stack(stack: &IoRealStackFixture) -> IcrcAccount {
    crate::icrc::account(
        stack.stream_manager,
        Some(crate::icrc::subaccount(
            io_stream_manager::scheduler::REDEMPTION_ACCOUNT,
        )),
    )
}

#[cfg(test)]
fn jupiter_icp_account() -> Account {
    io_stream_manager::clients::icp_ledger::mock_account(
        io_stream_manager::state::JUPITER_FAUCET_SOURCE,
    )
}

#[cfg(test)]
fn icp_account_balance_e8s(stack: &IoRealStackFixture, account: &Account) -> u64 {
    let balance: IcpTokens = crate::icrc::query_one(
        &stack.sns.pic,
        stack.sns.nns_ledger,
        "account_balance",
        IcpAccountBalanceArgs {
            account: account.icp_account_identifier_bytes().to_vec(),
        },
    );
    balance.e8s
}

#[cfg(test)]
fn wait_for_real_indexes(stack: &IoRealStackFixture) {
    stack.sns.pic.advance_time(Duration::from_secs(5));
    for _ in 0..80 {
        stack.sns.pic.tick();
    }
}

#[cfg(test)]
fn fund_real_sns_protocol_reserve_for_issuance(
    stack: &IoRealStackFixture,
    participant: Principal,
    amount_e8s: u64,
) {
    fund_real_sns_protocol_reserve_account_for_issuance(
        &stack.sns,
        stack.stream_manager,
        participant,
        amount_e8s,
    );
}

#[cfg(test)]
fn fund_real_sns_protocol_reserve_account_for_issuance(
    sns: &FinalizedSnsLifecycleFixture,
    stream_manager: Principal,
    participant: Principal,
    amount_e8s: u64,
) {
    let _disbursed =
        crate::sns_lifecycle::disburse_zero_delay_neuron_to_participant_for_test(sns, participant)
            .expect("finalized SNS neuron should disburse liquid tokens for reserve funding");
    let reserve = reserve_account_for_stream_manager(stream_manager);
    let transfer = crate::icrc::icrc1_transfer(
        &sns.pic,
        sns.ledger,
        participant,
        crate::icrc::transfer_arg(
            None,
            reserve.clone(),
            amount_e8s,
            Some(crate::icrc::FEE_E8S),
            Some(b"io-real-stack-reserve"),
            None,
        ),
    )
    .expect("participant should fund stream-manager protocol reserve on real SNS ledger");
    assert!(transfer.0 > 0_u32.into());
    for _ in 0..20 {
        sns.pic.tick();
    }
    let balance = crate::icrc::icrc1_balance_of(&sns.pic, sns.ledger, reserve);
    assert!(
        balance.0 >= amount_e8s.into(),
        "reserve balance {balance:?} should cover issuance amount {amount_e8s}"
    );
}

#[cfg(test)]
fn icp_transfer(
    stack: &IoRealStackFixture,
    from_subaccount: Option<[u8; 32]>,
    to: Account,
    amount_e8s: u64,
) -> u64 {
    let transfer: Result<u64, IcpTransferError> = crate::icrc::update_one(
        &stack.sns.pic,
        stack.sns.nns_ledger,
        Principal::anonymous(),
        "transfer",
        IcpTransferArgs {
            memo: 0,
            amount: IcpTokens { e8s: amount_e8s },
            fee: IcpTokens {
                e8s: ICP_LEDGER_TRANSFER_FEE_E8S,
            },
            from_subaccount: from_subaccount.map(|subaccount| subaccount.to_vec()),
            to: to.icp_account_identifier_bytes().to_vec(),
            created_at_time: None,
        },
    );
    transfer.expect("real local NNS ledger transfer should succeed")
}

#[cfg(test)]
fn fund_real_jupiter_deposit(stack: &IoRealStackFixture, amount_e8s: u64) -> u64 {
    let jupiter_account = io_stream_manager::clients::icp_ledger::mock_account(
        io_stream_manager::state::JUPITER_FAUCET_SOURCE,
    );
    let deposit_account = Account::new(
        stack.stream_manager,
        Some(io_stream_manager::clients::icp_ledger::mock_subaccount(
            io_stream_manager::scheduler::STREAM_MANAGER_DEPOSIT_ACCOUNT,
        )),
    );
    icp_transfer(
        stack,
        None,
        jupiter_account,
        amount_e8s + ICP_LEDGER_TRANSFER_FEE_E8S,
    );
    let jupiter_subaccount = io_stream_manager::clients::icp_ledger::mock_subaccount(
        io_stream_manager::state::JUPITER_FAUCET_SOURCE,
    )
    .0;
    let block = icp_transfer(stack, Some(jupiter_subaccount), deposit_account, amount_e8s);
    wait_for_real_indexes(stack);
    block
}

#[cfg(test)]
fn fund_real_two_week_maturity_deposit(stack: &IoRealStackFixture, amount_e8s: u64) -> candid::Nat {
    let source_account = io_stream_manager::clients::icp_ledger::mock_account(
        io_stream_manager::state::IO_NNS_NEURON_MANAGER_SOURCE,
    );
    icp_transfer(
        stack,
        None,
        source_account,
        amount_e8s + ICP_LEDGER_TRANSFER_FEE_E8S,
    );
    let source_subaccount = io_stream_manager::clients::icp_ledger::mock_subaccount(
        io_stream_manager::state::IO_NNS_NEURON_MANAGER_SOURCE,
    )
    .0;
    let deposit_account = crate::icrc::account(
        stack.stream_manager,
        Some(crate::icrc::subaccount(
            io_stream_manager::scheduler::STREAM_MANAGER_DEPOSIT_ACCOUNT,
        )),
    );
    let block = crate::icrc::icrc1_transfer(
        &stack.sns.pic,
        stack.sns.nns_ledger,
        Principal::anonymous(),
        crate::icrc::transfer_arg(
            Some(source_subaccount),
            deposit_account,
            amount_e8s,
            Some(ICP_LEDGER_TRANSFER_FEE_E8S),
            Some(io_stream_manager::state::TWO_WEEK_MATURITY_MEMO.as_bytes()),
            None,
        ),
    )
    .expect("local ICP ledger should accept ICRC two-week maturity transfer");
    wait_for_real_indexes(stack);
    block
}

#[cfg(test)]
fn fund_real_two_year_maturity_deposit(stack: &IoRealStackFixture, amount_e8s: u64) -> candid::Nat {
    let source_account = io_stream_manager::clients::icp_ledger::mock_account(
        io_stream_manager::state::IO_NNS_NEURON_MANAGER_SOURCE,
    );
    icp_transfer(
        stack,
        None,
        source_account,
        amount_e8s + ICP_LEDGER_TRANSFER_FEE_E8S,
    );
    let source_subaccount = io_stream_manager::clients::icp_ledger::mock_subaccount(
        io_stream_manager::state::IO_NNS_NEURON_MANAGER_SOURCE,
    )
    .0;
    let deposit_account = crate::icrc::account(
        stack.stream_manager,
        Some(crate::icrc::subaccount(
            io_stream_manager::scheduler::STREAM_MANAGER_DEPOSIT_ACCOUNT,
        )),
    );
    let block = crate::icrc::icrc1_transfer(
        &stack.sns.pic,
        stack.sns.nns_ledger,
        Principal::anonymous(),
        crate::icrc::transfer_arg(
            Some(source_subaccount),
            deposit_account,
            amount_e8s,
            Some(ICP_LEDGER_TRANSFER_FEE_E8S),
            Some(io_stream_manager::state::TWO_YEAR_MATURITY_MEMO.as_bytes()),
            None,
        ),
    )
    .expect("local ICP ledger should accept ICRC two-year maturity transfer");
    wait_for_real_indexes(stack);
    block
}

#[cfg(test)]
fn transfer_real_io_to_redemption_account(
    stack: &IoRealStackFixture,
    amount_e8s: u64,
) -> candid::Nat {
    let block = transfer_real_io_to_redemption_account_without_index_wait(stack, amount_e8s);
    wait_for_real_indexes(stack);
    block
}

#[cfg(test)]
fn transfer_real_io_to_redemption_account_without_index_wait(
    stack: &IoRealStackFixture,
    amount_e8s: u64,
) -> candid::Nat {
    crate::icrc::icrc1_transfer(
        &stack.sns.pic,
        stack.sns.ledger,
        Principal::anonymous(),
        crate::icrc::transfer_arg(
            Some(crate::icrc::subaccount(
                io_stream_manager::state::JUPITER_FAUCET_SOURCE,
            )),
            redemption_io_account_for_stack(stack),
            amount_e8s,
            Some(crate::icrc::FEE_E8S),
            Some(b"io-real-stack-redemption"),
            None,
        ),
    )
    .expect("Jupiter IO account should transfer redeemed IO to stream-manager redemption account")
}

#[cfg(test)]
fn transfer_participant_io_to_redemption_account(
    stack: &IoRealStackFixture,
    participant: Principal,
    amount_e8s: u64,
) -> candid::Nat {
    let block = transfer_participant_io_to_redemption_account_without_index_wait(
        stack,
        participant,
        amount_e8s,
    );
    wait_for_real_indexes(stack);
    block
}

#[cfg(test)]
fn transfer_participant_io_to_redemption_account_without_index_wait(
    stack: &IoRealStackFixture,
    participant: Principal,
    amount_e8s: u64,
) -> candid::Nat {
    crate::icrc::icrc1_transfer(
        &stack.sns.pic,
        stack.sns.ledger,
        participant,
        crate::icrc::transfer_arg(
            None,
            redemption_io_account_for_stack(stack),
            amount_e8s,
            Some(crate::icrc::FEE_E8S),
            Some(b"io-real-redeem-participant"),
            None,
        ),
    )
    .expect("participant should transfer real SNS tokens to stream-manager redemption account")
}

#[cfg(test)]
fn wait_for_real_sns_redemption_index_transaction(
    stack: &IoRealStackFixture,
    amount_e8s: u64,
) -> crate::icrc::GetTransactionsResult {
    let account = redemption_io_account_for_stack(stack);
    for _ in 0..12 {
        let balance =
            crate::icrc::icrc1_balance_of(&stack.sns.pic, stack.sns.ledger, account.clone());
        let page = crate::icrc::get_account_transactions(
            &stack.sns.pic,
            stack.sns.index,
            account.clone(),
            None,
            20,
        )
        .expect("finalized SNS index should answer redemption account history");
        let has_expected_transfer = page.transactions.iter().any(|tx| {
            tx.transaction
                .transfer
                .as_ref()
                .map(|transfer| transfer.to == account && transfer.amount == amount_e8s)
                .unwrap_or(false)
        });
        if has_expected_transfer {
            return page;
        }
        assert!(
            balance.0 >= amount_e8s.into(),
            "finalized SNS ledger balance for redemption account should include transfer before waiting for index; balance={balance:?}, expected={amount_e8s}"
        );
        stack.sns.pic.advance_time(Duration::from_secs(5));
        for _ in 0..80 {
            stack.sns.pic.tick();
        }
    }
    let balance = crate::icrc::icrc1_balance_of(&stack.sns.pic, stack.sns.ledger, account.clone());
    let page =
        crate::icrc::get_account_transactions(&stack.sns.pic, stack.sns.index, account, None, 20)
            .expect("finalized SNS index should answer redemption account history after wait");
    panic!(
        "finalized SNS index did not expose redemption transfer; balance={balance:?}, transactions={:?}",
        page.transactions
    );
}

#[cfg(test)]
fn wait_for_real_sns_refund_index_transaction(
    stack: &IoRealStackFixture,
    sender: IcrcAccount,
    amount_e8s: u64,
) -> crate::icrc::TransactionWithId {
    let refund_source = redemption_io_account_for_stack(stack);
    for _ in 0..12 {
        let page = crate::icrc::get_account_transactions(
            &stack.sns.pic,
            stack.sns.index,
            sender.clone(),
            None,
            50,
        )
        .expect("finalized SNS index should answer sender account history");
        if let Some(tx) = page.transactions.iter().find(|tx| {
            tx.transaction
                .transfer
                .as_ref()
                .map(|transfer| {
                    transfer.from == refund_source
                        && transfer.to == sender
                        && transfer.amount == amount_e8s
                })
                .unwrap_or(false)
        }) {
            return tx.clone();
        }
        stack.sns.pic.advance_time(Duration::from_secs(5));
        for _ in 0..80 {
            stack.sns.pic.tick();
        }
    }
    let page =
        crate::icrc::get_account_transactions(&stack.sns.pic, stack.sns.index, sender, None, 50)
            .expect("finalized SNS index should answer sender account history after wait");
    panic!(
        "finalized SNS index did not expose rejected-refund transfer; transactions={:?}",
        page.transactions
    );
}

#[cfg(test)]
fn count_real_sns_refund_transfers(
    stack: &IoRealStackFixture,
    sender: IcrcAccount,
    amount_e8s: u64,
) -> usize {
    let refund_source = redemption_io_account_for_stack(stack);
    crate::icrc::get_account_transactions(&stack.sns.pic, stack.sns.index, sender.clone(), None, 50)
        .expect("finalized SNS index should answer sender account history")
        .transactions
        .into_iter()
        .filter(|tx| {
            tx.transaction
                .transfer
                .as_ref()
                .map(|transfer| {
                    transfer.from == refund_source
                        && transfer.to == sender
                        && transfer.amount == amount_e8s
                })
                .unwrap_or(false)
        })
        .count()
}

#[cfg(test)]
fn count_real_sns_reward_transfers_to_neuron(
    stack: &IoRealStackFixture,
    neuron_id: &crate::sns_governance_setup::NeuronId,
    amount_e8s: u64,
) -> usize {
    let destination = crate::icrc::account(
        stack.sns.governance,
        Some(
            neuron_id
                .id
                .clone()
                .try_into()
                .expect("finalized SNS neuron id should be 32 bytes"),
        ),
    );
    let reserve = reserve_account_for_stack(stack);
    crate::icrc::get_account_transactions(
        &stack.sns.pic,
        stack.sns.index,
        destination.clone(),
        None,
        50,
    )
    .expect("finalized SNS index should answer reward destination account history")
    .transactions
    .into_iter()
    .filter(|tx| {
        tx.transaction
            .transfer
            .as_ref()
            .map(|transfer| {
                transfer.from == reserve
                    && transfer.to == destination
                    && transfer.amount == amount_e8s
            })
            .unwrap_or(false)
    })
    .count()
}

#[cfg(test)]
fn count_real_sns_protocol_reserve_reward_outgoing_transfers(stack: &IoRealStackFixture) -> usize {
    let reserve = reserve_account_for_stack(stack);
    crate::icrc::get_account_transactions(
        &stack.sns.pic,
        stack.sns.index,
        reserve.clone(),
        None,
        50,
    )
    .expect("finalized SNS index should answer protocol reserve account history")
    .transactions
    .into_iter()
    .filter(|tx| {
        tx.transaction
            .transfer
            .as_ref()
            .map(|transfer| transfer.from == reserve && transfer.to.owner == stack.sns.governance)
            .unwrap_or(false)
    })
    .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sns_lifecycle::{
        configure_finalized_neuron_dissolve_delay_for_test,
        deploy_finalized_sns_lifecycle_fixture_with_participants_for_test,
        disburse_zero_delay_neuron_to_participant_for_test, finalized_motion_function_id_for_test,
        find_direct_participation_neurons, follow_finalized_sns_neuron_for_test,
        list_finalized_sns_proposals_as, make_finalized_motion_proposal_for_test,
        register_finalized_sns_vote_for_test, set_finalized_sns_governance_following_for_test,
        stake_finalized_liquid_sns_tokens_for_test, start_finalized_neuron_dissolving_for_test,
    };
    use std::collections::BTreeSet;

    fn fake_ids() -> FinalizedSnsCanisterIds {
        FinalizedSnsCanisterIds {
            nns_governance: Principal::from_slice(&[1; 29]),
            nns_ledger: Principal::from_slice(&[2; 29]),
            nns_index: Principal::from_slice(&[3; 29]),
            governance: Principal::from_slice(&[7; 29]),
            ledger: Principal::from_slice(&[8; 29]),
            index: Principal::from_slice(&[9; 29]),
        }
    }

    fn direct_participation_neuron_id(
        sns: &FinalizedSnsLifecycleFixture,
        participant: Principal,
    ) -> crate::sns_governance_setup::NeuronId {
        find_direct_participation_neurons(sns, participant)
            .expect("participant direct-participation neurons should list")
            .into_iter()
            .max_by_key(|neuron| neuron.cached_neuron_stake_e8s)
            .and_then(|neuron| neuron.id)
            .expect("participant should have a finalized direct-participation neuron")
    }

    fn stake_eligible_finalized_neuron(
        sns: &FinalizedSnsLifecycleFixture,
        participant: Principal,
        amount_e8s: u64,
        memo: u64,
    ) -> crate::sns_governance_setup::NeuronId {
        disburse_zero_delay_neuron_to_participant_for_test(sns, participant)
            .expect("zero-delay finalized neuron should fund normal staking");
        let neuron_id =
            stake_finalized_liquid_sns_tokens_for_test(sns, participant, amount_e8s, memo)
                .expect("participant should stake a finalized SNS neuron");
        configure_finalized_neuron_dissolve_delay_for_test(
            sns,
            participant,
            &neuron_id,
            TWO_WEEK_DISSOLVE_DELAY_SECONDS as u32,
        )
        .expect("finalized governance should accept two-week dissolve delay");
        neuron_id
    }

    fn close_finalized_motion_proposal(
        sns: &FinalizedSnsLifecycleFixture,
        caller: Principal,
        proposal_id: &crate::sns_governance_setup::ProposalId,
    ) {
        for _ in 0..60 {
            sns.pic.advance_time(Duration::from_secs(1_800));
            sns.pic.tick();
        }
        let proposals = list_finalized_sns_proposals_as(sns, caller, 100)
            .expect("finalized governance list_proposals should decode");
        let proposal = proposals
            .proposals
            .iter()
            .find(|proposal| proposal.id.as_ref() == Some(proposal_id))
            .expect("submitted proposal should be listed after close");
        assert!(
            proposal.decided_timestamp_seconds > 0,
            "proposal should be closed before stream-manager reward snapshot: {proposal:?}"
        );
    }

    fn completed_two_week_reward_operation(
        stack: &IoRealStackFixture,
        issued_e8s: u128,
    ) -> io_stream_manager::StreamOperation {
        stream_manager_stable_state(stack)
            .operation_journal
            .into_iter()
            .find(|op| {
                op.kind == io_stream_manager::StreamOperationKind::TwoWeekMaturityStream
                    && op.io_issued_e8s == issued_e8s
                    && op.phase == io_stream_manager::OperationPhase::Completed
            })
            .expect("completed two-week reward operation should be journaled")
    }

    fn transfer_real_sns_protocol_reserve_from_participant(
        stack: &IoRealStackFixture,
        participant: Principal,
        amount_e8s: u64,
    ) {
        let reserve = reserve_account_for_stack(stack);
        let transfer = crate::icrc::icrc1_transfer(
            &stack.sns.pic,
            stack.sns.ledger,
            participant,
            crate::icrc::transfer_arg(
                None,
                reserve.clone(),
                amount_e8s,
                Some(crate::icrc::FEE_E8S),
                Some(b"io-real-stack-reserve"),
                None,
            ),
        )
        .expect("participant should fund stream-manager protocol reserve on real SNS ledger");
        assert!(transfer.0 > 0_u32.into());
        for _ in 0..20 {
            stack.sns.pic.tick();
        }
        let balance = crate::icrc::icrc1_balance_of(&stack.sns.pic, stack.sns.ledger, reserve);
        assert!(
            balance.0 >= amount_e8s.into(),
            "reserve balance {balance:?} should cover issuance amount {amount_e8s}"
        );
    }

    #[test]
    fn io_real_stack_install_args_use_finalized_sns_canister_ids() {
        let ids = fake_ids();
        let args = build_io_real_stack_install_args_from_ids(&ids, None);

        assert_eq!(
            args.stream_manager.io_sns_ledger_principal_text,
            Some(ids.ledger.to_text())
        );
        assert_eq!(
            args.stream_manager.io_sns_index_principal_text,
            Some(ids.index.to_text())
        );
        assert_eq!(
            args.stream_manager.sns_governance_principal_text,
            Some(ids.governance.to_text())
        );
        assert_eq!(
            args.nns_neuron_manager.nns_governance_principal_text,
            Some(ids.nns_governance.to_text())
        );
        assert_eq!(
            args.stream_manager.icp_index_principal_text,
            Some(ids.nns_index.to_text())
        );
        assert_eq!(
            args.nns_neuron_manager.icp_index_principal_text,
            Some(ids.nns_index.to_text())
        );
    }

    #[test]
    fn io_real_stack_rejects_production_fiduciary_ids_in_install_args() {
        let ids = fake_ids();
        let mut args = build_io_real_stack_install_args_from_ids(&ids, None);
        args.stream_manager.io_sns_ledger_principal_text =
            Some(PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID.to_string());

        assert!(matches!(
            validate_io_real_stack_install_args(&args),
            Err(IoRealStackError::ForbiddenPrincipal {
                reason: "production fiduciary canister",
                ..
            })
        ));
    }

    #[test]
    fn io_real_stack_rejects_devmainnet_ids_in_install_args() {
        let ids = fake_ids();
        let mut args = build_io_real_stack_install_args_from_ids(&ids, None);
        args.stream_manager.io_sns_index_principal_text =
            Some(DEV_MAINNET_HISTORIAN_CANISTER_ID.to_string());

        assert!(matches!(
            validate_io_real_stack_install_args(&args),
            Err(IoRealStackError::ForbiddenPrincipal {
                reason: "DevMainnet canister",
                ..
            })
        ));
    }

    #[test]
    fn io_real_stack_rejects_protected_canister_and_neuron_targets() {
        let ids = fake_ids();
        let mut args = build_io_real_stack_install_args_from_ids(&ids, None);
        args.nns_neuron_manager.controller_canister_principal_text =
            PROTECTED_IO_NEURON_OWNER_CANISTER.to_string();
        assert!(matches!(
            validate_io_real_stack_install_args(&args),
            Err(IoRealStackError::ForbiddenPrincipal {
                reason: "protected IO neuron-owner canister",
                ..
            })
        ));

        let mut args = build_io_real_stack_install_args_from_ids(&ids, None);
        args.nns_neuron_manager.two_year_nns_neuron_id = PROTECTED_IO_NNS_NEURON_ID;
        assert!(matches!(
            validate_io_real_stack_install_args(&args),
            Err(IoRealStackError::ForbiddenNeuron {
                reason: "protected IO NNS neuron",
                ..
            })
        ));
    }

    #[test]
    fn io_real_stack_install_args_are_local_framework_only() {
        let ids = fake_ids();
        let args =
            build_io_real_stack_install_args_from_ids(&ids, Some(Principal::from_slice(&[12; 29])));
        validate_io_real_stack_install_args(&args).unwrap();
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_real_stack_installs_stream_manager_on_application_subnet() {
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        assert_eq!(
            stack.sns.pic.get_subnet(stack.stream_manager),
            Some(stack.sns.application_subnet)
        );
        assert_io_real_stack_uses_finalized_sns_ids(&stack);
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_real_stack_installs_nns_neuron_manager_on_application_subnet() {
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        assert_eq!(
            stack.sns.pic.get_subnet(stack.nns_neuron_manager),
            Some(stack.sns.application_subnet)
        );
        assert_io_real_stack_uses_finalized_sns_ids(&stack);
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_real_stack_installs_historian_on_application_subnet() {
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        assert_eq!(
            stack.sns.pic.get_subnet(stack.historian),
            Some(stack.sns.application_subnet)
        );
        assert_io_real_stack_uses_finalized_sns_ids(&stack);
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_finalized_sns_list_neurons_updates_active_staked_io() {
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        let expected_active_stake = finalized_governance_expected_active_stake_e8s(&stack);
        assert!(
            expected_active_stake > 0,
            "finalized SNS should expose eligible active stake before stream-manager refresh"
        );

        let before = stream_manager_state(&stack);
        assert_eq!(before.active_staked_io_e8s, 0);

        let outcome = stream_manager_tick(&stack);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);

        let after = stream_manager_state(&stack);
        assert_eq!(after.active_staked_io_e8s, expected_active_stake);
        assert_io_real_stack_uses_finalized_sns_ids(&stack);
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_jupiter_deposit_scanned_from_real_icp_index() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            JUPITER_EXPECTED_IO_E8S as u64 + crate::icrc::FEE_E8S,
        );
        let deposit_block = fund_real_jupiter_deposit(&stack, JUPITER_DEPOSIT_ICP_E8S);

        let before = stream_manager_state(&stack);
        let jupiter_io_before = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            jupiter_io_recipient_account(),
        );
        let outcome = stream_manager_tick(&stack);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.scanned_icp_transactions, 1);
        assert_eq!(outcome.processed_authorized_streams, 1);
        assert_eq!(outcome.io_issued_e8s, JUPITER_EXPECTED_IO_E8S);
        let jupiter_io_after = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            jupiter_io_recipient_account(),
        );
        assert_eq!(
            jupiter_io_after.0 - jupiter_io_before.0,
            JUPITER_EXPECTED_IO_E8S.into()
        );

        let after = stream_manager_state(&stack);
        assert_eq!(
            after.protocol.liquid_icp_e8s - before.protocol.liquid_icp_e8s,
            6_000_000_000
        );
        assert_eq!(
            after.protocol.two_year_staked_icp_e8s - before.protocol.two_year_staked_icp_e8s,
            4_000_000_000
        );
        assert_eq!(
            after.processed_transaction_count,
            before.processed_transaction_count + 1
        );

        let replay = stream_manager_tick(&stack);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.io_issued_e8s, 0);
        assert_eq!(
            stream_manager_state(&stack).processed_transaction_count,
            after.processed_transaction_count
        );
        assert!(
            deposit_block > 0,
            "deposit should be recorded on real NNS ledger before stream-manager scan"
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_redemption_pays_icp_on_real_local_ledger() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            JUPITER_EXPECTED_IO_E8S as u64 + crate::icrc::FEE_E8S,
        );
        fund_real_jupiter_deposit(&stack, JUPITER_DEPOSIT_ICP_E8S);
        let issuance = stream_manager_tick(&stack);
        assert!(issuance.errors.is_empty(), "{:?}", issuance.errors);
        assert_eq!(issuance.io_issued_e8s, JUPITER_EXPECTED_IO_E8S);

        let redemption_block =
            transfer_real_io_to_redemption_account(&stack, JUPITER_REDEMPTION_IO_E8S);
        let redemption_index =
            wait_for_real_sns_redemption_index_transaction(&stack, JUPITER_REDEMPTION_IO_E8S);
        assert!(
            !redemption_index.transactions.is_empty(),
            "redemption account history should not be empty before stream-manager scan"
        );
        let before = stream_manager_state(&stack);
        let jupiter_icp = jupiter_icp_account();
        let jupiter_icp_before = icp_account_balance_e8s(&stack, &jupiter_icp);
        let reserve_before = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            reserve_account_for_stack(&stack),
        );
        let actual_io_fee = crate::icrc::icrc1_fee(&stack.sns.pic, stack.sns.ledger);

        let redemption = stream_manager_tick(&stack);
        assert!(redemption.errors.is_empty(), "{:?}", redemption.errors);
        assert_eq!(redemption.scanned_io_transactions, 1);
        assert_eq!(redemption.processed_redemptions, 1);
        assert_eq!(redemption.icp_paid_e8s, JUPITER_EXPECTED_REDEMPTION_ICP_E8S);

        let after = stream_manager_state(&stack);
        let jupiter_icp_after = icp_account_balance_e8s(&stack, &jupiter_icp);
        assert_eq!(
            u128::from(jupiter_icp_after - jupiter_icp_before),
            JUPITER_EXPECTED_REDEMPTION_ICP_E8S
        );
        assert_eq!(
            before.protocol.liquid_icp_e8s - after.protocol.liquid_icp_e8s,
            JUPITER_EXPECTED_REDEMPTION_ICP_E8S
        );
        assert_eq!(
            before.protocol.protocol_reserve_io_e8s + u128::from(JUPITER_REDEMPTION_IO_E8S),
            after.protocol.protocol_reserve_io_e8s
        );
        let reserve_after = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            reserve_account_for_stack(&stack),
        );
        assert_eq!(
            reserve_after.0 - reserve_before.0,
            (candid::Nat::from(JUPITER_REDEMPTION_IO_E8S) - actual_io_fee).0
        );

        let replay = stream_manager_tick(&stack);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.processed_redemptions, 0);
        assert_eq!(
            stream_manager_state(&stack).processed_transaction_count,
            after.processed_transaction_count
        );
        assert!(
            redemption_block.0 > 0_u32.into(),
            "redemption should be recorded on the finalized SNS ledger before scan"
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_redemption_rounding_fee_dust_accounted() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            JUPITER_EXPECTED_IO_E8S as u64 + crate::icrc::FEE_E8S,
        );
        fund_real_jupiter_deposit(&stack, JUPITER_DEPOSIT_ICP_E8S);
        let issuance = stream_manager_tick(&stack);
        assert!(issuance.errors.is_empty(), "{:?}", issuance.errors);
        assert_eq!(issuance.io_issued_e8s, JUPITER_EXPECTED_IO_E8S);

        let reserve_before = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            reserve_account_for_stack(&stack),
        );
        let actual_io_fee = crate::icrc::icrc1_fee(&stack.sns.pic, stack.sns.ledger);
        let actual_io_fee_e8s = actual_io_fee
            .0
            .to_str_radix(10)
            .parse::<u128>()
            .expect("SNS ledger fee should fit u128");
        let before = stream_manager_state(&stack);
        let redemption_block =
            transfer_real_io_to_redemption_account(&stack, JUPITER_REDEMPTION_IO_E8S);
        let _redemption_index =
            wait_for_real_sns_redemption_index_transaction(&stack, JUPITER_REDEMPTION_IO_E8S);

        let redemption = stream_manager_tick(&stack);
        assert!(redemption.errors.is_empty(), "{:?}", redemption.errors);
        assert_eq!(redemption.processed_redemptions, 1);
        assert_eq!(redemption.icp_paid_e8s, JUPITER_EXPECTED_REDEMPTION_ICP_E8S);

        let op = stream_manager_redemption_operation(&stack, &redemption_block);
        assert_eq!(op.kind, io_stream_manager::StreamOperationKind::Redemption);
        assert_eq!(op.phase, io_stream_manager::OperationPhase::Completed);
        assert_eq!(op.io_amount, u128::from(JUPITER_REDEMPTION_IO_E8S));
        assert_eq!(op.io_return_fee_e8s, actual_io_fee_e8s);
        assert_eq!(
            op.io_return_status,
            io_stream_manager::TransferStatus::Succeeded
        );
        assert!(op.io_return_block.is_some());
        assert_eq!(op.gross_icp_payout_e8s, JUPITER_EXPECTED_REDEMPTION_ICP_E8S);
        assert_eq!(
            op.net_user_icp_payout_e8s,
            JUPITER_EXPECTED_REDEMPTION_ICP_E8S
        );

        let after = stream_manager_state(&stack);
        let reserve_after = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            reserve_account_for_stack(&stack),
        );
        assert_eq!(
            reserve_after.0 - reserve_before.0,
            (candid::Nat::from(JUPITER_REDEMPTION_IO_E8S) - actual_io_fee).0
        );
        assert_eq!(
            api_redeemable_io_e8s(&before.protocol) - api_redeemable_io_e8s(&after.protocol),
            u128::from(JUPITER_REDEMPTION_IO_E8S)
        );
        assert_eq!(
            before.protocol.total_io_supply_e8s, after.protocol.total_io_supply_e8s,
            "redemption returns existing IO to reserve and must not mint replacement supply"
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_redemption_reads_actual_sns_ledger_fee() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            JUPITER_EXPECTED_IO_E8S as u64 + crate::icrc::FEE_E8S,
        );
        fund_real_jupiter_deposit(&stack, JUPITER_DEPOSIT_ICP_E8S);
        let issuance = stream_manager_tick(&stack);
        assert!(issuance.errors.is_empty(), "{:?}", issuance.errors);

        let actual_fee = crate::icrc::icrc1_fee(&stack.sns.pic, stack.sns.ledger);
        let actual_fee_e8s = actual_fee
            .0
            .to_str_radix(10)
            .parse::<u128>()
            .expect("SNS ledger fee should fit u128");
        let redemption_block =
            transfer_real_io_to_redemption_account(&stack, JUPITER_REDEMPTION_IO_E8S);
        let _redemption_index =
            wait_for_real_sns_redemption_index_transaction(&stack, JUPITER_REDEMPTION_IO_E8S);

        let redemption = stream_manager_tick(&stack);
        assert!(redemption.errors.is_empty(), "{:?}", redemption.errors);
        let op = stream_manager_redemption_operation(&stack, &redemption_block);

        assert_eq!(op.io_return_fee_e8s, actual_fee_e8s);
    }

    #[test]
    #[ignore = "requires a real SNS ledger fixture that can change transfer_fee through a supported upgrade argument"]
    fn io_stream_manager_real_redemption_fee_change_is_observed_on_next_operation() {
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        let before = crate::icrc::icrc1_fee(&stack.sns.pic, stack.sns.ledger);
        let after = crate::icrc::icrc1_fee(&stack.sns.pic, stack.sns.ledger);

        assert_eq!(
            before, after,
            "the current pinned SNS ledger fixture has no supported runtime fee-change hook"
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_redemption_below_io_return_fee_fails_closed() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            JUPITER_EXPECTED_IO_E8S as u64 + crate::icrc::FEE_E8S,
        );
        fund_real_jupiter_deposit(&stack, JUPITER_DEPOSIT_ICP_E8S);
        let issuance = stream_manager_tick(&stack);
        assert!(issuance.errors.is_empty(), "{:?}", issuance.errors);
        assert_eq!(issuance.io_issued_e8s, JUPITER_EXPECTED_IO_E8S);

        let actual_io_fee = crate::icrc::icrc1_fee(&stack.sns.pic, stack.sns.ledger);
        let actual_io_fee_e8s = actual_io_fee
            .0
            .to_str_radix(10)
            .parse::<u64>()
            .expect("SNS ledger fee should fit u64");
        let dust_redemption_e8s = actual_io_fee_e8s - 1;
        let redemption_block = transfer_real_io_to_redemption_account(&stack, dust_redemption_e8s);
        let _redemption_index =
            wait_for_real_sns_redemption_index_transaction(&stack, dust_redemption_e8s);
        let before = stream_manager_state(&stack);
        let jupiter_icp = jupiter_icp_account();
        let jupiter_icp_before = icp_account_balance_e8s(&stack, &jupiter_icp);

        let redemption = stream_manager_tick(&stack);
        assert_eq!(redemption.scanned_io_transactions, 1);
        assert_eq!(redemption.processed_redemptions, 0);
        assert_eq!(redemption.icp_paid_e8s, 0);
        assert!(
            redemption
                .errors
                .iter()
                .any(|err| err.contains("not above IO return fee")),
            "{:?}",
            redemption.errors
        );

        let op = stream_manager_redemption_operation(&stack, &redemption_block);
        assert_eq!(op.kind, io_stream_manager::StreamOperationKind::Redemption);
        assert_eq!(op.phase, io_stream_manager::OperationPhase::FailedTerminal);
        assert_eq!(op.io_amount, u128::from(dust_redemption_e8s));
        assert_eq!(op.io_return_fee_e8s, u128::from(actual_io_fee_e8s));
        assert_eq!(
            op.icp_payout_status,
            io_stream_manager::TransferStatus::FailedTerminal
        );
        assert_eq!(op.icp_payout_block, None);
        assert_eq!(op.io_return_block, None);
        assert!(
            op.last_error
                .as_deref()
                .is_some_and(|err| err.contains("not above IO return fee")),
            "{op:?}"
        );

        let replay = stream_manager_tick(&stack);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.processed_redemptions, 0);
        assert_eq!(replay.icp_paid_e8s, 0);
        assert_eq!(
            icp_account_balance_e8s(&stack, &jupiter_icp),
            jupiter_icp_before,
            "sub-fee redemption must not trigger ICP payout"
        );
        assert_eq!(
            stream_manager_state(&stack).protocol,
            before.protocol,
            "terminal sub-fee redemption must preserve accounting state"
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_redemption_rejects_insufficient_redeemable_supply() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            JUPITER_EXPECTED_IO_E8S as u64 + crate::icrc::FEE_E8S,
        );
        fund_real_jupiter_deposit(&stack, JUPITER_DEPOSIT_ICP_E8S);
        let issuance = stream_manager_tick(&stack);
        assert!(issuance.errors.is_empty(), "{:?}", issuance.errors);
        assert_eq!(issuance.io_issued_e8s, JUPITER_EXPECTED_IO_E8S);

        let before = stream_manager_state(&stack);
        assert_eq!(
            api_redeemable_io_e8s(&before.protocol),
            JUPITER_EXPECTED_IO_E8S
        );
        let over_redeemable_e8s = JUPITER_EXPECTED_IO_E8S as u64 + 1;
        let actual_io_fee_e8s = crate::icrc::icrc1_fee(&stack.sns.pic, stack.sns.ledger)
            .0
            .to_str_radix(10)
            .parse::<u64>()
            .expect("SNS ledger fee should fit u64");
        let expected_refund_e8s = over_redeemable_e8s - actual_io_fee_e8s;
        let participant_account = crate::icrc::account(participant, None);
        let redemption_block = transfer_participant_io_to_redemption_account_without_index_wait(
            &stack,
            participant,
            over_redeemable_e8s,
        );
        let _redemption_index =
            wait_for_real_sns_redemption_index_transaction(&stack, over_redeemable_e8s);
        let jupiter_icp_before = icp_account_balance_e8s(&stack, &jupiter_icp_account());
        let sender_after_deposit = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            participant_account.clone(),
        );
        let redemption_account = redemption_io_account_for_stack(&stack);
        let redemption_after_deposit = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            redemption_account.clone(),
        );

        let redemption = stream_manager_tick(&stack);
        assert_eq!(redemption.scanned_io_transactions, 1);
        assert_eq!(redemption.processed_redemptions, 0);
        assert_eq!(redemption.icp_paid_e8s, 0);
        assert!(
            redemption
                .errors
                .iter()
                .any(|err| err.contains("InsufficientRedeemableSupply")),
            "{:?}",
            redemption.errors
        );

        let after = stream_manager_state(&stack);
        assert_eq!(after.protocol, before.protocol);
        let sender_after_refund = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            participant_account.clone(),
        );
        assert_eq!(
            sender_after_refund.0.clone() - sender_after_deposit.0,
            candid::Nat::from(expected_refund_e8s).0,
            "rejected over-redemption should refund exactly amount minus the real SNS fee"
        );
        let redemption_after_refund = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            redemption_account.clone(),
        );
        assert_eq!(
            redemption_after_deposit.0 - redemption_after_refund.0.clone(),
            candid::Nat::from(expected_refund_e8s + actual_io_fee_e8s).0,
            "redemption subaccount should decrease by refund plus fee"
        );
        assert_eq!(
            icp_account_balance_e8s(&stack, &jupiter_icp_account()),
            jupiter_icp_before,
            "over-redeemable transfer must not trigger ICP payout"
        );
        let redemption_block_index =
            u64::try_from(redemption_block.0.clone()).expect("redemption block index fits u64");
        let stable = stream_manager_stable_state(&stack);
        let rejected = stable
            .operation_journal
            .iter()
            .find(|op| {
                op.kind == io_stream_manager::StreamOperationKind::RejectedRedemption
                    && op.io_redemption_block == Some(redemption_block_index)
            })
            .expect("rejected over-redeemable transfer records source block evidence");
        assert_eq!(rejected.icp_payout_block, None);
        assert_eq!(
            rejected.phase,
            io_stream_manager::OperationPhase::Completed,
            "refunded rejection should be completed, not operationally failed"
        );
        assert_eq!(rejected.retry_count, 0);
        assert_eq!(
            rejected.icp_payout_status,
            io_stream_manager::TransferStatus::NotApplicable
        );
        assert_eq!(
            rejected.io_return_status,
            io_stream_manager::TransferStatus::Succeeded
        );
        assert_eq!(rejected.io_return_fee_e8s, u128::from(actual_io_fee_e8s));
        match &rejected.rejected_fund_disposition {
            Some(io_stream_manager::RejectedFundDisposition::ReturnToSenderSucceeded {
                amount_e8s,
                ..
            }) => assert_eq!(*amount_e8s, u128::from(expected_refund_e8s)),
            other => {
                panic!("resolvable rejected over-redemption should be refunded, got {other:?}")
            }
        }
        assert!(
            stable.operation_journal.iter().all(|op| {
                op.io_redemption_block != Some(redemption_block_index)
                    || op.kind == io_stream_manager::StreamOperationKind::RejectedRedemption
            }),
            "rejected over-redeemable transfer must not fabricate terminal success evidence"
        );

        let replay = stream_manager_tick(&stack);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.processed_redemptions, 0);
        assert_eq!(replay.icp_paid_e8s, 0);
        assert_eq!(
            crate::icrc::icrc1_balance_of(
                &stack.sns.pic,
                stack.sns.ledger,
                participant_account.clone()
            ),
            sender_after_refund,
            "replay must not send a second rejected refund"
        );

        upgrade_stream_manager_same_wasm(&stack);
        let after_upgrade_replay = stream_manager_tick(&stack);
        assert!(
            after_upgrade_replay.errors.is_empty(),
            "{:?}",
            after_upgrade_replay.errors
        );
        assert_eq!(after_upgrade_replay.processed_redemptions, 0);
        assert_eq!(after_upgrade_replay.icp_paid_e8s, 0);
        assert_eq!(
            crate::icrc::icrc1_balance_of(&stack.sns.pic, stack.sns.ledger, participant_account),
            sender_after_refund,
            "same-Wasm upgrade replay must not send a second rejected refund"
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_redemption_after_index_lag_waits_or_fails_closed() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            JUPITER_EXPECTED_IO_E8S as u64 + crate::icrc::FEE_E8S,
        );
        fund_real_jupiter_deposit(&stack, JUPITER_DEPOSIT_ICP_E8S);
        let issuance = stream_manager_tick(&stack);
        assert!(issuance.errors.is_empty(), "{:?}", issuance.errors);
        assert_eq!(issuance.io_issued_e8s, JUPITER_EXPECTED_IO_E8S);

        let before = stream_manager_state(&stack);
        let jupiter_icp = jupiter_icp_account();
        let jupiter_icp_before = icp_account_balance_e8s(&stack, &jupiter_icp);
        let redemption_block = transfer_real_io_to_redemption_account_without_index_wait(
            &stack,
            JUPITER_REDEMPTION_IO_E8S,
        );

        let lag_tick = stream_manager_tick(&stack);
        if lag_tick.processed_redemptions == 0 {
            assert_eq!(lag_tick.icp_paid_e8s, 0);
            assert_eq!(stream_manager_state(&stack).protocol, before.protocol);
            assert_eq!(
                icp_account_balance_e8s(&stack, &jupiter_icp),
                jupiter_icp_before,
                "pre-index-lag tick must not fabricate ICP payout"
            );
        } else {
            assert_eq!(lag_tick.processed_redemptions, 1);
            assert_eq!(lag_tick.icp_paid_e8s, JUPITER_EXPECTED_REDEMPTION_ICP_E8S);
        }

        let _redemption_index =
            wait_for_real_sns_redemption_index_transaction(&stack, JUPITER_REDEMPTION_IO_E8S);
        let catch_up = stream_manager_tick(&stack);
        assert!(catch_up.errors.is_empty(), "{:?}", catch_up.errors);

        let after = stream_manager_state(&stack);
        let processed_total = lag_tick.processed_redemptions + catch_up.processed_redemptions;
        assert_eq!(processed_total, 1);
        assert_eq!(
            icp_account_balance_e8s(&stack, &jupiter_icp) - jupiter_icp_before,
            JUPITER_EXPECTED_REDEMPTION_ICP_E8S as u64
        );
        assert_eq!(
            before.protocol.liquid_icp_e8s - after.protocol.liquid_icp_e8s,
            JUPITER_EXPECTED_REDEMPTION_ICP_E8S
        );
        let op = stream_manager_redemption_operation(&stack, &redemption_block);
        assert_eq!(op.phase, io_stream_manager::OperationPhase::Completed);
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO debug Wasm artifacts, and POCKET_IC_BIN"]
    fn real_stack_rejected_refund_too_old_waits_for_index_proof_no_double_refund() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            JUPITER_EXPECTED_IO_E8S as u64 + crate::icrc::FEE_E8S,
        );
        fund_real_jupiter_deposit(&stack, JUPITER_DEPOSIT_ICP_E8S);
        let issuance = stream_manager_tick(&stack);
        assert!(issuance.errors.is_empty(), "{:?}", issuance.errors);
        assert_eq!(issuance.io_issued_e8s, JUPITER_EXPECTED_IO_E8S);

        let before = stream_manager_state(&stack);
        let jupiter_icp_before = icp_account_balance_e8s(&stack, &jupiter_icp_account());
        let actual_io_fee_e8s = crate::icrc::icrc1_fee(&stack.sns.pic, stack.sns.ledger)
            .0
            .to_str_radix(10)
            .parse::<u64>()
            .expect("real SNS ledger fee fits u64");
        let over_redeemable_e8s =
            (api_redeemable_io_e8s(&before.protocol) as u64).saturating_add(crate::icrc::FEE_E8S);
        let expected_refund_e8s = over_redeemable_e8s
            .checked_sub(actual_io_fee_e8s)
            .expect("test amount should exceed fee");

        let participant_account = crate::icrc::account(participant, None);
        let sender_before_deposit = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            participant_account.clone(),
        );
        let redemption_account = redemption_io_account_for_stack(&stack);
        let redemption_before_deposit = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            redemption_account.clone(),
        );
        let redemption_block =
            transfer_participant_io_to_redemption_account(&stack, participant, over_redeemable_e8s);
        let sender_after_deposit = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            participant_account.clone(),
        );
        let redemption_after_deposit = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            redemption_account.clone(),
        );
        assert_eq!(
            sender_before_deposit.0 - sender_after_deposit.0.clone(),
            candid::Nat::from(over_redeemable_e8s + actual_io_fee_e8s).0
        );
        assert_eq!(
            redemption_after_deposit.0.clone() - redemption_before_deposit.0,
            candid::Nat::from(over_redeemable_e8s).0
        );

        stream_manager_set_failpoint(
            &stack,
            Some(io_stream_manager::DebugFailpoint::AfterRejectedRefundTransferBeforeJournalUpdate),
        );
        let mut trapped = false;
        for _ in 0..12 {
            if stream_manager_tick_traps(&stack) {
                trapped = true;
                break;
            }
            stack.sns.pic.advance_time(Duration::from_secs(5));
            for _ in 0..80 {
                stack.sns.pic.tick();
            }
        }
        assert!(
            trapped,
            "debug failpoint should trap after successful rejected refund transfer"
        );
        let sender_after_trap = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            participant_account.clone(),
        );
        assert_eq!(
            sender_after_trap.0.clone() - sender_after_deposit.0,
            candid::Nat::from(expected_refund_e8s).0,
            "trap path should still execute exactly one real SNS refund transfer"
        );
        let redemption_after_trap = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            redemption_account.clone(),
        );
        assert_eq!(
            redemption_after_deposit.0 - redemption_after_trap.0.clone(),
            candid::Nat::from(expected_refund_e8s + actual_io_fee_e8s).0,
            "redemption subaccount should pay refund plus real SNS ledger fee"
        );
        assert_eq!(
            icp_account_balance_e8s(&stack, &jupiter_icp_account()),
            jupiter_icp_before,
            "failed rejected-redemption recovery must not send ICP"
        );
        assert_eq!(stream_manager_state(&stack).protocol, before.protocol);

        stack
            .sns
            .pic
            .stop_canister(stack.sns.index, Some(stack.sns.root))
            .expect("local SNS index should stop for proof-pending failure injection");
        stack
            .sns
            .pic
            .advance_time(Duration::from_secs(30 * 24 * 60 * 60));
        let too_old = stream_manager_tick(&stack);
        assert_eq!(too_old.processed_redemptions, 0);
        assert_eq!(too_old.icp_paid_e8s, 0);
        assert!(
            too_old
                .errors
                .iter()
                .any(|err| err.contains("refund proof pending")),
            "{:?}",
            too_old.errors
        );
        let redemption_block_index =
            u64::try_from(redemption_block.0.clone()).expect("redemption block index fits u64");
        let proof_pending = stream_manager_redemption_operation(&stack, &redemption_block);
        assert_eq!(proof_pending.retry_count, 1);
        assert_eq!(
            proof_pending.phase,
            io_stream_manager::OperationPhase::AwaitingIoReturn
        );
        assert_eq!(
            proof_pending.io_return_status,
            io_stream_manager::TransferStatus::FailedRetryable
        );
        assert_eq!(
            proof_pending.icp_payout_status,
            io_stream_manager::TransferStatus::NotApplicable
        );
        assert!(matches!(
            proof_pending.rejected_fund_disposition,
            Some(io_stream_manager::RejectedFundDisposition::ReturnToSenderProofPending { .. })
        ));
        stack
            .sns
            .pic
            .start_canister(stack.sns.index, Some(stack.sns.root))
            .expect("local SNS index should restart for proof catch-up");

        let refund_index = wait_for_real_sns_refund_index_transaction(
            &stack,
            participant_account.clone(),
            expected_refund_e8s,
        );
        let mut completed = stream_manager_redemption_operation(&stack, &redemption_block);
        let mut last_proof = None;
        for _ in 0..5 {
            let proof = stream_manager_tick(&stack);
            assert_eq!(proof.processed_redemptions, 0);
            assert_eq!(proof.icp_paid_e8s, 0);
            last_proof = Some(proof);
            completed = stream_manager_redemption_operation(&stack, &redemption_block);
            if completed.phase == io_stream_manager::OperationPhase::Completed {
                break;
            }
            stack.sns.pic.advance_time(Duration::from_secs(5));
            for _ in 0..20 {
                stack.sns.pic.tick();
            }
        }
        assert_eq!(completed.retry_count, 1);
        assert_eq!(completed.io_redemption_block, Some(redemption_block_index));
        assert_eq!(
            completed.phase,
            io_stream_manager::OperationPhase::Completed,
            "proof reconciliation should complete after indexed refund; last outcome={last_proof:?}; op={completed:?}"
        );
        assert_eq!(
            completed.io_return_status,
            io_stream_manager::TransferStatus::Succeeded
        );
        assert_eq!(
            completed.icp_payout_status,
            io_stream_manager::TransferStatus::NotApplicable
        );
        let refund_block_index =
            u64::try_from(refund_index.id.0.clone()).expect("refund block index fits u64");
        assert_eq!(completed.io_return_block, Some(refund_block_index));
        match completed.rejected_fund_disposition {
            Some(io_stream_manager::RejectedFundDisposition::ReturnToSenderSucceeded {
                block_index,
                amount_e8s,
            }) => {
                assert_eq!(block_index, refund_block_index);
                assert_eq!(amount_e8s, u128::from(expected_refund_e8s));
            }
            other => panic!("expected proof reconciliation success, got {other:?}"),
        }
        assert_eq!(stream_manager_state(&stack).protocol, before.protocol);
        assert_eq!(
            count_real_sns_refund_transfers(
                &stack,
                participant_account.clone(),
                expected_refund_e8s
            ),
            1
        );

        let replay = stream_manager_tick(&stack);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.processed_redemptions, 0);
        assert_eq!(replay.icp_paid_e8s, 0);
        assert_eq!(
            count_real_sns_refund_transfers(
                &stack,
                participant_account.clone(),
                expected_refund_e8s
            ),
            1
        );
        let stable_before_upgrade = stream_manager_stable_state(&stack);
        upgrade_stream_manager_same_wasm(&stack);
        let stable_after_upgrade = stream_manager_stable_state(&stack);
        assert_eq!(
            stable_after_upgrade.operation_journal,
            stable_before_upgrade.operation_journal
        );
        assert_eq!(
            stable_after_upgrade.scheduler_cursors,
            stable_before_upgrade.scheduler_cursors
        );
        let replay_after_upgrade = stream_manager_tick(&stack);
        assert!(
            replay_after_upgrade.errors.is_empty(),
            "{:?}",
            replay_after_upgrade.errors
        );
        assert_eq!(replay_after_upgrade.processed_redemptions, 0);
        assert_eq!(replay_after_upgrade.icp_paid_e8s, 0);
        assert_eq!(
            count_real_sns_refund_transfers(&stack, participant_account, expected_refund_e8s),
            1
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn real_stack_same_wasm_upgrade_preserves_operation_journal() {
        let (before, after, replay) = run_real_stack_same_wasm_upgrade_after_redemption();
        assert_eq!(after.operation_journal, before.operation_journal);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.processed_redemptions, 0);
        assert_eq!(replay.icp_paid_e8s, 0);
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn real_stack_same_wasm_upgrade_preserves_scheduler_cursors() {
        let (before, after, replay) = run_real_stack_same_wasm_upgrade_after_redemption();
        assert_eq!(after.scheduler_cursors, before.scheduler_cursors);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.processed_redemptions, 0);
        assert_eq!(replay.icp_paid_e8s, 0);
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn real_stack_same_wasm_upgrade_preserves_processed_tx_set() {
        let (before, after, replay) = run_real_stack_same_wasm_upgrade_after_redemption();
        assert_eq!(after.processed_transactions, before.processed_transactions);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.processed_redemptions, 0);
        assert_eq!(replay.icp_paid_e8s, 0);
    }

    fn run_real_stack_same_wasm_upgrade_after_redemption() -> (
        io_stream_manager::StableState,
        io_stream_manager::StableState,
        io_stream_manager::DebugTickOutcome,
    ) {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            JUPITER_EXPECTED_IO_E8S as u64 + crate::icrc::FEE_E8S,
        );
        fund_real_jupiter_deposit(&stack, JUPITER_DEPOSIT_ICP_E8S);
        let issuance = stream_manager_tick(&stack);
        assert!(issuance.errors.is_empty(), "{:?}", issuance.errors);

        let redemption_block =
            transfer_real_io_to_redemption_account(&stack, JUPITER_REDEMPTION_IO_E8S);
        let _redemption_index =
            wait_for_real_sns_redemption_index_transaction(&stack, JUPITER_REDEMPTION_IO_E8S);
        let redemption = stream_manager_tick(&stack);
        assert!(redemption.errors.is_empty(), "{:?}", redemption.errors);
        assert_eq!(redemption.processed_redemptions, 1);

        let before = stream_manager_stable_state(&stack);
        let before_op = stream_manager_redemption_operation(&stack, &redemption_block);
        assert_eq!(
            before_op.phase,
            io_stream_manager::OperationPhase::Completed
        );

        upgrade_stream_manager_same_wasm(&stack);
        let after = stream_manager_stable_state(&stack);

        let replay = stream_manager_tick(&stack);
        (before, after, replay)
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_redemption_after_holder_yield_is_higher_than_genesis() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            JUPITER_EXPECTED_IO_E8S as u64 + crate::icrc::FEE_E8S,
        );
        let deposit_block = fund_real_jupiter_deposit(&stack, JUPITER_DEPOSIT_ICP_E8S);
        let maturity_block = fund_real_two_year_maturity_deposit(&stack, 1_000_000_000);
        let issuance_and_maturity = stream_manager_tick(&stack);
        assert!(
            issuance_and_maturity.errors.is_empty(),
            "{:?}",
            issuance_and_maturity.errors
        );
        assert_eq!(issuance_and_maturity.scanned_icp_transactions, 2);
        assert_eq!(issuance_and_maturity.processed_authorized_streams, 2);
        assert_eq!(issuance_and_maturity.io_issued_e8s, JUPITER_EXPECTED_IO_E8S);
        assert!(
            deposit_block > 0,
            "Jupiter deposit should be recorded on real local ICP ledger before scan"
        );
        assert!(
            maturity_block.0 > 0_u32.into(),
            "two-year maturity should be recorded on real local ICP ledger before scan"
        );

        let redemption_block =
            transfer_real_io_to_redemption_account(&stack, JUPITER_REDEMPTION_IO_E8S);
        let _redemption_index =
            wait_for_real_sns_redemption_index_transaction(&stack, JUPITER_REDEMPTION_IO_E8S);
        let before = stream_manager_state(&stack);
        let jupiter_icp = jupiter_icp_account();
        let jupiter_icp_before = icp_account_balance_e8s(&stack, &jupiter_icp);

        let redemption = stream_manager_tick(&stack);
        assert!(redemption.errors.is_empty(), "{:?}", redemption.errors);
        assert_eq!(redemption.scanned_io_transactions, 1);
        assert_eq!(redemption.processed_redemptions, 1);
        assert_eq!(redemption.icp_paid_e8s, 1_100_000_000);
        assert!(redemption.icp_paid_e8s > JUPITER_EXPECTED_REDEMPTION_ICP_E8S);

        let after = stream_manager_state(&stack);
        let jupiter_icp_after = icp_account_balance_e8s(&stack, &jupiter_icp);
        assert_eq!(jupiter_icp_after - jupiter_icp_before, 1_100_000_000);
        assert_eq!(
            before.protocol.liquid_icp_e8s - after.protocol.liquid_icp_e8s,
            1_100_000_000
        );
        assert_eq!(
            before.protocol.protocol_reserve_io_e8s + u128::from(JUPITER_REDEMPTION_IO_E8S),
            after.protocol.protocol_reserve_io_e8s
        );
        assert_eq!(
            api_redeemable_io_e8s(&before.protocol) - api_redeemable_io_e8s(&after.protocol),
            u128::from(JUPITER_REDEMPTION_IO_E8S)
        );
        assert!(
            redemption_block.0 > 0_u32.into(),
            "redemption should be recorded on finalized SNS ledger before scan"
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_redemption_after_staker_rewards_preserves_rate() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        let reward_neuron_ids = finalized_governance_expected_reward_neuron_ids(&stack);
        assert!(
            !reward_neuron_ids.is_empty(),
            "finalized SNS should expose at least one eligible reward neuron"
        );
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            (JUPITER_EXPECTED_IO_E8S + 300_000_000 + u128::from(2 * crate::icrc::FEE_E8S)) as u64,
        );
        let deposit_block = fund_real_jupiter_deposit(&stack, JUPITER_DEPOSIT_ICP_E8S);
        let maturity_block = fund_real_two_week_maturity_deposit(&stack, TWO_WEEK_MATURITY_ICP_E8S);

        let issuance_and_rewards = stream_manager_tick(&stack);
        assert!(
            issuance_and_rewards.errors.is_empty(),
            "{:?}",
            issuance_and_rewards.errors
        );
        assert_eq!(issuance_and_rewards.scanned_icp_transactions, 2);
        assert_eq!(issuance_and_rewards.processed_authorized_streams, 2);
        assert_eq!(
            issuance_and_rewards.io_issued_e8s,
            JUPITER_EXPECTED_IO_E8S + 300_000_000
        );

        let before_redemption = stream_manager_state(&stack);
        assert_eq!(before_redemption.protocol.liquid_icp_e8s, 6_300_000_000);
        assert_eq!(
            api_redeemable_io_e8s(&before_redemption.protocol),
            6_300_000_000
        );

        let redemption_block =
            transfer_real_io_to_redemption_account(&stack, JUPITER_REDEMPTION_IO_E8S);
        let _redemption_index =
            wait_for_real_sns_redemption_index_transaction(&stack, JUPITER_REDEMPTION_IO_E8S);
        let jupiter_icp = jupiter_icp_account();
        let jupiter_icp_before = icp_account_balance_e8s(&stack, &jupiter_icp);

        let redemption = stream_manager_tick(&stack);
        assert!(redemption.errors.is_empty(), "{:?}", redemption.errors);
        assert_eq!(redemption.scanned_io_transactions, 1);
        assert_eq!(redemption.processed_redemptions, 1);
        assert_eq!(redemption.icp_paid_e8s, JUPITER_EXPECTED_REDEMPTION_ICP_E8S);

        let after_redemption = stream_manager_state(&stack);
        let jupiter_icp_after = icp_account_balance_e8s(&stack, &jupiter_icp);
        assert_eq!(
            u128::from(jupiter_icp_after - jupiter_icp_before),
            JUPITER_EXPECTED_REDEMPTION_ICP_E8S
        );
        assert_eq!(
            before_redemption.protocol.liquid_icp_e8s - after_redemption.protocol.liquid_icp_e8s,
            JUPITER_EXPECTED_REDEMPTION_ICP_E8S
        );
        assert_eq!(
            api_redeemable_io_e8s(&before_redemption.protocol)
                - api_redeemable_io_e8s(&after_redemption.protocol),
            u128::from(JUPITER_REDEMPTION_IO_E8S)
        );
        assert!(
            deposit_block > 0,
            "Jupiter deposit should be recorded on real local ICP ledger before scan"
        );
        assert!(
            maturity_block.0 > 0_u32.into(),
            "two-week maturity should be recorded on real local ICP ledger before scan"
        );
        assert!(
            redemption_block.0 > 0_u32.into(),
            "redemption should be recorded on finalized SNS ledger before scan"
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_two_week_maturity_5_icp_issues_exact_backed_reward_pool() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        let reward_neuron_ids = finalized_governance_expected_reward_neuron_ids(&stack);
        assert!(
            !reward_neuron_ids.is_empty(),
            "finalized SNS should expose at least one eligible reward neuron"
        );
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            JUPITER_EXPECTED_IO_E8S as u64 + crate::icrc::FEE_E8S,
        );
        let maturity_block = fund_real_two_week_maturity_deposit(&stack, TWO_WEEK_MATURITY_ICP_E8S);

        let before = stream_manager_state(&stack);
        let reward_stakes_before = reward_neuron_ids
            .iter()
            .map(|neuron_id| {
                (
                    neuron_id.clone(),
                    finalized_neuron_cached_stake_e8s(&stack, neuron_id),
                )
            })
            .collect::<Vec<_>>();

        let outcome = stream_manager_tick(&stack);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.scanned_icp_transactions, 1);
        assert_eq!(outcome.processed_authorized_streams, 1);
        assert_eq!(outcome.io_issued_e8s, 300_000_000);
        let stable = stream_manager_stable_state(&stack);
        let reward_ops = stable
            .operation_journal
            .iter()
            .filter(|op| {
                op.kind == io_stream_manager::StreamOperationKind::TwoWeekMaturityStream
                    && op.io_issued_e8s == outcome.io_issued_e8s
            })
            .collect::<Vec<_>>();
        assert_eq!(
            reward_ops.len(),
            1,
            "expected one two-week reward operation, journal: {:?}",
            stable.operation_journal
        );
        let reward_op = reward_ops[0];
        assert!(
            !reward_op.two_week_recipients.is_empty(),
            "expected finalized governance reward recipients, op: {:?}",
            reward_op
        );
        assert!(
            reward_op
                .two_week_recipients
                .iter()
                .all(|recipient| recipient.transfer_status
                    == io_stream_manager::TransferStatus::Succeeded
                    && recipient.transfer_block_index.is_some()
                    && recipient.ledger_transfer_status
                        == Some(io_stream_manager::TransferStatus::Succeeded)
                    && recipient.ledger_transfer_block.is_some()
                    && recipient.governance_refresh_status
                        == Some(io_stream_manager::TransferStatus::Succeeded)
                    && recipient.expected_stake_after_e8s == recipient.observed_stake_after_e8s),
            "expected all reward transfers and governance refreshes to succeed, op: {:?}",
            reward_op
        );

        let mut total_reward_delta = 0_u128;
        for (neuron_id, before_stake) in reward_stakes_before {
            let after_stake = finalized_neuron_cached_stake_e8s(&stack, &neuron_id);
            if after_stake > before_stake {
                total_reward_delta += u128::from(after_stake - before_stake);
            }
        }
        assert_eq!(total_reward_delta, outcome.io_issued_e8s);

        let after = stream_manager_state(&stack);
        assert_eq!(
            after.protocol.two_week_staked_icp_e8s - before.protocol.two_week_staked_icp_e8s,
            200_000_000
        );
        assert_eq!(
            after.protocol.liquid_icp_e8s - before.protocol.liquid_icp_e8s,
            300_000_000
        );
        assert!(
            maturity_block.0 > 0_u32.into(),
            "two-week maturity should be recorded on the local ICP ledger before stream-manager scan"
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_finalized_sns_participation_weighted_rewards_topup_exact_neurons() {
        let voter = Principal::from_slice(&[120; 29]);
        let non_voter = Principal::from_slice(&[121; 29]);
        let sns = deploy_finalized_sns_lifecycle_fixture_with_participants_for_test(
            true,
            &[
                (voter, 5 * PARTICIPANT_ICP_E8S),
                (non_voter, 5 * PARTICIPANT_ICP_E8S),
            ],
        )
        .unwrap();
        let voter_neuron = stake_eligible_finalized_neuron(
            &sns,
            voter,
            FINALIZED_SNS_PROPOSAL_REJECT_COST_E8S,
            40_001,
        );
        let non_voter_neuron = stake_eligible_finalized_neuron(
            &sns,
            non_voter,
            FINALIZED_SNS_PROPOSAL_REJECT_COST_E8S,
            40_002,
        );
        let proposal_id = make_finalized_motion_proposal_for_test(
            &sns,
            voter,
            &voter_neuron,
            "IO stream-manager voter-weighted reward smoke",
        )
        .expect("finalized governance should accept a motion proposal");
        close_finalized_motion_proposal(&sns, voter, &proposal_id);

        let stack = deploy_io_real_stack_on_fixture(sns).unwrap();
        transfer_real_sns_protocol_reserve_from_participant(
            &stack,
            voter,
            300_000_000 + crate::icrc::FEE_E8S,
        );
        fund_real_two_week_maturity_deposit(&stack, TWO_WEEK_MATURITY_ICP_E8S);
        let voter_before = finalized_neuron_cached_stake_e8s(&stack, &voter_neuron);
        let non_voter_before = finalized_neuron_cached_stake_e8s(&stack, &non_voter_neuron);

        let outcome = stream_manager_tick(&stack);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.io_issued_e8s, 300_000_000);

        let reward_op = completed_two_week_reward_operation(&stack, outcome.io_issued_e8s);
        let voter_reward_id = reward_id_for_sns_neuron_id(&voter_neuron);
        let non_voter_reward_id = reward_id_for_sns_neuron_id(&non_voter_neuron);
        assert!(
            reward_op.two_week_recipients.iter().any(|recipient| {
                recipient.neuron_id == voter_reward_id && recipient.amount_e8s == 300_000_000
            }),
            "full voter should receive the entire real reward pool when the equal-stake peer did not vote: {reward_op:?}"
        );
        assert!(
            reward_op
                .two_week_recipients
                .iter()
                .all(|recipient| recipient.neuron_id != non_voter_reward_id),
            "non-voter should not receive a stream-manager reward recipient after a closed proposal: {reward_op:?}"
        );
        assert_eq!(
            finalized_neuron_cached_stake_e8s(&stack, &voter_neuron) - voter_before,
            300_000_000
        );
        assert_eq!(
            finalized_neuron_cached_stake_e8s(&stack, &non_voter_neuron),
            non_voter_before
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn real_participation_reward_followed_vote_matches_policy() {
        let proposer = Principal::from_slice(&[123; 29]);
        let leader = Principal::from_slice(&[124; 29]);
        let follower = Principal::from_slice(&[125; 29]);
        let sns = deploy_finalized_sns_lifecycle_fixture_with_participants_for_test(
            true,
            &[
                (proposer, PARTICIPANT_ICP_E8S),
                (leader, 450_000_000),
                (follower, 450_000_000),
            ],
        )
        .unwrap();
        let proposer_neuron = direct_participation_neuron_id(&sns, proposer);
        let leader_neuron = stake_eligible_finalized_neuron(
            &sns,
            leader,
            FINALIZED_SNS_PROPOSAL_REJECT_COST_E8S,
            40_003,
        );
        let follower_neuron = stake_eligible_finalized_neuron(
            &sns,
            follower,
            FINALIZED_SNS_PROPOSAL_REJECT_COST_E8S,
            40_004,
        );
        configure_finalized_neuron_dissolve_delay_for_test(
            &sns,
            proposer,
            &proposer_neuron,
            TWO_WEEK_DISSOLVE_DELAY_SECONDS as u32,
        )
        .expect("finalized governance should accept proposer dissolve delay");

        let function_id = finalized_motion_function_id_for_test(&sns)
            .expect("finalized governance should expose Motion");
        follow_finalized_sns_neuron_for_test(
            &sns,
            follower,
            &follower_neuron,
            leader_neuron.clone(),
            function_id,
        )
        .expect("follower should be able to follow leader for Motion");
        set_finalized_sns_governance_following_for_test(
            &sns,
            follower,
            &follower_neuron,
            leader_neuron.clone(),
        )
        .expect("follower should be able to set topic following");
        let proposal_id = make_finalized_motion_proposal_for_test(
            &sns,
            proposer,
            &proposer_neuron,
            "IO stream-manager followed-vote reward smoke",
        )
        .expect("finalized governance should accept a motion proposal");
        register_finalized_sns_vote_for_test(&sns, leader, &leader_neuron, proposal_id.clone(), 1)
            .expect("leader should vote yes after proposal creation");
        close_finalized_motion_proposal(&sns, follower, &proposal_id);

        let stack = deploy_io_real_stack_on_fixture(sns).unwrap();
        transfer_real_sns_protocol_reserve_from_participant(
            &stack,
            leader,
            300_000_000 + 3 * crate::icrc::FEE_E8S,
        );
        fund_real_two_week_maturity_deposit(&stack, TWO_WEEK_MATURITY_ICP_E8S);
        let follower_before = finalized_neuron_cached_stake_e8s(&stack, &follower_neuron);

        let outcome = stream_manager_tick(&stack);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        let reward_op = completed_two_week_reward_operation(&stack, outcome.io_issued_e8s);
        let follower_reward_id = reward_id_for_sns_neuron_id(&follower_neuron);
        assert!(
            reward_op.two_week_recipients.iter().any(|recipient| {
                recipient.neuron_id == follower_reward_id
                    && recipient.amount_e8s > 0
                    && recipient.transfer_status == io_stream_manager::TransferStatus::Succeeded
                    && recipient.governance_refresh_status
                        == Some(io_stream_manager::TransferStatus::Succeeded)
            }),
            "follower should receive a stream-manager reward after real finalized SNS followed vote: {reward_op:?}"
        );
        assert!(
            finalized_neuron_cached_stake_e8s(&stack, &follower_neuron) > follower_before,
            "follower cached stake should increase after the reward refresh"
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn real_participation_reward_dust_unissued() {
        let participant_a = Principal::from_slice(&[127; 29]);
        let participant_b = Principal::from_slice(&[128; 29]);
        let reserve_funder = Principal::from_slice(&[129; 29]);
        let sns = deploy_finalized_sns_lifecycle_fixture_with_participants_for_test(
            true,
            &[
                (participant_a, PARTICIPANT_ICP_E8S),
                (participant_b, PARTICIPANT_ICP_E8S),
                (reserve_funder, PARTICIPANT_ICP_E8S),
            ],
        )
        .unwrap();
        let neuron_a = stake_eligible_finalized_neuron(&sns, participant_a, 100_000_000, 40_005);
        let neuron_b = stake_eligible_finalized_neuron(&sns, participant_b, 100_000_000, 40_006);
        let stack = deploy_io_real_stack_on_fixture(sns).unwrap();
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            reserve_funder,
            300_000_001 + 3 * crate::icrc::FEE_E8S,
        );
        fund_real_two_week_maturity_deposit(&stack, 500_000_002);

        let outcome = stream_manager_tick(&stack);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.io_issued_e8s, 300_000_002);
        let reward_op = completed_two_week_reward_operation(&stack, outcome.io_issued_e8s);
        let preflight = reward_op
            .reward_preflight
            .as_ref()
            .expect("reward preflight should be recorded");
        assert!(preflight.dust_e8s > 0);
        assert_eq!(
            reward_op
                .two_week_recipients
                .iter()
                .map(|recipient| recipient.amount_e8s)
                .sum::<u128>(),
            outcome.io_issued_e8s - preflight.dust_e8s
        );
        assert!(reward_op
            .two_week_recipients
            .iter()
            .any(|recipient| recipient.neuron_id == reward_id_for_sns_neuron_id(&neuron_a)));
        assert!(reward_op
            .two_week_recipients
            .iter()
            .any(|recipient| recipient.neuron_id == reward_id_for_sns_neuron_id(&neuron_b)));
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn real_participation_reward_exact_fee_sum() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        let reward_neuron_ids = finalized_governance_expected_reward_neuron_ids(&stack);
        assert!(
            !reward_neuron_ids.is_empty(),
            "finalized SNS should expose at least one eligible reward neuron"
        );
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            300_000_000 + crate::icrc::FEE_E8S,
        );
        fund_real_two_week_maturity_deposit(&stack, TWO_WEEK_MATURITY_ICP_E8S);

        let outcome = stream_manager_tick(&stack);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        let reward_op = completed_two_week_reward_operation(&stack, outcome.io_issued_e8s);
        let preflight = reward_op
            .reward_preflight
            .as_ref()
            .expect("reward preflight should be recorded");
        let fee_sum = reward_op
            .two_week_recipients
            .iter()
            .map(|recipient| {
                recipient
                    .ledger_transfer_fee_e8s
                    .expect("ledger fee should be recorded")
            })
            .sum::<u128>();
        assert_eq!(preflight.total_reward_e8s, 300_000_000);
        assert_eq!(preflight.total_fee_e8s, fee_sum);
        assert_eq!(
            preflight.total_reserve_debit_e8s,
            preflight.total_reward_e8s + fee_sum
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn real_participation_reward_replay_idempotent() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            300_000_000 + crate::icrc::FEE_E8S,
        );
        fund_real_two_week_maturity_deposit(&stack, TWO_WEEK_MATURITY_ICP_E8S);

        let first = stream_manager_tick(&stack);
        assert!(first.errors.is_empty(), "{:?}", first.errors);
        assert_eq!(first.io_issued_e8s, 300_000_000);
        let after_first = stream_manager_stable_state(&stack);
        let reward_ops_after_first = after_first
            .operation_journal
            .iter()
            .filter(|op| op.kind == io_stream_manager::StreamOperationKind::TwoWeekMaturityStream)
            .count();

        let replay = stream_manager_tick(&stack);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.io_issued_e8s, 0);
        assert_eq!(replay.processed_authorized_streams, 0);
        let after_replay = stream_manager_stable_state(&stack);
        assert_eq!(
            after_replay
                .operation_journal
                .iter()
                .filter(
                    |op| op.kind == io_stream_manager::StreamOperationKind::TwoWeekMaturityStream
                )
                .count(),
            reward_ops_after_first
        );
        assert_eq!(
            after_replay.operation_journal, after_first.operation_journal,
            "replay should not mutate completed real reward operations"
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO debug Wasm artifacts, and POCKET_IC_BIN"]
    fn real_finalized_sns_four_role_reward_reconciles_exactly_once() {
        let proposer = Principal::from_slice(&[131; 29]);
        let direct = Principal::from_slice(&[132; 29]);
        let follower = Principal::from_slice(&[133; 29]);
        let non_voter = Principal::from_slice(&[134; 29]);
        let dissolving = Principal::from_slice(&[135; 29]);
        let reserve_funder = Principal::from_slice(&[136; 29]);
        let role_stake_e8s = 100_000_000_u64;
        let reward_pool_e8s = 300_000_003_u128;
        let expected_role_reward_e8s = 150_000_001_u128;
        let expected_dust_e8s = 1_u128;
        let sns = deploy_finalized_sns_lifecycle_fixture_with_participants_for_test(
            true,
            &[
                (proposer, PARTICIPANT_ICP_E8S),
                (direct, PARTICIPANT_ICP_E8S),
                (follower, PARTICIPANT_ICP_E8S),
                (non_voter, PARTICIPANT_ICP_E8S),
                (dissolving, PARTICIPANT_ICP_E8S),
                (reserve_funder, PARTICIPANT_ICP_E8S),
            ],
        )
        .unwrap();
        let proposer_neuron = direct_participation_neuron_id(&sns, proposer);
        let direct_neuron = stake_eligible_finalized_neuron(&sns, direct, role_stake_e8s, 50_001);
        let follower_neuron =
            stake_eligible_finalized_neuron(&sns, follower, role_stake_e8s, 50_002);
        let non_voter_neuron =
            stake_eligible_finalized_neuron(&sns, non_voter, role_stake_e8s, 50_003);
        let dissolving_neuron =
            stake_eligible_finalized_neuron(&sns, dissolving, role_stake_e8s, 50_004);
        start_finalized_neuron_dissolving_for_test(&sns, dissolving, &dissolving_neuron)
            .expect("finalized governance should accept start dissolving");

        let function_id = finalized_motion_function_id_for_test(&sns)
            .expect("finalized governance should expose Motion");
        follow_finalized_sns_neuron_for_test(
            &sns,
            follower,
            &follower_neuron,
            direct_neuron.clone(),
            function_id,
        )
        .expect("follower should follow direct voter for Motion");
        set_finalized_sns_governance_following_for_test(
            &sns,
            follower,
            &follower_neuron,
            direct_neuron.clone(),
        )
        .expect("follower should set topic following");
        let proposal_id = make_finalized_motion_proposal_for_test(
            &sns,
            proposer,
            &proposer_neuron,
            "IO strict four-role participation reward",
        )
        .expect("finalized governance should accept a motion proposal");
        register_finalized_sns_vote_for_test(&sns, direct, &direct_neuron, proposal_id.clone(), 1)
            .expect("direct voter should vote yes");
        close_finalized_motion_proposal(&sns, follower, &proposal_id);
        let follower_proposals = list_finalized_sns_proposals_as(&sns, follower, 100)
            .expect("follower should list finalized proposals");
        let followed_proposal = follower_proposals
            .proposals
            .iter()
            .find(|proposal| proposal.id == Some(proposal_id.clone()))
            .expect("closed proposal should be visible to follower");
        assert!(
            followed_proposal
                .ballots
                .iter()
                .any(|(id, ballot)| id == &hex::encode(&follower_neuron.id) && ballot.vote == 1),
            "follower-visible real governance ballot should show propagated yes vote: {:?}",
            followed_proposal.ballots
        );

        let stack = deploy_io_real_stack_on_fixture_configured(
            sns,
            |sns, stream_manager, stream_manager_args| {
                fund_real_sns_protocol_reserve_account_for_issuance(
                    sns,
                    stream_manager,
                    reserve_funder,
                    reward_pool_e8s as u64 + 5 * crate::icrc::FEE_E8S,
                );
                let reserve_balance = crate::icrc::icrc1_balance_of(
                    &sns.pic,
                    sns.ledger,
                    reserve_account_for_stream_manager(stream_manager),
                );
                let total_supply = crate::icrc::icrc1_total_supply(&sns.pic, sns.ledger);
                stream_manager_args.initial_protocol_reserve_io_e8s =
                    nat_to_u128(&reserve_balance, "strict pre-install reserve balance");
                stream_manager_args.initial_total_io_supply_e8s =
                    nat_to_u128(&total_supply, "strict pre-install total supply");
                stream_manager_args.non_redeemable_governance_io_e8s = 0;
                Ok(())
            },
        )
        .unwrap();
        let role_stakes_before = [
            finalized_neuron_cached_stake_e8s(&stack, &direct_neuron),
            finalized_neuron_cached_stake_e8s(&stack, &follower_neuron),
            finalized_neuron_cached_stake_e8s(&stack, &non_voter_neuron),
            finalized_neuron_cached_stake_e8s(&stack, &dissolving_neuron),
        ];
        assert_eq!(role_stakes_before, [role_stake_e8s; 4]);
        let role_snapshots = [
            (&direct_neuron, 1, 1, false),
            (&follower_neuron, 1, 1, false),
            (&non_voter_neuron, 0, 1, false),
            (&dissolving_neuron, 1, 1, true),
        ]
        .into_iter()
        .map(
            |(id, voted, eligible, is_dissolving)| io_reward_policy::NeuronSnapshot {
                sns_neuron_id: io_governance_types::SnsNeuronId(id.id.clone()),
                neuron_id: reward_id_for_sns_neuron_id(id),
                staked_io_e8s: u128::from(role_stake_e8s),
                eligible_seconds: TWO_WEEK_DISSOLVE_DELAY_SECONDS,
                eligible_closed_proposals: eligible,
                voted_closed_proposals: voted,
                is_genesis_governance_neuron: false,
                is_protocol_owned: false,
                is_dissolving,
            },
        )
        .collect::<Vec<_>>();
        assert_eq!(
            io_reward_policy::reward_weight(&role_snapshots[0]),
            role_snapshots[0].staked_io_e8s * u128::from(TWO_WEEK_DISSOLVE_DELAY_SECONDS)
        );
        assert_eq!(
            io_reward_policy::reward_weight(&role_snapshots[1]),
            io_reward_policy::reward_weight(&role_snapshots[0])
        );
        assert_eq!(io_reward_policy::reward_weight(&role_snapshots[2]), 0);
        assert_eq!(io_reward_policy::reward_weight(&role_snapshots[3]), 0);
        let oracle = io_reward_policy::allocate_rewards(reward_pool_e8s, &role_snapshots);
        assert_eq!(oracle.dust_e8s, expected_dust_e8s);
        assert_eq!(oracle.allocations.len(), 2);
        assert!(oracle
            .allocations
            .iter()
            .all(|allocation| allocation.io_e8s == expected_role_reward_e8s));

        fund_real_two_week_maturity_deposit(&stack, 500_000_005);
        let model_before = stream_manager_state(&stack);
        let reserve_before = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            reserve_account_for_stack(&stack),
        );
        let supply_before = crate::icrc::icrc1_total_supply(&stack.sns.pic, stack.sns.ledger);
        let fee = crate::icrc::icrc1_fee(&stack.sns.pic, stack.sns.ledger);
        assert_eq!(fee, candid::Nat::from(crate::icrc::FEE_E8S));
        assert_eq!(
            model_before.protocol.protocol_reserve_io_e8s,
            nat_to_u128(&reserve_before, "strict pre-processing reserve balance")
        );
        assert_eq!(
            model_before.protocol.total_io_supply_e8s,
            nat_to_u128(&supply_before, "strict pre-processing total supply")
        );
        let expected_model_active_stake_snapshot_e8s =
            finalized_governance_expected_active_stake_e8s(&stack);

        let outcome = stream_manager_tick(&stack);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.io_issued_e8s, reward_pool_e8s);
        let reward_op = completed_two_week_reward_operation(&stack, outcome.io_issued_e8s);
        let preflight = reward_op.reward_preflight.as_ref().unwrap();
        assert_eq!(
            preflight.protocol_reserve_available_e8s,
            model_before.protocol.protocol_reserve_io_e8s
        );
        assert_eq!(
            preflight.real_ledger_reserve_balance_e8s,
            nat_to_u128(&reserve_before, "strict preflight reserve balance")
        );
        assert_eq!(preflight.dust_e8s, expected_dust_e8s);
        assert_eq!(preflight.ledger_fee_e8s, u128::from(crate::icrc::FEE_E8S));
        assert_eq!(
            preflight.total_fee_e8s,
            2 * u128::from(crate::icrc::FEE_E8S)
        );
        assert_eq!(
            preflight.total_reserve_debit_e8s,
            2 * expected_role_reward_e8s + preflight.total_fee_e8s
        );
        let canonical_recipient_ids = preflight
            .canonical_recipient_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let expected_canonical_recipient_ids =
            BTreeSet::from([direct_neuron.id.clone(), follower_neuron.id.clone()]);
        assert_eq!(canonical_recipient_ids, expected_canonical_recipient_ids);
        assert!(!preflight
            .canonical_recipient_ids
            .contains(&non_voter_neuron.id));
        assert!(!preflight
            .canonical_recipient_ids
            .contains(&dissolving_neuron.id));
        assert!(preflight
            .canonical_recipient_ids
            .iter()
            .all(|id| id.len() == 32));
        let mut expected_block_destinations = Vec::new();
        for allocation in &oracle.allocations {
            let recipient = reward_op
                .two_week_recipients
                .iter()
                .find(|recipient| recipient.neuron_id == allocation.neuron_id)
                .expect("oracle allocation should have a matching reward recipient");
            assert_eq!(recipient.amount_e8s, allocation.io_e8s);
            assert_eq!(
                recipient.ledger_transfer_fee_e8s,
                Some(u128::from(crate::icrc::FEE_E8S))
            );
            assert_eq!(
                recipient.reward_amount_received_e8s,
                Some(expected_role_reward_e8s)
            );
            assert_eq!(
                recipient.reserve_debit_e8s,
                Some(expected_role_reward_e8s + u128::from(crate::icrc::FEE_E8S))
            );
            assert_eq!(
                recipient.governance_refresh_status,
                Some(io_stream_manager::TransferStatus::Succeeded)
            );
            assert!(recipient.transfer_block_index.is_some());
            assert_eq!(
                recipient.transfer_block_index,
                recipient.ledger_transfer_block
            );
            assert_eq!(
                recipient.expected_stake_after_e8s,
                recipient.observed_stake_after_e8s
            );
            assert_eq!(
                recipient.expected_stake_after_e8s,
                Some(u128::from(role_stake_e8s) + expected_role_reward_e8s)
            );
            let canonical = <[u8; 32]>::try_from(
                recipient
                    .sns_neuron_id
                    .clone()
                    .expect("reward recipient should retain canonical SNS neuron id"),
            )
            .expect("canonical SNS neuron id should be 32 bytes");
            let expected_destination = Account::new(
                stack.sns.governance,
                Some(io_ledger_types::Subaccount(canonical)),
            );
            let attempt = recipient
                .reward_transfer_attempt
                .as_ref()
                .expect("reward recipient should retain durable transfer attempt");
            assert_eq!(attempt.amount_e8s, expected_role_reward_e8s);
            assert_eq!(attempt.fee_e8s, u128::from(crate::icrc::FEE_E8S));
            assert_eq!(attempt.destination_account, expected_destination);
            expected_block_destinations.push((
                recipient
                    .transfer_block_index
                    .expect("reward recipient should have a transfer block"),
                expected_destination.to_icrc_account(),
            ));
        }
        let direct_after = finalized_neuron_cached_stake_e8s(&stack, &direct_neuron);
        let follower_after = finalized_neuron_cached_stake_e8s(&stack, &follower_neuron);
        let non_voter_after = finalized_neuron_cached_stake_e8s(&stack, &non_voter_neuron);
        let dissolving_after = finalized_neuron_cached_stake_e8s(&stack, &dissolving_neuron);
        assert_eq!(
            u128::from(direct_after - role_stakes_before[0]),
            expected_role_reward_e8s
        );
        assert_eq!(
            u128::from(follower_after - role_stakes_before[1]),
            expected_role_reward_e8s
        );
        assert_eq!(non_voter_after, role_stakes_before[2]);
        assert_eq!(dissolving_after, role_stakes_before[3]);
        let model_after = stream_manager_state(&stack);
        let reserve_after = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            reserve_account_for_stack(&stack),
        );
        let supply_after = crate::icrc::icrc1_total_supply(&stack.sns.pic, stack.sns.ledger);
        assert_eq!(
            model_before.protocol.protocol_reserve_io_e8s
                - model_after.protocol.protocol_reserve_io_e8s,
            preflight.total_reserve_debit_e8s
        );
        assert_eq!(
            model_after.protocol.protocol_reserve_io_e8s,
            nat_to_u128(&reserve_after, "strict post-processing reserve balance")
        );
        assert_eq!(
            model_after.protocol.total_io_supply_e8s,
            nat_to_u128(&supply_after, "strict post-processing total supply")
        );
        assert_eq!(
            model_after.active_staked_io_e8s,
            expected_model_active_stake_snapshot_e8s
        );
        let observed_reward_stake_delta_e8s = u128::from(direct_after - role_stakes_before[0])
            + u128::from(follower_after - role_stakes_before[1]);
        assert_eq!(
            finalized_governance_expected_active_stake_e8s(&stack),
            expected_model_active_stake_snapshot_e8s + observed_reward_stake_delta_e8s
        );
        assert_eq!(
            candid::Nat(reserve_before.0.clone() - reserve_after.0.clone()),
            candid::Nat::from(preflight.total_reserve_debit_e8s)
        );
        assert_eq!(
            candid::Nat(supply_before.0.clone() - supply_after.0.clone()),
            candid::Nat::from(preflight.total_fee_e8s)
        );
        let transfer_blocks = reward_op
            .two_week_recipients
            .iter()
            .filter_map(|recipient| recipient.transfer_block_index)
            .collect::<Vec<_>>();
        assert_eq!(transfer_blocks.len(), 2);
        assert_eq!(
            transfer_blocks
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len(),
            2
        );
        for (block, destination) in &expected_block_destinations {
            let ledger_block =
                crate::icrc::ledger_get_transactions(&stack.sns.pic, stack.sns.ledger, *block, 1);
            assert_eq!(ledger_block.first_index, candid::Nat::from(*block));
            assert_eq!(ledger_block.transactions.len(), 1);
            let transfer = ledger_block.transactions[0]
                .transfer
                .as_ref()
                .unwrap_or_else(|| panic!("transfer block {block} should be a transfer"));
            assert_eq!(transfer.from, reserve_account_for_stack(&stack));
            assert_eq!(transfer.to, *destination);
            assert_eq!(transfer.amount, candid::Nat::from(expected_role_reward_e8s));
        }
        eprintln!(
            "strict_four_role_reward_summary role_stake_e8s={role_stake_e8s} weights={:?} allocations={:?} dust_e8s={} fee_e8s={} total_fee_e8s={} total_reserve_debit_e8s={} transfer_blocks={:?} stake_deltas={:?} model_active_stake_snapshot_e8s={} real_active_stake_after_refresh_e8s={} model_ledger_before=(reserve:{},supply:{}) model_ledger_after=(reserve:{},supply:{}) reserve_delta_e8s={} supply_delta_e8s={}",
            role_snapshots
                .iter()
                .map(io_reward_policy::reward_weight)
                .collect::<Vec<_>>(),
            oracle
                .allocations
                .iter()
                .map(|allocation| (allocation.neuron_id, allocation.io_e8s))
                .collect::<Vec<_>>(),
            preflight.dust_e8s,
            preflight.ledger_fee_e8s,
            preflight.total_fee_e8s,
            preflight.total_reserve_debit_e8s,
            transfer_blocks,
            [
                direct_after - role_stakes_before[0],
                follower_after - role_stakes_before[1],
                non_voter_after - role_stakes_before[2],
                dissolving_after - role_stakes_before[3],
            ],
            model_after.active_staked_io_e8s,
            finalized_governance_expected_active_stake_e8s(&stack),
            model_before.protocol.protocol_reserve_io_e8s,
            model_before.protocol.total_io_supply_e8s,
            model_after.protocol.protocol_reserve_io_e8s,
            model_after.protocol.total_io_supply_e8s,
            preflight.total_reserve_debit_e8s,
            preflight.total_fee_e8s
        );

        let before_upgrade = stream_manager_stable_state(&stack);
        upgrade_stream_manager_same_wasm(&stack);
        let replay = stream_manager_tick(&stack);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.io_issued_e8s, 0);
        assert_eq!(replay.processed_authorized_streams, 0);
        let after_replay_stable = stream_manager_stable_state(&stack);
        assert_eq!(
            after_replay_stable.operation_journal,
            before_upgrade.operation_journal
        );
        assert_eq!(
            after_replay_stable.processed_transactions,
            before_upgrade.processed_transactions
        );
        assert_eq!(
            finalized_neuron_cached_stake_e8s(&stack, &direct_neuron),
            direct_after
        );
        assert_eq!(
            finalized_neuron_cached_stake_e8s(&stack, &follower_neuron),
            follower_after
        );
        assert_eq!(
            crate::icrc::icrc1_balance_of(
                &stack.sns.pic,
                stack.sns.ledger,
                reserve_account_for_stack(&stack),
            ),
            reserve_after
        );
        assert_eq!(
            crate::icrc::icrc1_total_supply(&stack.sns.pic, stack.sns.ledger),
            supply_after
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO debug Wasm artifacts, and POCKET_IC_BIN"]
    fn real_finalized_sns_zero_recipient_reward_retains_full_pool_as_dust() {
        let reserve_funder = Principal::from_slice(&[142; 29]);
        let excluded_voter_owner = Principal::from_slice(&[143; 29]);
        let sns = deploy_finalized_sns_lifecycle_fixture_with_participants_for_test(
            true,
            &[
                (reserve_funder, PARTICIPANT_ICP_E8S),
                (excluded_voter_owner, PARTICIPANT_ICP_E8S),
            ],
        )
        .unwrap();
        let stack = deploy_io_real_stack_on_fixture_configured(
            sns,
            |sns, stream_manager, stream_manager_args| {
                fund_real_sns_protocol_reserve_account_for_issuance(
                    sns,
                    stream_manager,
                    reserve_funder,
                    500_000_000,
                );
                let excluded_voter = stake_eligible_finalized_neuron(
                    sns,
                    excluded_voter_owner,
                    FINALIZED_SNS_PROPOSAL_REJECT_COST_E8S,
                    60_001,
                );
                let proposal_id = make_finalized_motion_proposal_for_test(
                    sns,
                    excluded_voter_owner,
                    &excluded_voter,
                    "IO zero-recipient reward exclusion proof",
                )
                .expect("finalized governance should accept zero-recipient exclusion proposal");
                close_finalized_motion_proposal(sns, excluded_voter_owner, &proposal_id);
                start_finalized_neuron_dissolving_for_test(
                    sns,
                    excluded_voter_owner,
                    &excluded_voter,
                )
                .expect("finalized governance should exclude the only voting reward neuron");
                let reserve_balance = crate::icrc::icrc1_balance_of(
                    &sns.pic,
                    sns.ledger,
                    reserve_account_for_stream_manager(stream_manager),
                );
                let total_supply = crate::icrc::icrc1_total_supply(&sns.pic, sns.ledger);
                stream_manager_args.initial_protocol_reserve_io_e8s = nat_to_u128(
                    &reserve_balance,
                    "zero-recipient pre-install reserve balance",
                );
                stream_manager_args.initial_total_io_supply_e8s =
                    nat_to_u128(&total_supply, "zero-recipient pre-install total supply");
                stream_manager_args.non_redeemable_governance_io_e8s = 0;
                Ok(())
            },
        )
        .unwrap();

        fund_real_two_week_maturity_deposit(&stack, 500_000_005);
        let model_before = stream_manager_state(&stack);
        let reserve_before = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            reserve_account_for_stack(&stack),
        );
        let supply_before = crate::icrc::icrc1_total_supply(&stack.sns.pic, stack.sns.ledger);
        let reserve_outgoing_before =
            count_real_sns_protocol_reserve_reward_outgoing_transfers(&stack);
        assert_eq!(
            model_before.protocol.protocol_reserve_io_e8s,
            nat_to_u128(&reserve_before, "zero-recipient reserve before")
        );
        assert_eq!(
            model_before.protocol.total_io_supply_e8s,
            nat_to_u128(&supply_before, "zero-recipient supply before")
        );

        let outcome = stream_manager_tick(&stack);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert!(outcome.io_issued_e8s > 0);
        assert_eq!(outcome.processed_authorized_streams, 1);
        let reward_op = completed_two_week_reward_operation(&stack, outcome.io_issued_e8s);
        let preflight = reward_op
            .reward_preflight
            .as_ref()
            .expect("zero-recipient reward should have a durable preflight");
        assert!(reward_op.two_week_recipients.is_empty());
        assert_eq!(preflight.recipient_count, 0);
        assert_eq!(preflight.total_reward_e8s, 0);
        assert_eq!(preflight.total_fee_e8s, 0);
        assert_eq!(preflight.total_reserve_debit_e8s, 0);
        assert_eq!(preflight.dust_e8s, outcome.io_issued_e8s);
        assert!(preflight.canonical_recipient_ids.is_empty());
        assert!(preflight.compatibility_keys.is_empty());
        assert_eq!(
            reward_op.reward_reservation,
            Some(io_stream_manager::RewardReservation::default())
        );
        assert_eq!(reward_op.reserved_reward_debit_e8s, Some(0));

        wait_for_real_indexes(&stack);
        let model_after = stream_manager_state(&stack);
        let reserve_after = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            reserve_account_for_stack(&stack),
        );
        let supply_after = crate::icrc::icrc1_total_supply(&stack.sns.pic, stack.sns.ledger);
        assert_eq!(
            model_after.protocol.two_week_staked_icp_e8s
                - model_before.protocol.two_week_staked_icp_e8s,
            200_000_002
        );
        assert_eq!(
            model_after.protocol.liquid_icp_e8s - model_before.protocol.liquid_icp_e8s,
            300_000_003
        );
        assert_eq!(
            model_after.protocol.protocol_reserve_io_e8s,
            model_before.protocol.protocol_reserve_io_e8s
        );
        assert_eq!(
            model_after.protocol.protocol_reserve_io_e8s,
            nat_to_u128(&reserve_after, "zero-recipient reserve after")
        );
        assert_eq!(reserve_after, reserve_before);
        assert_eq!(
            model_after.protocol.total_io_supply_e8s,
            model_before.protocol.total_io_supply_e8s
        );
        assert_eq!(supply_after, supply_before);
        assert_eq!(
            count_real_sns_protocol_reserve_reward_outgoing_transfers(&stack),
            reserve_outgoing_before
        );

        let stable_before_upgrade = stream_manager_stable_state(&stack);
        upgrade_stream_manager_same_wasm(&stack);
        let replay = stream_manager_tick(&stack);
        assert!(replay.errors.is_empty(), "{:?}", replay.errors);
        assert_eq!(replay.io_issued_e8s, 0);
        assert_eq!(replay.processed_authorized_streams, 0);
        assert_eq!(
            stream_manager_stable_state(&stack).operation_journal,
            stable_before_upgrade.operation_journal
        );
        assert_eq!(
            crate::icrc::icrc1_balance_of(
                &stack.sns.pic,
                stack.sns.ledger,
                reserve_account_for_stack(&stack),
            ),
            reserve_after
        );
        assert_eq!(
            crate::icrc::icrc1_total_supply(&stack.sns.pic, stack.sns.ledger),
            supply_after
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO debug Wasm artifacts, and POCKET_IC_BIN"]
    fn real_stack_upgrade_after_reward_transfer_before_journal_update_no_double_transfer() {
        let participant = Principal::from_slice(&[105; 29]);
        let stack = deploy_finalized_sns_with_io_real_stack_for_test(true).unwrap();
        let reward_neuron_ids = finalized_governance_expected_reward_neuron_ids(&stack);
        assert!(
            !reward_neuron_ids.is_empty(),
            "finalized SNS should expose at least one eligible reward neuron"
        );
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            participant,
            JUPITER_EXPECTED_IO_E8S as u64 + crate::icrc::FEE_E8S,
        );
        fund_real_two_week_maturity_deposit(&stack, TWO_WEEK_MATURITY_ICP_E8S);

        stream_manager_set_failpoint(
            &stack,
            Some(io_stream_manager::DebugFailpoint::AfterTwoWeekRewardTransferBeforeJournalUpdate),
        );
        assert!(
            stream_manager_tick_traps(&stack),
            "debug failpoint should trap after the real SNS reward transfer and before local success journaling"
        );

        let trapped_stable = stream_manager_stable_state(&stack);
        let trapped_op = trapped_stable
            .operation_journal
            .iter()
            .find(|op| {
                op.kind == io_stream_manager::StreamOperationKind::TwoWeekMaturityStream
                    && op.io_issued_e8s == 300_000_000
            })
            .expect("two-week reward operation should be journaled before transfer attempt");
        let trapped_recipient = trapped_op
            .two_week_recipients
            .iter()
            .find(|recipient| recipient.reward_transfer_attempt.is_some())
            .expect("reward transfer attempt should be persisted before external call");
        let attempt = trapped_recipient
            .reward_transfer_attempt
            .as_ref()
            .expect("attempt should be present");
        let trapped_neuron_id = crate::sns_governance_setup::NeuronId {
            id: attempt.canonical_sns_neuron_id.clone(),
        };
        let stake_before = trapped_recipient
            .stake_before_e8s
            .expect("stake before should be persisted before reward transfer");
        let reward_amount = attempt.amount_e8s;
        assert_eq!(attempt.created_at_time, trapped_op.last_updated);
        assert_eq!(attempt.amount_e8s, trapped_recipient.amount_e8s);
        wait_for_real_indexes(&stack);
        assert_eq!(
            count_real_sns_reward_transfers_to_neuron(
                &stack,
                &trapped_neuron_id,
                reward_amount as u64,
            ),
            1,
            "trap path should execute exactly one real SNS reward transfer"
        );

        upgrade_stream_manager_same_wasm(&stack);
        let mut completed = false;
        let mut retry_outcomes = Vec::new();
        for _ in 0..12 {
            let retry = stream_manager_tick(&stack);
            retry_outcomes.push(format!("{retry:?}"));
            if retry.errors.is_empty() && retry.processed_authorized_streams == 1 {
                completed = true;
                break;
            }
            stack.sns.pic.advance_time(Duration::from_secs(5));
            for _ in 0..80 {
                stack.sns.pic.tick();
            }
        }
        let retry_stable = stream_manager_stable_state(&stack);
        assert!(
            completed,
            "retry after same-Wasm upgrade should complete the original reward op; outcomes: {retry_outcomes:?}; state: {retry_stable:?}"
        );

        assert_eq!(
            count_real_sns_reward_transfers_to_neuron(
                &stack,
                &trapped_neuron_id,
                reward_amount as u64,
            ),
            1,
            "retry must resolve Duplicate/TooOld proof without sending a second reward transfer"
        );
        assert_eq!(
            finalized_neuron_cached_stake_e8s(&stack, &trapped_neuron_id) as u128,
            stake_before + reward_amount,
            "controlled fixture should observe one exact cached-stake increase"
        );
        let final_stable = stream_manager_stable_state(&stack);
        let final_op = final_stable
            .operation_journal
            .iter()
            .find(|op| {
                op.kind == io_stream_manager::StreamOperationKind::TwoWeekMaturityStream
                    && op.io_issued_e8s == 300_000_000
            })
            .expect("two-week reward operation should remain journaled");
        assert_eq!(final_op.phase, io_stream_manager::OperationPhase::Completed);
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_two_week_maturity_rewards_only_eligible_stakers() {
        let participant = Principal::from_slice(&[110; 29]);
        let reserve_funder = Principal::from_slice(&[111; 29]);
        let sns = deploy_finalized_sns_lifecycle_fixture_with_participants_for_test(
            true,
            &[
                (participant, PARTICIPANT_ICP_E8S),
                (reserve_funder, PARTICIPANT_ICP_E8S),
            ],
        )
        .unwrap();
        disburse_zero_delay_neuron_to_participant_for_test(&sns, participant)
            .expect("zero-delay finalized neuron should fund normal staking");
        let eligible_neuron =
            stake_finalized_liquid_sns_tokens_for_test(&sns, participant, 100_000_000, 30_001)
                .expect("eligible finalized stake should claim a neuron");
        configure_finalized_neuron_dissolve_delay_for_test(
            &sns,
            participant,
            &eligible_neuron,
            TWO_WEEK_DISSOLVE_DELAY_SECONDS as u32,
        )
        .expect("finalized governance should accept eligible dissolve delay");
        let dissolving_neuron =
            stake_finalized_liquid_sns_tokens_for_test(&sns, participant, 100_000_000, 30_002)
                .expect("dissolving finalized stake should claim a neuron");
        configure_finalized_neuron_dissolve_delay_for_test(
            &sns,
            participant,
            &dissolving_neuron,
            TWO_WEEK_DISSOLVE_DELAY_SECONDS as u32,
        )
        .expect("finalized governance should accept dissolving-neuron delay");
        start_finalized_neuron_dissolving_for_test(&sns, participant, &dissolving_neuron)
            .expect("finalized governance should accept start dissolving");
        let eligible_reward_id = reward_id_for_sns_neuron_id(&eligible_neuron);
        let dissolving_reward_id = reward_id_for_sns_neuron_id(&dissolving_neuron);

        let stack = deploy_io_real_stack_on_fixture(sns).unwrap();
        fund_real_sns_protocol_reserve_for_issuance(
            &stack,
            reserve_funder,
            JUPITER_EXPECTED_IO_E8S as u64 + crate::icrc::FEE_E8S,
        );
        fund_real_two_week_maturity_deposit(&stack, TWO_WEEK_MATURITY_ICP_E8S);

        let eligible_before = finalized_neuron_cached_stake_e8s(&stack, &eligible_neuron);
        let dissolving_before = finalized_neuron_cached_stake_e8s(&stack, &dissolving_neuron);

        let outcome = stream_manager_tick(&stack);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.io_issued_e8s, 300_000_000);
        let stable = stream_manager_stable_state(&stack);
        let reward_op = stable
            .operation_journal
            .iter()
            .find(|op| {
                op.kind == io_stream_manager::StreamOperationKind::TwoWeekMaturityStream
                    && op.io_issued_e8s == outcome.io_issued_e8s
            })
            .expect("two-week reward operation should be journaled");
        assert!(
            reward_op
                .two_week_recipients
                .iter()
                .any(|recipient| recipient.neuron_id == eligible_reward_id
                    && recipient.transfer_status == io_stream_manager::TransferStatus::Succeeded
                    && recipient.governance_refresh_status
                        == Some(io_stream_manager::TransferStatus::Succeeded)
                    && recipient.amount_e8s > 0),
            "eligible finalized SNS neuron should receive a successful reward transfer and governance refresh: {reward_op:?}"
        );
        assert!(
            reward_op
                .two_week_recipients
                .iter()
                .all(|recipient| recipient.neuron_id != dissolving_reward_id),
            "dissolving finalized SNS neuron should not appear in reward recipients: {reward_op:?}"
        );

        let eligible_after = finalized_neuron_cached_stake_e8s(&stack, &eligible_neuron);
        let eligible_delta = eligible_after - eligible_before;
        assert!(
            eligible_delta > 0,
            "eligible finalized SNS neuron cached stake should increase by a positive reward share"
        );

        let dissolving_after = finalized_neuron_cached_stake_e8s(&stack, &dissolving_neuron);
        assert_eq!(
            dissolving_after, dissolving_before,
            "dissolving finalized SNS neuron should not receive rewards"
        );
    }

    #[test]
    #[ignore = "requires pinned real SNS/NNS Wasms, IO Wasm artifacts, and POCKET_IC_BIN"]
    fn io_stream_manager_real_sns_topup_increases_active_staked_io() {
        let participant = Principal::from_slice(&[112; 29]);
        let sns =
            deploy_finalized_sns_lifecycle_fixture_for_test(true, participant, PARTICIPANT_ICP_E8S)
                .unwrap();
        disburse_zero_delay_neuron_to_participant_for_test(&sns, participant)
            .expect("zero-delay finalized neuron should fund normal staking");
        let memo = 30_003;
        let neuron_id =
            stake_finalized_liquid_sns_tokens_for_test(&sns, participant, 100_000_000, memo)
                .expect("initial finalized stake should claim a neuron");
        configure_finalized_neuron_dissolve_delay_for_test(
            &sns,
            participant,
            &neuron_id,
            TWO_WEEK_DISSOLVE_DELAY_SECONDS as u32,
        )
        .expect("finalized governance should accept active-stake dissolve delay");

        let stack = deploy_io_real_stack_on_fixture(sns).unwrap();
        let before_tick = stream_manager_tick(&stack);
        assert!(before_tick.errors.is_empty(), "{:?}", before_tick.errors);
        let before_state = stream_manager_state(&stack);
        assert_eq!(
            before_state.active_staked_io_e8s,
            finalized_governance_expected_active_stake_e8s(&stack)
        );

        let topped_up =
            stake_finalized_liquid_sns_tokens_for_test(&stack.sns, participant, 50_000_000, memo)
                .expect("same memo/controller should top up finalized neuron");
        assert_eq!(topped_up, neuron_id);

        let after_tick = stream_manager_tick(&stack);
        assert!(after_tick.errors.is_empty(), "{:?}", after_tick.errors);
        let after_state = stream_manager_state(&stack);
        assert_eq!(
            after_state.active_staked_io_e8s,
            finalized_governance_expected_active_stake_e8s(&stack)
        );
        assert_eq!(
            after_state.active_staked_io_e8s - before_state.active_staked_io_e8s,
            50_000_000
        );
    }
}
