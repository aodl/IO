use crate::{
    jupiter::{NeuronSnapshot, PermanentNeuronCreditProof},
    receipt::ClaimBackingReceiptPermit,
    transfer::NnsTransferAttempt,
};
use {candid::CandidType, serde::Deserialize};

pub const MINIMUM_DISBURSEMENT_E8S: u64 = 100_000_000;
pub const DISBURSEMENT_DELAY_SECONDS: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureSplit {
    pub captured: u128,
    pub permanent_gross: u128,
    pub permanent_credit: u128,
    pub permanent_fee: u128,
    pub claim_gross: u128,
    pub claim_credit: u128,
    pub claim_fee: u128,
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

pub fn captured_balance(before: u128, current: u128) -> Option<u128> {
    current.checked_sub(before).filter(|captured| *captured > 0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum MaturityKind {
    TwoYear,
    TwoWeek,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MaturityPlan {
    pub neuron: NeuronSnapshot,
    pub observed_maturity_e8s: u64,
    pub staging_balance_before_e8s: u128,
    pub requested_at_seconds: u64,
    pub entitlement_batch_generation: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DisburseMaturitySubmission {
    pub plan: MaturityPlan,
    pub submitted_at_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DisburseMaturitySucceeded {
    pub submission: DisburseMaturitySubmission,
    pub amount_disbursed_e8s: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CanonicalDisbursementEvidence {
    pub amount_disbursed_e8s: u64,
    pub initiated_at_seconds: u64,
    pub scheduled_finalization_timestamp_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PendingMaturityDisbursement {
    pub kind: MaturityKind,
    pub scheduled_finalization_timestamp_seconds: u64,
    pub disburse_evidence: DisburseMaturitySucceeded,
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

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MaturityDeliveryOperation {
    pub pending: PendingMaturityDisbursement,
    pub permit: Option<ClaimBackingReceiptPermit>,
    pub permanent_credit: Option<PermanentCreditState>,
    pub claim_transfer: Option<NnsTransferAttempt>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum MaturityCommandPhase {
    Observed(MaturityPlan),
    DisburseMaturitySubmitted(DisburseMaturitySubmission),
    DisburseMaturitySucceeded(DisburseMaturitySucceeded),
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
    pub permanent_credit_e8s: u128,
    pub claim_credit_e8s: u128,
    pub entitlement_batch_generation: Option<u64>,
    pub completed_at_nanos: u64,
}

impl MaturityCommandOperation {
    pub fn plan(&self) -> &MaturityPlan {
        match &self.phase {
            MaturityCommandPhase::Observed(plan) => plan,
            MaturityCommandPhase::DisburseMaturitySubmitted(submission) => &submission.plan,
            MaturityCommandPhase::DisburseMaturitySucceeded(value) => &value.submission.plan,
            MaturityCommandPhase::Delivery(value) => {
                &value.pending.disburse_evidence.submission.plan
            }
        }
    }

    pub fn validate(
        &self,
        next_operation_sequence: u64,
        expected_neuron_id: u64,
    ) -> Result<(), String> {
        if self.operation_sequence == 0 || self.operation_sequence >= next_operation_sequence {
            return Err("maturity command sequence is malformed".into());
        }
        let plan = self.plan();
        if plan.neuron.neuron_id != expected_neuron_id
            || plan.observed_maturity_e8s < MINIMUM_DISBURSEMENT_E8S
            || plan.requested_at_seconds == 0
            || (self.kind == MaturityKind::TwoWeek) != plan.entitlement_batch_generation.is_some()
        {
            return Err("maturity command plan is inconsistent".into());
        }
        Ok(())
    }
}

impl PendingMaturityDisbursement {
    pub fn validate(
        &self,
        expected_kind: MaturityKind,
        expected_neuron_id: u64,
    ) -> Result<(), String> {
        let plan = &self.disburse_evidence.submission.plan;
        if self.kind != expected_kind
            || plan.neuron.neuron_id != expected_neuron_id
            || self.disburse_evidence.amount_disbursed_e8s < MINIMUM_DISBURSEMENT_E8S
            || self.scheduled_finalization_timestamp_seconds
                < self
                    .disburse_evidence
                    .submission
                    .submitted_at_seconds
                    .checked_add(DISBURSEMENT_DELAY_SECONDS)
                    .ok_or("maturity finalization overflow")?
            || self.captured_e8s == Some(0)
        {
            return Err("passive maturity disbursement is inconsistent".into());
        }
        Ok(())
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
    fn two_week_and_two_year_donations_follow_only_their_own_account() {
        let two_week = super::captured_balance(10, 10 + 60 + 7);
        let two_year = super::captured_balance(20, 20 + 40 + 3);
        assert_eq!(two_week, Some(67));
        assert_eq!(two_year, Some(43));
        assert_ne!(two_week, two_year);
        assert_eq!(super::captured_balance(10, 10), None);
        assert_eq!(super::captured_balance(11, 10), None);
    }
}
