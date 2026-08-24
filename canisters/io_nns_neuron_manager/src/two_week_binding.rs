use candid::Principal;

use crate::{
    api::{ApiError, MaturityProgress, PrepareTwoWeekMaturityArgs},
    maturity::{MaturityKind, MaturityPlan},
    maturity_flow,
    state::{self, Lifecycle, NnsOperation, NnsStateV1},
};

pub async fn prepare(
    caller: Principal,
    args: PrepareTwoWeekMaturityArgs,
) -> Result<MaturityProgress, ApiError> {
    let initial = state::read();
    if caller != initial.config.stream_manager {
        return Err(ApiError::Unauthorized);
    }
    if initial.lifecycle != Lifecycle::Ready {
        return Err(ApiError::Paused);
    }
    let snapshot = state::read();
    if args.entitlement_batch_generation == 0 {
        return Err(ApiError::Invalid(
            "two-week maturity generation must be positive".into(),
        ));
    }
    let target = snapshot
        .latest_pooled_target
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("two-week maturity lacks a pooled target".into()))?;
    if target.target_e8s != args.target_e8s {
        return Err(ApiError::Invalid(format!(
            "two-week maturity target {} differs from canonical {}",
            args.target_e8s, target.target_e8s
        )));
    }
    if !matches!(
        target.status,
        crate::state::PooledTargetStatus::AtTarget
            | crate::state::PooledTargetStatus::AtTargetWithinUnwindTolerance
    ) {
        return Err(ApiError::Invalid(
            "two-week maturity requires a reconciled pooled target".into(),
        ));
    }
    if snapshot.pooled_parent_id.is_none() {
        return Err(ApiError::Invalid(
            "two-week maturity requires a proved pooled parent".into(),
        ));
    }
    if args.entitlement_batch_generation == snapshot.latest_completed_two_week_generation {
        return snapshot
            .last_two_week_maturity
            .clone()
            .map(|completed| MaturityProgress::Completed(Box::new(completed)))
            .ok_or_else(|| ApiError::Invalid("completed generation lacks evidence".into()));
    }
    if args.entitlement_batch_generation == snapshot.latest_started_two_week_generation {
        if replay_matches(&snapshot, &args) {
            return Ok(MaturityProgress::Observed);
        }
        return Err(ApiError::Invalid(
            "two-week generation replay conflicts with stored evidence".into(),
        ));
    }
    let expected_generation = snapshot
        .latest_started_two_week_generation
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("two-week generation overflow".into()))?;
    if args.entitlement_batch_generation != expected_generation {
        return Err(ApiError::Invalid(format!(
            "expected two-week maturity generation {expected_generation}"
        )));
    }
    maturity_flow::start_observed(snapshot, MaturityKind::TwoWeek, Some(args)).await
}

fn replay_matches(state: &NnsStateV1, args: &PrepareTwoWeekMaturityArgs) -> bool {
    let plan_matches = |plan: &MaturityPlan| {
        plan.entitlement_batch_generation == Some(args.entitlement_batch_generation)
    };
    matches!(
        &state.active_operation,
        Some(NnsOperation::Maturity(operation))
            if operation.kind == MaturityKind::TwoWeek && plan_matches(operation.plan())
    ) || state
        .pending_two_week_maturity
        .as_ref()
        .is_some_and(|pending| plan_matches(&pending.disburse_evidence.submission.plan))
}
