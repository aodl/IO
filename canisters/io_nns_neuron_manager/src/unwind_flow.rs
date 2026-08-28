use candid::{CandidType, Principal};
use ic_cdk::call::Call;
use io_ledger_boundary::{exact_icp_transfer, icp_account_identifier};
use io_nns_types::backing::CohortProofState;
use serde::Deserialize;

use crate::{
    api::{ApiError, UnwindProgress},
    execution::{self, DissolveState},
    pool::{PassiveCohort, UnwindOperation, UnwindPhase},
    state::{self, Lifecycle, NnsOperation},
};

pub async fn resume(operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    match operation.phase.clone() {
        UnwindPhase::SplitPrepared => submit_split(operation).await,
        UnwindPhase::SplitSubmitted => recover_split(operation).await,
        UnwindPhase::ChildIdentified => prove_split(operation).await,
        UnwindPhase::SplitProved => submit_start(operation).await,
        UnwindPhase::StartDissolvingSubmitted => recover_start(operation, true).await,
        UnwindPhase::StartDissolvingProved => prove_start(operation).await,
        UnwindPhase::DisbursementPrepared => submit_disbursement(operation).await,
        UnwindPhase::DisbursementSubmitted => Ok(UnwindProgress::AwaitingTransferProof),
        UnwindPhase::PrincipalReturned => observe_cleanup(operation).await,
        UnwindPhase::DelayIncreaseSubmitted => recover_delay(operation, true).await,
        UnwindPhase::DelayIncreaseProved | UnwindPhase::MergePrepared => {
            submit_merge(operation).await
        }
        UnwindPhase::MergeSubmitted => recover_merge(operation, true).await,
        UnwindPhase::MergeProved => prove_cleanup(operation).await,
        UnwindPhase::CleanupProved => retire(operation),
        UnwindPhase::Stuck(reason) => Ok(UnwindProgress::Stuck(reason)),
    }
}

