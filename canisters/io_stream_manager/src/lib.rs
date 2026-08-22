pub mod api;
mod backing_inflow;
mod backing_registry;
pub mod canonical;
mod completed_receipt;
pub mod lifecycle;
mod pool_reconciliation;
pub mod receipt;
mod receipt_preparation;
pub mod redemption;
mod reward_evidence;
mod reward_timer;
pub mod rewards;
pub mod state;
mod status;
pub mod transfer;

use candid::CandidType;
use serde::Deserialize;

pub use api::{ApiError, JupiterReceiptProgress, RedemptionProgress, Status, StreamProgress};
pub use io_nns_types::inflow::{
    BackingInflowPermit, BackingInflowProgress, PrepareBackingInflowArgs, ProveBackingEffectArgs,
};
pub use io_nns_types::reward_boundary::BackingNotReadyReason;
pub use receipt::{CompleteJupiterReceiptArgs, JupiterReceiptPermit, PrepareJupiterReceiptArgs};
pub use redemption::RedeemArgs;
pub use rewards::RewardBackingProgress;
pub use state::CallerRedemptionState;
pub use state::{
    Account, Lifecycle, PendingEntitlementBatch, RewardEntitlementAccumulator,
    RewardEntitlementEntry, RewardEventClassification, RewardEventCredit, RewardEventId,
    RewardEventObservation, SkippedRewardEvent, StreamConfig, StreamStateV1,
};

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    pub config: StreamConfig,
}

#[cfg_attr(target_family = "wasm", ic_cdk::init)]
pub fn init(args: InitArgs) {
    // Launch stays inert; reviewed unpause installs at most one reward-event timer.
    let state = StreamStateV1 {
        launch_schema_marker: 3,
        config: args.config,
        lifecycle: Lifecycle::Paused,
        active_operation: None,
        reward_entitlements: RewardEntitlementAccumulator::default(),
        pending_entitlement_batch: None,
        backing_registry: Vec::new(),
        latest_reconciliation_checkpoint: None,
        latest_reconciliation_generation: 0,
        latest_entitlement_batch_generation: 0,
        next_nns_receipt_sequence: 0,
        next_operation_sequence: state::OperationSequence(0),
        control_epoch: 0,
        last_completed_receipt: None,
        last_completed_backing_inflow: None,
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
pub async fn prepare_jupiter_receipt(
    args: PrepareJupiterReceiptArgs,
) -> Result<JupiterReceiptPermit, ApiError> {
    receipt::prepare_jupiter_receipt(ic_cdk::api::msg_caller(), args, ic_cdk::api::time()).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn complete_jupiter_receipt(
    args: CompleteJupiterReceiptArgs,
) -> Result<JupiterReceiptProgress, ApiError> {
    receipt::complete_jupiter_receipt(ic_cdk::api::msg_caller(), args).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn prepare_backing_inflow(
    args: PrepareBackingInflowArgs,
) -> Result<BackingInflowPermit, ApiError> {
    backing_inflow::prepare(ic_cdk::api::msg_caller(), args).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn prove_backing_effect(
    args: ProveBackingEffectArgs,
) -> Result<BackingInflowProgress, ApiError> {
    backing_inflow::prove_effect(args).await
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
pub async fn resume_reward_work() -> Result<RewardEventObservation, ApiError> {
    rewards::observe(ic_cdk::api::time()).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn resume_reward_backing() -> Result<RewardBackingProgress, ApiError> {
    rewards::resume_backing(ic_cdk::api::time()).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn validate_set_paused(paused: bool) -> Result<String, String> {
    Ok(format!("Set IO stream paused: {paused}"))
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
        reward_timer::install(None);
        Ok(())
    } else {
        lifecycle::readiness_preflight(ic_cdk::api::canister_self(), control_epoch).await?;
        reward_timer::install_for_ready_state();
        Ok(())
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_status() -> Status {
    status::get_status()
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

ic_cdk::export_candid!();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_validator_renders_both_exact_payloads() {
        assert_eq!(
            validate_set_paused(true).unwrap(),
            "Set IO stream paused: true"
        );
        assert_eq!(
            validate_set_paused(false).unwrap(),
            "Set IO stream paused: false"
        );
    }

    #[test]
    fn candid_surface_is_exportable() {
        let candid = __export_service();
        assert!(!candid.is_empty());
        println!("{candid}");
    }
}
