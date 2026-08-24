use io_nns_types::backing::PoolCommandPhase;

use crate::{
    api::ApiError,
    maturity::MaturityKind,
    pool::UnwindPhase,
    state::{self, NnsOperation},
};

pub(crate) fn transit_backing(
    snapshot: &crate::state::NnsStateV1,
    observed_parent_principal_e8s: u128,
    active_unwind_fee_basis_e8s: Option<u128>,
) -> Result<u128, ApiError> {
    let backing = match &snapshot.active_operation {
        Some(NnsOperation::Pool(command))
            if !matches!(command.phase, PoolCommandPhase::AwaitingTransfer) =>
        {
            io_nns_types::backing::remaining_parent_transit(
                command.permit.expected_parent_principal_e8s,
                command.permit.expected_credit_e8s,
                observed_parent_principal_e8s,
            )
            .map_err(|error| {
                ApiError::Invalid(format!("pooled top-up transit failed: {error:?}"))
            })?
        }
        Some(NnsOperation::Unwind(command))
            if matches!(
                command.phase,
                UnwindPhase::ChildIdentified
                    | UnwindPhase::SplitProved
                    | UnwindPhase::StartDissolvingSubmitted
                    | UnwindPhase::StartDissolvingProved
            ) =>
        {
            io_nns_types::backing::net_committed_child_backing(
                command.principal_e8s,
                active_unwind_fee_basis_e8s.ok_or_else(|| {
                    ApiError::Invalid("active unwind lost its committed fee basis".into())
                })?,
            )
            .map_err(|_| {
                ApiError::Invalid(
                    "committed unwind transit cannot cover its future disbursement fee".into(),
                )
            })?
        }
        Some(NnsOperation::Jupiter(command)) => jupiter_claim_transit(command).unwrap_or_default(),
        Some(NnsOperation::Maturity(command)) => {
            let delivery = match &command.phase {
                crate::maturity::MaturityCommandPhase::ClaimReceiptDelivery(delivery) => delivery,
                _ => return Ok(0),
            };
            if claim_transfer_succeeded(delivery.claim_transfer.as_ref())
                || (command.kind == MaturityKind::TwoWeek && delivery.permit.is_none())
            {
                0
            } else {
                maturity_claim_transit(
                    &delivery.pending,
                    snapshot.config.expected_icp_fee_e8s,
                    true,
                )?
            }
        }
        _ => {
            let pending = snapshot
                .pending_two_week_maturity
                .as_ref()
                .or(snapshot.pending_two_year_maturity.as_ref());
            pending
                .map(|pending| {
                    maturity_claim_transit(pending, snapshot.config.expected_icp_fee_e8s, false)
                })
                .transpose()?
                .unwrap_or(0)
        }
    };
    Ok(backing)
}

pub(crate) fn jupiter_claim_transit(command: &crate::jupiter::JupiterOperation) -> Option<u128> {
    matches!(
        command.phase,
        crate::jupiter::JupiterPhase::ReceiptPermitPrepared { .. }
            | crate::jupiter::JupiterPhase::LiquidTransferPrepared { .. }
            | crate::jupiter::JupiterPhase::Stuck {
                transfer: Some(crate::jupiter::JupiterStuckTransfer::Liquid { .. }),
                ..
            }
    )
    .then_some(command.deposit.liquid_e8s)
}

pub(crate) fn active_unwind_fee_basis(
    snapshot: &crate::state::NnsStateV1,
) -> Result<Option<u128>, ApiError> {
    match &snapshot.active_operation {
        Some(NnsOperation::Unwind(command))
            if matches!(
                command.phase,
                UnwindPhase::ChildIdentified
                    | UnwindPhase::SplitProved
                    | UnwindPhase::StartDissolvingSubmitted
                    | UnwindPhase::StartDissolvingProved
            ) =>
        {
            crate::unwind_flow::committed_fee_basis(command).map(Some)
        }
        _ => Ok(None),
    }
}

