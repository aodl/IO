use crate::{
    api::{ApiError, MaturityProgress},
    execution,
    jupiter::{NeuronSnapshot, PermanentNeuronCreditProof},
    maturity::{MaturityCommandOperation, PermanentCreditState},
    maturity_flow, state,
    transfer::{NnsTransferAttempt, NnsTransferIntent},
};

pub(crate) fn prepare(
    operation: MaturityCommandOperation,
    before: NeuronSnapshot,
    amount: u128,
) -> Result<MaturityProgress, ApiError> {
    let config = state::read().config;
    let transfer = NnsTransferAttempt::prepared(NnsTransferIntent {
        ledger: config.icp_ledger,
        source_subaccount: config
            .maturity_staging
            .canonical()
            .map_err(ApiError::Invalid)?
            .subaccount,
        destination: execution::staking_account(&config, &before),
        amount_e8s: amount,
        fee_e8s: config.expected_icp_fee_e8s,
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
    maturity_flow::write_exact(&operation, replacement, false)?;
    Ok(MaturityProgress::DeliveringClaimReceipt)
}

pub(crate) async fn refresh(neuron_id: u64) -> Result<MaturityProgress, ApiError> {
    execution::refresh_neuron(&state::read().config, neuron_id)
        .await
        .map_err(|error| ApiError::Pending(format!("permanent refresh awaits proof: {error:?}")))?;
    Ok(MaturityProgress::DeliveringClaimReceipt)
}

pub(crate) async fn prove_or_refresh(
    operation: MaturityCommandOperation,
    before: NeuronSnapshot,
    transfer_block: u128,
    protocol_credit_e8s: u128,
) -> Result<MaturityProgress, ApiError> {
    let after = execution::query_neuron(&state::read().config, before.neuron_id).await?;
    maturity_flow::ensure_exact(&operation)?;
    let proof = PermanentNeuronCreditProof {
        neuron_id: before.neuron_id,
        staking_subaccount: before.staking_subaccount,
        before_cached_stake_e8s: before.cached_stake_e8s,
        protocol_credit_e8s,
        transfer_block,
        observed_after_cached_stake_e8s: after.cached_stake_e8s,
    };
    if after.neuron_id != proof.neuron_id
        || after.staking_subaccount != proof.staking_subaccount
        || proof.validate().is_err()
    {
        return refresh(before.neuron_id).await;
    }
    let mut replacement = operation.clone();
    maturity_flow::delivery_mut(&mut replacement).permanent_credit =
        Some(PermanentCreditState::Proved(proof));
    maturity_flow::write_exact(&operation, replacement, false)?;
    Ok(MaturityProgress::DeliveringClaimReceipt)
}
