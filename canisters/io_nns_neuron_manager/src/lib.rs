pub mod api;
mod claim_assets;
mod execution;
pub use io_nns_types::{jupiter, maturity, maturity::MaturityKind, pool, transfer};
mod jupiter_flow;
pub mod lifecycle;
mod maturity_flow;
mod permanent_credit;
mod pool_flow;
pub mod state;
mod two_week_binding;
mod unwind_flow;

use {candid::CandidType, serde::Deserialize};

pub use api::{
    ApiError, JupiterProgress, MaturityProgress, NnsProgress, NotifyJupiterDepositArgs,
    PoolProgress, PreparePoolReconciliationArgs, PrepareTwoWeekMaturityArgs, Status,
};
pub use state::{Lifecycle, NnsConfig, NnsStateV1, PooledTargetStatus};

fn active_operation_name(operation: &state::NnsOperation) -> &'static str {
    match operation {
        state::NnsOperation::Jupiter(_) => "Jupiter",
        state::NnsOperation::Maturity(_) => "Maturity",
        state::NnsOperation::Pool(_) => "Pool",
        state::NnsOperation::Unwind(_) => "Unwind",
    }
}

fn validate_start_maturity_state(
    snapshot: &NnsStateV1,
    kind: MaturityKind,
) -> Result<String, String> {
    if kind == MaturityKind::TwoWeek {
        return Err("two-week maturity requires a frozen stream batch".into());
    }
    if snapshot.lifecycle != Lifecycle::Ready {
        return Err("IO NNS manager is Paused".into());
    }
    if !snapshot.two_year_maturity_baseline_reconciled {
        return Err("two-year protected NNS neuron launch baseline is unreconciled".into());
    }
    if let Some(operation) = &snapshot.active_operation {
        return Err(format!(
            "IO NNS manager is busy with {}",
            active_operation_name(operation)
        ));
    }
    if snapshot.pending_two_year_maturity.is_some() {
        return Err("a two-year maturity disbursement is already pending".into());
    }
    if snapshot.next_operation_sequence == u64::MAX {
        return Err("NNS operation sequence is exhausted".into());
    }
    Ok(
        "Start reviewed two-year NNS maturity.\nThe manager is currently Ready and locally idle.\nNeuron maturity/configuration are revalidated at execution time."
            .into(),
    )
}

fn two_year_maturity_was_durably_accepted(before: &NnsStateV1, after: &NnsStateV1) -> bool {
    let expected_sequence = before.next_operation_sequence;
    let exact_active = matches!(
        &after.active_operation,
        Some(state::NnsOperation::Maturity(operation))
            if operation.kind == MaturityKind::TwoYear
                && operation.operation_sequence == expected_sequence
    );
    let new_pending =
        before.pending_two_year_maturity.is_none() && after.pending_two_year_maturity.is_some();
    let completed = before.last_two_year_maturity != after.last_two_year_maturity
        && after.last_two_year_maturity.is_some();
    after.next_operation_sequence == expected_sequence.saturating_add(1)
        && (exact_active || new_pending || completed)
}

fn two_year_maturity_committed_safety_pause(before: &NnsStateV1, after: &NnsStateV1) -> bool {
    let mut expected = before.clone();
    expected.lifecycle = Lifecycle::Paused;
    *after == expected
}

fn sns_reject(action: &str, reason: impl std::fmt::Display) -> ! {
    ic_cdk::trap(format!("SNS {action} not accepted: {reason}"))
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    pub config: NnsConfig,
}

