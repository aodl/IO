use candid::{CandidType, Reserved};
use ic_cdk::call::Call;
use io_nns_types::backing::{
    PoolProgress, PoolReconciliationAction, PreparePoolReconciliationArgs, TopUpPermit,
};
use serde::Deserialize;

use crate::{
    api::ApiError,
    canonical,
    state::{self, DispatchEpoch, ReconciliationCheckpoint, StreamConfig, StreamOperation},
    transfer::{
        classify_result, ClassifiedResult, OwnTransferIntent, TransferAttempt, TransferState,
    },
};

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PoolTopUpOperation {
    pub checkpoint: ReconciliationCheckpoint,
    pub permit: TopUpPermit,
    pub transfer: TransferAttempt,
    pub nns_transfer_proved: bool,
}

impl PoolTopUpOperation {
    pub fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        self.transfer.validate()?;
        if self.nns_transfer_proved
            && !matches!(self.transfer.state, TransferState::Succeeded { .. })
        {
            return Err("NNS top-up proof precedes the Stream transfer proof".into());
        }
        match &self.transfer.intent {
            OwnTransferIntent::Icrc1 {
                ledger,
                from_subaccount,
                to,
                amount,
                fee,
                memo,
                created_at_time,
            } if *ledger == config.icp_ledger
                && *from_subaccount == config.liquid_icp.canonical()?.subaccount
                && to.effective_eq(&self.permit.destination)?
                && *amount == self.permit.expected_credit_e8s
                && *fee == self.permit.fee_e8s
                && *memo == self.permit.memo
                && *created_at_time == self.permit.prepared_at_nanos =>
            {
                Ok(())
            }
            _ => Err("pool top-up transfer differs from its exact NNS permit".into()),
        }
    }
}

