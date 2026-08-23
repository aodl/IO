use io_ledger_boundary::{exact_icp_transfer, icp_account_identifier};
use io_nns_types::backing::CohortProofState;

use crate::{
    api::{ApiError, UnwindProgress},
    execution::{self, DissolveState},
    pool::{PassiveCohort, UnwindOperation, UnwindPhase},
    state::{self, Lifecycle, NnsOperation},
};

pub async fn resume(operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    match operation.phase.clone() {
        UnwindPhase::SplitPrepared => submit_split(operation).await,
        UnwindPhase::SplitSubmitted => pause(operation, "Split callback is unresolved".into()),
        UnwindPhase::ChildIdentified => prove_split(operation).await,
        UnwindPhase::SplitProved => submit_start(operation).await,
        UnwindPhase::StartDissolvingSubmitted => recover_start(operation).await,
        UnwindPhase::StartDissolvingProved => prove_start(operation).await,
        UnwindPhase::DisbursementPrepared => submit_disbursement(operation).await,
        UnwindPhase::DisbursementSubmitted => Ok(UnwindProgress::AwaitingTransferProof),
        UnwindPhase::PrincipalReturned => observe_cleanup(operation).await,
        UnwindPhase::DelayIncreaseSubmitted => recover_delay(operation).await,
        UnwindPhase::DelayIncreaseProved | UnwindPhase::MergePrepared => {
            submit_merge(operation).await
        }
        UnwindPhase::MergeSubmitted => recover_merge(operation).await,
        UnwindPhase::MergeProved => prove_cleanup(operation).await,
        UnwindPhase::CleanupProved => retire(operation),
        UnwindPhase::Stuck(reason) => Ok(UnwindProgress::Stuck(reason)),
    }
}

pub async fn resume_passive(cohort: PassiveCohort) -> Result<UnwindProgress, ApiError> {
    let now = ic_cdk::api::time() / 1_000_000_000;
    if cohort.proof == CohortProofState::Dissolving && now < cohort.ready_at_seconds {
        return Ok(UnwindProgress::Waiting);
    }
    let phase = match cohort.proof {
        CohortProofState::Dissolving => UnwindPhase::DisbursementPrepared,
        CohortProofState::PrincipalReturned => UnwindPhase::PrincipalReturned,
        CohortProofState::MaturityHandled => UnwindPhase::MergeProved,
        CohortProofState::CleanupComplete => UnwindPhase::CleanupProved,
        CohortProofState::DisbursementSubmitted => return Err(ApiError::Busy),
    };
    promote(cohort, phase)?;
    let Some(NnsOperation::Unwind(operation)) = state::read().active_operation else {
        return Err(ApiError::Busy);
    };
    resume(operation).await
}

async fn submit_split(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    operation.phase = UnwindPhase::SplitSubmitted;
    replace(&expected, operation.clone())?;
    let current = state::read();
    let parent = current
        .pooled_parent_id
        .ok_or_else(|| ApiError::Invalid("pooled parent is absent".into()))?;
    match execution::split_neuron(
        &current.config,
        parent,
        operation.gross_e8s,
        operation.generation,
    )
    .await
    {
        Ok(child) => {
            ensure(&operation)?;
            let submitted = operation.clone();
            operation.child_neuron_id = child;
            operation.phase = UnwindPhase::ChildIdentified;
            replace(&submitted, operation)?;
            Ok(UnwindProgress::Waiting)
        }
        Err(error) => pause(
            operation,
            format!("Split outcome requires proof: {error:?}"),
        ),
    }
}

async fn prove_split(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    let current = state::read();
    let observed =
        execution::query_neuron_observation(&current.config, operation.child_neuron_id).await?;
    ensure(&expected)?;
    let principal = operation
        .gross_e8s
        .checked_sub(current.config.expected_icp_fee_e8s)
        .ok_or_else(|| ApiError::Invalid("Split gross cannot cover the fee".into()))?;
    if observed.snapshot.cached_stake_e8s != principal {
        return pause(operation, "Split principal proof mismatch".into());
    }
    operation.principal_e8s = principal;
    operation.child_staking_subaccount = observed.snapshot.staking_subaccount.to_vec();
    operation.phase = UnwindPhase::SplitProved;
    replace(&expected, operation)?;
    Ok(UnwindProgress::Waiting)
}