pub(crate) fn has_ambiguous_backing_effect(snapshot: &crate::state::NnsStateV1) -> bool {
    match &snapshot.active_operation {
        Some(NnsOperation::Unwind(command)) => matches!(
            command.phase,
            UnwindPhase::SplitSubmitted | UnwindPhase::DisbursementSubmitted
        ),
        Some(NnsOperation::Jupiter(command)) => match &command.phase {
            crate::jupiter::JupiterPhase::LiquidTransferSubmitted { .. } => true,
            crate::jupiter::JupiterPhase::Stuck {
                transfer: Some(crate::jupiter::JupiterStuckTransfer::Liquid { attempt, .. }),
                ..
            } => ambiguous_claim_transfer(Some(attempt)),
            _ => false,
        },
        Some(NnsOperation::Maturity(command)) => match &command.phase {
            crate::maturity::MaturityCommandPhase::ClaimReceiptDelivery(delivery) => {
                ambiguous_claim_transfer(delivery.claim_transfer.as_ref())
            }
            _ => false,
        },
        _ => false,
    }
}

pub(crate) fn claim_transfer_succeeded(
    attempt: Option<&crate::transfer::NnsTransferAttempt>,
) -> bool {
    attempt.is_some_and(|attempt| {
        matches!(
            attempt.state,
            crate::transfer::TransferState::Succeeded { .. }
        )
    })
}

pub(crate) fn ambiguous_claim_transfer(
    attempt: Option<&crate::transfer::NnsTransferAttempt>,
) -> bool {
    attempt.is_some_and(|attempt| {
        matches!(
            attempt.state,
            crate::transfer::TransferState::Submitted { .. }
        )
    })
}

pub(crate) fn insufficient_claim_asset_requirement(
    snapshot: &crate::state::NnsStateV1,
) -> Result<Option<(state::Account, u128)>, ApiError> {
    use crate::transfer::{TransferOutcomeClassification, TransferState};
    let attempt = match &snapshot.active_operation {
        Some(NnsOperation::Jupiter(command)) => match &command.phase {
            crate::jupiter::JupiterPhase::Stuck {
                pause_reason: crate::jupiter::JupiterPauseReason::InsufficientFunds,
                transfer: Some(crate::jupiter::JupiterStuckTransfer::Liquid { attempt, .. }),
                ..
            } => Some(attempt),
            _ => None,
        },
        Some(NnsOperation::Maturity(command)) => match &command.phase {
            crate::maturity::MaturityCommandPhase::ClaimReceiptDelivery(delivery) => {
                delivery.claim_transfer.as_ref().filter(|attempt| {
                    matches!(
                        attempt.state,
                        TransferState::Paused {
                            classification: TransferOutcomeClassification::InsufficientFunds,
                            ..
                        }
                    )
                })
            }
            _ => None,
        },
        _ => None,
    };
    attempt
        .map(|attempt| {
            let required = attempt
                .intent
                .amount_e8s
                .checked_add(attempt.intent.fee_e8s)
                .ok_or_else(|| ApiError::Invalid("claim transit funding overflow".into()))?;
            Ok((
                state::Account {
                    owner: ic_cdk::api::canister_self(),
                    subaccount: Some(attempt.intent.source_subaccount.to_vec()),
                },
                required,
            ))
        })
        .transpose()
}

fn maturity_claim_transit(
    pending: &crate::maturity::PendingMaturityDisbursement,
    fee_e8s: u128,
    paired_permitted: bool,
) -> Result<u128, ApiError> {
    let mint = match &pending.mint_proof {
        crate::maturity::MintProofState::Proved(mint)
        | crate::maturity::MintProofState::Delivering(mint) => mint,
        crate::maturity::MintProofState::Awaiting => return Ok(0),
    };
    maturity_ingress_transit(
        pending.kind,
        mint.actual_minted_icp_e8s,
        fee_e8s,
        paired_permitted,
    )
}

pub(crate) fn maturity_ingress_transit(
    kind: MaturityKind,
    minted_e8s: u128,
    fee_e8s: u128,
    paired_permitted: bool,
) -> Result<u128, ApiError> {
    match kind {
        MaturityKind::TwoYear => io_reward_policy::permanent_maturity_credit(minted_e8s, fee_e8s)
            .map_err(|error| ApiError::Invalid(format!("maturity transit failed: {error:?}"))),
        MaturityKind::TwoWeek if !paired_permitted => Ok(0),
        MaturityKind::TwoWeek => {
            let claim = io_core_model::split_40_60(minted_e8s)
                .map_err(|error| {
                    ApiError::Invalid(format!("maturity transit split failed: {error:?}"))
                })?
                .claim;
            claim
                .checked_sub(fee_e8s)
                .filter(|credit| *credit > 0)
                .ok_or_else(|| ApiError::Invalid("pooled claim transit cannot pay its fee".into()))
        }
    }
}
