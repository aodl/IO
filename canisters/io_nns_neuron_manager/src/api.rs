use candid::{CandidType, Principal};
pub use io_receipt_types::{BackingNotReadyReason, TwoWeekBackingReadiness};
use serde::Deserialize;

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
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct Status {
    pub lifecycle: Lifecycle,
    pub active_operation: Option<String>,
    pub two_year_maturity_baseline_reconciled: bool,
    pub two_week_maturity_baseline_reconciled: bool,
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
        None if snapshot.pending_unwind.is_some() => crate::unwind_flow::resume_passive(
            snapshot.pending_unwind.expect("checked passive unwind"),
        )
        .await
        .map(NnsProgress::Unwind),
        None => Ok(NnsProgress::Idle),
        Some(NnsOperation::Jupiter(operation)) => crate::jupiter_flow::resume(*operation)
            .await
            .map(NnsProgress::Jupiter),
        Some(NnsOperation::Maturity(operation)) => crate::maturity_flow::resume_active(*operation)
            .await
            .map(NnsProgress::Maturity),
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
    if !snapshot.two_week_maturity_baseline_reconciled {
        return Ok(TwoWeekBackingReadiness::NotReady(
            BackingNotReadyReason::BaselineUnreconciled,
        ));
    }
    reconcile_two_week_target(args.target_e8s).await
}

async fn reconcile_two_week_target(target_e8s: u128) -> Result<TwoWeekBackingReadiness, ApiError> {
    let snapshot = ready()?;
    let observation =
        execution::query_neuron_observation(&snapshot.config, snapshot.config.two_week_neuron_id)
            .await?;
    let maturity = observation.maturity_e8s;
    let (retained, liquid) = crate::maturity::split_maturity(maturity)
        .ok_or_else(|| ApiError::Invalid("maturity readiness split overflow".into()))?;
    let tolerance = snapshot
        .config
        .expected_icp_fee_e8s
        .checked_mul(2)
        .ok_or_else(|| ApiError::Invalid("unwind tolerance overflow".into()))?;
    let target_status =
        state::target_status(observation.snapshot.cached_stake_e8s, target_e8s, tolerance);
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
            observation.snapshot.cached_stake_e8s,
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
                NnsOperation::Unwind(_) => "Unwind".into(),
            }),
        two_year_maturity_baseline_reconciled: current.two_year_maturity_baseline_reconciled,
        two_week_maturity_baseline_reconciled: current.two_week_maturity_baseline_reconciled,
        latest_started_two_week_generation: current.latest_started_two_week_generation,
        latest_completed_two_week_generation: current.latest_completed_two_week_generation,
        latest_two_week_target: current.latest_two_week_target.clone(),
        unwinding_child_principal_e8s: current
            .pending_unwind
            .as_ref()
            .or(match &current.active_operation {
                Some(NnsOperation::Unwind(operation)) => Some(operation),
                _ => None,
            })
            .map_or(0, |operation| operation.principal_e8s),
    }
}

fn reconcile_unwind(
    state: &mut crate::state::NnsStateV1,
    target_e8s: u128,
    actual_e8s: u128,
    target_status: TwoWeekTargetStatus,
) -> Result<(), ApiError> {
    if state.pending_unwind.is_some() {
        return Ok(());
    }
    if let Some(NnsOperation::Unwind(operation)) = state.active_operation.as_mut() {
        if operation.phase == UnwindPhase::SplitPrepared {
            if target_status == TwoWeekTargetStatus::OverTarget {
                operation.target_e8s = target_e8s;
                operation.excess_e8s = actual_e8s - target_e8s;
            } else {
                state.active_operation = None;
            }
        }
        return Ok(());
    }
    if target_status != TwoWeekTargetStatus::OverTarget || state.active_operation.is_some() {
        return Ok(());
    }
    let operation_sequence = state.next_operation_sequence;
    state.next_operation_sequence = operation_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("operation sequence exhausted".into()))?;
    state.active_operation = Some(NnsOperation::Unwind(UnwindOperation {
        operation_sequence,
        target_e8s,
        excess_e8s: actual_e8s - target_e8s,
        child_neuron_id: 0,
        principal_e8s: 0,
        child_staking_subaccount: Vec::new(),
        phase: UnwindPhase::SplitPrepared,
    }));
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
        assert_eq!(operation.excess_e8s, 200_000);

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
    fn passive_child_blocks_a_second_split_for_every_target_status() {
        let mut state = crate::state::NnsStateV1::test_placeholder();
        state.latest_two_week_target = Some(TwoWeekTarget {
            target_e8s: 800_000,
            status: TwoWeekTargetStatus::OverTarget,
        });
        state.pending_unwind = Some(UnwindOperation {
            operation_sequence: 1,
            target_e8s: 900_000,
            excess_e8s: 100_000,
            child_neuron_id: 7,
            principal_e8s: 90_000,
            child_staking_subaccount: vec![7; 32],
            phase: UnwindPhase::Dissolving,
        });
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
        assert_eq!(state.pending_unwind.as_ref().unwrap().child_neuron_id, 7);
        assert_eq!(state.next_operation_sequence, 2);
    }
}