async fn submit_start(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    operation.phase = UnwindPhase::StartDissolvingSubmitted;
    replace(&expected, operation.clone())?;
    let result =
        execution::set_dissolving(&state::read().config, operation.child_neuron_id, true).await;
    ensure(&operation)?;
    if let Err(error) = result {
        return pause(
            operation,
            format!("StartDissolving outcome requires proof: {error:?}"),
        );
    }
    let submitted = operation.clone();
    operation.phase = UnwindPhase::StartDissolvingProved;
    replace(&submitted, operation)?;
    Ok(UnwindProgress::Waiting)
}

async fn recover_start(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    let observed =
        execution::query_neuron_observation(&state::read().config, operation.child_neuron_id)
            .await?;
    ensure(&expected)?;
    if !matches!(
        observed.dissolve_state,
        Some(DissolveState::WhenDissolvedTimestampSeconds(_))
    ) {
        return pause(operation, "StartDissolving remains unproved".into());
    }
    operation.phase = UnwindPhase::StartDissolvingProved;
    replace(&expected, operation)?;
    Ok(UnwindProgress::Waiting)
}

async fn prove_start(operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let current = state::read();
    let observed =
        execution::query_neuron_observation(&current.config, operation.child_neuron_id).await?;
    ensure(&operation)?;
    let Some(DissolveState::WhenDissolvedTimestampSeconds(ready_at_seconds)) =
        observed.dissolve_state
    else {
        return pause(operation, "child is not canonically dissolving".into());
    };
    move_to_passive(
        &operation,
        PassiveCohort {
            generation: operation.generation,
            reconciliation_request_fingerprint: operation
                .reconciliation_request_fingerprint
                .clone(),
            child_neuron_id: operation.child_neuron_id,
            principal_e8s: operation.principal_e8s,
            child_staking_subaccount: operation.child_staking_subaccount.clone(),
            ready_at_seconds,
            proof: CohortProofState::Dissolving,
            disbursement_block: None,
        },
    )?;
    Ok(UnwindProgress::Waiting)
}

async fn submit_disbursement(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    operation.phase = UnwindPhase::DisbursementSubmitted;
    operation.submitted_at_seconds = ic_cdk::api::time() / 1_000_000_000;
    replace(&expected, operation.clone())?;
    let current = state::read();
    let result = execution::disburse_neuron(
        &current.config,
        operation.child_neuron_id,
        &current.config.stream_liquid_account,
    )
    .await;
    ensure(&operation)?;
    if let Ok(block) = result {
        let submitted = operation.clone();
        operation.expected_block_index = Some(block);
        replace(&submitted, operation)?;
    }
    Ok(UnwindProgress::AwaitingTransferProof)
}

pub async fn prove(
    mut operation: UnwindOperation,
    block_index: u128,
) -> Result<UnwindProgress, ApiError> {
    if operation.phase != UnwindPhase::DisbursementSubmitted
        || operation
            .expected_block_index
            .is_some_and(|expected| expected != block_index)
    {
        return Err(ApiError::Invalid(
            "unwind is not awaiting this block".into(),
        ));
    }
    let current = state::read();
    let exact = exact_icp_transfer(current.config.icp_ledger, block_index)
        .await
        .map_err(ApiError::Invalid)?;
    ensure(&operation)?;
    let from = icp_account_identifier(&state::Account {
        owner: current.config.nns_governance,
        subaccount: Some(operation.child_staking_subaccount.clone()),
    })
    .map_err(ApiError::Invalid)?;
    let to =
        icp_account_identifier(&current.config.stream_liquid_account).map_err(ApiError::Invalid)?;
    let amount = operation
        .principal_e8s
        .checked_sub(current.config.expected_icp_fee_e8s)
        .ok_or_else(|| ApiError::Invalid("child principal cannot cover fee".into()))?;
    if exact.from != from
        || exact.to != to
        || exact.amount_e8s != amount
        || exact.fee_e8s != current.config.expected_icp_fee_e8s
        || exact.native_memo_u64 < operation.submitted_at_seconds
    {
        return Err(ApiError::Invalid(
            "exact child disbursement mismatch".into(),
        ));
    }
    update_cohort(
        operation.generation,
        CohortProofState::PrincipalReturned,
        Some(block_index),
    )?;
    let expected = operation.clone();
    operation.phase = UnwindPhase::PrincipalReturned;
    replace(&expected, operation)?;
    Ok(UnwindProgress::Completed {
        block_index,
        liquid_e8s: amount,
    })
}