pub async fn ensure_latest(now: u64) -> Result<bool, ApiError> {
    let stream = state::read();
    if matches!(stream.active_operation, Some(StreamOperation::PoolTopUp(_))) {
        return Ok(false);
    }
    if stream.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    let checkpoint = stream
        .latest_reconciliation_checkpoint
        .clone()
        .ok_or_else(|| ApiError::Pending("no canonical daily reconciliation exists".into()))?;
    let canonical = canonical::redemption_snapshot(&stream.config)
        .await
        .map_err(ApiError::Ledger)?;
    if canonical.active_unwind_generation.is_some() {
        wake_nns(stream.config.nns_manager).await?;
        return Ok(false);
    }
    let excluded = canonical
        .excluded_io_balances
        .iter()
        .try_fold(0u128, |sum, (_, value)| sum.checked_add(*value))
        .ok_or_else(|| ApiError::Invalid("nonredeemable IO overflow".into()))?;
    let claims = io_core_model::claim_supply(
        canonical.total_supply_e8s,
        canonical.reserve_io_e8s,
        &[excluded],
    )
    .map_err(|error| ApiError::Invalid(format!("claim supply failed: {error:?}")))?;
    let plan = io_core_model::reconcile(
        io_core_model::EconomicState {
            backing: io_core_model::Backing {
                liquid: canonical.liquid_icp_e8s,
                pooled: canonical.pooled_principal_e8s,
                unwinding: canonical.unwinding_principal_e8s,
                transit: canonical.transit_backing_e8s,
            },
            claims,
            active_backing: canonical.active_backing_io_e8s,
            active_reward: canonical.active_reward_io_e8s,
        },
        canonical.icp_fee_e8s,
        canonical.minimum_parent_stake_e8s,
        io_nns_types::maturity::MINIMUM_DISBURSEMENT_E8S as u128 + canonical.icp_fee_e8s,
    )
    .map_err(|error| ApiError::Invalid(format!("pool reconciliation failed: {error:?}")))?;
    match plan {
        io_core_model::ReconcilePlan::Hold { target } => {
            let result = prepare_nns(
                stream.config.nns_manager,
                PreparePoolReconciliationArgs {
                    generation: checkpoint.generation,
                    target_e8s: target,
                    action: PoolReconciliationAction::Hold,
                    fee_e8s: canonical.icp_fee_e8s,
                    snapshot_fingerprint: canonical.nns_fingerprint,
                    memo: reconciliation_memo(checkpoint.generation),
                    created_at_time_nanos: now,
                },
            )
            .await?;
            Ok(matches!(
                result,
                PoolProgress::Held { .. } | PoolProgress::UnwindCommitted { .. }
            ))
        }
        io_core_model::ReconcilePlan::Unwind { target, gross, .. } => {
            let result = prepare_nns(
                stream.config.nns_manager,
                PreparePoolReconciliationArgs {
                    generation: checkpoint.generation,
                    target_e8s: target,
                    action: PoolReconciliationAction::Unwind {
                        expected_gross_e8s: gross,
                    },
                    fee_e8s: canonical.icp_fee_e8s,
                    snapshot_fingerprint: canonical.nns_fingerprint,
                    memo: reconciliation_memo(checkpoint.generation),
                    created_at_time_nanos: now,
                },
            )
            .await?;
            match result {
                PoolProgress::UnwindPrepared { .. } => {
                    wake_nns(stream.config.nns_manager).await?;
                    Ok(false)
                }
                PoolProgress::UnwindCommitted { .. } | PoolProgress::Held { .. } => Ok(true),
                PoolProgress::CapacityPending => Ok(false),
                _ => Err(ApiError::Invalid(
                    "NNS returned a contradictory unwind reconciliation result".into(),
                )),
            }
        }
        io_core_model::ReconcilePlan::TopUp {
            target,
            credit,
            debit,
        } => {
            if debit > canonical.liquid_icp_e8s {
                return Ok(false);
            }
            let result = prepare_nns(
                stream.config.nns_manager,
                PreparePoolReconciliationArgs {
                    generation: checkpoint.generation,
                    target_e8s: target,
                    action: PoolReconciliationAction::TopUp {
                        expected_credit_e8s: credit,
                    },
                    fee_e8s: canonical.icp_fee_e8s,
                    snapshot_fingerprint: canonical.nns_fingerprint,
                    memo: reconciliation_memo(checkpoint.generation),
                    created_at_time_nanos: now,
                },
            )
            .await?;
            let PoolProgress::AwaitingTransfer(permit) = result else {
                return Err(ApiError::Busy);
            };
            let transfer = TransferAttempt::prepared(OwnTransferIntent::Icrc1 {
                ledger: stream.config.icp_ledger,
                from_subaccount: stream
                    .config
                    .liquid_icp
                    .canonical()
                    .map_err(ApiError::Invalid)?
                    .subaccount,
                to: permit.destination.clone(),
                amount: permit.expected_credit_e8s,
                fee: permit.fee_e8s,
                memo: permit.memo.clone(),
                created_at_time: permit.prepared_at_nanos,
            })
            .map_err(ApiError::Invalid)?;
            let operation = PoolTopUpOperation {
                checkpoint,
                permit,
                transfer,
                nns_transfer_proved: false,
            };
            operation
                .validate(&stream.config)
                .map_err(ApiError::Invalid)?;
            let mut latest = state::read();
            if latest != stream || latest.active_operation.is_some() {
                return Err(ApiError::Busy);
            }
            latest.active_operation = Some(StreamOperation::PoolTopUp(Box::new(operation)));
            state::write(latest);
            Ok(false)
        }
    }
}

