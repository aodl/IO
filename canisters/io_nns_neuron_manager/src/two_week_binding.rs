use candid::Principal;

use crate::{
    api::{ApiError, PrepareTwoWeekMaturityArgs},
    maturity::MaturityKind,
    maturity_flow,
    state::{self, Lifecycle, NnsOperation, NnsStateV1},
};

pub async fn prepare(caller: Principal, args: PrepareTwoWeekMaturityArgs) -> Result<(), ApiError> {
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
    if let Some(completed) = snapshot
        .last_two_week_maturity
        .as_ref()
        .filter(|completed| {
            completed.entitlement_batch_generation == Some(args.entitlement_batch_generation)
        })
    {
        return if completed.two_week_target_e8s == Some(args.target_e8s) {
            Ok(())
        } else {
            Err(ApiError::Invalid(
                "completed two-week replay target differs".into(),
            ))
        };
    }
    let latest_generation = snapshot.latest_two_week_generation();
    if args.entitlement_batch_generation == latest_generation {
        if !replay_matches(&snapshot, &args) {
            return Err(ApiError::Invalid(
                "two-week generation replay conflicts with stored intent".into(),
            ));
        }
        return match snapshot.active_operation {
            Some(NnsOperation::Maturity(operation)) if operation.kind == MaturityKind::TwoWeek => {
                maturity_flow::resume_active(*operation).await.map(|_| ())
            }
            None if snapshot.pending_two_week_maturity.is_some() => {
                maturity_flow::resume_kind(MaturityKind::TwoWeek)
                    .await
                    .map(|_| ())
            }
            _ => Err(ApiError::Busy),
        };
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
    let expected_generation = latest_generation
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("two-week generation overflow".into()))?;
    if args.entitlement_batch_generation != expected_generation {
        return Err(ApiError::Invalid(format!(
            "expected two-week maturity generation {expected_generation}"
        )));
    }
    maturity_flow::start_observed(snapshot, MaturityKind::TwoWeek, Some(args)).await?;
    let operation = match state::read().active_operation {
        Some(NnsOperation::Maturity(operation)) if operation.kind == MaturityKind::TwoWeek => {
            *operation
        }
        _ => return Err(ApiError::Busy),
    };
    maturity_flow::resume_active(operation).await.map(|_| ())
}

fn replay_matches(state: &NnsStateV1, args: &PrepareTwoWeekMaturityArgs) -> bool {
    let intent_matches = |generation: Option<u64>, target: Option<u128>| {
        generation == Some(args.entitlement_batch_generation) && target == Some(args.target_e8s)
    };
    matches!(
        &state.active_operation,
        Some(NnsOperation::Maturity(operation))
            if operation.kind == MaturityKind::TwoWeek
                && intent_matches(
                    operation.intent().entitlement_batch_generation,
                    operation.intent().two_week_target_e8s,
                )
    ) || state
        .pending_two_week_maturity
        .as_ref()
        .is_some_and(|pending| {
            intent_matches(
                pending.entitlement_batch_generation,
                pending.two_week_target_e8s,
            )
        })
}
