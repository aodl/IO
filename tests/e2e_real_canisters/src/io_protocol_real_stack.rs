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

    let mut install_args = build_io_real_stack_install_args(&sns, None);
    validate_io_real_stack_install_args(&install_args)?;

    let stream_manager = create_application_canister_on_subnet(
        &sns.pic,
        sns.application_subnet,
        stream_wasm,
        encode_one(&install_args.stream_manager).expect("stream-manager init args encode"),
    );
    grant_stream_manager_governance_visibility(&sns, stream_manager)?;

    install_args
        .nns_neuron_manager
        .io_stream_manager_principal_text = Some(stream_manager.to_text());
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
    let canister = pic.create_canister_on_subnet(None, None, application_subnet);
    pic.add_cycles(canister, APP_CANISTER_CYCLES);
    pic.install_canister(canister, wasm, arg, None);
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
fn finalized_governance_expected_reward_neuron_ids(stack: &IoRealStackFixture) -> Vec<u64> {
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
            let id = io_governance_types::SnsNeuronId(
                neuron
                    .id
                    .expect("eligible finalized SNS neuron should have an id")
                    .id,
            );
            io_reward_policy::sns_neuron_id_to_u64(&id)
                .expect("eligible finalized SNS neuron id should map to reward key")
        })
        .collect()
}

#[cfg(test)]
fn reward_id_for_sns_neuron_id(neuron_id: &crate::sns_governance_setup::NeuronId) -> u64 {
    io_reward_policy::sns_neuron_id_to_u64(&io_governance_types::SnsNeuronId(neuron_id.id.clone()))
        .expect("finalized SNS neuron id should map to reward key")
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
fn reserve_account_for_stack(stack: &IoRealStackFixture) -> IcrcAccount {
    crate::icrc::account(
        stack.stream_manager,
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
fn reward_account_for_stack(stack: &IoRealStackFixture, neuron_id: u64) -> IcrcAccount {
    crate::icrc::account(
        stack.stream_manager,
        Some(crate::icrc::subaccount(&format!(
            "{}{}",
            io_stream_manager::scheduler::TWO_WEEK_REWARD_ACCOUNT_PREFIX,
            neuron_id
        ))),
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
    let _disbursed = crate::sns_lifecycle::disburse_zero_delay_neuron_to_participant_for_test(
        &stack.sns,
        participant,
    )
    .expect("finalized SNS neuron should disburse liquid tokens for reserve funding");
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
fn transfer_real_io_to_redemption_account(
    stack: &IoRealStackFixture,
    amount_e8s: u64,
) -> candid::Nat {
    let block = crate::icrc::icrc1_transfer(
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
    .expect("Jupiter IO account should transfer redeemed IO to stream-manager redemption account");
    wait_for_real_indexes(stack);
    block
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
mod tests {
    use super::*;
    use crate::sns_lifecycle::{
        configure_finalized_neuron_dissolve_delay_for_test,
        deploy_finalized_sns_lifecycle_fixture_with_participants_for_test,
        disburse_zero_delay_neuron_to_participant_for_test,
        stake_finalized_liquid_sns_tokens_for_test, start_finalized_neuron_dissolving_for_test,
    };

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
            (JUPITER_REDEMPTION_IO_E8S - crate::icrc::FEE_E8S).into()
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
        let reward_balances_before = reward_neuron_ids
            .iter()
            .map(|neuron_id| {
                (
                    *neuron_id,
                    crate::icrc::icrc1_balance_of(
                        &stack.sns.pic,
                        stack.sns.ledger,
                        reward_account_for_stack(&stack, *neuron_id),
                    ),
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
                    && recipient.transfer_block_index.is_some()),
            "expected all reward transfers to succeed, op: {:?}",
            reward_op
        );

        let mut total_reward_delta = candid::Nat::from(0_u8);
        for (neuron_id, before_balance) in reward_balances_before {
            let after_balance = crate::icrc::icrc1_balance_of(
                &stack.sns.pic,
                stack.sns.ledger,
                reward_account_for_stack(&stack, neuron_id),
            );
            if after_balance > before_balance {
                total_reward_delta += after_balance - before_balance;
            }
        }
        assert_eq!(total_reward_delta, candid::Nat::from(outcome.io_issued_e8s));

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

        let eligible_before = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            reward_account_for_stack(&stack, eligible_reward_id),
        );
        let dissolving_before = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            reward_account_for_stack(&stack, dissolving_reward_id),
        );

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
                    && recipient.amount_e8s > 0),
            "eligible finalized SNS neuron should receive a successful reward transfer: {reward_op:?}"
        );
        assert!(
            reward_op
                .two_week_recipients
                .iter()
                .all(|recipient| recipient.neuron_id != dissolving_reward_id),
            "dissolving finalized SNS neuron should not appear in reward recipients: {reward_op:?}"
        );

        let eligible_after = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            reward_account_for_stack(&stack, eligible_reward_id),
        );
        let eligible_delta = eligible_after - eligible_before;
        assert!(
            eligible_delta > candid::Nat::from(0_u8),
            "eligible finalized SNS neuron should receive a positive reward share"
        );

        let dissolving_after = crate::icrc::icrc1_balance_of(
            &stack.sns.pic,
            stack.sns.ledger,
            reward_account_for_stack(&stack, dissolving_reward_id),
        );
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