#[cfg_attr(target_family = "wasm", ic_cdk::init)]
pub fn init(args: InitArgs) {
    state::initialize(
        NnsStateV1 {
            launch_schema_marker: state::LAUNCH_SCHEMA_MARKER,
            config: args.config,
            lifecycle: Lifecycle::Paused,
            active_operation: None,
            pooled_parent_id: None,
            pooled_parent_staking_account: None,
            live_cohorts: Vec::new(),
            last_completed_pool: None,
            last_completed_unwind: None,
            last_held_reconciliation: None,
            latest_reconciliation_generation: 0,
            latest_pooled_target: None,
            two_year_maturity_baseline_reconciled: false,
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
    api::notify_jupiter_deposit(args).await
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
pub async fn prepare_pool_reconciliation(
    args: PreparePoolReconciliationArgs,
) -> Result<PoolProgress, ApiError> {
    api::prepare_pool_reconciliation(ic_cdk::api::msg_caller(), args).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn observe_claim_assets() -> Result<io_nns_types::backing::ClaimAssetObservation, ApiError>
{
    api::observe_claim_assets(ic_cdk::api::msg_caller()).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn observe_pool_policy() -> Result<io_nns_types::backing::PoolPolicyObservation, ApiError>
{
    api::observe_pool_policy(ic_cdk::api::msg_caller()).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn start_maturity(kind: MaturityKind) -> Result<MaturityProgress, ApiError> {
    let caller = ic_cdk::api::msg_caller();
    let before = state::read();
    if caller != before.config.sns_governance {
        return Err(ApiError::Unauthorized);
    }
    // SNS Governance validates before proposal creation, but state can change while the
    // proposal is voting. The reviewed SNS implementation treats every target-method reply
    // as successful execution without decoding an application-level Result. Therefore an
    // SNS-governed request that has neither been durably accepted nor committed a deliberate
    // safety state must reject at the transport boundary instead of replying with Candid Err.
    if let Err(reason) = validate_start_maturity_state(&before, kind) {
        sns_reject("maturity action", reason);
    }
    let result = api::start_maturity(caller, kind).await;
    let after = state::read();
    let committed_safety = matches!(result, Err(ApiError::Invalid(_) | ApiError::Stuck(_)))
        && two_year_maturity_committed_safety_pause(&before, &after);
    if two_year_maturity_was_durably_accepted(&before, &after) || committed_safety {
        return result;
    }
    match result {
        Ok(progress) => sns_reject(
            "maturity action",
            format!("returned {progress:?} without durable two-year maturity state"),
        ),
        Err(error) => sns_reject("maturity action", format!("{error:?}")),
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn validate_start_maturity(kind: MaturityKind) -> Result<String, String> {
    validate_start_maturity_state(&state::read(), kind)
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn prepare_two_week_maturity(args: PrepareTwoWeekMaturityArgs) -> Result<(), ApiError> {
    api::prepare_two_week_maturity(ic_cdk::api::msg_caller(), args).await
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn validate_set_paused(paused: bool) -> Result<String, String> {
    validate_set_paused_state(&state::read(), paused)
}

fn validate_set_paused_state(snapshot: &NnsStateV1, paused: bool) -> Result<String, String> {
    let changes_lifecycle = paused != (snapshot.lifecycle == Lifecycle::Paused);
    if changes_lifecycle && snapshot.control_epoch == u64::MAX {
        return Err("NNS control epoch is exhausted".into());
    }
    if !paused && snapshot.lifecycle == Lifecycle::Paused {
        if let Some(operation) = &snapshot.active_operation {
            return Err(format!(
                "IO NNS manager cannot become Ready while busy with {}",
                active_operation_name(operation)
            ));
        }
        if snapshot.pending_two_year_maturity.is_some()
            || snapshot.pending_two_week_maturity.is_some()
            || !snapshot.live_cohorts.is_empty()
        {
            return Err("IO NNS manager has locally visible recovery work".into());
        }
    }
    Ok(format!(
        "Set IO NNS manager paused: {paused}. Current lifecycle: {:?}",
        snapshot.lifecycle
    ))
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn set_paused(paused: bool) -> Result<(), ApiError> {
    // As with maturity execution, SNS Governance treats any normal target reply as success.
    // Keep typed Unauthorized for ordinary callers, but transport-reject an authenticated SNS
    // request unless the requested lifecycle state is durable.
    let caller = ic_cdk::api::msg_caller();
    let snapshot = state::read();
    if caller != snapshot.config.sns_governance {
        return Err(ApiError::Unauthorized);
    }
    if (paused && snapshot.lifecycle == Lifecycle::Paused)
        || (!paused && snapshot.lifecycle == Lifecycle::Ready)
    {
        return Ok(());
    }
    if let Err(reason) = validate_set_paused_state(&snapshot, paused) {
        sns_reject("NNS lifecycle action", reason);
    }
    let control_epoch = lifecycle::begin_control_request().map_err(ApiError::Invalid)?;
    if paused {
        lifecycle::set_paused();
        Ok(())
    } else {
        let result =
            lifecycle::readiness_preflight(ic_cdk::api::canister_self(), control_epoch).await;
        if state::read().lifecycle == Lifecycle::Ready {
            result
        } else {
            match result {
                Ok(()) => sns_reject(
                    "NNS lifecycle action",
                    "readiness returned without durable Ready state",
                ),
                Err(error) => sns_reject("NNS lifecycle action", format!("{error:?}")),
            }
        }
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_status() -> Status {
    api::get_status()
}

#[cfg(debug_assertions)]
#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_state() -> NnsStateV1 {
    state::read()
}

#[cfg(debug_assertions)]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_replace_state(replacement: NnsStateV1) -> Result<(), String> {
    replacement.validate(ic_cdk::api::canister_self())?;
    state::write(replacement);
    Ok(())
}

#[cfg(debug_assertions)]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_yield_before_jupiter_refresh_once() {
    jupiter_flow::debug_yield_before_refresh_once();
}

#[cfg(debug_assertions)]
#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_jupiter_refresh_boundary_ready() -> bool {
    matches!(
        state::read().active_operation,
        Some(state::NnsOperation::Jupiter(operation))
            if matches!(operation.phase, jupiter::JupiterPhase::StakeTransferSucceeded(_))
    )
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
