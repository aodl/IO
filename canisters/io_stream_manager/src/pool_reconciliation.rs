use candid::{CandidType, Reserved};
use ic_cdk::call::Call;
use io_nns_types::backing::{
    PoolProgress, PoolReconciliationAction, PreparePoolReconciliationArgs, TopUpPermit,
};
use serde::Deserialize;

use crate::{
    api::ApiError,
    canonical,
    state::{self, DispatchEpoch, StreamConfig, StreamOperation},
    transfer::{
        classify_result, ClassifiedResult, OwnTransferIntent, TransferAttempt, TransferState,
    },
};

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PoolTopUpOperation {
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
                && *created_at_time >= self.permit.prepared_at_nanos =>
            {
                Ok(())
            }
            _ => Err("pool top-up transfer differs from its exact NNS permit".into()),
        }
    }
}

pub async fn ensure_latest() -> Result<bool, ApiError> {
    let mut stream = state::read();
    if matches!(stream.active_operation, Some(StreamOperation::PoolTopUp(_))) {
        return Ok(false);
    }
    if stream.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    if stream.stake_observation_due {
        return Err(ApiError::Pending(
            "fresh daily stake observation is required".into(),
        ));
    }
    if let Some(request) = stream.prepared_exit_reconciliation.clone() {
        return resolve_prepared_exit(&stream, request).await;
    }
    let checkpoint = stream
        .latest_reconciliation_checkpoint
        .clone()
        .ok_or_else(|| ApiError::Pending("no canonical daily reconciliation exists".into()))?;
    let canonical = canonical::claim_snapshot(&stream.config)
        .await
        .map_err(ApiError::Ledger)?;
    let plan = io_core_model::reconcile(
        io_core_model::EconomicState {
            backing: io_core_model::Backing {
                liquid: canonical.liquid_icp_e8s,
                pooled: canonical.pooled_principal_e8s,
                unwinding: canonical.unwinding_net_backing_e8s,
                transit: canonical.transit_backing_e8s,
            },
            claims: canonical.claim_supply_e8s,
            active_backing: checkpoint.active_backing_io_e8s,
            active_reward: checkpoint.active_reward_io_e8s,
        },
        canonical.icp_fee_e8s,
        canonical.anchor_available_e8s,
        io_nns_types::maturity::MINIMUM_DISBURSEMENT_E8S as u128,
    )
    .map_err(|error| ApiError::Invalid(format!("pool reconciliation failed: {error:?}")))?;
    match plan {
        io_core_model::ReconcilePlan::Hold { target } => {
            let result = prepare_initial_reconciliation(
                stream.config.nns_manager,
                PreparePoolReconciliationArgs {
                    generation: checkpoint.generation,
                    target_e8s: target,
                    action: PoolReconciliationAction::Hold,
                    fee_e8s: canonical.icp_fee_e8s,
                    snapshot_fingerprint: canonical.nns_fingerprint,
                    memo: reconciliation_memo(checkpoint.generation),
                    created_at_time_nanos: checkpoint.observed_at_nanos,
                },
            )
            .await?;
            Ok(matches!(
                result,
                PoolProgress::Held { .. } | PoolProgress::UnwindCommitted { .. }
            ))
        }
        io_core_model::ReconcilePlan::Unwind { target, gross, .. } => {
            if canonical.icp_fee_e8s != stream.config.expected_icp_fee_e8s {
                pause_reconciliation(
                    &stream,
                    "canonical ICP fee differs from configured unwind fee",
                )?;
                return Err(ApiError::Invalid(
                    "canonical ICP fee differs from configured unwind fee".into(),
                ));
            }
            let request = PreparePoolReconciliationArgs {
                generation: checkpoint.generation,
                target_e8s: target,
                action: PoolReconciliationAction::Unwind {
                    expected_gross_e8s: gross,
                },
                fee_e8s: canonical.icp_fee_e8s,
                snapshot_fingerprint: canonical.nns_fingerprint,
                memo: reconciliation_memo(checkpoint.generation),
                created_at_time_nanos: checkpoint.observed_at_nanos,
            };
            prepare_exit_generation(&mut stream, request.clone())?;
            resolve_prepared_exit(&stream, request).await
        }
        io_core_model::ReconcilePlan::TopUp {
            target,
            transfer,
            claim_credit,
        } => {
            if claim_credit > canonical.liquid_icp_e8s {
                return Ok(false);
            }
            let result = prepare_initial_reconciliation(
                stream.config.nns_manager,
                PreparePoolReconciliationArgs {
                    generation: checkpoint.generation,
                    target_e8s: target,
                    action: PoolReconciliationAction::TopUp {
                        expected_transfer_e8s: transfer,
                        expected_claim_credit_e8s: claim_credit,
                    },
                    fee_e8s: canonical.icp_fee_e8s,
                    snapshot_fingerprint: canonical.nns_fingerprint,
                    memo: reconciliation_memo(checkpoint.generation),
                    created_at_time_nanos: checkpoint.observed_at_nanos,
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

fn prepare_exit_generation(
    expected: &mut state::StreamStateV1,
    request: PreparePoolReconciliationArgs,
) -> Result<(), ApiError> {
    let generation = request.generation;
    if let Some(existing) = &expected.prepared_exit_reconciliation {
        return if existing == &request {
            Ok(())
        } else {
            Err(ApiError::Busy)
        };
    }
    if expected.neuron_registry.iter().any(|record| {
        matches!(
            record.status,
            crate::state::BackingRewardStatus::ExitPrepared { generation: prepared }
                if prepared != generation
        )
    }) {
        return Err(ApiError::Busy);
    }
    let prior = expected.clone();
    crate::backing_registry::prepare_observed_exits(&mut expected.neuron_registry, generation)
        .map_err(ApiError::Invalid)?;
    expected.prepared_exit_reconciliation = Some(request);
    if state::read() != prior {
        return Err(ApiError::Busy);
    }
    state::write(expected.clone());
    Ok(())
}

fn commit_exit_generation(generation: u64) -> Result<(), ApiError> {
    let mut latest = state::read();
    if latest
        .prepared_exit_reconciliation
        .as_ref()
        .is_none_or(|request| request.generation != generation)
    {
        return Err(ApiError::Busy);
    }
    crate::backing_registry::commit_prepared_exits(&mut latest.neuron_registry, generation);
    latest.prepared_exit_reconciliation = None;
    state::write(latest);
    Ok(())
}

fn rollback_exit_generation(generation: u64) -> Result<(), ApiError> {
    let mut latest = state::read();
    if latest
        .prepared_exit_reconciliation
        .as_ref()
        .is_none_or(|request| request.generation != generation)
    {
        return Err(ApiError::Busy);
    }
    crate::backing_registry::rollback_prepared_exits(&mut latest.neuron_registry, generation);
    latest.prepared_exit_reconciliation = None;
    state::write(latest);
    Ok(())
}

async fn resolve_prepared_exit(
    stream: &state::StreamStateV1,
    request: PreparePoolReconciliationArgs,
) -> Result<bool, ApiError> {
    let generation = request.generation;
    let result = match prepare_nns(stream.config.nns_manager, request).await {
        Ok(result) => result,
        Err(ApiError::Pending(reason)) => return Err(ApiError::Pending(reason)),
        Err(error) => {
            rollback_exit_generation(generation)?;
            return Err(error);
        }
    };
    match result {
        PoolProgress::UnwindPrepared {
            generation: observed,
            ..
        } if observed == generation => {
            commit_exit_generation(generation)?;
            wake_nns(stream.config.nns_manager).await?;
            Ok(false)
        }
        PoolProgress::UnwindCommitted {
            generation: observed,
            ..
        } if observed == generation => {
            commit_exit_generation(generation)?;
            Ok(true)
        }
        PoolProgress::Held { .. } => {
            rollback_exit_generation(generation)?;
            Ok(true)
        }
        _ => {
            rollback_exit_generation(generation)?;
            Err(ApiError::Invalid(
                "NNS returned a contradictory prepared-exit result".into(),
            ))
        }
    }
}

fn pause_reconciliation(expected: &state::StreamStateV1, _reason: &str) -> Result<(), ApiError> {
    let mut latest = state::read();
    if latest != *expected {
        return Err(ApiError::Busy);
    }
    latest.reward_checkpoint.reward_processing_paused = true;
    latest.reward_checkpoint.governance_parameters_fresh = false;
    state::write(latest);
    Ok(())
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
            let progress = match prove_nns(state::read().config.nns_manager, block).await {
                Err(ApiError::Busy) => replay_nns(&operation).await?,
                result => result?,
            };
            if matches!(progress, PoolProgress::Completed { .. }) {
                clear(operation)?;
            } else {
                mark_nns_transfer_proved(operation)?;
            }
            Ok(())
        }
        TransferState::Succeeded { .. } => {
            let progress = match resume_nns(state::read().config.nns_manager).await {
                Err(ApiError::Busy) => replay_nns(&operation).await?,
                result => result?,
            };
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
    } = &operation.transfer.intent;
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
        || exact.created_at_time < operation.permit.prepared_at_nanos
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
                // A decoded ledger rejection is canonical proof that this exact
                // attempt created no block. The callback may therefore replace
                // only its deduplication timestamp and retry the same permit;
                // an ambiguous callback remains Submitted and never reaches
                // this branch. NNS still exact-matches the eventual block's
                // economics and requires its timestamp to be no earlier than
                // the original permit boundary.
                let mut retry = operation.clone();
                match &mut retry.transfer.intent {
                    OwnTransferIntent::Icrc1 {
                        created_at_time, ..
                    } => {
                        *created_at_time = now.max(operation.permit.prepared_at_nanos);
                    }
                }
                retry.transfer.state = TransferState::Prepared;
                persist(&operation, retry)?;
                Err(ApiError::Pending(format!(
                    "pool top-up had no effect and has a fresh exact retry intent: {reason}"
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
                ApiError::Pending(format!("NNS pool prepare decode ambiguous: {error:?}"))
            })?;
    result.map_err(|_| ApiError::Invalid("NNS rejected pool reconciliation".into()))
}

async fn prepare_initial_reconciliation(
    nns_manager: candid::Principal,
    args: PreparePoolReconciliationArgs,
) -> Result<PoolProgress, ApiError> {
    match prepare_nns(nns_manager, args).await {
        Err(ApiError::Invalid(reason)) => Err(ApiError::Pending(format!(
            "NNS reconciliation was concurrently prepared or rejected: {reason}"
        ))),
        result => result,
    }
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

async fn replay_nns(operation: &PoolTopUpOperation) -> Result<PoolProgress, ApiError> {
    prepare_nns(state::read().config.nns_manager, replay_request(operation)?).await
}

fn replay_request(
    operation: &PoolTopUpOperation,
) -> Result<PreparePoolReconciliationArgs, ApiError> {
    let target_e8s = operation
        .permit
        .expected_parent_principal_e8s
        .checked_add(operation.permit.claim_credit_e8s)
        .ok_or_else(|| ApiError::Invalid("pool top-up replay target overflow".into()))?;
    Ok(PreparePoolReconciliationArgs {
        generation: operation.permit.generation,
        target_e8s,
        action: PoolReconciliationAction::TopUp {
            expected_transfer_e8s: operation.permit.expected_credit_e8s,
            expected_claim_credit_e8s: operation.permit.claim_credit_e8s,
        },
        fee_e8s: operation.permit.fee_e8s,
        snapshot_fingerprint: operation.permit.snapshot_fingerprint.clone(),
        memo: operation.permit.memo.clone(),
        created_at_time_nanos: operation.permit.prepared_at_nanos,
    })
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
        Pool(Box<PoolProgress>),
        Idle,
    }
    let result: Result<NnsProgress, Reserved> = Call::bounded_wait(nns, method)
        .with_arg(args)
        .await
        .map_err(|error| ApiError::Pending(format!("NNS {method} ambiguous: {error:?}")))?
        .candid()
        .map_err(|error| ApiError::Invalid(format!("NNS {method} decode failed: {error:?}")))?;
    match result.map_err(|_| ApiError::Invalid(format!("NNS {method} rejected")))? {
        NnsProgress::Pool(progress) => Ok(*progress),
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

#[cfg(test)]
mod tests {
    use candid::Principal;
    use io_nns_types::backing::{PoolReconciliationAction, TopUpPermit};

    use crate::transfer::{OwnTransferIntent, TransferAttempt, TransferState};

    use super::{replay_request, PoolTopUpOperation};

    #[test]
    fn malformed_and_transport_prepare_replies_are_ambiguous_not_rollback_authority() {
        let source = include_str!("pool_reconciliation.rs");
        assert!(source.contains("NNS pool prepare ambiguous"));
        assert!(source.contains("NNS pool prepare decode ambiguous"));
        let resolver = source
            .split("async fn resolve_prepared_exit")
            .nth(1)
            .expect("prepared-exit resolver");
        let pending = resolver
            .find("Err(ApiError::Pending(reason)) => return Err(ApiError::Pending(reason))")
            .expect("ambiguous reply retention");
        let rollback = resolver
            .find("rollback_exit_generation(generation)?")
            .expect("decoded no-effect rollback");
        assert!(pending < rollback);
    }

    #[test]
    fn completed_keeper_replay_reconstructs_the_exact_frozen_top_up() {
        let destination = io_accounts::Account {
            owner: Principal::from_slice(&[2; 29]),
            subaccount: Some(vec![3; 32]),
        };
        let permit = TopUpPermit {
            generation: 7,
            operation_sequence: 9,
            expected_parent_principal_e8s: 400_000_000,
            expected_parent_physical_e8s: 1_400_000_000,
            destination: destination.clone(),
            expected_credit_e8s: 99_990_000,
            claim_credit_e8s: 100_000_000,
            fee_e8s: 10_000,
            memo: b"exact-pool-replay".to_vec(),
            prepared_at_nanos: 123_000_000_000,
            snapshot_fingerprint: vec![4; 32],
        };
        let operation = PoolTopUpOperation {
            permit: permit.clone(),
            transfer: TransferAttempt {
                intent: OwnTransferIntent::Icrc1 {
                    ledger: Principal::from_slice(&[1; 29]),
                    from_subaccount: [5; 32],
                    to: destination,
                    amount: permit.expected_credit_e8s,
                    fee: permit.fee_e8s,
                    memo: permit.memo.clone(),
                    created_at_time: permit.prepared_at_nanos,
                },
                state: TransferState::Succeeded { block: 11 },
            },
            nns_transfer_proved: true,
        };

        let request = replay_request(&operation).expect("exact replay request");
        assert_eq!(request.generation, permit.generation);
        assert_eq!(request.target_e8s, 500_000_000);
        assert_eq!(
            request.action,
            PoolReconciliationAction::TopUp {
                expected_transfer_e8s: permit.expected_credit_e8s,
                expected_claim_credit_e8s: permit.claim_credit_e8s,
            }
        );
        assert_eq!(request.fee_e8s, permit.fee_e8s);
        assert_eq!(request.snapshot_fingerprint, permit.snapshot_fingerprint);
        assert_eq!(request.memo, permit.memo);
        assert_eq!(request.created_at_time_nanos, permit.prepared_at_nanos);
    }
}
