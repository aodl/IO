pub mod api;
mod execution;
pub use io_nns_types::{jupiter, maturity, pool, transfer};
mod jupiter_flow;
pub mod lifecycle;
mod maturity_flow;
pub mod state;
mod unwind_flow;

use candid::CandidType;
use serde::Deserialize;

pub use api::{
    ApiError, JupiterProgress, MaturityProgress, NnsProgress, NotifyJupiterDepositArgs,
    SetTwoWeekTargetArgs, Status,
};
pub use maturity::MaturityKind;
pub use state::{Lifecycle, NnsConfig, NnsStateV1, TwoWeekTargetStatus};

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    pub config: NnsConfig,
}

#[cfg_attr(target_family = "wasm", ic_cdk::init)]
pub fn init(args: InitArgs) {
    state::initialize(
        NnsStateV1 {
            config: args.config,
            lifecycle: Lifecycle::Paused,
            active_operation: None,
            latest_two_week_target: None,
            latest_target_generation: 0,
            pending_two_year_maturity: None,
            pending_two_week_maturity: None,
            last_two_year_maturity: None,
            last_two_week_maturity: None,
            next_operation_sequence: 1,
            control_epoch: 0,
        },
        ic_cdk::api::canister_self(),
    )
    .unwrap_or_else(|error| ic_cdk::trap(&error));
}

#[cfg_attr(target_family = "wasm", ic_cdk::post_upgrade)]
pub fn post_upgrade() {
    state::reopen(ic_cdk::api::canister_self());
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn notify_jupiter_deposit(
    args: NotifyJupiterDepositArgs,
) -> Result<JupiterProgress, ApiError> {
    api::notify_jupiter_deposit(ic_cdk::api::msg_caller(), args).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn set_two_week_target(
    args: SetTwoWeekTargetArgs,
) -> Result<TwoWeekTargetStatus, ApiError> {
    api::set_two_week_target(ic_cdk::api::msg_caller(), args).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn resume() -> Result<NnsProgress, ApiError> {
    api::resume().await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn prove_active_transfer(block_index: u128) -> Result<NnsProgress, ApiError> {
    api::prove_active_transfer(block_index).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn start_maturity(kind: MaturityKind) -> Result<MaturityProgress, ApiError> {
    api::start_maturity(ic_cdk::api::msg_caller(), kind).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn resume_maturity(kind: MaturityKind) -> Result<MaturityProgress, ApiError> {
    api::resume_maturity(kind).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn prove_maturity_mint(
    kind: MaturityKind,
    block_index: u128,
) -> Result<MaturityProgress, ApiError> {
    api::prove_maturity_mint(kind, block_index).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn set_paused(paused: bool) -> Result<(), ApiError> {
    let state = state::read();
    if ic_cdk::api::msg_caller() != state.config.sns_governance {
        return Err(ApiError::Unauthorized);
    }
    let control_epoch = lifecycle::begin_control_request().map_err(ApiError::Invalid)?;
    if paused {
        lifecycle::set_paused();
        Ok(())
    } else {
        lifecycle::readiness_preflight(ic_cdk::api::canister_self(), control_epoch).await
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_status() -> Status {
    api::get_status()
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

ic_cdk::export_candid!();
