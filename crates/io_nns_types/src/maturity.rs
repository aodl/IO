use crate::{
    jupiter::{NeuronSnapshot, PermanentNeuronCreditProof},
    receipt::ClaimBackingReceiptPermit,
    transfer::NnsTransferAttempt,
};
use {candid::CandidType, serde::Deserialize};

pub const MINIMUM_DISBURSEMENT_E8S: u64 = 100_000_000;
pub const DISBURSEMENT_DELAY_SECONDS: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CaptureSplit {
    pub captured: u128,
    pub permanent_gross: u128,
    pub permanent_credit: u128,
    pub permanent_fee: u128,
    pub claim_gross: u128,
    pub claim_credit: u128,
    pub claim_fee: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TwoYearReplenishmentPlan {
    pub captured: u128,
    pub anchor_reimbursement: u128,
    pub anchor_reimbursement_fee: u128,
    pub ordinary: Option<CaptureSplit>,
    pub carried: u128,
}

impl TwoYearReplenishmentPlan {
    pub fn validate(self, transfer_fee: u128) -> Result<(), String> {
        if self.captured == 0 || transfer_fee == 0 {
            return Err("two-year replenishment plan is empty".into());
        }
        if self.anchor_reimbursement_fee
            != transfer_fee
                .checked_mul(u128::from(self.anchor_reimbursement > 0))
                .ok_or("anchor reimbursement fee overflow")?
            || (self.ordinary.is_some() && self.carried > 0)
        {
            return Err("two-year anchor reimbursement fee or carry policy is inconsistent".into());
        }
        if let Some(ordinary) = self.ordinary {
            if ordinary.captured == 0
                || ordinary.permanent_fee != transfer_fee
                || ordinary.claim_fee != transfer_fee
                || capture_40_60(ordinary.captured, transfer_fee, transfer_fee)
                    .map_err(|error| format!("ordinary maturity split is invalid: {error:?}"))?
                    != ordinary
            {
                return Err("ordinary maturity split differs from frozen economics".into());
            }
        }
        let accounted = self
            .anchor_reimbursement
            .checked_add(self.anchor_reimbursement_fee)
            .and_then(|value| {
                value.checked_add(self.ordinary.map_or(0, |ordinary| ordinary.captured))
            })
            .and_then(|value| value.checked_add(self.carried))
            .ok_or("two-year plan accounting overflow")?;
        if accounted != self.captured {
            return Err("two-year plan does not conserve the frozen capture".into());
        }
        Ok(())
    }
}

fn reimbursable(available: u128, deficit: u128, transfer_fee: u128) -> u128 {
    if deficit == 0 || available <= transfer_fee {
        0
    } else {
        deficit.min(available - transfer_fee)
    }
}

pub fn plan_two_year_replenishment(
    captured: u128,
    anchor_target: u128,
    anchor_available: u128,
    transfer_fee: u128,
) -> Result<TwoYearReplenishmentPlan, io_core_model::EconomicsError> {
    use io_core_model::{checked_add, EconomicsError};
    if anchor_available > anchor_target || transfer_fee == 0 {
        return Err(EconomicsError::InsufficientBacking);
    }
    let mut remaining = captured;
    let anchor_reimbursement =
        reimbursable(remaining, anchor_target - anchor_available, transfer_fee);
    let mut anchor_reimbursement_fee = 0;
    if anchor_reimbursement > 0 {
        remaining = remaining
            .checked_sub(checked_add(anchor_reimbursement, transfer_fee)?)
            .ok_or(EconomicsError::InsufficientBacking)?;
        anchor_reimbursement_fee = transfer_fee;
    }
    let ordinary = if remaining > 0 {
        capture_40_60(remaining, transfer_fee, transfer_fee).ok()
    } else {
        None
    };
    let carried = if ordinary.is_some() { 0 } else { remaining };
    Ok(TwoYearReplenishmentPlan {
        captured,
        anchor_reimbursement,
        anchor_reimbursement_fee,
        ordinary,
        carried,
    })
}

pub fn capture_40_60(
    captured: u128,
    permanent_fee: u128,
    claim_fee: u128,
) -> Result<CaptureSplit, io_core_model::EconomicsError> {
    use io_core_model::{checked_add, EconomicsError};
    let split = io_core_model::split_40_60(captured)?;
    let permanent_credit = split
        .permanent
        .checked_sub(permanent_fee)
        .filter(|credit| *credit > 0)
        .ok_or(EconomicsError::PayoutDoesNotCoverFee)?;
    let claim_credit = split
        .claim
        .checked_sub(claim_fee)
        .filter(|credit| *credit > 0)
        .ok_or(EconomicsError::PayoutDoesNotCoverFee)?;
    let accounted = checked_add(
        checked_add(permanent_credit, permanent_fee)?,
        checked_add(claim_credit, claim_fee)?,
    )?;
    if accounted != captured {
        return Err(EconomicsError::ArithmeticOverflow);
    }
    Ok(CaptureSplit {
        captured,
        permanent_gross: split.permanent,
        permanent_credit,
        permanent_fee,
        claim_gross: split.claim,
        claim_credit,
        claim_fee,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum MaturityKind {
    TwoYear,
    TwoWeek,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MaturityIntent {
    pub entitlement_batch_generation: Option<u64>,
    pub two_week_target_e8s: Option<u128>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CanonicalDisbursementEvidence {
    pub amount_disbursed_e8s: u64,
    pub initiated_at_seconds: u64,
    pub scheduled_finalization_timestamp_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PendingMaturityDisbursement {
    pub nominal_disbursed_e8s: u64,
    pub initiated_at_seconds: u64,
    pub scheduled_finalization_timestamp_seconds: u64,
    pub entitlement_batch_generation: Option<u64>,
    pub two_week_target_e8s: Option<u128>,
    pub captured_e8s: Option<u128>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum PermanentCreditState {
    Prepared {
        before: NeuronSnapshot,
        transfer: Box<NnsTransferAttempt>,
    },
    RefreshSubmitted {
        before: NeuronSnapshot,
        transfer_block: u128,
    },
    Proved(PermanentNeuronCreditProof),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NeuronCreditRole {
    AnchorReimbursement,
    OrdinaryPermanent,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MaturityDeliveryOperation {
    pub pending: PendingMaturityDisbursement,
    pub two_year_plan: Option<TwoYearReplenishmentPlan>,
    pub anchor_reimbursement: Option<PermanentCreditState>,
    pub permit: Option<ClaimBackingReceiptPermit>,
    pub permanent_credit: Option<PermanentCreditState>,
    pub claim_transfer: Option<NnsTransferAttempt>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum MaturityCommandPhase {
    Observed(MaturityIntent),
    DisburseMaturitySubmitted {
        intent: MaturityIntent,
        submitted_at_seconds: u64,
    },
    DisburseMaturitySucceeded {
        intent: MaturityIntent,
        submitted_at_seconds: u64,
        amount_disbursed_e8s: u64,
    },
    Delivery(MaturityDeliveryOperation),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MaturityCommandOperation {
    pub operation_sequence: u64,
    pub dispatch_epoch: u64,
    pub kind: MaturityKind,
    pub phase: MaturityCommandPhase,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CompletedMaturity {
    pub kind: MaturityKind,
    pub captured_e8s: u128,
    pub anchor_reimbursement_e8s: u128,
    pub anchor_reimbursement_fee_e8s: u128,
    pub carried_e8s: u128,
    pub permanent_credit_e8s: u128,
    pub claim_credit_e8s: u128,
    pub entitlement_batch_generation: Option<u64>,
    pub two_week_target_e8s: Option<u128>,
    pub completed_at_nanos: u64,
}

impl MaturityCommandOperation {
    pub fn intent(&self) -> MaturityIntent {
        match &self.phase {
            MaturityCommandPhase::Observed(intent) => *intent,
            MaturityCommandPhase::DisburseMaturitySubmitted { intent, .. }
            | MaturityCommandPhase::DisburseMaturitySucceeded { intent, .. } => *intent,
            MaturityCommandPhase::Delivery(value) => value.pending.intent(),
        }
    }

    pub fn validate(&self, next_operation_sequence: u64) -> Result<(), String> {
        if self.operation_sequence == 0 || self.operation_sequence >= next_operation_sequence {
            return Err("maturity command sequence is malformed".into());
        }
        validate_intent(self.kind, &self.intent())?;
        match &self.phase {
            MaturityCommandPhase::Observed(_) => {}
            MaturityCommandPhase::DisburseMaturitySubmitted {
                submitted_at_seconds,
                ..
            } => validate_submission(*submitted_at_seconds)?,
            MaturityCommandPhase::DisburseMaturitySucceeded {
                submitted_at_seconds,
                amount_disbursed_e8s,
                ..
            } => {
                validate_submission(*submitted_at_seconds)?;
                if *amount_disbursed_e8s < MINIMUM_DISBURSEMENT_E8S {
                    return Err("maturity response amount is below the minimum".into());
                }
            }
            MaturityCommandPhase::Delivery(delivery) => {
                delivery.pending.validate(self.kind)?;
                match (self.kind, delivery.two_year_plan) {
                    (MaturityKind::TwoYear, Some(plan))
                        if delivery.pending.captured_e8s == Some(plan.captured) => {}
                    (MaturityKind::TwoWeek, None) => {}
                    _ => return Err("maturity delivery plan is inconsistent with its role".into()),
                }
            }
        }
        Ok(())
    }
}

impl PendingMaturityDisbursement {
    pub fn intent(&self) -> MaturityIntent {
        MaturityIntent {
            entitlement_batch_generation: self.entitlement_batch_generation,
            two_week_target_e8s: self.two_week_target_e8s,
        }
    }

    pub fn validate(&self, expected_kind: MaturityKind) -> Result<(), String> {
        if self.nominal_disbursed_e8s < MINIMUM_DISBURSEMENT_E8S
            || self.initiated_at_seconds == 0
            || self.scheduled_finalization_timestamp_seconds
                < self
                    .initiated_at_seconds
                    .checked_add(DISBURSEMENT_DELAY_SECONDS)
                    .ok_or("maturity finalization overflow")?
            || self.captured_e8s == Some(0)
        {
            return Err("passive maturity disbursement is inconsistent".into());
        }
        validate_intent(expected_kind, &self.intent())
    }
}

fn validate_submission(submitted_at_seconds: u64) -> Result<(), String> {
    if submitted_at_seconds == 0 {
        return Err("maturity submission time is zero".into());
    }
    Ok(())
}

fn validate_intent(kind: MaturityKind, intent: &MaturityIntent) -> Result<(), String> {
    match (
        kind,
        intent.entitlement_batch_generation,
        intent.two_week_target_e8s,
    ) {
        (MaturityKind::TwoYear, None, None) => Ok(()),
        (MaturityKind::TwoWeek, Some(generation), Some(target)) if generation > 0 && target > 0 => {
            Ok(())
        }
        _ => Err("maturity intent is inconsistent with its role".into()),
    }
}

pub fn disburse_percentage() -> u32 {
    100
}

#[cfg(test)]
mod tests {
    #[test]
    fn account_capture_policy_disburses_all_without_staking_maturity() {
        assert_eq!(super::disburse_percentage(), 100);
        assert_eq!(super::MINIMUM_DISBURSEMENT_E8S, 100_000_000);
        assert_eq!(super::DISBURSEMENT_DELAY_SECONDS, 604_800);
    }

    #[test]
    fn both_roles_use_the_same_command() {
        assert_ne!(super::MaturityKind::TwoYear, super::MaturityKind::TwoWeek);
    }

    #[test]
    fn capture_split_debits_exactly_the_frozen_amount() {
        let split = super::capture_40_60(100_000, 10_000, 10_000).unwrap();
        assert_eq!(split.permanent_gross, 40_000);
        assert_eq!(split.permanent_credit, 30_000);
        assert_eq!(split.claim_gross, 60_000);
        assert_eq!(split.claim_credit, 50_000);
        assert_eq!(
            split.permanent_credit + split.permanent_fee + split.claim_credit + split.claim_fee,
            split.captured
        );
        assert!(super::capture_40_60(20_000, 10_000, 10_000).is_err());
    }

    #[test]
    fn maturity_intent_is_only_role_binding() {
        let two_week = super::MaturityIntent {
            entitlement_batch_generation: Some(7),
            two_week_target_e8s: Some(100),
        };
        assert!(super::validate_intent(super::MaturityKind::TwoWeek, &two_week).is_ok());
        assert!(super::validate_intent(super::MaturityKind::TwoYear, &two_week).is_err());
    }

    #[test]
    fn two_year_replenishes_anchor_then_splits_fresh_remainder() {
        let plan =
            super::plan_two_year_replenishment(1_000_000, 1_000_000, 900_000, 10_000).unwrap();
        assert_eq!(plan.anchor_reimbursement, 100_000);
        assert_eq!(plan.anchor_reimbursement_fee, 10_000);
        let ordinary = plan.ordinary.unwrap();
        assert_eq!(ordinary.captured, 890_000);
        assert_eq!(ordinary.permanent_gross, 356_000);
        assert_eq!(ordinary.claim_gross, 534_000);
        assert_eq!(ordinary.permanent_credit, 346_000);
        assert_eq!(ordinary.claim_credit, 524_000);
        assert_eq!(plan.carried, 0);
    }

    #[test]
    fn two_year_partial_and_tiny_replenishment_never_recurses() {
        let partial =
            super::plan_two_year_replenishment(60_000, 1_000_000, 900_000, 10_000).unwrap();
        assert_eq!(partial.anchor_reimbursement, 50_000);
        assert_eq!(partial.anchor_reimbursement_fee, 10_000);
        assert!(partial.ordinary.is_none());
        assert_eq!(partial.carried, 0);

        let tiny = super::plan_two_year_replenishment(10_000, 1_000_000, 900_000, 10_000).unwrap();
        assert_eq!(tiny.anchor_reimbursement, 0);
        assert_eq!(tiny.anchor_reimbursement_fee, 0);
        assert_eq!(tiny.carried, 10_000);
    }

    #[test]
    fn two_year_replenishment_case_table_preserves_priority_and_carry() {
        const TARGET: u128 = 100;
        const FEE: u128 = 10;

        let no_deficits = super::plan_two_year_replenishment(100, TARGET, TARGET, FEE).unwrap();
        assert_eq!(no_deficits.anchor_reimbursement, 0);
        assert_eq!(no_deficits.anchor_reimbursement_fee, 0);
        assert_eq!(no_deficits.ordinary.unwrap().captured, 100);

        let ordinary_too_small =
            super::plan_two_year_replenishment(20, TARGET, TARGET, FEE).unwrap();
        assert!(ordinary_too_small.ordinary.is_none());
        assert_eq!(ordinary_too_small.carried, 20);

        let only_anchor = super::plan_two_year_replenishment(100, TARGET, 60, FEE).unwrap();
        assert_eq!(only_anchor.anchor_reimbursement, 40);
        assert_eq!(only_anchor.anchor_reimbursement_fee, FEE);
        assert_eq!(only_anchor.ordinary.unwrap().captured, 50);

        let exact = super::plan_two_year_replenishment(30, TARGET, 80, FEE).unwrap();
        assert_eq!(exact.anchor_reimbursement, 20);
        assert_eq!(exact.anchor_reimbursement_fee, FEE);
        assert!(exact.ordinary.is_none());
        assert_eq!(exact.carried, 0);

        let partial_anchor = super::plan_two_year_replenishment(25, TARGET, 80, FEE).unwrap();
        assert_eq!(partial_anchor.anchor_reimbursement, 15);
        assert_eq!(partial_anchor.anchor_reimbursement_fee, FEE);
        assert_eq!(partial_anchor.carried, 0);

        let full_plus_split = super::plan_two_year_replenishment(100, TARGET, 80, FEE).unwrap();
        assert_eq!(full_plus_split.anchor_reimbursement, 20);
        assert_eq!(full_plus_split.anchor_reimbursement_fee, FEE);
        assert_eq!(full_plus_split.ordinary.unwrap().captured, 70);

        let unusable = super::plan_two_year_replenishment(FEE, TARGET, 80, FEE).unwrap();
        assert_eq!(unusable.anchor_reimbursement, 0);
        assert_eq!(unusable.anchor_reimbursement_fee, 0);
        assert_eq!(unusable.carried, FEE);

        for plan in [
            no_deficits,
            ordinary_too_small,
            only_anchor,
            exact,
            partial_anchor,
            full_plus_split,
            unusable,
        ] {
            plan.validate(FEE).unwrap();
        }
    }
}
