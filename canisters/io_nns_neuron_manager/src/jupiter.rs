use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::{state::Account, transfer::NnsTransferAttempt};

pub const PINNED_DFINITY_IC_COMMIT: &str = "0c7c8b83144844e1a598633585b3ee1beebe338b";

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct JupiterDeposit {
    pub block_index: u128,
    pub gross_e8s: u128,
    pub stake_e8s: u128,
    pub liquid_e8s: u128,
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
pub struct StakeIncreaseProof {
    pub before: NeuronSnapshot,
    pub after_cached_stake_e8s: u128,
    pub stake_transfer_block: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StreamReceiptPermit {
    pub sequence: u64,
    pub destination: Account,
    pub memo: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LiquidTransferSucceeded {
    pub proof: StakeIncreaseProof,
    pub permit: StreamReceiptPermit,
    pub block_index: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct JupiterCompleted {
    pub deposit_block: u128,
    pub gross_e8s: u128,
    pub stake_e8s: u128,
    pub liquid_e8s: u128,
    pub stake_transfer_block: u128,
    pub liquid_transfer_block: u128,
    pub stream_receipt_sequence: u64,
    pub completed_at_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum JupiterStuckTransfer {
    Stake {
        before: NeuronSnapshot,
        attempt: NnsTransferAttempt,
    },
    Liquid {
        proof: StakeIncreaseProof,
        permit: StreamReceiptPermit,
        attempt: NnsTransferAttempt,
    },
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
    StakeIncreaseProved(StakeIncreaseProof),
    ReceiptPermitPrepared {
        proof: StakeIncreaseProof,
        permit: StreamReceiptPermit,
    },
    LiquidTransferPrepared {
        proof: StakeIncreaseProof,
        permit: StreamReceiptPermit,
        attempt: NnsTransferAttempt,
    },
    LiquidTransferSubmitted {
        proof: StakeIncreaseProof,
        permit: StreamReceiptPermit,
        attempt: NnsTransferAttempt,
    },
    LiquidTransferSucceeded(LiquidTransferSucceeded),
    ReceiptCompletionSubmitted(LiquidTransferSucceeded),
    AwaitingStreamSettlement(LiquidTransferSucceeded),
    Stuck {
        reason: String,
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
            || checked_split(self.deposit.gross_e8s)?
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
                validate_neuron(&proof.before)?;
                validate_stake_increase(proof, self.deposit.stake_e8s)
            }
            JupiterPhase::ReceiptPermitPrepared { proof, permit }
            | JupiterPhase::LiquidTransferPrepared { proof, permit, .. }
            | JupiterPhase::LiquidTransferSubmitted { proof, permit, .. } => {
                validate_neuron(&proof.before)?;
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
                validate_neuron(&value.proof.before)?;
                validate_stake_increase(&value.proof, self.deposit.stake_e8s)?;
                validate_permit(&value.permit)
            }
            JupiterPhase::Stuck { reason, transfer } => {
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
                            validate_neuron(&proof.before)?;
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

fn validate_stake_increase(proof: &StakeIncreaseProof, stake_e8s: u128) -> Result<(), String> {
    if proof
        .after_cached_stake_e8s
        .checked_sub(proof.before.cached_stake_e8s)
        != Some(stake_e8s)
    {
        return Err("protected neuron stake increase is not exact".into());
    }
    Ok(())
}

fn validate_permit(permit: &StreamReceiptPermit) -> Result<(), String> {
    if permit.memo.is_empty() || permit.memo.len() > 32 {
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
}