pub async fn resume(now: u64) -> Result<(), ApiError> {
    let operation = active()?;
    match operation.transfer.state {
        TransferState::Prepared => submit(operation, now).await,
        TransferState::Submitted { .. } => Err(ApiError::Pending(
            "pool top-up transfer callback is ambiguous".into(),
        )),
        TransferState::Stuck { ref reason } => Err(ApiError::Stuck(reason.clone())),
        TransferState::Succeeded { block } if !operation.nns_transfer_proved => {
            let progress = prove_nns(state::read().config.nns_manager, block).await?;
            if matches!(progress, PoolProgress::Completed { .. }) {
                clear(operation)?;
            } else {
                mark_nns_transfer_proved(operation)?;
            }
            Ok(())
        }
        TransferState::Succeeded { .. } => {
            let progress = resume_nns(state::read().config.nns_manager).await?;
            if matches!(progress, PoolProgress::Completed { .. }) {
                clear(operation)?;
            }
            Ok(())
        }
    }
}

pub async fn prove_transfer(block_index: u128) -> Result<(), ApiError> {
    let operation = active()?;
    if let TransferState::Succeeded { block } = operation.transfer.state {
        return if block == block_index {
            Ok(())
        } else {
            Err(ApiError::Invalid("conflicting pool top-up block".into()))
        };
    }
    if !matches!(operation.transfer.state, TransferState::Submitted { .. }) {
        return Err(ApiError::Invalid(
            "pool top-up is not awaiting an exact block proof".into(),
        ));
    }
    let exact = canonical::exact_icp_transfer(state::read().config.icp_ledger, block_index)
        .await
        .map_err(ApiError::Ledger)?;
    let OwnTransferIntent::Icrc1 {
        from_subaccount,
        to,
        amount,
        fee,
        memo,
        created_at_time,
        ..
    } = &operation.transfer.intent
    else {
        return Err(ApiError::Invalid("pool top-up intent is not ICP".into()));
    };
    let source = crate::state::Account {
        owner: ic_cdk::api::canister_self(),
        subaccount: Some(from_subaccount.to_vec()),
    };
    if exact.from != canonical::icp_account_identifier(&source).map_err(ApiError::Invalid)?
        || exact.to != canonical::icp_account_identifier(to).map_err(ApiError::Invalid)?
        || exact.amount_e8s != *amount
        || exact.fee_e8s != *fee
        || exact.icrc1_memo.as_deref() != Some(memo.as_slice())
        || exact.created_at_time != *created_at_time
        || exact.spender.is_some()
    {
        return Err(ApiError::Invalid(
            "exact pool top-up block differs from the frozen intent".into(),
        ));
    }
    let mut proved = operation.clone();
    proved.transfer.state = TransferState::Succeeded { block: block_index };
    persist(&operation, proved)
}

async fn submit(mut operation: PoolTopUpOperation, now: u64) -> Result<(), ApiError> {
    operation.transfer.state = TransferState::Submitted {
        epoch: DispatchEpoch(1),
        first_submitted_at: now,
        last_submitted_at: now,
    };
    let expected = active()?;
    persist(&expected, operation.clone())?;
    match crate::api::submit(&operation.transfer.intent).await {
        Err(error) => Err(ApiError::Pending(error)),
        Ok(result) => match classify_result(result).map_err(ApiError::Ledger)? {
            ClassifiedResult::Succeeded(block) => {
                let mut succeeded = operation.clone();
                succeeded.transfer.state = TransferState::Succeeded { block };
                persist(&operation, succeeded)?;
                Ok(())
            }
            ClassifiedResult::NoEffect(reason) => {
                clear(operation)?;
                Err(ApiError::Pending(format!(
                    "pool top-up had no effect and may be prepared again: {reason}"
                )))
            }
            ClassifiedResult::Ambiguous(reason) => Err(ApiError::Pending(reason)),
        },
    }
}

async fn prepare_nns(
    nns: candid::Principal,
    args: PreparePoolReconciliationArgs,
) -> Result<PoolProgress, ApiError> {
    let result: Result<PoolProgress, Reserved> =
        Call::bounded_wait(nns, "prepare_pool_reconciliation")
            .with_arg(args)
            .await
            .map_err(|error| ApiError::Pending(format!("NNS pool prepare ambiguous: {error:?}")))?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("NNS pool prepare decode failed: {error:?}"))
            })?;
    result.map_err(|_| ApiError::Invalid("NNS rejected pool reconciliation".into()))
}

