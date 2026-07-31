pub mod api;
pub mod jupiter;
pub mod lifecycle;
pub mod maturity;
pub mod pool;
pub mod state;
pub mod transfer;

use candid::CandidType;
use serde::Deserialize;

pub use api::{ApiError, NotifyJupiterDepositArgs, SetTwoWeekTargetArgs, Status};
pub use state::{Lifecycle, NnsConfig, NnsStateV1};

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    pub config: NnsConfig,
    pub initial_lifecycle: Lifecycle,
}

#[cfg_attr(target_family = "wasm", ic_cdk::init)]
pub fn init(args: InitArgs) {
    state::initialize(NnsStateV1 {
        config: args.config,
        lifecycle: args.initial_lifecycle,
        active_operation: None,
        next_jupiter_sequence: 0,
        latest_two_week_target: None,
        latest_target_generation: 0,
        pending_two_year_maturity: None,
        pending_two_week_maturity: None,
        pending_unwind: None,
    })
    .unwrap_or_else(|error| ic_cdk::trap(&error));
}

#[cfg_attr(target_family = "wasm", ic_cdk::post_upgrade)]
pub fn post_upgrade() {
    state::reopen();
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn notify_jupiter_deposit(args: NotifyJupiterDepositArgs) -> Result<(), ApiError> {
    api::notify_jupiter_deposit(ic_cdk::api::msg_caller(), args).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn set_two_week_target(args: SetTwoWeekTargetArgs) -> Result<(), ApiError> {
    api::set_two_week_target(ic_cdk::api::msg_caller(), args)
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn resume() -> Result<(), ApiError> {
    api::resume().await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn prove_active_transfer(block_index: u128) -> Result<(), ApiError> {
    api::prove_active_transfer(block_index).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn set_paused(paused: bool) -> Result<(), ApiError> {
    let state = state::read();
    if ic_cdk::api::msg_caller() != state.config.sns_governance {
        return Err(ApiError::Unauthorized);
    }
    lifecycle::set_paused(paused).map_err(ApiError::Invalid)
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_status() -> Status {
    api::get_status()
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

ic_cdk::export_candid!();
