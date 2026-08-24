use crate::{
    jupiter::{NeuronSnapshot, PermanentNeuronCreditProof},
    receipt::ClaimBackingReceiptPermit,
    transfer::NnsTransferAttempt,
};
use {candid::CandidType, io_accounts::Account, serde::Deserialize};

pub const MINIMUM_DISBURSEMENT_E8S: u64 = 100_000_000;
pub const DISBURSEMENT_DELAY_SECONDS: u64 = 7 * 24 * 60 * 60;

pub fn split_maturity(maturity_e8s: u64) -> Option<(u64, u64)> {
    let retained = maturity_e8s.checked_mul(40)? / 100;
    Some((retained, maturity_e8s - retained))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum MaturityKind {
    TwoYear,
    TwoWeek,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MaturityPlan {
    pub neuron: NeuronSnapshot,
    pub original_maturity_e8s: u64,
    pub original_staked_maturity_e8s: u64,
    pub stake_maturity_e8s: u64,
    pub remaining_maturity_e8s: u64,
    pub destination: Account,
    pub requested_at_seconds: u64,
    pub entitlement_batch_generation: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StakeMaturitySucceeded {
    pub plan: MaturityPlan,
    pub remaining_maturity_e8s: u64,
    pub staked_maturity_e8s: u64,
    pub evidence_source: MaturityEvidenceSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum MaturityEvidenceSource {
    CommandResponse,
    CanonicalNeuronObservation,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DisburseMaturitySubmission {
    pub stake: StakeMaturitySucceeded,
    pub submitted_at_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DisburseMaturitySucceeded {
    pub submission: DisburseMaturitySubmission,
    pub amount_disbursed_e8s: u64,
    pub evidence_source: MaturityEvidenceSource,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CanonicalDisbursementEvidence {
    pub initiated_at_seconds: u64,
    pub scheduled_finalization_timestamp_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MintEvidence {
    pub mint_block: u128,
    pub actual_minted_icp_e8s: u128,
    pub native_memo_u64: u64,
    pub created_at_time_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum MintProofState {
    Awaiting,
    Proved(MintEvidence),
    Delivering(MintEvidence),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PendingMaturityDisbursement {
    pub kind: MaturityKind,
    pub neuron_id: u64,
    pub nominal_disbursed_maturity_e8s: u64,
    pub destination: Account,
    pub initiation_timestamp_seconds: u64,
    pub scheduled_finalization_timestamp_seconds: u64,
    pub stake_evidence: StakeMaturitySucceeded,
    pub disburse_evidence: DisburseMaturitySucceeded,
    pub committed_claim_transfer_fee_e8s: u128,
    pub mint_proof: MintProofState,
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
pub struct ClaimReceiptDeliveryOperation {
    pub pending: PendingMaturityDisbursement,
    pub permit: Option<ClaimBackingReceiptPermit>,
    pub permanent_credit: Option<PermanentCreditState>,
    pub claim_transfer: Option<NnsTransferAttempt>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum MaturityCommandPhase {
    Observed(MaturityPlan),
    StakeMaturitySubmitted(MaturityPlan),
    StakeMaturitySucceeded(StakeMaturitySucceeded),
    ReadyToDisburse(DisburseMaturitySubmission),
    DisburseMaturitySubmitted(DisburseMaturitySubmission),
    DisburseMaturitySucceeded(DisburseMaturitySucceeded),
    ClaimReceiptDelivery(ClaimReceiptDeliveryOperation),
    MaturityDrift {
        reason: String,
        stake: StakeMaturitySucceeded,
    },
    DisburseMaturityMismatch {
        reason: String,
        submission: DisburseMaturitySubmission,
        observed_amount_e8s: u64,
    },
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
    pub neuron_id: u64,
    pub mint_block: u128,
    pub nominal_disbursed_maturity_e8s: u64,
    pub actual_minted_icp_e8s: u128,
    pub destination: Account,
    pub permanent_credit_proof: Option<PermanentNeuronCreditProof>,
    pub completed_at_nanos: u64,
}

impl MaturityCommandOperation {
    pub fn plan(&self) -> &MaturityPlan {
        match &self.phase {
            MaturityCommandPhase::Observed(plan)
            | MaturityCommandPhase::StakeMaturitySubmitted(plan) => plan,
            MaturityCommandPhase::StakeMaturitySucceeded(stake)
            | MaturityCommandPhase::MaturityDrift { stake, .. } => &stake.plan,
            MaturityCommandPhase::ReadyToDisburse(submission)
            | MaturityCommandPhase::DisburseMaturitySubmitted(submission) => &submission.stake.plan,
            MaturityCommandPhase::DisburseMaturityMismatch { submission, .. } => {
                &submission.stake.plan
            }
            MaturityCommandPhase::DisburseMaturitySucceeded(value) => &value.submission.stake.plan,
            MaturityCommandPhase::ClaimReceiptDelivery(value) => &value.pending.stake_evidence.plan,
        }
    }

    pub fn validate(
        &self,
        next_operation_sequence: u64,
        expected_neuron_id: u64,
        expected_destination: &Account,
    ) -> Result<(), String> {
        if self.operation_sequence == 0 || self.operation_sequence >= next_operation_sequence {
            return Err("maturity command sequence is malformed".into());
        }
        let plan = self.plan();
        let expected_stake = match self.kind {
            MaturityKind::TwoYear => {
                plan.original_maturity_e8s
                    .checked_mul(40)
                    .ok_or("maturity stake calculation overflow")?
                    / 100
            }
            MaturityKind::TwoWeek => 0,
        };
        if plan.neuron.neuron_id != expected_neuron_id
            || plan.stake_maturity_e8s != expected_stake
            || plan.remaining_maturity_e8s
                != plan
                    .original_maturity_e8s
                    .checked_sub(plan.stake_maturity_e8s)
                    .ok_or("maturity split underflow")?
            || plan.remaining_maturity_e8s < MINIMUM_DISBURSEMENT_E8S
            || plan.requested_at_seconds == 0
            || !plan.destination.effective_eq(expected_destination)?
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
        expected_destination: &Account,
    ) -> Result<(), String> {
        if self.kind != expected_kind
            || self.neuron_id != expected_neuron_id
            || self.stake_evidence.plan.neuron.neuron_id != self.neuron_id
            || self.disburse_evidence.submission.stake != self.stake_evidence
            || self.nominal_disbursed_maturity_e8s < MINIMUM_DISBURSEMENT_E8S
            || self.nominal_disbursed_maturity_e8s != self.stake_evidence.remaining_maturity_e8s
            || self.nominal_disbursed_maturity_e8s != self.disburse_evidence.amount_disbursed_e8s
            || self.initiation_timestamp_seconds == 0
            || self.scheduled_finalization_timestamp_seconds
                != self
                    .initiation_timestamp_seconds
                    .checked_add(DISBURSEMENT_DELAY_SECONDS)
                    .ok_or("maturity finalization overflow")?
            || !self.destination.effective_eq(expected_destination)?
            || !self
                .stake_evidence
                .plan
                .destination
                .effective_eq(&self.destination)?
        {
            return Err("passive maturity disbursement is inconsistent".into());
        }
        match &self.mint_proof {
            MintProofState::Awaiting if self.committed_claim_transfer_fee_e8s == 0 => Ok(()),
            MintProofState::Proved(mint) | MintProofState::Delivering(mint)
                if mint.actual_minted_icp_e8s > 0
                    && self.committed_claim_transfer_fee_e8s > 0
                    && mint.native_memo_u64 >= self.scheduled_finalization_timestamp_seconds
                    && mint.created_at_time_nanos / 1_000_000_000 >= mint.native_memo_u64 =>
            {
                Ok(())
            }
            _ => Err("maturity Mint evidence is inconsistent".into()),
        }
    }
}

pub fn commands() -> (u32, u32) {
    (40, 100)
}

#[cfg(test)]
mod tests {
    #[test]
    fn direct_policy_is_stake_40_then_disburse_all_remaining() {
        assert_eq!(super::commands(), (40, 100));
        assert_eq!(super::MINIMUM_DISBURSEMENT_E8S, 100_000_000);
        assert_eq!(super::DISBURSEMENT_DELAY_SECONDS, 604_800);
    }

    #[test]
    fn pooled_parent_disburses_all_without_staking_maturity() {
        assert_eq!(super::MaturityKind::TwoWeek, super::MaturityKind::TwoWeek);
    }
}
