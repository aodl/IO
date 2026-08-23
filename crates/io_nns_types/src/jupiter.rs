use ic_stable_structures::{storable::Bound, Storable};
use {
    candid::{CandidType, Principal},
    serde::Deserialize,
    std::borrow::Cow,
};

use crate::transfer::NnsTransferAttempt;
pub use io_receipt_types::ClaimBackingReceiptPermit as StreamReceiptPermit;

pub const PINNED_NNS_GOVERNANCE_COMMIT: &str = "8aa4680e378f3248e7e7b9b8237915aded999bd9";
pub const PINNED_ICP_LEDGER_COMMIT: &str = "021bf342f66296d5605b355a61b2430406a83783";

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct JupiterDeposit {
    pub block_index: u128,
    pub gross_e8s: u128,
    pub stake_e8s: u128,
    pub liquid_e8s: u128,
    pub fee_e8s: u128,
    pub created_at_time_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NeuronSnapshot {
    pub neuron_id: u64,
    pub staking_subaccount: [u8; 32],
    pub cached_stake_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StakeTransferSucceeded {
    pub before: NeuronSnapshot,
    pub block_index: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PermanentNeuronCreditProof {
    pub neuron_id: u64,
    pub staking_subaccount: [u8; 32],
    pub before_cached_stake_e8s: u128,
    pub protocol_credit_e8s: u128,
    pub transfer_block: u128,
    pub observed_after_cached_stake_e8s: u128,
}

impl PermanentNeuronCreditProof {
    pub fn before(&self) -> NeuronSnapshot {
        NeuronSnapshot {
            neuron_id: self.neuron_id,
            staking_subaccount: self.staking_subaccount,
            cached_stake_e8s: self.before_cached_stake_e8s,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let expected_after = self
            .before_cached_stake_e8s
            .checked_add(self.protocol_credit_e8s)
            .ok_or("permanent credit expectation overflow")?;
        if self.neuron_id == 0
            || self.protocol_credit_e8s == 0
            || self.observed_after_cached_stake_e8s < expected_after
        {
            return Err("permanent neuron protocol credit is not proved".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LiquidTransferSucceeded {
    pub proof: PermanentNeuronCreditProof,
    pub permit: StreamReceiptPermit,
    pub block_index: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct JupiterCompleted {
    pub deposit_block: u128,
    pub gross_e8s: u128,
    pub stake_e8s: u128,
    pub observed_after_cached_stake_e8s: u128,
    pub liquid_e8s: u128,
    pub stake_transfer_block: u128,
    pub liquid_transfer_block: u128,
    pub stream_receipt_sequence: u64,
    pub backed_io_e8s: u128,
    pub io_transfer_block: u128,
    pub io_fee_e8s: u128,
    pub stream_receipt_fingerprint: Vec<u8>,
    pub completed_at_nanos: u64,
}

impl Storable for JupiterCompleted {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(candid::encode_one(self).expect("Jupiter result must encode"))
    }
    fn into_bytes(self) -> Vec<u8> {
        candid::encode_one(self).expect("Jupiter result must encode")
    }
    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        candid::decode_one(bytes.as_ref()).expect("Jupiter result must decode")
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: 512,
        is_fixed_size: false,
    };
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum JupiterStuckTransfer {
    Stake {
        before: NeuronSnapshot,
        attempt: NnsTransferAttempt,
    },
    Liquid {
        proof: PermanentNeuronCreditProof,
        permit: StreamReceiptPermit,
        attempt: NnsTransferAttempt,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum JupiterPauseReason {
    AmbiguousPossibleEffect,
    InsufficientFunds,
    BadFee,
    RefreshUnconfirmed,
    StakeIncreaseMismatch,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum JupiterPhase {
    DepositProved,
    StakeTransferPrepared {
        before: NeuronSnapshot,
        attempt: NnsTransferAttempt,
    },
    StakeTransferSubmitted {
        before: NeuronSnapshot,
        attempt: NnsTransferAttempt,
    },
    StakeTransferSucceeded(StakeTransferSucceeded),
    RefreshSubmitted(StakeTransferSucceeded),
    StakeIncreaseProved(PermanentNeuronCreditProof),
    ReceiptPermitPrepared {
        proof: PermanentNeuronCreditProof,
        permit: StreamReceiptPermit,
    },
    LiquidTransferPrepared {
        proof: PermanentNeuronCreditProof,
        permit: StreamReceiptPermit,
        attempt: NnsTransferAttempt,
    },
    LiquidTransferSubmitted {
        proof: PermanentNeuronCreditProof,
        permit: StreamReceiptPermit,
        attempt: NnsTransferAttempt,
    },
    LiquidTransferSucceeded(LiquidTransferSucceeded),
    ReceiptCompletionSubmitted(LiquidTransferSucceeded),
    AwaitingStreamSettlement(LiquidTransferSucceeded),
    Stuck {
        reason: String,
        pause_reason: JupiterPauseReason,
        transfer: Option<JupiterStuckTransfer>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct JupiterOperation {
    pub operation_sequence: u64,
    pub dispatch_epoch: u64,
    pub captured_control_epoch: u64,
    pub deposit: JupiterDeposit,
    pub phase: JupiterPhase,
}

impl JupiterOperation {
    pub fn validate(&self, icp_ledger: Principal, nns_governance: Principal) -> Result<(), String> {
        if self.operation_sequence == 0
            || self.deposit.block_index > u128::from(u64::MAX)
            || self.deposit.gross_e8s == 0
            || self.deposit.created_at_time_nanos == 0
            || fee_reduced_split(self.deposit.gross_e8s, self.deposit.fee_e8s)?
                != (self.deposit.stake_e8s, self.deposit.liquid_e8s)
        {
            return Err("Jupiter operation identity is malformed".into());
        }
        let validate_neuron = |neuron: &NeuronSnapshot| {
            if neuron.neuron_id == 0 || neuron.cached_stake_e8s == 0 {
                return Err("protected neuron snapshot is malformed".into());
            }
            Ok(())
        };
        let validate_attempt = |attempt: &NnsTransferAttempt| -> Result<(), String> {
            attempt.validate()?;
            if attempt.intent.ledger != icp_ledger {
                return Err("Jupiter transfer uses the wrong ledger".into());
            }
            Ok(())
        };
        match &self.phase {
            JupiterPhase::DepositProved => Ok(()),
            JupiterPhase::StakeTransferPrepared { before, attempt }
            | JupiterPhase::StakeTransferSubmitted { before, attempt } => {
                validate_neuron(before)?;
                validate_attempt(attempt)?;
                if attempt.intent.destination.owner != nns_governance
                    || attempt.intent.destination.canonical()?.subaccount
                        != before.staking_subaccount
                    || attempt.intent.amount_e8s != self.deposit.stake_e8s
                {
                    return Err("Jupiter stake transfer is inconsistent".into());
                }
                Ok(())
            }
            JupiterPhase::StakeTransferSucceeded(value) | JupiterPhase::RefreshSubmitted(value) => {
                validate_neuron(&value.before)
            }
            JupiterPhase::StakeIncreaseProved(proof) => {
                validate_neuron(&proof.before())?;
                validate_stake_increase(proof, self.deposit.stake_e8s)
            }
            JupiterPhase::ReceiptPermitPrepared { proof, permit }
            | JupiterPhase::LiquidTransferPrepared { proof, permit, .. }
            | JupiterPhase::LiquidTransferSubmitted { proof, permit, .. } => {
                validate_neuron(&proof.before())?;
                validate_stake_increase(proof, self.deposit.stake_e8s)?;
                validate_permit(permit)?;
                if let JupiterPhase::LiquidTransferPrepared { attempt, .. }
                | JupiterPhase::LiquidTransferSubmitted { attempt, .. } = &self.phase
                {
                    validate_attempt(attempt)?;
                    if !attempt
                        .intent
                        .destination
                        .effective_eq(&permit.destination)?
                        || attempt.intent.amount_e8s != self.deposit.liquid_e8s
                    {
                        return Err("Jupiter liquid transfer is inconsistent".into());
                    }
                }
                Ok(())
            }
            JupiterPhase::LiquidTransferSucceeded(value)
            | JupiterPhase::ReceiptCompletionSubmitted(value)
            | JupiterPhase::AwaitingStreamSettlement(value) => {
                validate_neuron(&value.proof.before())?;
                validate_stake_increase(&value.proof, self.deposit.stake_e8s)?;
                validate_permit(&value.permit)
            }
            JupiterPhase::Stuck {
                reason, transfer, ..
            } => {
                if reason.is_empty() || reason.len() > 512 {
                    return Err("Jupiter Stuck reason is malformed".into());
                }
                if let Some(transfer) = transfer {
                    match transfer {
                        JupiterStuckTransfer::Stake { before, attempt } => {
                            validate_neuron(before)?;
                            validate_attempt(attempt)?;
                        }
                        JupiterStuckTransfer::Liquid {
                            proof,
                            permit,
                            attempt,
                        } => {
                            validate_neuron(&proof.before())?;
                            validate_stake_increase(proof, self.deposit.stake_e8s)?;
                            validate_permit(permit)?;
                            validate_attempt(attempt)?;
                        }
                    }
                }
                Ok(())
            }
        }
    }
}

fn validate_stake_increase(
    proof: &PermanentNeuronCreditProof,
    stake_e8s: u128,
) -> Result<(), String> {
    proof.validate()?;
    if proof.protocol_credit_e8s != stake_e8s {
        return Err("Jupiter proof attributes the wrong protocol credit".into());
    }
    Ok(())
}

fn validate_permit(permit: &StreamReceiptPermit) -> Result<(), String> {
    if permit.stream_operation_sequence == 0
        || permit.amount_e8s == 0
        || permit.memo.len() != 32
        || permit.request_fingerprint.len() != 32
    {
        return Err("stream receipt permit memo is malformed".into());
    }
    permit.destination.validate()
}

pub fn checked_split(gross_e8s: u128) -> Result<(u128, u128), String> {
    let stake = gross_e8s.checked_mul(40).ok_or("Jupiter split overflow")? / 100;
    let liquid = gross_e8s
        .checked_sub(stake)
        .ok_or("Jupiter split underflow")?;
    if stake == 0 || liquid == 0 {
        return Err("Jupiter deposit is too small for a nonzero 40/60 split".into());
    }
    Ok((stake, liquid))
}

pub fn fee_reduced_split(gross_e8s: u128, fee_e8s: u128) -> Result<(u128, u128), String> {
    let (permanent_gross, claim_gross) = checked_split(gross_e8s)?;
    let permanent_credit = permanent_gross
        .checked_sub(fee_e8s)
        .ok_or("Jupiter permanent gross cannot cover its fee")?;
    let claim_credit = claim_gross
        .checked_sub(fee_e8s)
        .ok_or("Jupiter claim gross cannot cover its fee")?;
    if permanent_credit == 0 || claim_credit == 0 {
        return Err("Jupiter destination credit must be positive".into());
    }
    Ok((permanent_credit, claim_credit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_checked_40_60_split() {
        assert_eq!(checked_split(101).unwrap(), (40, 61));
        assert!(checked_split(1).is_err());
        assert_eq!(
            checked_split(u128::MAX),
            Err("Jupiter split overflow".into())
        );
    }

    #[test]
    fn internal_fees_reduce_each_physical_credit() {
        assert_eq!(fee_reduced_split(100_000, 10_000), Ok((30_000, 50_000)));
        assert!(fee_reduced_split(20_000, 10_000).is_err());
    }

    #[test]
    fn permanent_credit_proof_is_monotone_and_does_not_attribute_donations() {
        let proof = PermanentNeuronCreditProof {
            neuron_id: 7,
            staking_subaccount: [8; 32],
            before_cached_stake_e8s: 1_000,
            protocol_credit_e8s: 400,
            transfer_block: 9,
            observed_after_cached_stake_e8s: 1_500,
        };
        proof.validate().unwrap();
        assert_eq!(proof.protocol_credit_e8s, 400);
        assert_eq!(proof.observed_after_cached_stake_e8s, 1_500);

        let mut pending = proof;
        pending.observed_after_cached_stake_e8s = 1_399;
        assert!(pending.validate().is_err());
    }
}
