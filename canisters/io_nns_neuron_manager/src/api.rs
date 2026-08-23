use candid::{CandidType, Principal};
use io_nns_types::backing::{
    ClaimAssetObservation, CohortObservation, CohortProofState, FollowPolicy,
    ParentAssetObservation, ParentPolicyObservation, PoolCommand, PoolCommandKind,
    PoolCommandPhase, PoolPolicyObservation, TopUpPermit, POOLED_PARENT_DELAY_SECONDS,
};
pub use io_nns_types::backing::{PoolProgress, PreparePoolReconciliationArgs};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    execution,
    jupiter::JupiterCompleted,
    maturity::{CompletedMaturity, MaturityKind},
    pool::{UnwindOperation, UnwindPhase},
    state::{self, Lifecycle, NnsOperation, PooledTarget, PooledTargetStatus},
};

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ApiError {
    Unauthorized,
    Paused,
    Busy,
    Invalid(String),
    Pending(String),
    Stuck(String),
    BelowMaturityThreshold {
        remaining_e8s: u64,
        minimum_e8s: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NotifyJupiterDepositArgs {
    pub block_index: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PrepareTwoWeekMaturityArgs {
    pub entitlement_batch_generation: u64,
    pub target_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum JupiterProgress {
    DepositProved,
    StakeTransferPrepared,
    StakeTransferSubmitted,
    StakeTransferSucceeded,
    RefreshSubmitted,
    StakeIncreaseProved,
    ReceiptPermitPrepared,
    LiquidTransferPrepared,
    LiquidTransferSubmitted,
    LiquidTransferSucceeded,
    ReceiptCompletionSubmitted,
    AwaitingStreamSettlement,
    Completed(JupiterCompleted),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum MaturityProgress {
    Observed,
    StakeMaturitySubmitted,
    StakeMaturitySucceeded,
    DisburseMaturitySubmitted,
    DisburseMaturitySucceeded,
    AwaitingMintProof,
    MintProved,
    DeliveringClaimReceipt,
    Completed(CompletedMaturity),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum UnwindProgress {
    Waiting,
    AwaitingTransferProof,
    Completed { block_index: u128, liquid_e8s: u128 },
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsProgress {
    Jupiter(JupiterProgress),
    Maturity(MaturityProgress),
    Unwind(UnwindProgress),
    Pool(PoolProgress),
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct Status {
    pub lifecycle: Lifecycle,
    pub active_operation: Option<String>,
    pub two_year_maturity_baseline_reconciled: bool,
    pub latest_started_two_week_generation: u64,
    pub latest_completed_two_week_generation: u64,
    pub latest_pooled_target: Option<PooledTarget>,
    pub live_child_physical_principal_e8s: u128,
    pub live_child_net_backing_e8s: u128,
    pub live_child_committed_fee_liability_e8s: u128,
}

pub(crate) fn ready() -> Result<crate::state::NnsStateV1, ApiError> {
    let state = state::read();
    let is_ready = state.lifecycle == Lifecycle::Ready;
    is_ready.then_some(state).ok_or(ApiError::Paused)
}

pub async fn notify_jupiter_deposit(
    args: NotifyJupiterDepositArgs,
) -> Result<JupiterProgress, ApiError> {
    crate::jupiter_flow::notify_jupiter_deposit(args).await
}

pub async fn resume() -> Result<NnsProgress, ApiError> {
    let snapshot = state::read();
    match snapshot.active_operation {
        None if snapshot.pending_two_week_maturity.is_some() => {
            crate::maturity_flow::resume_kind(MaturityKind::TwoWeek)
                .await
                .map(NnsProgress::Maturity)
        }
        None if snapshot.pending_two_year_maturity.is_some() => {
            crate::maturity_flow::resume_kind(MaturityKind::TwoYear)
                .await
                .map(NnsProgress::Maturity)
        }
        None if !snapshot.live_cohorts.is_empty() => {
            let now = ic_cdk::api::time() / 1_000_000_000;
            let cohort = snapshot
                .live_cohorts
                .iter()
                .filter(|cohort| cohort.ready_at_seconds <= now)
                .min_by_key(|cohort| (cohort.ready_at_seconds, cohort.generation))
                .cloned();
            match cohort {
                Some(cohort) => crate::unwind_flow::resume_passive(cohort)
                    .await
                    .map(NnsProgress::Unwind),
                None => Ok(NnsProgress::Idle),
            }
        }
        None => Ok(NnsProgress::Idle),
        Some(NnsOperation::Jupiter(operation)) => crate::jupiter_flow::resume(*operation)
            .await
            .map(NnsProgress::Jupiter),
        Some(NnsOperation::Maturity(operation)) => crate::maturity_flow::resume_active(*operation)
            .await
            .map(NnsProgress::Maturity),
        Some(NnsOperation::Pool(operation)) => crate::pool_flow::resume(operation)
            .await
            .map(NnsProgress::Pool),
        Some(NnsOperation::Unwind(operation)) => crate::unwind_flow::resume(operation)
            .await
            .map(NnsProgress::Unwind),
    }
}

pub async fn prove_active_transfer(block_index: u128) -> Result<NnsProgress, ApiError> {
    match state::read().active_operation {
        Some(NnsOperation::Unwind(operation)) => crate::unwind_flow::prove(operation, block_index)
            .await
            .map(NnsProgress::Unwind),
        Some(NnsOperation::Maturity(operation)) => {
            crate::maturity_flow::prove_active_transfer(*operation, block_index)
                .await
                .map(NnsProgress::Maturity)
        }
        Some(NnsOperation::Jupiter(_)) => crate::jupiter_flow::prove_active_transfer(block_index)
            .await
            .map(NnsProgress::Jupiter),
        Some(NnsOperation::Pool(operation)) => {
            crate::pool_flow::prove_transfer(operation, block_index)
                .await
                .map(NnsProgress::Pool)
        }
        None => match state::read().last_completed_pool {
            Some(completed) if completed.transfer_block_index == block_index => {
                Ok(NnsProgress::Pool(PoolProgress::Completed {
                    parent_neuron_id: completed.parent_neuron_id,
                    principal_e8s: completed.principal_e8s,
                    target_status: if completed.principal_e8s
                        == completed
                            .permit
                            .expected_parent_principal_e8s
                            .saturating_add(completed.permit.expected_credit_e8s)
                    {
                        io_nns_types::backing::PoolTargetResult::AtTarget
                    } else {
                        io_nns_types::backing::PoolTargetResult::OverTarget
                    },
                }))
            }
            _ => Err(ApiError::Invalid("no active transfer proof slot".into())),
        },
    }
}

pub async fn start_maturity(
    caller: Principal,
    kind: MaturityKind,
) -> Result<MaturityProgress, ApiError> {
    crate::maturity_flow::start(caller, kind).await
}

pub async fn prepare_two_week_maturity(
    caller: Principal,
    args: PrepareTwoWeekMaturityArgs,
) -> Result<MaturityProgress, ApiError> {
    crate::two_week_binding::prepare(caller, args).await
}

pub async fn prove_maturity_mint(
    kind: MaturityKind,
    block_index: u128,
) -> Result<MaturityProgress, ApiError> {
    crate::maturity_mint::prove(kind, block_index).await
}

pub fn get_status() -> Status {
    let current = state::read();
    let (physical, net) = current
        .live_cohorts
        .iter()
        .filter(|cohort| cohort.proof != CohortProofState::PrincipalReturned)
        .fold((0, 0), |(physical, net), cohort| {
            (
                physical + cohort.principal_e8s,
                net + cohort.principal_e8s - current.config.expected_icp_fee_e8s,
            )
        });
    Status {
        lifecycle: current.lifecycle,
        active_operation: current
            .active_operation
            .as_ref()
            .map(|operation| match operation {
                NnsOperation::Jupiter(_) => "Jupiter".into(),
                NnsOperation::Maturity(_) => "Maturity".into(),
                NnsOperation::Pool(_) => "Pool".into(),
                NnsOperation::Unwind(_) => "Unwind".into(),
            }),
        two_year_maturity_baseline_reconciled: current.two_year_maturity_baseline_reconciled,
        latest_started_two_week_generation: current.latest_started_two_week_generation,
        latest_completed_two_week_generation: current.latest_completed_two_week_generation,
        latest_pooled_target: current.latest_pooled_target.clone(),
        live_child_physical_principal_e8s: physical,
        live_child_net_backing_e8s: net,
        live_child_committed_fee_liability_e8s: physical - net,
    }
}

pub async fn prepare_pool_reconciliation(
    caller: Principal,
    args: PreparePoolReconciliationArgs,
) -> Result<PoolProgress, ApiError> {
    use io_nns_types::backing::PoolReconciliationAction;

    let snapshot = state::read();
    let reconciliation_request_fingerprint = reconciliation_request_fingerprint(&args)?;
    if caller != snapshot.config.stream_manager {
        return Err(ApiError::Unauthorized);
    }
    if let Some(completed) = snapshot
        .last_completed_unwind
        .as_ref()
        .filter(|completed| completed.generation == args.generation)
    {
        if completed.reconciliation_request_fingerprint != reconciliation_request_fingerprint {
            return Err(ApiError::Invalid(
                "completed reconciliation replay fingerprint differs".into(),
            ));
        }
        return Ok(PoolProgress::UnwindCommitted {
            generation: completed.generation,
            principal_e8s: completed.physical_principal_e8s,
        });
    }
    if let Some(cohort) = snapshot
        .live_cohorts
        .iter()
        .find(|cohort| cohort.generation == args.generation)
    {
        if cohort.reconciliation_request_fingerprint != reconciliation_request_fingerprint {
            return Err(ApiError::Invalid(
                "reconciliation generation replay fingerprint differs".into(),
            ));
        }
        return Ok(PoolProgress::UnwindCommitted {
            generation: cohort.generation,
            principal_e8s: cohort.principal_e8s,
        });
    }
    if let Some(NnsOperation::Unwind(operation)) = &snapshot.active_operation {
        if operation.generation == args.generation
            && operation.reconciliation_request_fingerprint == reconciliation_request_fingerprint
            && operation.target_e8s == args.target_e8s
            && matches!(
                args.action,
                PoolReconciliationAction::Unwind { expected_gross_e8s }
                    if expected_gross_e8s == operation.gross_e8s
            )
        {
            return Ok(PoolProgress::UnwindPrepared {
                generation: operation.generation,
                gross_e8s: operation.gross_e8s,
            });
        }
    }
    if let Some(NnsOperation::Pool(operation)) = &snapshot.active_operation {
        if operation.permit.generation == args.generation
            && operation.permit.snapshot_fingerprint == args.snapshot_fingerprint
            && operation.permit.fee_e8s == args.fee_e8s
            && operation.permit.memo == args.memo
            && operation.permit.prepared_at_nanos == args.created_at_time_nanos
            && operation
                .permit
                .expected_parent_principal_e8s
                .checked_add(operation.permit.expected_credit_e8s)
                == Some(args.target_e8s)
            && matches!(
                args.action,
                PoolReconciliationAction::TopUp { expected_credit_e8s }
                    if expected_credit_e8s == operation.permit.expected_credit_e8s
            )
        {
            return Ok(PoolProgress::AwaitingTransfer(operation.permit.clone()));
        }
    }
    if snapshot.lifecycle != Lifecycle::Ready {
        return Err(ApiError::Paused);
    }
    if args.generation == snapshot.latest_reconciliation_generation
        && snapshot.active_operation.is_none()
    {
        if let Some(held) = &snapshot.last_held_reconciliation {
            if held.generation == args.generation
                && held.target_e8s == args.target_e8s
                && held.snapshot_fingerprint == args.snapshot_fingerprint
                && matches!(args.action, PoolReconciliationAction::Hold)
            {
                return Ok(PoolProgress::Held {
                    principal_e8s: held.principal_e8s,
                });
            }
        }
        return Err(ApiError::Invalid(
            "reconciliation generation was already consumed".into(),
        ));
    }
    if snapshot.active_operation.is_some() {
        return Err(ApiError::Busy);
    }

    let observation = observe_claim_assets().await?;
    if observation.fingerprint != args.snapshot_fingerprint
        || args.generation == 0
        || args.generation <= snapshot.latest_reconciliation_generation
        || args.fee_e8s != snapshot.config.expected_icp_fee_e8s
        || args.created_at_time_nanos == 0
        || args.memo.is_empty()
        || args.memo.len() > 32
    {
        return Err(ApiError::Invalid(
            "pool reconciliation snapshot or intent is invalid".into(),
        ));
    }
    require_pool_policy(&snapshot).await?;
    let actual = observation.pooled_parent_principal_e8s;
    match args.action {
        PoolReconciliationAction::Hold => {
            validate_hold(&snapshot, actual, args.target_e8s, args.fee_e8s)?;
            refresh_parent_for_reconciliation(&snapshot).await?;
            commit_reconciliation_generation(
                &snapshot,
                args.generation,
                args.target_e8s,
                actual,
                args.snapshot_fingerprint,
            )?;
            Ok(PoolProgress::Held {
                principal_e8s: actual,
            })
        }
        PoolReconciliationAction::TopUp {
            expected_credit_e8s,
        } => {
            let expected_credit = args
                .target_e8s
                .checked_sub(actual)
                .ok_or_else(|| ApiError::Invalid("pool reconciliation is not a top-up".into()))?;
            if expected_credit == 0 || expected_credit != expected_credit_e8s {
                return Err(ApiError::Invalid(
                    "pool top-up credit does not match the target".into(),
                ));
            }
            prepare_top_up(snapshot, observation, args, expected_credit)
        }
        PoolReconciliationAction::Unwind { expected_gross_e8s } => {
            let expected_gross = actual
                .checked_sub(args.target_e8s)
                .ok_or_else(|| ApiError::Invalid("pool reconciliation is not an unwind".into()))?;
            if expected_gross == 0 || expected_gross != expected_gross_e8s {
                return Err(ApiError::Invalid(
                    "pool unwind gross does not match the target".into(),
                ));
            }
            if snapshot.live_cohorts.len() >= io_nns_types::backing::MAX_LIVE_UNWIND_COHORTS {
                return Ok(PoolProgress::CapacityPending);
            }
            refresh_parent_for_reconciliation(&snapshot).await?;
            let mut latest = state::read();
            if latest != snapshot || latest.active_operation.is_some() {
                return Err(ApiError::Busy);
            }
            let operation_sequence = latest.next_operation_sequence;
            latest.next_operation_sequence = operation_sequence
                .checked_add(1)
                .ok_or_else(|| ApiError::Invalid("operation sequence exhausted".into()))?;
            latest.latest_reconciliation_generation = args.generation;
            latest.last_held_reconciliation = None;
            latest.latest_pooled_target = Some(PooledTarget {
                target_e8s: args.target_e8s,
                status: PooledTargetStatus::OverTarget,
            });
            let operation = UnwindOperation {
                operation_sequence,
                generation: args.generation,
                reconciliation_request_fingerprint: reconciliation_request_fingerprint.clone(),
                target_e8s: args.target_e8s,
                gross_e8s: expected_gross,
                child_neuron_id: 0,
                principal_e8s: 0,
                child_staking_subaccount: Vec::new(),
                submitted_at_seconds: 0,
                expected_block_index: None,
                child_maturity_e8s: 0,
                parent_maturity_e8s: 0,
                parent_principal_e8s: 0,
                phase: UnwindPhase::SplitPrepared,
            };
            latest.active_operation = Some(NnsOperation::Unwind(operation.clone()));
            state::write(latest);
            match crate::unwind_flow::resume(operation).await {
                Ok(_) => Ok(PoolProgress::UnwindPrepared {
                    generation: args.generation,
                    gross_e8s: expected_gross,
                }),
                Err(_)
                    if matches!(
                        state::read().active_operation,
                        Some(NnsOperation::Unwind(ref active))
                            if active.generation == args.generation
                                && active.reconciliation_request_fingerprint
                                    == reconciliation_request_fingerprint
                    ) =>
                {
                    Ok(PoolProgress::UnwindPrepared {
                        generation: args.generation,
                        gross_e8s: expected_gross,
                    })
                }
                Err(error) => Err(error),
            }
        }
    }
}

fn reconciliation_request_fingerprint(
    args: &PreparePoolReconciliationArgs,
) -> Result<Vec<u8>, ApiError> {
    let encoded = candid::encode_one(args).map_err(|error| {
        ApiError::Invalid(format!(
            "reconciliation request fingerprint failed: {error}"
        ))
    })?;
    Ok(Sha256::digest(encoded).to_vec())
}

fn prepare_top_up(
    snapshot: crate::state::NnsStateV1,
    observation: ClaimAssetObservation,
    args: PreparePoolReconciliationArgs,
    expected_credit: u128,
) -> Result<PoolProgress, ApiError> {
    let kind = if snapshot.pooled_parent_id.is_some() {
        PoolCommandKind::TopUp
    } else {
        if expected_credit < snapshot.config.minimum_parent_stake_e8s {
            return Err(ApiError::Pending(
                "pooled parent minimum is not reached".into(),
            ));
        }
        PoolCommandKind::Bootstrap
    };
    let destination = snapshot
        .pooled_parent_staking_account
        .clone()
        .unwrap_or_else(|| {
            execution::parent_staking_account(&snapshot.config, snapshot.config.pooled_parent_memo)
        });
    let mut latest = state::read();
    if latest != snapshot || latest.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    let operation_sequence = latest.next_operation_sequence;
    latest.next_operation_sequence = operation_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("operation sequence exhausted".into()))?;
    let permit = TopUpPermit {
        generation: args.generation,
        operation_sequence,
        expected_parent_principal_e8s: observation.pooled_parent_principal_e8s,
        destination,
        expected_credit_e8s: expected_credit,
        fee_e8s: args.fee_e8s,
        memo: args.memo,
        prepared_at_nanos: args.created_at_time_nanos,
        snapshot_fingerprint: observation.fingerprint,
    };
    let operation = PoolCommand {
        kind,
        permit: permit.clone(),
        transfer_block_index: None,
        parent_neuron_id: snapshot.pooled_parent_id,
        phase: PoolCommandPhase::AwaitingTransfer,
    };
    operation
        .validate(latest.next_operation_sequence)
        .map_err(ApiError::Invalid)?;
    latest.latest_reconciliation_generation = args.generation;
    latest.last_held_reconciliation = None;
    latest.latest_pooled_target = Some(PooledTarget {
        target_e8s: args.target_e8s,
        status: PooledTargetStatus::UnderTarget,
    });
    latest.active_operation = Some(NnsOperation::Pool(operation));
    state::write(latest);
    Ok(PoolProgress::AwaitingTransfer(permit))
}

async fn refresh_parent_for_reconciliation(
    snapshot: &crate::state::NnsStateV1,
) -> Result<(), ApiError> {
    if let Some(parent) = snapshot.pooled_parent_id {
        execution::refresh_voting_power(&snapshot.config, parent).await?;
        if state::read() != *snapshot {
            return Err(ApiError::Busy);
        }
    }
    Ok(())
}

fn commit_reconciliation_generation(
    snapshot: &crate::state::NnsStateV1,
    generation: u64,
    target_e8s: u128,
    actual_e8s: u128,
    snapshot_fingerprint: Vec<u8>,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    if latest != *snapshot {
        return Err(ApiError::Busy);
    }
    latest.latest_reconciliation_generation = generation;
    latest.last_held_reconciliation = Some(state::HeldReconciliation {
        generation,
        target_e8s,
        principal_e8s: actual_e8s,
        snapshot_fingerprint,
    });
    latest.latest_pooled_target = Some(PooledTarget {
        target_e8s,
        status: state::target_status(actual_e8s, target_e8s, snapshot.config.expected_icp_fee_e8s),
    });
    state::write(latest);
    Ok(())
}

fn validate_hold(
    snapshot: &crate::state::NnsStateV1,
    actual_e8s: u128,
    target_e8s: u128,
    fee_e8s: u128,
) -> Result<(), ApiError> {
    let delta = actual_e8s.abs_diff(target_e8s);
    let valid = actual_e8s == target_e8s
        || (actual_e8s < target_e8s
            && (delta <= fee_e8s
                || (snapshot.pooled_parent_id.is_none()
                    && target_e8s < snapshot.config.minimum_parent_stake_e8s)))
        || (actual_e8s > target_e8s
            && delta
                < u128::from(crate::maturity::MINIMUM_DISBURSEMENT_E8S)
                    .checked_add(fee_e8s)
                    .ok_or_else(|| ApiError::Invalid("minimum unwind gross overflow".into()))?);
    if valid {
        Ok(())
    } else {
        Err(ApiError::Invalid(
            "hold contradicts the canonical target".into(),
        ))
    }
}
pub async fn observe_claim_assets() -> Result<ClaimAssetObservation, ApiError> {
    let snapshot = state::read();
    if has_ambiguous_backing_effect(&snapshot) {
        return Err(ApiError::Pending(
            "claim backing has an ambiguous submitted monetary effect".into(),
        ));
    }
    let pool_staking_account =
        execution::parent_staking_account(&snapshot.config, snapshot.config.pooled_parent_memo);
    let parent = match snapshot.pooled_parent_id {
        Some(parent_id) => {
            let observed = execution::query_neuron_observation(&snapshot.config, parent_id).await?;
            let staking_account = execution::staking_account(&snapshot.config, &observed.snapshot);
            if staking_account != pool_staking_account
                || snapshot.pooled_parent_staking_account.as_ref() != Some(&staking_account)
            {
                return Err(ApiError::Invalid(
                    "pooled parent identity or staking Account drifted".into(),
                ));
            }
            Some(ParentAssetObservation {
                neuron_id: parent_id,
                staking_account,
                physical_principal_e8s: observed.snapshot.cached_stake_e8s,
            })
        }
        None => None,
    };
    if state::read() != snapshot {
        return Err(ApiError::Busy);
    }
    let mut live_cohorts = Vec::with_capacity(snapshot.live_cohorts.len());
    for cohort in &snapshot.live_cohorts {
        let physical = if matches!(
            cohort.proof,
            CohortProofState::Dissolving | CohortProofState::DisbursementSubmitted
        ) {
            let observed =
                execution::query_neuron_observation(&snapshot.config, cohort.child_neuron_id)
                    .await?;
            if observed.snapshot.staking_subaccount.as_slice()
                != cohort.child_staking_subaccount.as_slice()
                || observed.snapshot.cached_stake_e8s != cohort.principal_e8s
            {
                return Err(ApiError::Invalid(
                    "live unwind child identity or physical principal drifted".into(),
                ));
            }
            observed.snapshot.cached_stake_e8s
        } else {
            0
        };
        let net = if physical == 0 {
            0
        } else {
            io_nns_types::backing::net_committed_child_backing(
                physical,
                snapshot.config.expected_icp_fee_e8s,
            )
            .map_err(|_| {
                ApiError::Invalid(
                    "child principal cannot cover its committed disbursement fee".into(),
                )
            })?
        };
        live_cohorts.push(CohortObservation {
            generation: cohort.generation,
            child_neuron_id: cohort.child_neuron_id,
            physical_principal_e8s: physical,
            net_backing_e8s: net,
            ready_at_seconds: cohort.ready_at_seconds,
            proof: cohort.proof,
        });
    }
    if state::read() != snapshot {
        return Err(ApiError::Busy);
    }
    let (live_child_physical_principal_e8s, live_child_net_backing_e8s) = live_cohorts
        .iter()
        .try_fold((0u128, 0u128), |(physical, net), cohort| {
            Ok::<_, ApiError>((
                physical
                    .checked_add(cohort.physical_principal_e8s)
                    .ok_or_else(|| {
                        ApiError::Invalid("cohort physical principal overflow".into())
                    })?,
                net.checked_add(cohort.net_backing_e8s)
                    .ok_or_else(|| ApiError::Invalid("cohort net backing overflow".into()))?,
            ))
        })?;
    let live_child_committed_fee_liability_e8s = live_child_physical_principal_e8s
        .checked_sub(live_child_net_backing_e8s)
        .ok_or_else(|| ApiError::Invalid("cohort fee liability underflow".into()))?;
    let pooled_parent_principal_e8s = parent
        .as_ref()
        .map_or(0, |value| value.physical_principal_e8s);
    let transit_backing_e8s = transit_backing(&snapshot, pooled_parent_principal_e8s)?;
    let active_operation_sequence = active_operation_sequence(&snapshot);
    let oldest_ready_at_seconds = live_cohorts
        .iter()
        .map(|cohort| cohort.ready_at_seconds)
        .min();
    let encoded = candid::encode_one((
        &parent,
        &pool_staking_account,
        snapshot.config.minimum_parent_stake_e8s,
        pooled_parent_principal_e8s,
        &live_cohorts,
        live_child_physical_principal_e8s,
        live_child_net_backing_e8s,
        live_child_committed_fee_liability_e8s,
        transit_backing_e8s,
        active_operation_sequence,
        snapshot
            .last_completed_pool
            .as_ref()
            .map(|completed| completed.permit.operation_sequence),
        snapshot.control_epoch,
    ))
    .map_err(|error| ApiError::Invalid(format!("observation fingerprint failed: {error}")))?;
    let result = ClaimAssetObservation {
        parent,
        pool_staking_account,
        minimum_parent_stake_e8s: snapshot.config.minimum_parent_stake_e8s,
        pooled_parent_principal_e8s,
        live_cohorts,
        live_child_physical_principal_e8s,
        live_child_net_backing_e8s,
        live_child_committed_fee_liability_e8s,
        transit_backing_e8s,
        active_operation_sequence,
        last_completed_pool_operation_sequence: snapshot
            .last_completed_pool
            .as_ref()
            .map(|completed| completed.permit.operation_sequence),
        control_epoch: snapshot.control_epoch,
        fingerprint: Sha256::digest(encoded).to_vec(),
        oldest_ready_at_seconds,
    };
    result.validate().map_err(ApiError::Invalid)?;
    Ok(result)
}

pub async fn observe_pool_policy() -> Result<PoolPolicyObservation, ApiError> {
    let snapshot = state::read();
    pool_policy_observation(&snapshot).await
}

async fn require_pool_policy(snapshot: &crate::state::NnsStateV1) -> Result<(), ApiError> {
    match pool_policy_observation(snapshot).await {
        Ok(_) => Ok(()),
        Err(error) => {
            let mut latest = state::read();
            if latest == *snapshot {
                latest.lifecycle = Lifecycle::Paused;
                state::write(latest);
            }
            Err(error)
        }
    }
}

async fn pool_policy_observation(
    snapshot: &crate::state::NnsStateV1,
) -> Result<PoolPolicyObservation, ApiError> {
    let parent = match snapshot.pooled_parent_id {
        Some(parent_id) => {
            let observed = execution::query_neuron_observation(&snapshot.config, parent_id).await?;
            execution::validate_parent_configuration(
                &observed,
                FollowPolicy {
                    followee_neuron_id: snapshot.config.pooled_parent_followee_id,
                },
            )
            .map_err(ApiError::Invalid)?;
            Some(ParentPolicyObservation {
                neuron_id: parent_id,
                dissolve_delay_seconds: POOLED_PARENT_DELAY_SECONDS,
                auto_stake_maturity: observed.auto_stake_maturity,
                follow_policy: FollowPolicy {
                    followee_neuron_id: snapshot.config.pooled_parent_followee_id,
                },
                voting_power_refreshed_at_seconds: observed
                    .voting_power_refreshed_timestamp_seconds
                    .filter(|timestamp| *timestamp > 0)
                    .ok_or_else(|| {
                        ApiError::Pending("pooled parent voting power is stale".into())
                    })?,
            })
        }
        None => None,
    };
    if state::read() != *snapshot {
        return Err(ApiError::Busy);
    }
    let active_operation_sequence = active_operation_sequence(snapshot);
    let encoded = candid::encode_one((&parent, snapshot.control_epoch, active_operation_sequence))
        .map_err(|error| ApiError::Invalid(format!("policy fingerprint failed: {error}")))?;
    let result = PoolPolicyObservation {
        parent,
        control_epoch: snapshot.control_epoch,
        active_operation_sequence,
        fingerprint: Sha256::digest(encoded).to_vec(),
    };
    result.validate().map_err(ApiError::Invalid)?;
    Ok(result)
}

fn active_operation_sequence(snapshot: &crate::state::NnsStateV1) -> u64 {
    match &snapshot.active_operation {
        Some(NnsOperation::Jupiter(value)) => value.operation_sequence,
        Some(NnsOperation::Maturity(value)) => value.operation_sequence,
        Some(NnsOperation::Pool(value)) => value.permit.operation_sequence,
        Some(NnsOperation::Unwind(value)) => value.operation_sequence,
        None => 0,
    }
}

fn transit_backing(
    snapshot: &crate::state::NnsStateV1,
    observed_parent_principal_e8s: u128,
) -> Result<u128, ApiError> {
    let backing = match &snapshot.active_operation {
        Some(NnsOperation::Pool(command))
            if !matches!(command.phase, PoolCommandPhase::AwaitingTransfer) =>
        {
            io_nns_types::backing::remaining_parent_transit(
                command.permit.expected_parent_principal_e8s,
                command.permit.expected_credit_e8s,
                observed_parent_principal_e8s,
            )
            .map_err(|error| {
                ApiError::Invalid(format!("pooled top-up transit failed: {error:?}"))
            })?
        }
        Some(NnsOperation::Unwind(command))
            if matches!(
                command.phase,
                UnwindPhase::ChildIdentified
                    | UnwindPhase::SplitProved
                    | UnwindPhase::StartDissolvingSubmitted
                    | UnwindPhase::StartDissolvingProved
            ) =>
        {
            if command.phase == UnwindPhase::ChildIdentified {
                command
                    .gross_e8s
                    .checked_sub(snapshot.config.expected_icp_fee_e8s)
                    .ok_or_else(|| ApiError::Invalid("unwind transit underflow".into()))?
            } else {
                io_nns_types::backing::net_committed_child_backing(
                    command.principal_e8s,
                    snapshot.config.expected_icp_fee_e8s,
                )
                .map_err(|_| {
                    ApiError::Invalid(
                        "committed unwind transit cannot cover its future disbursement fee".into(),
                    )
                })?
            }
        }
        Some(NnsOperation::Jupiter(command))
            if matches!(
                command.phase,
                crate::jupiter::JupiterPhase::DepositProved
                    | crate::jupiter::JupiterPhase::StakeTransferPrepared { .. }
                    | crate::jupiter::JupiterPhase::StakeTransferSubmitted { .. }
                    | crate::jupiter::JupiterPhase::StakeTransferSucceeded(_)
                    | crate::jupiter::JupiterPhase::RefreshSubmitted(_)
                    | crate::jupiter::JupiterPhase::StakeIncreaseProved(_)
                    | crate::jupiter::JupiterPhase::ReceiptPermitPrepared { .. }
                    | crate::jupiter::JupiterPhase::LiquidTransferPrepared { .. }
            ) =>
        {
            command.deposit.liquid_e8s
        }
        Some(NnsOperation::Maturity(command)) => {
            let delivery = match &command.phase {
                crate::maturity::MaturityCommandPhase::ClaimReceiptDelivery(delivery) => delivery,
                _ => return Ok(0),
            };
            if delivery.claim_transfer.as_ref().is_some_and(|attempt| {
                matches!(
                    attempt.state,
                    crate::transfer::TransferState::Succeeded { .. }
                )
            }) {
                0
            } else {
                maturity_claim_transit(&delivery.pending, snapshot.config.expected_icp_fee_e8s)?
            }
        }
        _ => {
            let pending = snapshot
                .pending_two_week_maturity
                .as_ref()
                .or(snapshot.pending_two_year_maturity.as_ref());
            pending
                .map(|pending| {
                    maturity_claim_transit(pending, snapshot.config.expected_icp_fee_e8s)
                })
                .transpose()?
                .unwrap_or(0)
        }
    };
    Ok(backing)
}

fn has_ambiguous_backing_effect(snapshot: &crate::state::NnsStateV1) -> bool {
    use crate::transfer::TransferState;
    match &snapshot.active_operation {
        Some(NnsOperation::Unwind(command)) => matches!(
            command.phase,
            UnwindPhase::SplitSubmitted | UnwindPhase::DisbursementSubmitted
        ),
        Some(NnsOperation::Jupiter(command)) => match &command.phase {
            crate::jupiter::JupiterPhase::LiquidTransferSubmitted { .. } => true,
            crate::jupiter::JupiterPhase::Stuck {
                transfer: Some(crate::jupiter::JupiterStuckTransfer::Liquid { attempt, .. }),
                ..
            } => matches!(attempt.state, TransferState::Submitted { .. }),
            _ => false,
        },
        Some(NnsOperation::Maturity(command)) => match &command.phase {
            crate::maturity::MaturityCommandPhase::ClaimReceiptDelivery(delivery) => delivery
                .claim_transfer
                .as_ref()
                .is_some_and(|attempt| matches!(attempt.state, TransferState::Submitted { .. })),
            _ => false,
        },
        _ => false,
    }
}

fn maturity_claim_transit(
    pending: &crate::maturity::PendingMaturityDisbursement,
    fee_e8s: u128,
) -> Result<u128, ApiError> {
    let mint = match &pending.mint_proof {
        crate::maturity::MintProofState::Proved(mint)
        | crate::maturity::MintProofState::Delivering(mint) => mint,
        crate::maturity::MintProofState::Awaiting => return Ok(0),
    };
    match pending.kind {
        MaturityKind::TwoYear => {
            io_reward_policy::permanent_maturity_credit(mint.actual_minted_icp_e8s, fee_e8s)
                .map_err(|error| ApiError::Invalid(format!("maturity transit failed: {error:?}")))
        }
        MaturityKind::TwoWeek => {
            let claim = io_core_model::split_40_60(mint.actual_minted_icp_e8s)
                .map_err(|error| {
                    ApiError::Invalid(format!("maturity transit split failed: {error:?}"))
                })?
                .claim;
            claim
                .checked_sub(fee_e8s)
                .filter(|credit| *credit > 0)
                .ok_or_else(|| ApiError::Invalid("pooled claim transit cannot pay its fee".into()))
        }
    }
}