fn reconciliation_memo(generation: u64) -> Vec<u8> {
    crate::transfer::deterministic_memo(
        b"io-pool-reconciliation-v1",
        ic_cdk::api::canister_self(),
        generation,
    )
}

async fn prove_nns(nns: candid::Principal, block: u128) -> Result<PoolProgress, ApiError> {
    nns_progress_call(nns, "prove_active_transfer", block).await
}

async fn resume_nns(nns: candid::Principal) -> Result<PoolProgress, ApiError> {
    nns_progress_call(nns, "resume", ()).await
}

async fn wake_nns(nns: candid::Principal) -> Result<(), ApiError> {
    #[derive(CandidType, Deserialize)]
    enum NnsProgress {
        Jupiter(Reserved),
        Maturity(Reserved),
        Unwind(Reserved),
        Pool(Reserved),
        Idle,
    }
    let result: Result<NnsProgress, Reserved> = Call::bounded_wait(nns, "resume")
        .with_arg(())
        .await
        .map_err(|error| ApiError::Pending(format!("NNS resume ambiguous: {error:?}")))?
        .candid()
        .map_err(|error| ApiError::Invalid(format!("NNS resume decode failed: {error:?}")))?;
    match result.map_err(|_| ApiError::Invalid("NNS resume rejected".into()))? {
        NnsProgress::Jupiter(value)
        | NnsProgress::Maturity(value)
        | NnsProgress::Unwind(value)
        | NnsProgress::Pool(value) => {
            let _ = value;
        }
        NnsProgress::Idle => {}
    }
    Ok(())
}

async fn nns_progress_call<A: CandidType>(
    nns: candid::Principal,
    method: &str,
    args: A,
) -> Result<PoolProgress, ApiError> {
    #[derive(CandidType, Deserialize)]
    enum NnsProgress {
        Jupiter(Reserved),
        Maturity(Reserved),
        Unwind(Reserved),
        Pool(PoolProgress),
        Idle,
    }
    let result: Result<NnsProgress, Reserved> = Call::bounded_wait(nns, method)
        .with_arg(args)
        .await
        .map_err(|error| ApiError::Pending(format!("NNS {method} ambiguous: {error:?}")))?
        .candid()
        .map_err(|error| ApiError::Invalid(format!("NNS {method} decode failed: {error:?}")))?;
    match result.map_err(|_| ApiError::Invalid(format!("NNS {method} rejected")))? {
        NnsProgress::Pool(progress) => Ok(progress),
        NnsProgress::Jupiter(_)
        | NnsProgress::Maturity(_)
        | NnsProgress::Unwind(_)
        | NnsProgress::Idle => Err(ApiError::Busy),
    }
}

fn mark_nns_transfer_proved(operation: PoolTopUpOperation) -> Result<(), ApiError> {
    let mut updated = operation.clone();
    updated.nns_transfer_proved = true;
    persist(&operation, updated)
}

fn clear(operation: PoolTopUpOperation) -> Result<(), ApiError> {
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(StreamOperation::PoolTopUp(active)) if **active == operation)
    {
        return Err(ApiError::Busy);
    }
    latest.active_operation = None;
    state::write(latest);
    Ok(())
}

fn active() -> Result<PoolTopUpOperation, ApiError> {
    match state::read().active_operation {
        Some(StreamOperation::PoolTopUp(operation)) => Ok(*operation),
        Some(_) => Err(ApiError::Busy),
        None => Err(ApiError::Invalid("no active pool top-up".into())),
    }
}

fn persist(expected: &PoolTopUpOperation, replacement: PoolTopUpOperation) -> Result<(), ApiError> {
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(StreamOperation::PoolTopUp(active)) if **active == *expected)
    {
        return Err(ApiError::Busy);
    }
    replacement
        .validate(&latest.config)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(StreamOperation::PoolTopUp(Box::new(replacement)));
    state::write(latest);
    Ok(())
}
