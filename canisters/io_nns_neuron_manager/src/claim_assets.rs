use io_nns_types::backing::{PoolCommandPhase, TransitComponentKind, TransitComponentObservation};

use crate::{
    api::ApiError,
    maturity::MaturityKind,
    pool::UnwindPhase,
    state::{self, NnsOperation},
};

pub(crate) fn transit_components(
    snapshot: &crate::state::NnsStateV1,
    observed_parent_principal_e8s: u128,
) -> Result<Vec<TransitComponentObservation>, ApiError> {
    let mut components = Vec::new();
    let mut push = |kind, backing, fee_basis| {
        if backing > 0 {
            components.push(TransitComponentObservation {
                kind,
                backing_e8s: backing,
                fee_basis_e8s: fee_basis,
            });
        }
    };
    let pool = match &snapshot.active_operation {
        Some(NnsOperation::Pool(command))
            if !matches!(command.phase, PoolCommandPhase::AwaitingTransfer) =>
        {
            io_nns_types::backing::remaining_parent_transit(
                command.permit.expected_parent_principal_e8s,
                command.permit.claim_credit_e8s,
                observed_parent_principal_e8s,
            )
            .map_err(|error| {
                ApiError::Invalid(format!("pooled top-up transit failed: {error:?}"))
            })?
        }
        _ => 0,
    };
    push(TransitComponentKind::PoolTopUp, pool, None);
    let (unwind, unwind_fee) = match &snapshot.active_operation {
        Some(NnsOperation::Unwind(command))
            if matches!(
                command.phase,
                UnwindPhase::ChildIdentified
                    | UnwindPhase::SplitProved
                    | UnwindPhase::StartDissolvingSubmitted
                    | UnwindPhase::StartDissolvingProved
            ) =>
        {
            let fee_basis = crate::unwind_flow::committed_fee_basis(command)?;
            let backing = io_nns_types::backing::net_committed_child_backing(
                command.principal_e8s,
                fee_basis,
            )
            .map_err(|_| {
                ApiError::Invalid(
                    "committed unwind transit cannot cover its future disbursement fee".into(),
                )
            })?;
            (backing, Some(fee_basis))
        }
        _ => (0, None),
    };
    push(TransitComponentKind::ActiveUnwind, unwind, unwind_fee);
    let (jupiter, jupiter_fee) = match &snapshot.active_operation {
        Some(NnsOperation::Jupiter(command)) => (
            jupiter_claim_transit(command).unwrap_or_default(),
            jupiter_claim_fee_basis(command),
        ),
        _ => (0, None),
    };
    push(TransitComponentKind::ActiveJupiter, jupiter, jupiter_fee);
    let (maturity, maturity_fee) = match &snapshot.active_operation {
        Some(NnsOperation::Maturity(command)) => {
            let delivery = match &command.phase {
                crate::maturity::MaturityCommandPhase::Delivery(delivery) => Some(delivery),
                _ => None,
            };
            let backing =
                active_maturity_claim_transit(command, snapshot.config.expected_icp_fee_e8s)?;
            (
                backing,
                (backing > 0).then_some(
                    delivery
                        .expect("positive maturity transit has delivery evidence")
                        .pending
                        .captured_e8s
                        .map(|_| snapshot.config.expected_icp_fee_e8s)
                        .unwrap_or_default(),
                ),
            )
        }
        _ => (0, None),
    };
    push(TransitComponentKind::ActiveMaturity, maturity, maturity_fee);
    let active_pending = match &snapshot.active_operation {
        Some(NnsOperation::Maturity(command)) => match &command.phase {
            crate::maturity::MaturityCommandPhase::Delivery(delivery) => Some(&delivery.pending),
            _ => None,
        },
        _ => None,
    };
    for (component_kind, maturity_kind, pending) in [
        (
            TransitComponentKind::PendingTwoYearMaturity,
            MaturityKind::TwoYear,
            snapshot.pending_two_year_maturity.as_ref(),
        ),
        (
            TransitComponentKind::PendingTwoWeekMaturity,
            MaturityKind::TwoWeek,
            snapshot.pending_two_week_maturity.as_ref(),
        ),
    ] {
        let backing = pending
            .filter(|pending| active_pending != Some(*pending))
            .map(|pending| {
                maturity_claim_transit(
                    maturity_kind,
                    pending,
                    snapshot.config.expected_icp_fee_e8s,
                    false,
                )
            })
            .transpose()?
            .unwrap_or_default();
        push(
            component_kind,
            backing,
            pending
                .filter(|_| backing > 0)
                .map(|_| snapshot.config.expected_icp_fee_e8s),
        );
    }
    Ok(components)
}

