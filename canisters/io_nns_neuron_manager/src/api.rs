use candid::{CandidType, Principal};
use io_nns_types::backing::{
    ClaimBackingObservation, CohortObservation, CohortProofState, FollowPolicy, ParentObservation,
    PoolCommand, PoolCommandKind, PoolCommandPhase, TopUpPermit, POOLED_PARENT_DELAY_SECONDS,
};
pub use io_receipt_types::{BackingNotReadyReason, TwoWeekBackingReadiness};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    execution,
    jupiter::JupiterCompleted,
    maturity::{CompletedMaturity, MaturityKind},
    pool::{UnwindOperation, UnwindPhase},
    state::{self, Lifecycle, NnsOperation, TwoWeekTarget, TwoWeekTargetStatus},
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
pub struct ReconcileTwoWeekBackingReadinessArgs {
    pub target_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PreparePoolReconciliationArgs {
    pub generation: u64,
    pub target_e8s: u128,
    pub expected_credit_e8s: u128,
    pub fee_e8s: u128,
    pub snapshot_fingerprint: Vec<u8>,
    pub memo: Vec<u8>,
    pub created_at_time_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum PoolProgress {
    AwaitingTransfer(TopUpPermit),
    ConfiguringParent,
    AwaitingParentProof,
    Completed {
        parent_neuron_id: u64,
        principal_e8s: u128,
    },
    CapacityPending,
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
    DeliveringTwoWeekReceipt,
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
    pub latest_two_week_target: Option<TwoWeekTarget>,
    pub unwinding_child_principal_e8s: u128,
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
        None => Err(ApiError::Invalid("no active transfer proof slot".into())),
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

pub async fn reconcile_two_week_backing_readiness(
    caller: Principal,
    args: ReconcileTwoWeekBackingReadinessArgs,
) -> Result<TwoWeekBackingReadiness, ApiError> {
    let snapshot = state::read();
    if caller != snapshot.config.stream_manager {
        return Err(ApiError::Unauthorized);
    }
    if snapshot.lifecycle != Lifecycle::Ready {
        return Ok(TwoWeekBackingReadiness::NotReady(
            BackingNotReadyReason::Paused,
        ));
    }
    reconcile_two_week_target(args.target_e8s).await
}

async fn reconcile_two_week_target(target_e8s: u128) -> Result<TwoWeekBackingReadiness, ApiError> {
    let snapshot = ready()?;
    let observation = match snapshot.pooled_parent_id {
        Some(parent_id) => {
            let before = execution::query_neuron_observation(&snapshot.config, parent_id).await?;
            execution::refresh_voting_power(&snapshot.config, parent_id).await?;
            let after = execution::query_neuron_observation(&snapshot.config, parent_id).await?;
            if after.voting_power_refreshed_timestamp_seconds
                < before.voting_power_refreshed_timestamp_seconds
            {
                return Err(ApiError::Invalid(
                    "pooled parent voting-power refresh regressed".into(),
                ));
            }
            Some(after)
        }
        None => None,
    };
    if let Some(observation) = &observation {
        execution::validate_parent_configuration(
            observation,
            io_nns_types::backing::FollowPolicy {
                followee_neuron_id: snapshot.config.pooled_parent_followee_id,
            },
        )
        .map_err(ApiError::Invalid)?;
    }
    let maturity = observation.as_ref().map_or(0, |value| value.maturity_e8s);
    let (retained, liquid) = crate::maturity::split_maturity(maturity)
        .ok_or_else(|| ApiError::Invalid("maturity readiness split overflow".into()))?;
    let tolerance = snapshot
        .config
        .expected_icp_fee_e8s
        .checked_mul(2)
        .ok_or_else(|| ApiError::Invalid("unwind tolerance overflow".into()))?;
    let target_status = state::target_status(
        observation
            .as_ref()
            .map_or(0, |value| value.snapshot.cached_stake_e8s),
        target_e8s,
        tolerance,
    );
    if state::read() != snapshot {
        return Err(ApiError::Busy);
    }
    let mut latest = snapshot;
    latest.latest_two_week_target = Some(TwoWeekTarget {
        target_e8s,
        status: target_status,
    });
    if latest.pending_two_week_maturity.is_none() {
        reconcile_unwind(
            &mut latest,
            target_e8s,
            observation
                .as_ref()
                .map_or(0, |value| value.snapshot.cached_stake_e8s),
            target_status,
        )?;
    }
    let busy = latest.active_operation.is_some() || latest.pending_two_week_maturity.is_some();
    state::write(latest);
    let reason = match target_status {
        TwoWeekTargetStatus::UnderTarget => Some(BackingNotReadyReason::UnderTarget),
        TwoWeekTargetStatus::OverTarget => Some(BackingNotReadyReason::OverTarget),
        _ if busy => Some(BackingNotReadyReason::Busy),
        _ if liquid < crate::maturity::MINIMUM_DISBURSEMENT_E8S => {
            Some(BackingNotReadyReason::BelowThreshold)
        }
        _ => None,
    };
    if let Some(reason) = reason {
        return Ok(TwoWeekBackingReadiness::NotReady(reason));
    }
    Ok(TwoWeekBackingReadiness::Ready {
        target_status,
        ordinary_maturity_e8s: maturity,
        retained_maturity_e8s: retained,
        liquid_maturity_e8s: liquid,
        minimum_disbursement_e8s: crate::maturity::MINIMUM_DISBURSEMENT_E8S,
    })
}

pub async fn prove_maturity_mint(
    kind: MaturityKind,
    block_index: u128,
) -> Result<MaturityProgress, ApiError> {
    crate::maturity_flow::prove_mint(kind, block_index).await
}

pub fn get_status() -> Status {
    let current = state::read();
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
        latest_two_week_target: current.latest_two_week_target.clone(),
        unwinding_child_principal_e8s: current.live_cohorts.iter().fold(0, |total, cohort| {
            total.saturating_add(cohort.principal_e8s)
        }),
    }
}

pub async fn prepare_pool_reconciliation(
    caller: Principal,
    args: PreparePoolReconciliationArgs,
) -> Result<PoolProgress, ApiError> {
    let snapshot = ready()?;
    if caller != snapshot.config.stream_manager {
        return Err(ApiError::Unauthorized);
    }
    if snapshot.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    let observation = observe_claim_backing().await?;
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
    let expected_credit = args
        .target_e8s
        .checked_sub(observation.pooled_principal_e8s)
        .ok_or_else(|| ApiError::Invalid("pool reconciliation is not a top-up".into()))?;
    if expected_credit == 0 || expected_credit != args.expected_credit_e8s {
        return Err(ApiError::Invalid(
            "pool top-up credit does not match the target".into(),
        ));
    }
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
        expected_parent_principal_e8s: observation.pooled_principal_e8s,
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
        parent_neuron_id: snapshot.pooled_parent_id,
        phase: PoolCommandPhase::AwaitingTransfer,
    };
    operation
        .validate(latest.next_operation_sequence)
        .map_err(ApiError::Invalid)?;
    latest.latest_reconciliation_generation = args.generation;
    latest.active_operation = Some(NnsOperation::Pool(operation));
    state::write(latest);
    Ok(PoolProgress::AwaitingTransfer(permit))
}

pub async fn observe_claim_backing() -> Result<ClaimBackingObservation, ApiError> {
    let snapshot = state::read();
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
            Some(ParentObservation {
                neuron_id: parent_id,
                staking_account: execution::staking_account(&snapshot.config, &observed.snapshot),
                principal_e8s: observed.snapshot.cached_stake_e8s,
                dissolve_delay_seconds: POOLED_PARENT_DELAY_SECONDS,
                auto_stake_maturity: observed.auto_stake_maturity,
                follow_policy: FollowPolicy {
                    followee_neuron_id: snapshot.config.pooled_parent_followee_id,
                },
                voting_power_refreshed_at_seconds: observed
                    .voting_power_refreshed_timestamp_seconds
                    .ok_or_else(|| {
                        ApiError::Pending("pooled parent voting power was not refreshed".into())
                    })?,
            })
        }
        None => None,
    };
    if state::read() != snapshot {
        return Err(ApiError::Busy);
    }
    let live_cohorts = snapshot
        .live_cohorts
        .iter()
        .map(|cohort| CohortObservation {
            generation: cohort.generation,
            child_neuron_id: cohort.child_neuron_id,
            principal_e8s: if matches!(
                cohort.proof,
                CohortProofState::Dissolving | CohortProofState::DisbursementSubmitted
            ) {
                cohort.principal_e8s
            } else {
                0
            },
            ready_at_seconds: cohort.ready_at_seconds,
            proof: cohort.proof,
        })
        .collect::<Vec<_>>();
    let unwinding_principal_e8s = live_cohorts.iter().try_fold(0u128, |total, cohort| {
        total
            .checked_add(cohort.principal_e8s)
            .ok_or_else(|| ApiError::Invalid("cohort backing overflow".into()))
    })?;
    let transit_backing_e8s = transit_backing(&snapshot);
    let active_operation_sequence = match &snapshot.active_operation {
        Some(NnsOperation::Jupiter(value)) => value.operation_sequence,
        Some(NnsOperation::Maturity(value)) => value.operation_sequence,
        Some(NnsOperation::Pool(value)) => value.permit.operation_sequence,
        Some(NnsOperation::Unwind(value)) => value.operation_sequence,
        None => 0,
    };
    let active_unwind_generation = match &snapshot.active_operation {
        Some(NnsOperation::Unwind(value)) => Some(value.generation),
        _ => None,
    };
    let pooled_principal_e8s = parent.as_ref().map_or(0, |value| value.principal_e8s);
    let oldest_ready_at_seconds = live_cohorts
        .iter()
        .map(|cohort| cohort.ready_at_seconds)
        .min();
    let encoded = candid::encode_one((
        &parent,
        pooled_principal_e8s,
        &live_cohorts,
        unwinding_principal_e8s,
        transit_backing_e8s,
        active_operation_sequence,
        active_unwind_generation,
        snapshot.control_epoch,
    ))
    .map_err(|error| ApiError::Invalid(format!("observation fingerprint failed: {error}")))?;
    let result = ClaimBackingObservation {
        parent,
        pooled_principal_e8s,
        live_cohorts,
        unwinding_principal_e8s,
        transit_backing_e8s,
        active_operation_sequence,
        active_unwind_generation,
        control_epoch: snapshot.control_epoch,
        fingerprint: Sha256::digest(encoded).to_vec(),
        oldest_ready_at_seconds,
    };
    result.validate().map_err(ApiError::Invalid)?;
    Ok(result)
}

