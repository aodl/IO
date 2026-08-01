use candid::Principal;

use crate::{
    api::{ApiError, MaturityProgress, PrepareTwoWeekMaturityArgs},
    execution,
    maturity::{
        MaturityCommandOperation, MaturityCommandPhase, MaturityKind, MaturityPlan, MintProofState,
        PendingMaturityDisbursement, TwoWeekDeliveryOperation, TWO_WEEK_COHORT_SECONDS,
    },
    maturity_flow,
    state::{self, Lifecycle, NnsOperation, NnsStateV1},
};

pub async fn start_delivery(
    expected: PendingMaturityDisbursement,
) -> Result<MaturityProgress, ApiError> {
    let MintProofState::Proved(mint) = expected.mint_proof.clone() else {
        return Err(ApiError::Busy);
    };
    let config = state::read().config;
    let balance = execution::icp_balance(&config, &config.two_week_maturity_staging).await?;
    maturity_flow::ensure_pending(&expected)?;
    let required = mint
        .actual_minted_icp_e8s
        .checked_add(config.two_week_fee_float_e8s)
        .ok_or_else(|| ApiError::Invalid("two-week delivery preflight overflow".into()))?;
    if balance < required {
        return Err(ApiError::Stuck(format!(
            "two-week staging balance {balance} is below actual Mint plus fee float {required}"
        )));
    }
    let mut latest = state::read();
    if latest.active_operation.is_some()
        || maturity_flow::pending_from(&latest, MaturityKind::TwoWeek).as_ref() != Some(&expected)
    {
        return Err(ApiError::Busy);
    }
    let operation_sequence = latest.next_operation_sequence;
    latest.next_operation_sequence = operation_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("operation sequence exhausted".into()))?;
    let mut passive = expected;
    passive.mint_proof = MintProofState::Delivering(mint);
    latest.pending_two_week_maturity = Some(passive.clone());
    latest.active_operation = Some(NnsOperation::Maturity(Box::new(MaturityCommandOperation {
        operation_sequence,
        dispatch_epoch: 0,
        kind: MaturityKind::TwoWeek,
        phase: MaturityCommandPhase::TwoWeekDelivery(TwoWeekDeliveryOperation {
            pending: passive,
            permit: None,
            transfer: None,
            receipt_completed: false,
        }),
    })));
    state::write(latest);
    Ok(MaturityProgress::DeliveringTwoWeekReceipt)
}

pub async fn prepare(
    caller: Principal,
    args: PrepareTwoWeekMaturityArgs,
) -> Result<MaturityProgress, ApiError> {
    let snapshot = state::read();
    if snapshot.lifecycle != Lifecycle::Ready {
        return Err(ApiError::Paused);
    }
    if caller != snapshot.config.stream_manager {
        return Err(ApiError::Unauthorized);
    }
    if args.cohort_generation == 0
        || args
            .captured_at_timestamp_seconds
            .checked_add(TWO_WEEK_COHORT_SECONDS)
            != Some(args.closes_at_timestamp_seconds)
        || now_seconds()? < args.closes_at_timestamp_seconds
        || args.cohort_generation != snapshot.latest_target_generation
        || snapshot
            .latest_two_week_target
            .as_ref()
            .is_none_or(|target| target.generation != args.cohort_generation)
        || !snapshot.two_week_maturity_baseline_reconciled
    {
        return Err(ApiError::Invalid(
            "two-week maturity does not match one closed target generation".into(),
        ));
    }
    if args.cohort_generation == snapshot.latest_completed_two_week_generation {
        return snapshot
            .last_two_week_maturity
            .clone()
            .map(MaturityProgress::Completed)
            .ok_or_else(|| ApiError::Invalid("completed generation lacks evidence".into()));
    }
    if args.cohort_generation == snapshot.latest_started_two_week_generation {
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
    if args.cohort_generation != expected_generation {
        return Err(ApiError::Invalid(format!(
            "expected two-week maturity generation {expected_generation}"
        )));
    }
    maturity_flow::start_observed(snapshot, MaturityKind::TwoWeek, Some(args)).await
}

fn replay_matches(state: &NnsStateV1, args: &PrepareTwoWeekMaturityArgs) -> bool {
    let plan_matches = |plan: &MaturityPlan| {
        plan.cohort_generation == Some(args.cohort_generation)
            && plan.cohort_captured_at_seconds == Some(args.captured_at_timestamp_seconds)
            && plan.cohort_closes_at_seconds == Some(args.closes_at_timestamp_seconds)
    };
    matches!(
        &state.active_operation,
        Some(NnsOperation::Maturity(operation))
            if operation.kind == MaturityKind::TwoWeek && plan_matches(operation.plan())
    ) || state
        .pending_two_week_maturity
        .as_ref()
        .is_some_and(|pending| plan_matches(&pending.stake_evidence.plan))
}

fn now_seconds() -> Result<u64, ApiError> {
    let now = ic_cdk::api::time() / 1_000_000_000;
    (now > 0)
        .then_some(now)
        .ok_or_else(|| ApiError::Invalid("time is zero".into()))
}