pub(crate) fn active_maturity_claim_transit(
    command: &crate::maturity::MaturityCommandOperation,
    fee_e8s: u128,
) -> Result<u128, ApiError> {
    let crate::maturity::MaturityCommandPhase::Delivery(delivery) = &command.phase else {
        return Ok(0);
    };
    if claim_transfer_succeeded(delivery.claim_transfer.as_ref())
        || (command.kind == MaturityKind::TwoWeek && delivery.permit.is_none())
    {
        return Ok(0);
    }
    match command.kind {
        MaturityKind::TwoYear => Ok(delivery
            .two_year_plan
            .ok_or_else(|| ApiError::Invalid("two-year maturity lacks frozen economics".into()))?
            .ordinary
            .map_or(0, |split| split.claim_gross)),
        MaturityKind::TwoWeek => {
            maturity_claim_transit(command.kind, &delivery.pending, fee_e8s, true)
        }
    }
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

fn jupiter_claim_fee_basis(command: &crate::jupiter::JupiterOperation) -> Option<u128> {
    match &command.phase {
        crate::jupiter::JupiterPhase::ReceiptPermitPrepared { .. } => Some(command.deposit.fee_e8s),
        crate::jupiter::JupiterPhase::LiquidTransferPrepared { attempt, .. }
        | crate::jupiter::JupiterPhase::LiquidTransferSubmitted { attempt, .. }
        | crate::jupiter::JupiterPhase::Stuck {
            transfer: Some(crate::jupiter::JupiterStuckTransfer::Liquid { attempt, .. }),
            ..
        } => Some(attempt.intent.fee_e8s),
        _ => None,
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
                pause_reason: crate::jupiter::JupiterPauseReason::AmbiguousPossibleEffect,
                transfer: Some(crate::jupiter::JupiterStuckTransfer::Liquid { .. }),
                ..
            } => true,
            crate::jupiter::JupiterPhase::Stuck {
                transfer: Some(crate::jupiter::JupiterStuckTransfer::Liquid { attempt, .. }),
                ..
            } => ambiguous_claim_transfer(Some(attempt)),
            _ => false,
        },
        Some(NnsOperation::Maturity(command)) => match &command.phase {
            crate::maturity::MaturityCommandPhase::Delivery(delivery) => {
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
                | crate::transfer::TransferState::Paused {
                    classification:
                        crate::transfer::TransferOutcomeClassification::AmbiguousPossibleEffect,
                    ..
                }
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
            crate::maturity::MaturityCommandPhase::Delivery(delivery) => {
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
    kind: MaturityKind,
    pending: &crate::maturity::PendingMaturityDisbursement,
    fee_e8s: u128,
    paired_permitted: bool,
) -> Result<u128, ApiError> {
    let Some(captured_e8s) = pending.captured_e8s else {
        return Ok(0);
    };
    maturity_ingress_transit(kind, captured_e8s, fee_e8s, paired_permitted)
}

pub(crate) fn maturity_ingress_transit(
    kind: MaturityKind,
    captured_e8s: u128,
    fee_e8s: u128,
    paired_permitted: bool,
) -> Result<u128, ApiError> {
    match kind {
        MaturityKind::TwoWeek if !paired_permitted => Ok(0),
        MaturityKind::TwoYear | MaturityKind::TwoWeek => {
            let claim = io_nns_types::maturity::capture_40_60(captured_e8s, fee_e8s, fee_e8s)
                .map_err(|error| {
                    ApiError::Invalid(format!("maturity transit split failed: {error:?}"))
                })?
                .claim_gross;
            Ok(claim)
        }
    }
}

pub(crate) fn maturity_delivery_has_unpaid_fee(
    delivery: &crate::maturity::MaturityDeliveryOperation,
) -> bool {
    let transfer_unpaid = |state: Option<&crate::maturity::PermanentCreditState>| match state {
        Some(crate::maturity::PermanentCreditState::Prepared { transfer, .. }) => !matches!(
            transfer.state,
            crate::transfer::TransferState::Succeeded { .. }
        ),
        Some(crate::maturity::PermanentCreditState::RefreshSubmitted { .. })
        | Some(crate::maturity::PermanentCreditState::Proved(_)) => false,
        None => true,
    };
    if let Some(plan) = delivery.two_year_plan {
        if plan.anchor_reimbursement > 0 && transfer_unpaid(delivery.anchor_reimbursement.as_ref())
        {
            return true;
        }
        if plan.permanent_reimbursement > 0
            && transfer_unpaid(delivery.permanent_reimbursement.as_ref())
        {
            return true;
        }
        if plan.ordinary.is_none() {
            return false;
        }
    }
    transfer_unpaid(delivery.permanent_credit.as_ref())
        || !matches!(
            delivery
                .claim_transfer
                .as_ref()
                .map(|attempt| &attempt.state),
            Some(crate::transfer::TransferState::Succeeded { .. })
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        jupiter::{JupiterDeposit, JupiterOperation, JupiterPhase, PermanentNeuronCreditProof},
        maturity::{
            MaturityCommandOperation, MaturityCommandPhase, MaturityDeliveryOperation,
            PendingMaturityDisbursement,
        },
        pool::UnwindOperation,
    };
    use io_nns_types::backing::{PoolCommand, PoolCommandKind, TopUpPermit};

    const FEE: u128 = 10;

    fn pending(
        _state: &crate::state::NnsStateV1,
        kind: MaturityKind,
    ) -> PendingMaturityDisbursement {
        let (generation, target) = match kind {
            MaturityKind::TwoYear => (None, None),
            MaturityKind::TwoWeek => (Some(1), Some(1_000)),
        };
        PendingMaturityDisbursement {
            nominal_disbursed_e8s: 100_000_000,
            initiated_at_seconds: 1,
            scheduled_finalization_timestamp_seconds: 604_801,
            entitlement_batch_generation: generation,
            two_week_target_e8s: target,
            captured_e8s: Some(100_000_000),
        }
    }

    fn permit(
        state: &crate::state::NnsStateV1,
        amount_e8s: u128,
    ) -> crate::jupiter::StreamReceiptPermit {
        crate::jupiter::StreamReceiptPermit {
            stream_operation_sequence: 1,
            destination: state.config.stream_liquid_account.clone(),
            amount_e8s,
            memo: io_nns_types::receipt::receipt_memo(1),
        }
    }

    fn jupiter(state: &crate::state::NnsStateV1) -> NnsOperation {
        NnsOperation::Jupiter(Box::new(JupiterOperation {
            operation_sequence: 1,
            dispatch_epoch: 1,
            captured_control_epoch: 1,
            deposit: JupiterDeposit {
                block_index: 1,
                gross_e8s: 100,
                stake_e8s: 30,
                liquid_e8s: 50,
                fee_e8s: FEE,
                created_at_time_nanos: 1,
            },
            phase: JupiterPhase::ReceiptPermitPrepared {
                proof: PermanentNeuronCreditProof {
                    neuron_id: 1,
                    staking_subaccount: [1; 32],
                    before_cached_stake_e8s: 1_000,
                    protocol_credit_e8s: 30,
                    transfer_block: 2,
                    observed_after_cached_stake_e8s: 1_030,
                },
                permit: permit(state, 50),
            },
        }))
    }

    fn active_maturity(
        state: &crate::state::NnsStateV1,
        kind: MaturityKind,
        value: PendingMaturityDisbursement,
    ) -> NnsOperation {
        NnsOperation::Maturity(Box::new(MaturityCommandOperation {
            operation_sequence: 1,
            dispatch_epoch: 1,
            kind,
            phase: MaturityCommandPhase::Delivery(MaturityDeliveryOperation {
                permit: Some(permit(state, 59_990_000)),
                pending: value,
                two_year_plan: (kind == MaturityKind::TwoYear).then(|| {
                    io_nns_types::maturity::plan_two_year_replenishment(
                        100_000_000,
                        io_nns_types::backing::DYNAMIC_ANCHOR_TARGET_E8S,
                        state.anchor_available_e8s,
                        state.permanent_fee_shortfall_e8s,
                        state.config.expected_icp_fee_e8s,
                    )
                    .unwrap()
                }),
                anchor_reimbursement: None,
                permanent_reimbursement: None,
                permanent_credit: None,
                claim_transfer: None,
            }),
        }))
    }

    fn component_values(
        state: &crate::state::NnsStateV1,
        parent: u128,
    ) -> Vec<(TransitComponentKind, u128)> {
        transit_components(state, parent)
            .unwrap()
            .into_iter()
            .map(|component| (component.kind, component.backing_e8s))
            .collect()
    }

    #[test]
    fn two_year_yield_is_composed_with_each_independent_active_owner() {
        let (_, base) = crate::state::tests::valid_state();
        let permanent = pending(&base, MaturityKind::TwoYear);

        let mut state = base.clone();
        state.pending_two_year_maturity = Some(permanent.clone());
        state.active_operation = Some(jupiter(&state));
        assert_eq!(
            component_values(&state, 0),
            vec![
                (TransitComponentKind::ActiveJupiter, 50),
                (TransitComponentKind::PendingTwoYearMaturity, 60_000_000),
            ]
        );

        let mut state = base.clone();
        state.pending_two_year_maturity = Some(permanent.clone());
        let pooled = pending(&state, MaturityKind::TwoWeek);
        state.pending_two_week_maturity = Some(pooled.clone());
        state.active_operation = Some(active_maturity(&state, MaturityKind::TwoWeek, pooled));
        assert_eq!(
            component_values(&state, 0),
            vec![
                (TransitComponentKind::ActiveMaturity, 60_000_000),
                (TransitComponentKind::PendingTwoYearMaturity, 60_000_000),
            ]
        );

        let mut state = base.clone();
        state.pending_two_year_maturity = Some(permanent.clone());
        state.active_operation = Some(NnsOperation::Pool(PoolCommand {
            kind: PoolCommandKind::TopUp,
            permit: TopUpPermit {
                generation: 1,
                operation_sequence: 1,
                expected_parent_principal_e8s: 100,
                expected_parent_physical_e8s: 1_000_000_100,
                destination: crate::state::Account {
                    owner: state.config.nns_governance,
                    subaccount: Some(vec![9; 32]),
                },
                expected_credit_e8s: 50,
                claim_credit_e8s: 60,
                fee_e8s: FEE,
                memo: vec![1],
                prepared_at_nanos: 1,
                snapshot_fingerprint: vec![2; 32],
            },
            transfer_block_index: Some(1),
            parent_neuron_id: Some(2),
            phase: PoolCommandPhase::RefreshSubmitted,
        }));
        assert_eq!(
            component_values(&state, 100),
            vec![
                (TransitComponentKind::PoolTopUp, 60),
                (TransitComponentKind::PendingTwoYearMaturity, 60_000_000),
            ]
        );

        let mut state = base;
        state.pending_two_year_maturity = Some(permanent);
        state.active_operation = Some(NnsOperation::Unwind(UnwindOperation {
            operation_sequence: 1,
            generation: 1,
            reconciliation_request_fingerprint: vec![3; 32],
            target_e8s: 100,
            gross_e8s: 120,
            split_fee_e8s: FEE,
            committed_disbursement_fee_e8s: FEE,
            parent_principal_before_split_e8s: 220,
            child_neuron_id: 3,
            principal_e8s: 110,
            child_staking_subaccount: vec![3; 32],
            submitted_at_seconds: 1,
            expected_block_index: None,
            child_maturity_e8s: 0,
            parent_maturity_e8s: 0,
            parent_principal_e8s: 0,
            phase: UnwindPhase::ChildIdentified,
        }));
        assert_eq!(
            component_values(&state, 100),
            vec![
                (TransitComponentKind::ActiveUnwind, 100),
                (TransitComponentKind::PendingTwoYearMaturity, 60_000_000),
            ]
        );
    }

    #[test]
    fn both_pending_slots_are_inspected_and_active_pending_is_excluded_once() {
        let (_, mut state) = crate::state::tests::valid_state();
        state.pending_two_year_maturity = Some(pending(&state, MaturityKind::TwoYear));
        state.pending_two_week_maturity = Some(pending(&state, MaturityKind::TwoWeek));
        assert_eq!(
            component_values(&state, 0),
            vec![(TransitComponentKind::PendingTwoYearMaturity, 60_000_000)]
        );

        let pooled = state.pending_two_week_maturity.clone().unwrap();
        state.active_operation = Some(active_maturity(&state, MaturityKind::TwoWeek, pooled));
        let values = component_values(&state, 0);
        assert_eq!(
            values,
            vec![
                (TransitComponentKind::ActiveMaturity, 60_000_000),
                (TransitComponentKind::PendingTwoYearMaturity, 60_000_000),
            ]
        );
        assert_eq!(
            values.iter().map(|(_, value)| value).sum::<u128>(),
            120_000_000
        );
    }

    #[test]
    fn yield_prevents_paired_credit_from_doubling_claim_backing() {
        let backing_before = 100_u128;
        let supply_before = 100_u128;
        let permanent_yield = 100_u128;
        let paired_claim_credit = 60_u128;
        let backing_after_yield = backing_before.checked_add(permanent_yield).unwrap();
        assert_eq!(backing_after_yield / supply_before, 2);
        let maximum_new_io = paired_claim_credit / 2;
        assert_eq!(maximum_new_io, 30);
        assert_ne!(maximum_new_io, paired_claim_credit);
    }
}