pub async fn resume_passive(cohort: PassiveCohort) -> Result<UnwindProgress, ApiError> {
    let now = ic_cdk::api::time() / 1_000_000_000;
    if cohort.proof == CohortProofState::Dissolving && now < cohort.ready_at_seconds {
        return Ok(UnwindProgress::Pending);
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
    let current = state::read();
    let governance_fee = execution::nns_transaction_fee(&current.config).await?;
    ensure(&expected)?;
    let ledger_fee = io_ledger_boundary::icp_fee(current.config.icp_ledger)
        .await
        .map_err(ApiError::Pending)?;
    ensure(&expected)?;
    let parent = current
        .pooled_parent_id
        .ok_or_else(|| ApiError::Invalid("pooled parent is absent".into()))?;
    let parent_before = execution::query_neuron(&current.config, parent).await?;
    ensure(&expected)?;
    if governance_fee != current.config.expected_icp_fee_e8s
        || ledger_fee != current.config.expected_icp_fee_e8s
        || governance_fee != ledger_fee
    {
        let mut latest = state::read();
        if !matches!(latest.active_operation, Some(NnsOperation::Unwind(ref active)) if active == &expected)
        {
            return Err(ApiError::Busy);
        }
        latest.lifecycle = Lifecycle::Paused;
        state::write(latest);
        return Err(ApiError::Stuck(format!(
            "Split fee parameter drift: approved {}, Governance {governance_fee}, Ledger {ledger_fee}; Split was not submitted",
            current.config.expected_icp_fee_e8s
        )));
    }
    operation.split_fee_e8s = governance_fee;
    operation.committed_disbursement_fee_e8s = ledger_fee;
    operation.parent_principal_before_split_e8s = parent_before.cached_stake_e8s;
    operation.phase = UnwindPhase::SplitSubmitted;
    replace(&expected, operation.clone())?;
    match execution::split_neuron(
        &current.config,
        parent,
        operation.gross_e8s,
        operation.generation,
    )
    .await?
    {
        execution::SplitCallOutcome::Created(child) => {
            ensure(&operation)?;
            let submitted = operation.clone();
            operation.child_neuron_id = child;
            operation.principal_e8s = io_nns_types::backing::expected_split_child_principal(
                operation.gross_e8s,
                operation.split_fee_e8s,
            )
            .map_err(|_| ApiError::Invalid("Split gross cannot cover the fee".into()))?;
            operation.phase = UnwindPhase::ChildIdentified;
            replace(&submitted, operation.clone())?;
            prove_split(operation).await
        }
        execution::SplitCallOutcome::RejectedNoEffect(reason) => {
            ensure(&operation)?;
            let submitted = operation.clone();
            operation.phase = UnwindPhase::SplitPrepared;
            operation.split_fee_e8s = 0;
            operation.committed_disbursement_fee_e8s = 0;
            operation.parent_principal_before_split_e8s = 0;
            replace(&submitted, operation)?;
            Err(ApiError::Pending(format!(
                "{reason}; exact Split intent may be retried"
            )))
        }
        execution::SplitCallOutcome::Ambiguous(reason) => {
            ensure(&operation)?;
            Err(ApiError::Pending(reason))
        }
    }
}

async fn recover_split(operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let snapshot = state::read();
    let parent_id = snapshot
        .pooled_parent_id
        .ok_or_else(|| ApiError::Invalid("pooled parent is absent".into()))?;
    let expected_principal = io_nns_types::backing::expected_split_child_principal(
        operation.gross_e8s,
        operation.split_fee_e8s,
    )
    .map_err(|_| ApiError::Invalid("Split gross cannot cover the frozen fee".into()))?;
    let controller = ic_cdk::api::canister_self();
    let expected_child_subaccount =
        execution::split_child_subaccount(controller, operation.generation);
    let candidate =
        match split_child_by_subaccount(&snapshot.config, expected_child_subaccount).await {
            Ok(candidate) => candidate,
            Err(ApiError::Invalid(reason)) => return fail_split_recovery(&operation, reason),
            Err(error) => return Err(error),
        };
    ensure(&operation)?;
    let existing = |child_id| {
        snapshot
            .live_cohorts
            .iter()
            .any(|cohort| cohort.child_neuron_id == child_id)
            || snapshot
                .last_completed_unwind
                .as_ref()
                .is_some_and(|completed| completed.child_neuron_id == child_id)
    };
    let Some(candidate) = candidate else {
        return Err(ApiError::Pending(
            "ambiguous Split has no canonical child candidate yet".into(),
        ));
    };
    if candidate.neuron_id == snapshot.config.two_year_neuron_id
        || candidate.neuron_id == parent_id
        || existing(candidate.neuron_id)
        || candidate.controller != controller
        || candidate.staking_subaccount != expected_child_subaccount
        || candidate.physical_principal_e8s != expected_principal
        || candidate.dissolve_state
            != Some(DissolveState::DissolveDelaySeconds(
                io_nns_types::backing::POOLED_PARENT_DELAY_SECONDS,
            ))
    {
        return fail_split_recovery(
            &operation,
            "exact Split subaccount returned conflicting child evidence".into(),
        );
    }
    let parent = execution::query_neuron(&snapshot.config, parent_id).await?;
    ensure(&operation)?;
    let expected_parent = operation
        .parent_principal_before_split_e8s
        .checked_sub(operation.gross_e8s)
        .ok_or_else(|| ApiError::Invalid("frozen parent Split principal underflow".into()))?;
    if parent.cached_stake_e8s != expected_parent {
        return Err(ApiError::Pending(
            "ambiguous Split parent reduction is not canonically reflected".into(),
        ));
    }
    let expected = operation.clone();
    let mut identified = operation;
    identified.child_neuron_id = candidate.neuron_id;
    identified.principal_e8s = expected_principal;
    identified.phase = UnwindPhase::ChildIdentified;
    replace(&expected, identified.clone())?;
    prove_split(identified).await
}

#[derive(CandidType, Deserialize)]
struct SplitListRequest {
    neuron_ids: Vec<u64>,
    include_neurons_readable_by_caller: bool,
    include_empty_neurons_readable_by_caller: Option<bool>,
    include_public_neurons_in_full_neurons: Option<bool>,
    page_number: Option<u64>,
    page_size: Option<u64>,
    neuron_subaccounts: Option<Vec<SplitSubaccount>>,
}

#[derive(CandidType, Deserialize)]
struct SplitSubaccount {
    subaccount: Vec<u8>,
}

#[derive(CandidType, Deserialize)]
struct SplitListResponse {
    full_neurons: Vec<SplitNeuron>,
    total_pages_available: Option<u64>,
}

#[derive(CandidType, Deserialize)]
struct SplitNeuron {
    id: Option<SplitNeuronId>,
    controller: Option<Principal>,
    account: Vec<u8>,
    cached_neuron_stake_e8s: u64,
    dissolve_state: Option<DissolveState>,
}

#[derive(CandidType, Deserialize)]
struct SplitNeuronId {
    id: u64,
}

struct SplitCandidate {
    neuron_id: u64,
    controller: Principal,
    physical_principal_e8s: u128,
    staking_subaccount: Vec<u8>,
    dissolve_state: Option<DissolveState>,
}

async fn split_child_by_subaccount(
    config: &state::NnsConfig,
    subaccount: [u8; 32],
) -> Result<Option<SplitCandidate>, ApiError> {
    let response: SplitListResponse = Call::bounded_wait(config.nns_governance, "list_neurons")
        .with_arg(SplitListRequest {
            neuron_ids: Vec::new(),
            include_neurons_readable_by_caller: false,
            include_empty_neurons_readable_by_caller: Some(false),
            include_public_neurons_in_full_neurons: Some(false),
            page_number: Some(0),
            page_size: Some(1),
            neuron_subaccounts: Some(vec![SplitSubaccount {
                subaccount: subaccount.to_vec(),
            }]),
        })
        .await
        .map_err(|error| ApiError::Pending(format!("split-child lookup failed: {error:?}")))?
        .candid()
        .map_err(|error| {
            ApiError::Pending(format!("split-child lookup decode failed: {error:?}"))
        })?;
    if response.total_pages_available.unwrap_or_default() > 1 || response.full_neurons.len() > 1 {
        return Err(ApiError::Invalid(
            "exact Split subaccount returned conflicting neurons".into(),
        ));
    }
    let Some(neuron) = response.full_neurons.into_iter().next() else {
        return Ok(None);
    };
    let neuron_id = neuron
        .id
        .map(|id| id.id)
        .filter(|id| *id > 0)
        .ok_or_else(|| ApiError::Invalid("split child has no ID".into()))?;
    let controller = neuron
        .controller
        .ok_or_else(|| ApiError::Invalid("split child has no controller".into()))?;
    if neuron.account.len() != 32 {
        return Err(ApiError::Invalid(
            "split child subaccount is not 32 bytes".into(),
        ));
    }
    Ok(Some(SplitCandidate {
        neuron_id,
        controller,
        physical_principal_e8s: neuron.cached_neuron_stake_e8s.into(),
        staking_subaccount: neuron.account,
        dissolve_state: neuron.dissolve_state,
    }))
}

async fn prove_split(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    let current = state::read();
    let observed =
        execution::query_neuron_observation(&current.config, operation.child_neuron_id).await?;
    ensure(&expected)?;
    if observed.snapshot.cached_stake_e8s != operation.principal_e8s {
        return pause(operation, "Split principal proof mismatch".into());
    }
    operation.child_staking_subaccount = observed.snapshot.staking_subaccount.to_vec();
    operation.phase = UnwindPhase::SplitProved;
    replace(&expected, operation.clone())?;
    submit_start(operation).await
}

async fn submit_start(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    operation.phase = UnwindPhase::StartDissolvingSubmitted;
    replace(&expected, operation.clone())?;
    let result =
        execution::set_dissolving(&state::read().config, operation.child_neuron_id, true).await;
    ensure(&operation)?;
    result?;
    recover_start(operation, false).await
}

async fn recover_start(
    mut operation: UnwindOperation,
    retry_missing: bool,
) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    let observed =
        execution::query_neuron_observation(&state::read().config, operation.child_neuron_id)
            .await?;
    ensure(&expected)?;
    match observed.dissolve_state {
        Some(DissolveState::WhenDissolvedTimestampSeconds(_)) => {}
        Some(DissolveState::DissolveDelaySeconds(_)) => {
            if !retry_missing {
                return Err(ApiError::Pending(
                    "StartDissolving awaits canonical reflection".into(),
                ));
            }
            let result =
                execution::set_dissolving(&state::read().config, operation.child_neuron_id, true)
                    .await;
            ensure(&operation)?;
            result?;
            return Box::pin(recover_start(operation, false)).await;
        }
        None => return pause(operation, "child dissolve identity is contradictory".into()),
    }
    operation.phase = UnwindPhase::StartDissolvingProved;
    replace(&expected, operation.clone())?;
    prove_start(operation).await
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
            committed_fee_e8s: committed_fee_basis(&operation)?,
            child_staking_subaccount: operation.child_staking_subaccount.clone(),
            ready_at_seconds,
            proof: CohortProofState::Dissolving,
            disbursement_block: None,
        },
    )?;
    Ok(UnwindProgress::Pending)
}

