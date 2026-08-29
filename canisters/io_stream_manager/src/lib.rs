pub mod api;
mod backing_registry;
pub mod canonical;
mod daily_stake;
pub mod lifecycle;
mod pool_reconciliation;
pub mod receipt;
pub mod redemption;
mod reward_evidence;
mod reward_timer;
pub mod rewards;
pub mod state;
mod status;
pub mod transfer;

use candid::CandidType;
use serde::Deserialize;

pub use api::{ApiError, RedemptionProgress, Status, StreamProgress};
pub use io_nns_types::reward_boundary::BackingNotReadyReason;
pub use io_receipt_types::{
    ClaimBackingReceiptPermit, ClaimBackingReceiptProgress, PrepareClaimBackingReceiptArgs,
    ProveClaimBackingReceiptArgs,
};
pub use redemption::RedeemArgs;
pub use rewards::RewardBackingProgress;
pub use state::CallerRedemptionState;
pub use state::{
    Account, Lifecycle, PendingEntitlementBatch, RewardCheckpoint, RewardEventClassification,
    RewardEventCredit, RewardEventId, RewardEventObservation, SkippedRewardEvent, StreamConfig,
    StreamStateV1,
};

fn validate_set_paused_state(snapshot: &StreamStateV1, paused: bool) -> Result<String, String> {
    let changes_lifecycle = paused != (snapshot.lifecycle == Lifecycle::Paused);
    if changes_lifecycle && snapshot.control_epoch == u64::MAX {
        return Err("stream control epoch is exhausted".into());
    }
    if !paused && snapshot.lifecycle == Lifecycle::Paused {
        if !lifecycle::is_readiness_resumable_operation(&snapshot.active_operation) {
            return Err("IO stream has an active operation that blocks readiness".into());
        }
        if snapshot.prepared_exit_reconciliation.is_some() {
            return Err("IO stream has a prepared exit reconciliation".into());
        }
    }
    Ok(format!(
        "Set IO stream paused: {paused}. Current lifecycle: {:?}",
        snapshot.lifecycle
    ))
}

fn sns_reject(action: &str, reason: impl std::fmt::Display) -> ! {
    ic_cdk::trap(format!("SNS {action} not accepted: {reason}"))
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    pub config: StreamConfig,
}

#[cfg_attr(target_family = "wasm", ic_cdk::init)]
pub fn init(args: InitArgs) {
    // Launch stays inert; reviewed unpause installs at most one reward-event timer.
    let state = StreamStateV1 {
        launch_schema_marker: state::LAUNCH_SCHEMA_MARKER,
        config: args.config,
        lifecycle: Lifecycle::Paused,
        active_operation: None,
        reward_checkpoint: RewardCheckpoint::default(),
        pending_entitlement_batch: None,
        neuron_registry: Vec::new(),
        stake_observation_due: true,
        latest_reconciliation_checkpoint: None,
        prepared_exit_reconciliation: None,
        latest_reconciliation_generation: 0,
        latest_entitlement_batch_generation: 0,
        next_operation_sequence: state::OperationSequence(1),
        control_epoch: 0,
        last_completed_claim_receipt: None,
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
pub async fn prepare_claim_backing_receipt(
    args: PrepareClaimBackingReceiptArgs,
) -> Result<ClaimBackingReceiptPermit, ApiError> {
    receipt::prepare(ic_cdk::api::msg_caller(), args).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn prove_claim_backing_receipt(
    args: ProveClaimBackingReceiptArgs,
) -> Result<ClaimBackingReceiptProgress, ApiError> {
    receipt::prove_liquid(ic_cdk::api::msg_caller(), args).await
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
    validate_set_paused_state(&state::read(), paused)
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn set_paused(paused: bool) -> Result<(), ApiError> {
    let caller = ic_cdk::api::msg_caller();
    let snapshot = state::read();
    if caller != snapshot.config.sns_governance {
        return Err(ApiError::Unauthorized);
    }
    // SNS Governance runs the validator before proposal creation, but conditions can change
    // before execution. The reviewed SNS implementation counts any target-method reply as
    // success without decoding Candid Err, so a lifecycle request that was not accepted must
    // reject at the transport boundary. A normal reply is reserved for the requested durable
    // lifecycle state.
    if (paused && snapshot.lifecycle == Lifecycle::Paused)
        || (!paused && snapshot.lifecycle == Lifecycle::Ready)
    {
        return Ok(());
    }
    if let Err(reason) = validate_set_paused_state(&snapshot, paused) {
        sns_reject("stream lifecycle action", reason);
    }
    let control_epoch = lifecycle::begin_control_request().map_err(ApiError::Invalid)?;
    if paused {
        lifecycle::set_paused();
        reward_timer::install(None);
        Ok(())
    } else {
        let result =
            lifecycle::readiness_preflight(ic_cdk::api::canister_self(), control_epoch).await;
        if state::read().lifecycle == Lifecycle::Ready {
            reward_timer::install_for_ready_state();
            result
        } else {
            match result {
                Ok(()) => sns_reject(
                    "stream lifecycle action",
                    "readiness returned without durable Ready state",
                ),
                Err(error) => sns_reject("stream lifecycle action", format!("{error:?}")),
            }
        }
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_status() -> Status {
    status::get_status()
}

#[cfg(debug_assertions)]
#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_state() -> StreamStateV1 {
    state::read()
}

#[cfg(debug_assertions)]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_replace_state(replacement: StreamStateV1) -> Result<(), String> {
    replacement.validate(ic_cdk::api::canister_self())?;
    state::write(replacement);
    Ok(())
}

#[cfg(debug_assertions)]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_fail_malformed_prepare_after_persist(enabled: bool) {
    receipt::debug_fail_malformed_prepare_after_persist(enabled);
}

#[cfg(debug_assertions)]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_trap_after_caller_result_write(enabled: bool) {
    api::debug_trap_after_caller_result_write(enabled);
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
    fn candid_surface_is_exportable() {
        let candid = __export_service();
        assert!(!candid.is_empty());
        println!("{candid}");
    }
}
