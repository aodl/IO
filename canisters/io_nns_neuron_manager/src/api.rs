use candid::{CandidType, Principal};
use io_nns_types::backing::{
    ClaimAssetObservation, CohortObservation, CohortProofState, FollowPolicy,
    ParentAssetObservation, ParentPolicyObservation, PoolCommand, PoolCommandKind,
    PoolCommandPhase, PoolPolicyObservation, PoolReconciliationAction, TopUpPermit,
    POOLED_PARENT_DELAY_SECONDS,
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

pub const MAX_VOTING_POWER_REFRESH_AGE_SECONDS: u64 = 7 * 24 * 60 * 60;

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
    DisburseMaturitySubmitted,
    DisburseMaturitySucceeded,
    AwaitingMintProof,
    MintProved,
    DeliveringClaimReceipt,
    Completed(Box<CompletedMaturity>),
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
    match snapshot.active_operation.clone() {
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
        None => resume_passive_work(snapshot).await,
    }
}

async fn resume_passive_work(snapshot: crate::state::NnsStateV1) -> Result<NnsProgress, ApiError> {
    let now = ic_cdk::api::time() / 1_000_000_000;
    if let Some(ready) = snapshot
        .live_cohorts
        .iter()
        .filter(|cohort| cohort.ready_at_seconds <= now)
        .min_by_key(|cohort| (cohort.ready_at_seconds, cohort.generation))
        .cloned()
    {
        return crate::unwind_flow::resume_passive(ready)
            .await
            .map(NnsProgress::Unwind);
    }
    for (kind, pending) in [
        (
            MaturityKind::TwoWeek,
            snapshot.pending_two_week_maturity.as_ref(),
        ),
        (
            MaturityKind::TwoYear,
            snapshot.pending_two_year_maturity.as_ref(),
        ),
    ] {
        if pending.is_some_and(|pending| {
            !matches!(
                pending.mint_proof,
                crate::maturity::MintProofState::Awaiting
            )
        }) {
            return crate::maturity_flow::resume_kind(kind)
                .await
                .map(NnsProgress::Maturity);
        }
    }
    if snapshot.pending_two_week_maturity.is_some() || snapshot.pending_two_year_maturity.is_some()
    {
        return Ok(NnsProgress::Maturity(MaturityProgress::AwaitingMintProof));
    }
    if snapshot.live_cohorts.is_empty() {
        Ok(NnsProgress::Idle)
    } else {
        Ok(NnsProgress::Unwind(UnwindProgress::Waiting))
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
                net + cohort.principal_e8s - cohort.committed_fee_e8s,
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
    let snapshot = state::read();
    if caller != snapshot.config.stream_manager {
        return Err(ApiError::Unauthorized);
    }
    let reconciliation_request_fingerprint = reconciliation_request_fingerprint(&args)?;
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
    if let Some(completed) = snapshot
        .last_completed_pool
        .as_ref()
        .filter(|completed| completed.permit.generation == args.generation)
    {
        if !pool_permit_matches(&completed.permit, &args) {
            return Err(ApiError::Invalid(
                "completed pool replay fingerprint differs".into(),
            ));
        }
        return Ok(PoolProgress::Completed {
            parent_neuron_id: completed.parent_neuron_id,
            principal_e8s: completed.principal_e8s,
            target_status: if completed.principal_e8s == args.target_e8s {
                io_nns_types::backing::PoolTargetResult::AtTarget
            } else {
                io_nns_types::backing::PoolTargetResult::OverTarget
            },
        });
    }
    if let Some(NnsOperation::Unwind(operation)) = &snapshot.active_operation {
        if operation.generation == args.generation {
            if operation.reconciliation_request_fingerprint != reconciliation_request_fingerprint
                || operation.target_e8s != args.target_e8s
                || !matches!(
                    args.action,
                    PoolReconciliationAction::Unwind { expected_gross_e8s }
                        if expected_gross_e8s == operation.gross_e8s
                )
            {
                return Err(ApiError::Invalid(
                    "active unwind replay fingerprint differs".into(),
                ));
            }
            return Ok(PoolProgress::UnwindPrepared {
                generation: operation.generation,
                gross_e8s: operation.gross_e8s,
            });
        }
    }
    if let Some(NnsOperation::Pool(operation)) = &snapshot.active_operation {
        let stable_identity = pool_permit_matches(&operation.permit, &args);
        if operation.permit.generation == args.generation && !stable_identity {
            return Err(ApiError::Invalid(
                "active pool replay fingerprint differs".into(),
            ));
        }
        if stable_identity {
            return Ok(PoolProgress::AwaitingTransfer(operation.permit.clone()));
        }
    }
    if args.generation == snapshot.latest_reconciliation_generation
        && snapshot.active_operation.is_none()
    {
        if let Some(held) = &snapshot.last_held_reconciliation {
            if held.generation == args.generation
                && held.reconciliation_request_fingerprint == reconciliation_request_fingerprint
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
    if snapshot.lifecycle != Lifecycle::Ready {
        return Err(ApiError::Paused);
    }
    if snapshot.active_operation.is_some() {
        return Err(ApiError::Busy);
    }

    let observation = claim_asset_observation().await?;
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
    refresh_parent_for_reconciliation(&snapshot).await?;
    require_pool_policy(&snapshot).await?;
    let actual = observation.pooled_parent_principal_e8s;
    match args.action {
        PoolReconciliationAction::Hold => {
            validate_hold(&snapshot, actual, args.target_e8s, args.fee_e8s)?;
            commit_reconciliation_generation(
                &snapshot,
                args.generation,
                args.target_e8s,
                actual,
                args.snapshot_fingerprint,
                reconciliation_request_fingerprint,
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
                split_fee_e8s: 0,
                committed_disbursement_fee_e8s: 0,
                parent_principal_before_split_e8s: 0,
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
            Ok(PoolProgress::UnwindPrepared {
                generation: args.generation,
                gross_e8s: expected_gross,
            })
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

fn pool_permit_matches(permit: &TopUpPermit, args: &PreparePoolReconciliationArgs) -> bool {
    permit.generation == args.generation
        && permit.fee_e8s == args.fee_e8s
        && permit.memo == args.memo
        && permit.prepared_at_nanos == args.created_at_time_nanos
        && permit.snapshot_fingerprint == args.snapshot_fingerprint
        && permit
            .expected_parent_principal_e8s
            .checked_add(permit.expected_credit_e8s)
            == Some(args.target_e8s)
        && matches!(
            args.action,
            PoolReconciliationAction::TopUp { expected_credit_e8s }
                if expected_credit_e8s == permit.expected_credit_e8s
        )
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
    reconciliation_request_fingerprint: Vec<u8>,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    if latest != *snapshot {
        return Err(ApiError::Busy);
    }
    latest.latest_reconciliation_generation = generation;
    latest.last_held_reconciliation = Some(state::HeldReconciliation {
        generation,
        reconciliation_request_fingerprint,
        target_e8s,
        principal_e8s: actual_e8s,
        snapshot_fingerprint,
    });
    latest.latest_pooled_target = Some(PooledTarget {
        target_e8s,
        status: state::target_status(
            actual_e8s,
            target_e8s,
            hold_excess_tolerance(snapshot.config.expected_icp_fee_e8s)?,
        ),
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
        || (actual_e8s > target_e8s && delta <= hold_excess_tolerance(fee_e8s)?);
    if valid {
        Ok(())
    } else {
        Err(ApiError::Invalid(
            "hold contradicts the canonical target".into(),
        ))
    }
}

pub(crate) fn hold_excess_tolerance(fee_e8s: u128) -> Result<u128, ApiError> {
    u128::from(crate::maturity::MINIMUM_DISBURSEMENT_E8S)
        .checked_add(fee_e8s)
        .and_then(|threshold| threshold.checked_sub(1))
        .ok_or_else(|| ApiError::Invalid("minimum unwind gross overflow".into()))
}

pub async fn observe_claim_assets(caller: Principal) -> Result<ClaimAssetObservation, ApiError> {
    if caller != state::read().config.stream_manager {
        return Err(ApiError::Unauthorized);
    }
    claim_asset_observation().await
}

pub(crate) async fn claim_asset_observation() -> Result<ClaimAssetObservation, ApiError> {
    let snapshot = state::read();
    if crate::claim_assets::has_ambiguous_backing_effect(&snapshot) {
        return Err(ApiError::Pending(
            "claim backing has an ambiguous submitted monetary effect".into(),
        ));
    }
    if let Some((account, required)) =
        crate::claim_assets::insufficient_claim_asset_requirement(&snapshot)?
    {
        let balance = execution::icp_balance(&snapshot.config, &account).await?;
        if state::read() != snapshot {
            return Err(ApiError::Busy);
        }
        if balance < required {
            return Err(ApiError::Stuck(format!(
                "claim transit asset deficiency: staging balance {balance} is below immutable requirement {required}"
            )));
        }
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
            cohort.principal_e8s
        } else {
            0
        };
        let net = if physical == 0 {
            0
        } else {
            io_nns_types::backing::net_committed_child_backing(physical, cohort.committed_fee_e8s)
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
            committed_fee_e8s: cohort.committed_fee_e8s,
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
    let transit_components =
        crate::claim_assets::transit_components(&snapshot, pooled_parent_principal_e8s)?;
    let transit_backing_e8s = transit_components
        .iter()
        .try_fold(0u128, |total, component| {
            total
                .checked_add(component.backing_e8s)
                .ok_or_else(|| ApiError::Invalid("transit backing overflow".into()))
        })?;
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
        &transit_components,
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
        transit_components,
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

pub async fn observe_pool_policy(caller: Principal) -> Result<PoolPolicyObservation, ApiError> {
    if caller != state::read().config.stream_manager {
        return Err(ApiError::Unauthorized);
    }
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
            let refreshed_at = observed
                .voting_power_refreshed_timestamp_seconds
                .filter(|timestamp| *timestamp > 0)
                .ok_or_else(|| ApiError::Pending("pooled parent voting power is stale".into()))?;
            let now = ic_cdk::api::time() / 1_000_000_000;
            if !voting_power_refresh_is_current(refreshed_at, now) {
                return Err(ApiError::Pending(format!(
                    "pooled parent voting power refresh is older than {MAX_VOTING_POWER_REFRESH_AGE_SECONDS} seconds"
                )));
            }
            Some(ParentPolicyObservation {
                neuron_id: parent_id,
                dissolve_delay_seconds: POOLED_PARENT_DELAY_SECONDS,
                auto_stake_maturity: observed.auto_stake_maturity,
                follow_policy: FollowPolicy {
                    followee_neuron_id: snapshot.config.pooled_parent_followee_id,
                },
                voting_power_refreshed_at_seconds: refreshed_at,
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

fn voting_power_refresh_is_current(refreshed_at: u64, now: u64) -> bool {
    refreshed_at > 0
        && refreshed_at <= now
        && now - refreshed_at <= MAX_VOTING_POWER_REFRESH_AGE_SECONDS
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim_assets::{
        ambiguous_claim_transfer, claim_transfer_succeeded, jupiter_claim_transit,
        maturity_ingress_transit,
    };

    #[test]
    fn held_excess_and_maturity_readiness_use_the_same_unwind_threshold() {
        let tolerance = hold_excess_tolerance(10_000).unwrap();
        assert_eq!(tolerance, 100_009_999);
        assert_eq!(
            state::target_status(200_009_999, 100_000_000, tolerance),
            state::PooledTargetStatus::AtTargetWithinUnwindTolerance
        );
        assert_eq!(
            state::target_status(200_010_000, 100_000_000, tolerance),
            state::PooledTargetStatus::OverTarget
        );
    }

    #[test]
    fn passive_resume_prioritizes_actionable_work_deterministically() {
        let source = include_str!("api.rs");
        let body = source.split("async fn resume_passive_work").nth(1).unwrap();
        let ready = body.find("resume_passive(ready)").unwrap();
        let two_week = body.find("MaturityKind::TwoWeek").unwrap();
        let two_year = body.find("MaturityKind::TwoYear").unwrap();
        let awaiting = body.find("MaturityProgress::AwaitingMintProof").unwrap();
        let waiting = body.find("UnwindProgress::Waiting").unwrap();
        assert!(ready < two_week && two_week < two_year && two_year < awaiting);
        assert!(awaiting < waiting);
    }

    #[test]
    fn voting_power_refresh_age_is_conservatively_bounded() {
        let now = 10 * 24 * 60 * 60;
        assert!(voting_power_refresh_is_current(
            now - MAX_VOTING_POWER_REFRESH_AGE_SECONDS,
            now
        ));
        assert!(!voting_power_refresh_is_current(
            now - MAX_VOTING_POWER_REFRESH_AGE_SECONDS - 1,
            now
        ));
        assert!(!voting_power_refresh_is_current(0, now));
        assert!(!voting_power_refresh_is_current(now + 1, now));
    }

    #[test]
    fn paired_mint_is_quarantined_until_permit_but_permanent_mint_is_not() {
        assert_eq!(
            maturity_ingress_transit(MaturityKind::TwoWeek, 200_000, 10_000, false),
            Ok(0)
        );
        assert_eq!(
            maturity_ingress_transit(MaturityKind::TwoWeek, 200_000, 10_000, true),
            Ok(110_000)
        );
        assert_eq!(
            maturity_ingress_transit(MaturityKind::TwoYear, 200_000, 10_000, false),
            Ok(110_000)
        );
    }

    #[test]
    fn rejected_post_permit_jupiter_transfer_retains_transit() {
        let principal = candid::Principal::from_slice(&[1; 29]);
        let proof = crate::jupiter::PermanentNeuronCreditProof {
            neuron_id: 1,
            staking_subaccount: [1; 32],
            before_cached_stake_e8s: 100,
            protocol_credit_e8s: 40,
            transfer_block: 1,
            observed_after_cached_stake_e8s: 140,
        };
        let permit = crate::jupiter::StreamReceiptPermit {
            stream_operation_sequence: 1,
            destination: state::Account {
                owner: principal,
                subaccount: None,
            },
            amount_e8s: 60,
            memo: vec![1],
            request_fingerprint: vec![2; 32],
        };
        let mut attempt =
            crate::transfer::NnsTransferAttempt::prepared(crate::transfer::NnsTransferIntent {
                ledger: principal,
                source_subaccount: [0; 32],
                destination: permit.destination.clone(),
                amount_e8s: 60,
                fee_e8s: 10,
                memo: permit.memo.clone(),
                created_at_time_nanos: 1,
            })
            .unwrap();
        attempt.state = crate::transfer::TransferState::Submitted {
            epoch: 1,
            first_submitted_at_nanos: 1,
            last_submitted_at_nanos: 1,
        };
        assert!(ambiguous_claim_transfer(Some(&attempt)));
        assert!(!claim_transfer_succeeded(Some(&attempt)));
        attempt.state = crate::transfer::TransferState::Paused {
            epoch: 1,
            first_submitted_at_nanos: 1,
            last_submitted_at_nanos: 1,
            reason: "lost callback after possible effect".into(),
            classification: crate::transfer::TransferOutcomeClassification::AmbiguousPossibleEffect,
        };
        assert!(ambiguous_claim_transfer(Some(&attempt)));
        attempt.state = crate::transfer::TransferState::Paused {
            epoch: 1,
            first_submitted_at_nanos: 1,
            last_submitted_at_nanos: 1,
            reason: "controlled BadFee".into(),
            classification: crate::transfer::TransferOutcomeClassification::BadFee,
        };
        assert!(!ambiguous_claim_transfer(Some(&attempt)));
        assert!(!claim_transfer_succeeded(Some(&attempt)));
        let operation = |phase| crate::jupiter::JupiterOperation {
            operation_sequence: 1,
            dispatch_epoch: 1,
            captured_control_epoch: 1,
            deposit: crate::jupiter::JupiterDeposit {
                block_index: 1,
                gross_e8s: 120,
                stake_e8s: 40,
                liquid_e8s: 60,
                fee_e8s: 10,
                created_at_time_nanos: 1,
            },
            phase,
        };
        assert_eq!(
            jupiter_claim_transit(&operation(crate::jupiter::JupiterPhase::DepositProved)),
            None
        );
        assert_eq!(
            jupiter_claim_transit(&operation(crate::jupiter::JupiterPhase::Stuck {
                reason: "BadFee retained staging asset".into(),
                pause_reason: crate::jupiter::JupiterPauseReason::BadFee,
                transfer: Some(crate::jupiter::JupiterStuckTransfer::Liquid {
                    proof,
                    permit,
                    attempt,
                }),
            })),
            Some(60)
        );
    }

    #[test]
    fn unwind_prepare_has_no_after_persist_error_or_child_submission() {
        let source = include_str!("api.rs");
        let persist = source
            .find("latest.active_operation = Some(NnsOperation::Unwind(operation.clone()))")
            .expect("unwind preparation persistence");
        let tail = &source[persist..];
        let returned = tail
            .find("Ok(PoolProgress::UnwindPrepared")
            .expect("prepared response after persistence");
        let boundary = tail.find("fn reconciliation_request_fingerprint").unwrap();
        assert!(returned < boundary);
        assert!(!tail[..boundary].contains("unwind_flow::resume"));
        assert!(!tail[..boundary].contains(".await"));
        assert!(!tail[..boundary].contains("Err("));
    }

    #[test]
    fn claim_observation_has_one_parent_query_and_no_child_query_loop() {
        let source = include_str!("api.rs");
        let body = source
            .split("pub(crate) async fn claim_asset_observation")
            .nth(1)
            .unwrap()
            .split("pub async fn observe_pool_policy")
            .next()
            .unwrap();
        assert_eq!(body.matches("query_neuron_observation").count(), 1);
        assert!(!body.contains("cohort.child_neuron_id).await"));
        assert!(body.contains("Vec::with_capacity(snapshot.live_cohorts.len())"));
    }
}
