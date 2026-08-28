use crate::{
    api::{ApiError, MaturityProgress},
    execution,
    jupiter::{NeuronSnapshot, PermanentNeuronCreditProof},
    maturity::{MaturityCommandOperation, PermanentCreditState},
    maturity_flow, state,
    transfer::{NnsTransferAttempt, NnsTransferIntent},
};

pub(crate) fn observe_credit(
    before: &NeuronSnapshot,
    transfer_block: u128,
    protocol_credit_e8s: u128,
    after: &NeuronSnapshot,
) -> Result<Option<PermanentNeuronCreditProof>, ApiError> {
    if after.neuron_id != before.neuron_id
        || after.staking_subaccount != before.staking_subaccount
        || after.cached_stake_e8s < before.cached_stake_e8s
    {
        return Err(ApiError::Stuck(
            "permanent neuron credit identity or monotonicity contradicted".into(),
        ));
    }
    let proof = PermanentNeuronCreditProof {
        neuron_id: before.neuron_id,
        staking_subaccount: before.staking_subaccount,
        before_cached_stake_e8s: before.cached_stake_e8s,
        protocol_credit_e8s,
        transfer_block,
        observed_after_cached_stake_e8s: after.cached_stake_e8s,
    };
    match proof.validate() {
        Ok(()) => Ok(Some(proof)),
        Err(_) => Ok(None),
    }
}

pub(crate) async fn prepare(
    operation: MaturityCommandOperation,
    before: NeuronSnapshot,
    amount: u128,
    fee_e8s: u128,
) -> Result<MaturityProgress, ApiError> {
    let config = state::read().config;
    let transfer = NnsTransferAttempt::prepared(NnsTransferIntent {
        ledger: config.icp_ledger,
        source_subaccount: maturity_flow::staging_account(
            ic_cdk::api::canister_self(),
            operation.kind,
        )
        .canonical()
        .map_err(ApiError::Invalid)?
        .subaccount,
        destination: execution::staking_account(&config, &before),
        amount_e8s: amount,
        fee_e8s,
        memo: maturity_flow::maturity_transfer_memo(
            b"io-pooled-maturity-permanent-v1",
            operation.operation_sequence,
        ),
        created_at_time_nanos: maturity_flow::now_nanos()?,
    })
    .map_err(ApiError::Invalid)?;
    let mut replacement = operation.clone();
    maturity_flow::delivery_mut(&mut replacement).permanent_credit =
        Some(PermanentCreditState::Prepared {
            before,
            transfer: Box::new(transfer),
        });
    maturity_flow::write_exact(&operation, replacement.clone(), false)?;
    Box::pin(maturity_flow::resume_active(replacement)).await
}

pub(crate) async fn refresh(
    operation: MaturityCommandOperation,
    before: NeuronSnapshot,
    transfer_block: u128,
    protocol_credit_e8s: u128,
) -> Result<MaturityProgress, ApiError> {
    let neuron_id = before.neuron_id;
    let result = execution::refresh_neuron(&state::read().config, neuron_id).await;
    maturity_flow::ensure_exact(&operation)?;
    result?;
    Box::pin(prove_or_refresh(
        operation,
        before,
        transfer_block,
        protocol_credit_e8s,
        false,
    ))
    .await
}

pub(crate) async fn prove_or_refresh(
    operation: MaturityCommandOperation,
    before: NeuronSnapshot,
    transfer_block: u128,
    protocol_credit_e8s: u128,
    retry_missing: bool,
) -> Result<MaturityProgress, ApiError> {
    let after = execution::query_neuron(&state::read().config, before.neuron_id).await?;
    maturity_flow::ensure_exact(&operation)?;
    let Some(proof) = observe_credit(&before, transfer_block, protocol_credit_e8s, &after)? else {
        if !retry_missing {
            return Err(ApiError::Pending(
                "permanent-leg ClaimOrRefresh awaits canonical stake reflection".into(),
            ));
        }
        return refresh(operation, before, transfer_block, protocol_credit_e8s).await;
    };
    let mut replacement = operation.clone();
    maturity_flow::delivery_mut(&mut replacement).permanent_credit =
        Some(PermanentCreditState::Proved(proof));
    maturity_flow::write_exact(&operation, replacement.clone(), false)?;
    Box::pin(maturity_flow::resume_active(replacement)).await
}