async fn submit_disbursement(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    let current = state::read();
    let governance_fee = execution::nns_transaction_fee(&current.config).await?;
    ensure(&expected)?;
    let ledger_fee = io_ledger_boundary::icp_fee(current.config.icp_ledger)
        .await
        .map_err(ApiError::Pending)?;
    ensure(&expected)?;
    if governance_fee != current.config.expected_icp_fee_e8s
        || ledger_fee != current.config.expected_icp_fee_e8s
        || governance_fee != ledger_fee
        || governance_fee != operation.committed_disbursement_fee_e8s
    {
        let mut latest = state::read();
        if !matches!(latest.active_operation, Some(NnsOperation::Unwind(ref active)) if active == &expected)
        {
            return Err(ApiError::Busy);
        }
        latest.lifecycle = Lifecycle::Paused;
        state::write(latest);
        return Err(ApiError::Stuck(format!(
            "child Disburse fee drift: frozen {}, Governance {governance_fee}, Ledger {ledger_fee}; Disburse was not submitted",
            operation.committed_disbursement_fee_e8s
        )));
    }
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
    match result? {
        execution::GovernanceCallOutcome::Succeeded(block) => {
            let submitted = operation.clone();
            operation.expected_block_index = Some(block);
            replace(&submitted, operation)?;
        }
        execution::GovernanceCallOutcome::RejectedNoEffect(_) => {
            let submitted = operation.clone();
            operation.phase = UnwindPhase::DisbursementPrepared;
            operation.submitted_at_seconds = 0;
            operation.expected_block_index = None;
            replace(&submitted, operation)?;
            return Ok(UnwindProgress::Pending);
        }
        execution::GovernanceCallOutcome::Ambiguous(_) => {}
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
        .checked_sub(operation.committed_disbursement_fee_e8s)
        .ok_or_else(|| ApiError::Invalid("child principal cannot cover fee".into()))?;
    if exact.from != from
        || exact.to != to
        || exact.amount_e8s != amount
        || exact.fee_e8s != operation.committed_disbursement_fee_e8s
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
    replace(&expected, operation.clone())?;
    observe_cleanup(operation).await
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
    let result = execution::increase_delay(&current.config, operation.child_neuron_id, 1).await;
    ensure(&operation)?;
    result?;
    recover_delay(operation, false).await
}

async fn recover_delay(
    mut operation: UnwindOperation,
    retry_missing: bool,
) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    let observed =
        execution::query_neuron_observation(&state::read().config, operation.child_neuron_id)
            .await?;
    ensure(&expected)?;
    match observed.dissolve_state {
        Some(DissolveState::DissolveDelaySeconds(1)) => {}
        Some(DissolveState::DissolveDelaySeconds(0)) => {
            if !retry_missing {
                return Err(ApiError::Pending(
                    "zero-principal delay command awaits canonical reflection".into(),
                ));
            }
            let result =
                execution::increase_delay(&state::read().config, operation.child_neuron_id, 1)
                    .await;
            ensure(&operation)?;
            result?;
            return Box::pin(recover_delay(operation, false)).await;
        }
        _ => {
            return pause(
                operation,
                "child delay state contradicted cleanup intent".into(),
            )
        }
    }
    operation.phase = UnwindPhase::DelayIncreaseProved;
    replace(&expected, operation.clone())?;
    submit_merge(operation).await
}

