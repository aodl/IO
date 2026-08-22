use io_ledger_boundary::{exact_icp_block, icp_account_identifier, IcpExactResult};

use crate::{
    api::{ApiError, MaturityProgress},
    execution,
    maturity::{
        CompletedMaturity, MaturityCommandOperation, MaturityKind, MintEvidence, MintProofState,
    },
    maturity_flow,
    state::{self, NnsOperation},
};

pub async fn prove(kind: MaturityKind, block_index: u128) -> Result<MaturityProgress, ApiError> {
    let snapshot = state::read();
    let expected = match maturity_flow::pending_from(&snapshot, kind) {
        Some(pending) => pending,
        None => {
            let completed = match kind {
                MaturityKind::TwoYear => snapshot.last_two_year_maturity.as_ref(),
                MaturityKind::TwoWeek => snapshot.last_two_week_maturity.as_ref(),
            };
            return completed
                .filter(|completed| completed.mint_block == block_index)
                .cloned()
                .map(MaturityProgress::Completed)
                .ok_or_else(|| ApiError::Invalid("no pending matching maturity Mint".into()));
        }
    };
    if !matches!(expected.mint_proof, MintProofState::Awaiting) {
        return maturity_flow::replay_proved(&expected, block_index);
    }
    let exact = exact_icp_block(state::read().config.icp_ledger, block_index)
        .await
        .map_err(ApiError::Invalid)?;
    maturity_flow::ensure_pending(&expected)?;
    let mint = match exact {
        IcpExactResult::Mint(mint) => mint,
        IcpExactResult::Transfer(_) => {
            return Err(ApiError::Invalid(
                "maturity proof block is not an ICP Mint".into(),
            ))
        }
    };
    let destination = icp_account_identifier(&expected.destination).map_err(ApiError::Invalid)?;
    if mint.to != destination
        || mint.amount_e8s == 0
        || mint.icrc1_memo.is_some()
        || mint.native_memo_u64 < expected.scheduled_finalization_timestamp_seconds
        || mint.created_at_time / 1_000_000_000 < mint.native_memo_u64
    {
        return Err(ApiError::Invalid(
            "exact Mint does not match pinned NNS maturity finalization behavior".into(),
        ));
    }
    let observation =
        execution::query_neuron_observation(&state::read().config, expected.neuron_id).await?;
    maturity_flow::ensure_pending(&expected)?;
    if execution::has_exact_maturity_disbursement(
        &observation,
        expected.nominal_disbursed_maturity_e8s,
        &expected.destination,
        expected.initiation_timestamp_seconds,
        expected.scheduled_finalization_timestamp_seconds,
    ) {
        return Err(ApiError::Pending(
            "canonical neuron still contains the pending maturity disbursement".into(),
        ));
    }
    let evidence = MintEvidence {
        mint_block: block_index,
        actual_minted_icp_e8s: mint.amount_e8s,
        native_memo_u64: mint.native_memo_u64,
        created_at_time_nanos: mint.created_at_time,
    };
    let mut replacement = expected.clone();
    replacement.mint_proof = MintProofState::Proved(evidence);
    maturity_flow::replace_pending(&expected, replacement)?;
    Ok(MaturityProgress::MintProved)
}

pub(crate) fn finish(operation: MaturityCommandOperation) -> Result<MaturityProgress, ApiError> {
    let delivery = maturity_flow::delivery_ref(&operation);
    let MintProofState::Delivering(mint) = &delivery.pending.mint_proof else {
        return Err(ApiError::Invalid(
            "completed inflow lacks exact Mint".into(),
        ));
    };
    let completed = CompletedMaturity {
        kind: operation.kind,
        neuron_id: delivery.pending.neuron_id,
        mint_block: mint.mint_block,
        nominal_disbursed_maturity_e8s: delivery.pending.nominal_disbursed_maturity_e8s,
        actual_minted_icp_e8s: mint.actual_minted_icp_e8s,
        destination: delivery.pending.destination.clone(),
        completed_at_nanos: ic_cdk::api::time(),
    };
    if completed.completed_at_nanos == 0 {
        return Err(ApiError::Invalid("canister time is zero".into()));
    }
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(NnsOperation::Maturity(active)) if **active == operation)
    {
        return Err(ApiError::Busy);
    }
    latest.active_operation = None;
    match operation.kind {
        MaturityKind::TwoYear => {
            latest.pending_two_year_maturity = None;
            latest.last_two_year_maturity = Some(completed.clone());
        }
        MaturityKind::TwoWeek => {
            let generation = delivery
                .pending
                .stake_evidence
                .plan
                .entitlement_batch_generation
                .ok_or_else(|| {
                    ApiError::Invalid("pooled maturity lacks entitlement generation".into())
                })?;
            latest.pending_two_week_maturity = None;
            latest.last_two_week_maturity = Some(completed.clone());
            latest.latest_completed_two_week_generation = generation;
        }
    }
    state::write(latest);
    Ok(MaturityProgress::Completed(completed))
}