fn transit_backing(snapshot: &crate::state::NnsStateV1) -> u128 {
    match &snapshot.active_operation {
        Some(NnsOperation::Pool(command))
            if !matches!(command.phase, PoolCommandPhase::AwaitingTransfer) =>
        {
            command.permit.expected_credit_e8s
        }
        Some(NnsOperation::Unwind(command))
            if matches!(
                command.phase,
                UnwindPhase::SplitProved
                    | UnwindPhase::StartDissolvingSubmitted
                    | UnwindPhase::StartDissolvingProved
            ) =>
        {
            command.principal_e8s
        }
        Some(NnsOperation::Jupiter(command))
            if !matches!(
                command.phase,
                crate::jupiter::JupiterPhase::LiquidTransferSucceeded(_)
                    | crate::jupiter::JupiterPhase::ReceiptCompletionSubmitted(_)
                    | crate::jupiter::JupiterPhase::AwaitingStreamSettlement(_)
            ) =>
        {
            command.deposit.liquid_e8s
        }
        _ => 0,
    }
}

fn reconcile_unwind(
    state: &mut crate::state::NnsStateV1,
    target_e8s: u128,
    actual_e8s: u128,
    target_status: TwoWeekTargetStatus,
) -> Result<(), ApiError> {
    if let Some(NnsOperation::Unwind(operation)) = state.active_operation.as_mut() {
        if operation.phase == UnwindPhase::SplitPrepared {
            if target_status == TwoWeekTargetStatus::OverTarget {
                operation.target_e8s = target_e8s;
                operation.gross_e8s = actual_e8s - target_e8s;
            } else {
                state.active_operation = None;
            }
        }
        return Ok(());
    }
    if target_status != TwoWeekTargetStatus::OverTarget
        || state.active_operation.is_some()
        || state.live_cohorts.len() >= io_nns_types::backing::MAX_LIVE_UNWIND_COHORTS
    {
        return Ok(());
    }
    let generation = state
        .latest_reconciliation_generation
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("reconciliation generation exhausted".into()))?;
    let operation_sequence = state.next_operation_sequence;
    state.next_operation_sequence = operation_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("operation sequence exhausted".into()))?;
    state.active_operation = Some(NnsOperation::Unwind(UnwindOperation {
        operation_sequence,
        generation,
        target_e8s,
        gross_e8s: actual_e8s - target_e8s,
        child_neuron_id: 0,
        principal_e8s: 0,
        child_staking_subaccount: Vec::new(),
        submitted_at_seconds: 0,
        expected_block_index: None,
        child_maturity_e8s: 0,
        parent_maturity_e8s: 0,
        phase: UnwindPhase::SplitPrepared,
    }));
    state.latest_reconciliation_generation = generation;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_unsubmitted_unwind_is_replayed_retargeted_or_cancelled() {
        let mut state = crate::state::NnsStateV1::test_placeholder();
        state.latest_two_week_target = Some(TwoWeekTarget {
            target_e8s: 900_000,
            status: TwoWeekTargetStatus::OverTarget,
        });
        reconcile_unwind(
            &mut state,
            900_000,
            1_000_000,
            TwoWeekTargetStatus::OverTarget,
        )
        .unwrap();
        let first = state.active_operation.clone();
        reconcile_unwind(
            &mut state,
            900_000,
            1_000_000,
            TwoWeekTargetStatus::OverTarget,
        )
        .unwrap();
        assert_eq!(state.active_operation, first);
        assert_eq!(state.next_operation_sequence, 2);

        reconcile_unwind(
            &mut state,
            800_000,
            1_000_000,
            TwoWeekTargetStatus::OverTarget,
        )
        .unwrap();
        let Some(NnsOperation::Unwind(operation)) = &state.active_operation else {
            panic!("one unwind must remain")
        };
        assert_eq!(operation.operation_sequence, 1);
        assert_eq!(operation.gross_e8s, 200_000);

        reconcile_unwind(
            &mut state,
            1_000_000,
            1_000_000,
            TwoWeekTargetStatus::AtTarget,
        )
        .unwrap();
        assert!(state.active_operation.is_none());
    }

    #[test]
    fn live_capacity_blocks_a_second_split_for_every_target_status() {
        let mut state = crate::state::NnsStateV1::test_placeholder();
        state.latest_two_week_target = Some(TwoWeekTarget {
            target_e8s: 800_000,
            status: TwoWeekTargetStatus::OverTarget,
        });
        state.live_cohorts = (1..=io_nns_types::backing::MAX_LIVE_UNWIND_COHORTS as u64)
            .map(|generation| crate::pool::PassiveCohort {
                generation,
                child_neuron_id: generation + 100,
                principal_e8s: 90_000,
                child_staking_subaccount: vec![generation as u8; 32],
                ready_at_seconds: 1_000 + generation,
                proof: io_nns_types::backing::CohortProofState::Dissolving,
                disbursement_block: None,
            })
            .collect();
        state.next_operation_sequence = 2;
        reconcile_unwind(
            &mut state,
            800_000,
            900_000,
            TwoWeekTargetStatus::OverTarget,
        )
        .unwrap();
        assert!(state.active_operation.is_none());
        assert_eq!(state.next_operation_sequence, 2);
        reconcile_unwind(
            &mut state,
            1_000_000,
            900_000,
            TwoWeekTargetStatus::UnderTarget,
        )
        .unwrap();
        assert!(state.active_operation.is_none());
        assert_eq!(state.live_cohorts.len(), 32);
        assert_eq!(state.next_operation_sequence, 2);
    }
}
