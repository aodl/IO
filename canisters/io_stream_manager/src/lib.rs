pub mod api;
pub mod canonical;
pub mod lifecycle;
pub mod receipt;
mod receipt_preparation;
pub mod redemption;
mod reward_evidence;
mod reward_settlement;
pub mod rewards;
pub mod state;
pub mod transfer;

use candid::{CandidType, Principal};
use serde::Deserialize;

pub use api::{ApiError, LiquidReceiptProgress, RedemptionProgress, Status, StreamProgress};
pub use receipt::{
    CompleteLiquidReceiptArgs, CompletedReceiptResult, LiquidReceiptPermit,
    PrepareLiquidReceiptArgs, ReceiptKind,
};
pub use redemption::RedeemArgs;
pub use state::CallerRedemptionState;
pub use state::{Account, Lifecycle, RewardCohort, StreamConfig, StreamStateV1};

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    pub config: StreamConfig,
    pub next_cohort_timestamp_seconds: u64,
}

#[cfg_attr(target_family = "wasm", ic_cdk::init)]
pub fn init(args: InitArgs) {
    let state = StreamStateV1 {
        config: args.config,
        lifecycle: Lifecycle::Paused,
        active_operation: None,
        active_reward_cohort: None,
        pending_reward_cohort: None,
        latest_cohort_generation: 0,
        next_nns_receipt_sequence: 0,
        next_cohort_timestamp_seconds: args.next_cohort_timestamp_seconds,
        next_operation_sequence: state::OperationSequence(0),
        control_epoch: 0,
        last_completed_receipt: None,
    };
    state::initialize(state, ic_cdk::api::canister_self())
        .unwrap_or_else(|error| ic_cdk::trap(&error));
}

#[cfg_attr(target_family = "wasm", ic_cdk::post_upgrade)]
pub fn post_upgrade() {
    state::reopen(ic_cdk::api::canister_self());
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn redeem(args: RedeemArgs) -> Result<RedemptionProgress, ApiError> {
    api::redeem(ic_cdk::api::msg_caller(), args, ic_cdk::api::time()).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn prepare_liquid_receipt(
    args: PrepareLiquidReceiptArgs,
) -> Result<LiquidReceiptPermit, ApiError> {
    receipt::prepare_liquid_receipt(ic_cdk::api::msg_caller(), args, ic_cdk::api::time()).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn complete_liquid_receipt(
    args: CompleteLiquidReceiptArgs,
) -> Result<LiquidReceiptProgress, ApiError> {
    receipt::complete_liquid_receipt(ic_cdk::api::msg_caller(), args).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn resume() -> Result<StreamProgress, ApiError> {
    api::resume_stream(ic_cdk::api::time()).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn prove_active_transfer(block_index: u128) -> Result<(), ApiError> {
    api::prove_active_transfer(block_index).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn capture_reward_cohort() -> Result<RewardCohort, ApiError> {
    rewards::capture(ic_cdk::api::time() / 1_000_000_000).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn close_reward_cohort() -> Result<RewardCohort, ApiError> {
    rewards::close(ic_cdk::api::time() / 1_000_000_000).await
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

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_caller_redemption_state() -> Result<CallerRedemptionState, ApiError> {
    let caller = ic_cdk::api::msg_caller();
    if caller == candid::Principal::anonymous() {
        return Err(ApiError::Anonymous);
    }
    let state = state::caller_state(caller);
    state.validate().map_err(ApiError::Invalid)?;
    Ok(state)
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[allow(dead_code)]
fn _principal_type(_: Principal) {}

ic_cdk::export_candid!();