async fn observe_cleanup(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    let current = state::read();
    let child =
        execution::query_neuron_observation(&current.config, operation.child_neuron_id).await?;
    ensure(&expected)?;
    if child.snapshot.cached_stake_e8s != 0 {
        return Err(ApiError::Pending(
            "child principal has not reached zero".into(),
        ));
    }
    let maturity = u128::from(child.maturity_e8s)
        .checked_add(u128::from(child.staked_maturity_e8s))
        .ok_or_else(|| ApiError::Invalid("child maturity overflow".into()))?;
    if maturity == 0 {
        update_cohort(
            operation.generation,
            CohortProofState::CleanupComplete,
            None,
        )?;
        operation.phase = UnwindPhase::CleanupProved;
        replace(&expected, operation.clone())?;
        return retire(operation);
    }
    let parent = current
        .pooled_parent_id
        .ok_or_else(|| ApiError::Invalid("maturity cleanup requires the parent".into()))?;
    let parent = execution::query_neuron_observation(&current.config, parent).await?;
    ensure(&expected)?;
    operation.child_maturity_e8s = maturity;
    operation.parent_maturity_e8s = u128::from(parent.maturity_e8s)
        .checked_add(u128::from(parent.staked_maturity_e8s))
        .ok_or_else(|| ApiError::Invalid("parent maturity overflow".into()))?;
    operation.parent_principal_e8s = parent.snapshot.cached_stake_e8s;
    operation.phase = UnwindPhase::DelayIncreaseSubmitted;
    replace(&expected, operation.clone())?;
    execution::increase_delay(&current.config, operation.child_neuron_id, 1).await?;
    ensure(&operation)?;
    let submitted = operation.clone();
    operation.phase = UnwindPhase::DelayIncreaseProved;
    replace(&submitted, operation)?;
    Ok(UnwindProgress::Waiting)
}

async fn recover_delay(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    let observed =
        execution::query_neuron_observation(&state::read().config, operation.child_neuron_id)
            .await?;
    ensure(&expected)?;
    if observed.dissolve_state != Some(DissolveState::DissolveDelaySeconds(1)) {
        return pause(operation, "child delay increase remains unproved".into());
    }
    operation.phase = UnwindPhase::DelayIncreaseProved;
    replace(&expected, operation)?;
    Ok(UnwindProgress::Waiting)
}

async fn submit_merge(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    operation.phase = UnwindPhase::MergeSubmitted;
    replace(&expected, operation.clone())?;
    let current = state::read();
    let parent = current
        .pooled_parent_id
        .ok_or_else(|| ApiError::Invalid("pooled parent is absent".into()))?;
    execution::merge_neuron(&current.config, parent, operation.child_neuron_id).await?;
    ensure(&operation)?;
    let submitted = operation.clone();
    operation.phase = UnwindPhase::MergeProved;
    replace(&submitted, operation)?;
    Ok(UnwindProgress::Waiting)
}

async fn recover_merge(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    let child =
        execution::query_neuron_observation(&state::read().config, operation.child_neuron_id)
            .await?;
    ensure(&expected)?;
    if child.maturity_e8s != 0 || child.staked_maturity_e8s != 0 {
        return pause(operation, "child maturity merge remains unproved".into());
    }
    operation.phase = UnwindPhase::MergeProved;
    replace(&expected, operation)?;
    Ok(UnwindProgress::Waiting)
}

async fn prove_cleanup(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    let current = state::read();
    let child =
        execution::query_neuron_observation(&current.config, operation.child_neuron_id).await?;
    let parent = execution::query_neuron_observation(
        &current.config,
        current
            .pooled_parent_id
            .ok_or_else(|| ApiError::Invalid("parent absent".into()))?,
    )
    .await?;
    ensure(&expected)?;
    let parent_maturity = u128::from(parent.maturity_e8s)
        .checked_add(u128::from(parent.staked_maturity_e8s))
        .ok_or_else(|| ApiError::Invalid("parent maturity overflow".into()))?;
    let expected_maturity = operation
        .parent_maturity_e8s
        .checked_add(operation.child_maturity_e8s)
        .ok_or_else(|| ApiError::Invalid("cleanup maturity overflow".into()))?;
    if child.snapshot.cached_stake_e8s != 0
        || child.maturity_e8s != 0
        || child.staked_maturity_e8s != 0
        || parent_maturity != expected_maturity
        || parent.snapshot.cached_stake_e8s != operation.parent_principal_e8s
    {
        return pause(operation, "child cleanup conservation proof failed".into());
    }
    update_cohort(
        operation.generation,
        CohortProofState::CleanupComplete,
        None,
    )?;
    operation.phase = UnwindPhase::CleanupProved;
    replace(&expected, operation.clone())?;
    retire(operation)
}