async fn submit_merge(mut operation: UnwindOperation) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    operation.phase = UnwindPhase::MergeSubmitted;
    replace(&expected, operation.clone())?;
    let current = state::read();
    let parent = current
        .pooled_parent_id
        .ok_or_else(|| ApiError::Invalid("pooled parent is absent".into()))?;
    let result = execution::merge_neuron(&current.config, parent, operation.child_neuron_id).await;
    ensure(&operation)?;
    result?;
    recover_merge(operation, false).await
}

async fn recover_merge(
    mut operation: UnwindOperation,
    retry_missing: bool,
) -> Result<UnwindProgress, ApiError> {
    let expected = operation.clone();
    let child =
        execution::query_neuron_observation(&state::read().config, operation.child_neuron_id)
            .await?;
    ensure(&expected)?;
    let remaining = u128::from(child.maturity_e8s)
        .checked_add(u128::from(child.staked_maturity_e8s))
        .ok_or_else(|| ApiError::Invalid("child maturity retry overflow".into()))?;
    if remaining > operation.child_maturity_e8s {
        return pause(operation, "child maturity increased during cleanup".into());
    }
    if remaining != 0 {
        if !retry_missing {
            return Err(ApiError::Pending(
                "zero-principal maturity merge awaits canonical reflection".into(),
            ));
        }
        let current = state::read();
        let parent = current
            .pooled_parent_id
            .ok_or_else(|| ApiError::Invalid("pooled parent is absent".into()))?;
        let result =
            execution::merge_neuron(&current.config, parent, operation.child_neuron_id).await;
        ensure(&operation)?;
        result?;
        return Box::pin(recover_merge(operation, false)).await;
    }
    operation.phase = UnwindPhase::MergeProved;
    replace(&expected, operation.clone())?;
    prove_cleanup(operation).await
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
        child_neuron_id: operation.child_neuron_id,
        physical_principal_e8s: operation.principal_e8s,
    });
    latest.live_cohorts.remove(index);
    state::write(latest);
    Ok(UnwindProgress::Completed)
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
        gross_e8s: cohort
            .principal_e8s
            .checked_add(cohort.committed_fee_e8s)
            .ok_or_else(|| ApiError::Invalid("promoted unwind gross overflow".into()))?,
        split_fee_e8s: 0,
        committed_disbursement_fee_e8s: cohort.committed_fee_e8s,
        parent_principal_before_split_e8s: 0,
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

pub(crate) fn committed_fee_basis(operation: &UnwindOperation) -> Result<u128, ApiError> {
    (operation.committed_disbursement_fee_e8s > 0)
        .then_some(operation.committed_disbursement_fee_e8s)
        .ok_or_else(|| ApiError::Invalid("committed unwind fee basis is invalid".into()))
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

fn fail_split_recovery(
    operation: &UnwindOperation,
    reason: String,
) -> Result<UnwindProgress, ApiError> {
    ensure(operation)?;
    let mut latest = state::read();
    latest.lifecycle = Lifecycle::Paused;
    state::write(latest);
    Err(ApiError::Stuck(reason))
}

fn clear_active(state: &mut state::NnsStateV1, expected: &UnwindOperation) -> Result<(), ApiError> {
    if !matches!(&state.active_operation, Some(NnsOperation::Unwind(active)) if active == expected)
    {
        return Err(ApiError::Busy);
    }
    state.active_operation = None;
    Ok(())
}
