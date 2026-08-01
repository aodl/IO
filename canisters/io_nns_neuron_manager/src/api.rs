use candid::{CandidType, Principal};
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
    ImplementationIncomplete(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NotifyJupiterDepositArgs {
    pub block_index: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SetTwoWeekTargetArgs {
    pub target_e8s: u128,
    pub generation: u64,
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
    ChildCreated,
    Dissolving,
    MergeBack,
    MergedBack,
    ReadyToDisburse,
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
    pub latest_target_generation: u64,
    pub has_pending_two_year_maturity: bool,
    pub has_pending_two_week_maturity: bool,
    pub has_pending_unwind: bool,
}

pub(crate) fn ready() -> Result<crate::state::NnsStateV1, ApiError> {
    let state = state::read();
    match state.lifecycle {
        Lifecycle::Ready => Ok(state),
        Lifecycle::Paused => Err(ApiError::Paused),
    }
}

pub async fn notify_jupiter_deposit(
    caller: Principal,
    args: NotifyJupiterDepositArgs,
) -> Result<JupiterProgress, ApiError> {
    crate::jupiter_flow::notify_jupiter_deposit(caller, args).await
}

pub async fn set_two_week_target(
    caller: Principal,
    args: SetTwoWeekTargetArgs,
) -> Result<TwoWeekTargetStatus, ApiError> {
    if caller != state::read().config.stream_manager {
        return Err(ApiError::Unauthorized);
    }
    let mut current = ready()?;
    let expected = current
        .latest_target_generation
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("target generation overflow".into()))?;
    if args.generation == current.latest_target_generation {
        return match &current.latest_two_week_target {
            Some(target) if target.target_e8s == args.target_e8s => Ok(target.status),
            _ => Err(ApiError::Invalid(
                "target generation conflicts with existing intent".into(),
            )),
        };
    }
    if args.generation != expected {
        return Err(ApiError::Invalid(format!(
            "expected target generation {expected}"
        )));
    }
    if matches!(current.active_operation, Some(NnsOperation::Unwind(ref operation))
        if operation.phase != UnwindPhase::Dissolving)
    {
        return Err(ApiError::Busy);
    }
    let observation =
        execution::query_neuron_observation(&current.config, current.config.two_week_neuron_id)
            .await?;
    if state::read() != current {
        return Err(ApiError::Busy);
    }
    let actual = observation
        .snapshot
        .cached_stake_e8s
        .checked_add(match &current.active_operation {
            Some(NnsOperation::Unwind(operation)) => operation.principal_e8s,
            _ => 0,
        })
        .ok_or_else(|| ApiError::Invalid("two-week principal overflow".into()))?;
    let status = state::target_status(actual, args.target_e8s);
    current.latest_two_week_target = Some(TwoWeekTarget {
        generation: args.generation,
        target_e8s: args.target_e8s,
        actual_cached_principal_e8s: actual,
        status,
    });
    current.latest_target_generation = args.generation;
    if status == TwoWeekTargetStatus::OverTarget && current.active_operation.is_none() {
        let excess_e8s = actual - args.target_e8s;
        let minimum = current
            .config
            .expected_icp_fee_e8s
            .checked_mul(2)
            .ok_or_else(|| ApiError::Invalid("unwind fee threshold overflow".into()))?;
        if excess_e8s <= minimum {
            return Err(ApiError::Invalid(
                "unwind excess cannot cover Split and Disburse fees".into(),
            ));
        }
        let operation_sequence = current.next_operation_sequence;
        current.next_operation_sequence = operation_sequence
            .checked_add(1)
            .ok_or_else(|| ApiError::Invalid("operation sequence exhausted".into()))?;
        current.active_operation = Some(NnsOperation::Unwind(UnwindOperation {
            operation_sequence,
            generation: args.generation,
            target_e8s: args.target_e8s,
            excess_e8s,
            child_neuron_id: 0,
            principal_e8s: 0,
            child_staking_subaccount: Vec::new(),
            phase: UnwindPhase::SplitPrepared,
        }));
    }
    state::write(current);
    Ok(status)
}

pub async fn resume() -> Result<NnsProgress, ApiError> {
    match state::read().active_operation {
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
        _ => crate::jupiter_flow::prove_active_transfer(block_index)
            .await
            .map(NnsProgress::Jupiter),
    }
}

pub async fn start_maturity(
    caller: Principal,
    kind: MaturityKind,
) -> Result<MaturityProgress, ApiError> {
    crate::maturity_flow::start(caller, kind).await
}

pub async fn resume_maturity(kind: MaturityKind) -> Result<MaturityProgress, ApiError> {
    crate::maturity_flow::resume_kind(kind).await
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
        latest_target_generation: current.latest_target_generation,
        has_pending_two_year_maturity: current.pending_two_year_maturity.is_some(),
        has_pending_two_week_maturity: current.pending_two_week_maturity.is_some(),
        has_pending_unwind: matches!(current.active_operation, Some(NnsOperation::Unwind(_))),
    }
}