fn retire(operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let mut latest = state::read();
    clear_active(&mut latest, &operation)?;
    let index = latest
        .live_cohorts
        .iter()
        .position(|cohort| cohort.generation == operation.generation)
        .ok_or(ApiError::Busy)?;
    if latest.live_cohorts[index].proof != CohortProofState::CleanupComplete {
        return Err(ApiError::Busy);
    }
    latest.last_completed_unwind = Some(state::CompletedUnwindReconciliation {
        generation: operation.generation,
        reconciliation_request_fingerprint: operation.reconciliation_request_fingerprint.clone(),
        physical_principal_e8s: operation.principal_e8s,
    });
    latest.live_cohorts.remove(index);
    state::write(latest);
    Ok(UnwindProgress::Waiting)
}

fn ensure(expected: &UnwindOperation) -> Result<(), ApiError> {
    matches!(state::read().active_operation, Some(NnsOperation::Unwind(active)) if active == *expected)
        .then_some(())
        .ok_or(ApiError::Busy)
}

fn replace(expected: &UnwindOperation, replacement: UnwindOperation) -> Result<(), ApiError> {
    let mut latest = state::read();
    clear_active(&mut latest, expected)?;
    replacement
        .validate(latest.next_operation_sequence)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(NnsOperation::Unwind(replacement));
    state::write(latest);
    Ok(())
}

fn move_to_passive(expected: &UnwindOperation, cohort: PassiveCohort) -> Result<(), ApiError> {
    let mut latest = state::read();
    clear_active(&mut latest, expected)?;
    if latest.live_cohorts.len() >= io_nns_types::backing::MAX_LIVE_UNWIND_COHORTS
        || latest.live_cohorts.iter().any(|item| {
            item.generation == cohort.generation || item.child_neuron_id == cohort.child_neuron_id
        })
    {
        return Err(ApiError::Busy);
    }
    latest.live_cohorts.push(cohort);
    latest.live_cohorts.sort_by_key(|item| item.generation);
    state::write(latest);
    Ok(())
}

fn promote(cohort: PassiveCohort, phase: UnwindPhase) -> Result<(), ApiError> {
    let mut latest = state::read();
    if latest.active_operation.is_some() || !latest.live_cohorts.iter().any(|item| item == &cohort)
    {
        return Err(ApiError::Busy);
    }
    latest.active_operation = Some(NnsOperation::Unwind(UnwindOperation {
        operation_sequence: latest.next_operation_sequence,
        generation: cohort.generation,
        reconciliation_request_fingerprint: cohort.reconciliation_request_fingerprint,
        target_e8s: 0,
        gross_e8s: cohort.principal_e8s,
        child_neuron_id: cohort.child_neuron_id,
        principal_e8s: cohort.principal_e8s,
        child_staking_subaccount: cohort.child_staking_subaccount,
        submitted_at_seconds: 0,
        expected_block_index: cohort.disbursement_block,
        child_maturity_e8s: 0,
        parent_maturity_e8s: 0,
        parent_principal_e8s: 0,
        phase,
    }));
    latest.next_operation_sequence = latest
        .next_operation_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("operation sequence overflow".into()))?;
    state::write(latest);
    Ok(())
}

fn update_cohort(
    generation: u64,
    proof: CohortProofState,
    block: Option<u128>,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    let cohort = latest
        .live_cohorts
        .iter_mut()
        .find(|cohort| cohort.generation == generation)
        .ok_or(ApiError::Busy)?;
    cohort.proof = proof;
    if block.is_some() {
        cohort.disbursement_block = block;
    }
    state::write(latest);
    Ok(())
}

fn pause(mut operation: UnwindOperation, reason: String) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    operation.phase = UnwindPhase::Stuck(reason.clone());
    replace(&expected, operation)?;
    let mut latest = state::read();
    latest.lifecycle = Lifecycle::Paused;
    state::write(latest);
    Ok(UnwindProgress::Stuck(reason))
}

fn clear_active(state: &mut state::NnsStateV1, expected: &UnwindOperation) -> Result<(), ApiError> {
    if !matches!(&state.active_operation, Some(NnsOperation::Unwind(active)) if active == expected)
    {
        return Err(ApiError::Busy);
    }
    state.active_operation = None;
    Ok(())
}
