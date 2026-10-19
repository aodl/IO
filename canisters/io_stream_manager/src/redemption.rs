use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::{
    state::{Account, OperationSequence, StreamConfig},
    transfer::{deterministic_memo, OwnTransferIntent, TransferAttempt, TransferState},
};

pub const AUTOMATIC_POLL_MIN_SECONDS: u64 = 10;
pub const AUTOMATIC_POLL_MAX_SECONDS: u64 = 3_600;
pub const MAX_INDEX_TRANSACTIONS_PER_PAGE: usize = 32;
pub const MAX_PENDING_CANDIDATES: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ClaimSnapshot {
    pub total_supply_e8s: u128,
    pub reserve_io_e8s: u128,
    pub excluded_io_balances: Vec<(Account, u128)>,
    pub claim_supply_e8s: u128,
    pub liquid_icp_e8s: u128,
    pub pooled_principal_e8s: u128,
    pub unwinding_net_backing_e8s: u128,
    pub transit_backing_e8s: u128,
    pub total_claim_backing_e8s: u128,
    pub nns_control_epoch: u64,
    pub nns_operation_sequence: u64,
    pub last_completed_pool_operation_sequence: Option<u64>,
    pub nns_fingerprint: Vec<u8>,
    pub pool_staking_account: Account,
    pub anchor_target_e8s: u128,
    pub anchor_available_e8s: u128,
    pub excluded_dynamic_surplus_e8s: u128,
    pub stream_control_epoch: u64,
    pub observation_fingerprint: Vec<u8>,
    pub io_fee_e8s: u128,
    pub icp_fee_e8s: u128,
}

impl Default for ClaimSnapshot {
    fn default() -> Self {
        let account = Account {
            owner: Principal::anonymous(),
            subaccount: None,
        };
        Self {
            total_supply_e8s: 0,
            reserve_io_e8s: 0,
            excluded_io_balances: Vec::new(),
            claim_supply_e8s: 0,
            liquid_icp_e8s: 0,
            pooled_principal_e8s: 0,
            unwinding_net_backing_e8s: 0,
            transit_backing_e8s: 0,
            total_claim_backing_e8s: 0,
            nns_control_epoch: 0,
            nns_operation_sequence: 0,
            last_completed_pool_operation_sequence: None,
            nns_fingerprint: Vec::new(),
            pool_staking_account: account,
            anchor_target_e8s: 0,
            anchor_available_e8s: 0,
            excluded_dynamic_surplus_e8s: 0,
            stream_control_epoch: 0,
            observation_fingerprint: Vec::new(),
            io_fee_e8s: 0,
            icp_fee_e8s: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StructuralStakeObservation {
    pub sns_neuron_id: Vec<u8>,
    pub staking_account: Account,
    pub state: crate::state::StructuralStakeState,
    pub ledger_balance_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RedemptionStage {
    Payout(TransferAttempt),
    Sweep {
        payout_block: u128,
        attempt: TransferAttempt,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RedemptionOperation {
    pub sequence: OperationSequence,
    pub source_io_block: u128,
    pub source_account: Account,
    pub staged_io_amount_e8s: u128,
    pub gross_icp_e8s: u128,
    pub net_icp_e8s: u128,
    pub icp_fee_e8s: u128,
    pub io_sweep_fee_e8s: u128,
    pub stage: RedemptionStage,
}

impl RedemptionOperation {
    pub fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        self.source_account.validate()?;
        if self.source_io_block > u128::from(u64::MAX)
            || self.source_account.owner == Principal::anonymous()
            || self.staged_io_amount_e8s < config.minimum_redemption_io_e8s
            || self.staged_io_amount_e8s <= self.io_sweep_fee_e8s
            || self.icp_fee_e8s != config.expected_icp_fee_e8s
            || self.io_sweep_fee_e8s != config.expected_io_fee_e8s
            || self.gross_icp_e8s.checked_sub(self.icp_fee_e8s) != Some(self.net_icp_e8s)
            || self.net_icp_e8s == 0
        {
            return Err("redemption economics are inconsistent".into());
        }
        match &self.stage {
            RedemptionStage::Payout(attempt) => {
                validate_attempt_state(attempt)?;
                validate_payout(self, config, attempt)
            }
            RedemptionStage::Sweep {
                payout_block,
                attempt,
            } => {
                if *payout_block == 0 {
                    return Err("redemption payout proof block is missing".into());
                }
                validate_attempt_state(attempt)?;
                validate_sweep(self, config, attempt)
            }
        }
    }

    pub fn active_attempt(&self) -> &TransferAttempt {
        match &self.stage {
            RedemptionStage::Payout(attempt) | RedemptionStage::Sweep { attempt, .. } => attempt,
        }
    }

    pub fn is_stuck(&self) -> bool {
        matches!(self.active_attempt().state, TransferState::Stuck { .. })
    }
}

fn validate_attempt_state(attempt: &TransferAttempt) -> Result<(), String> {
    attempt.validate()?;
    if matches!(
        attempt.state,
        TransferState::Prepared | TransferState::Succeeded { .. }
    ) {
        return Err("active redemption transfer must be submitted or stuck".into());
    }
    Ok(())
}

fn validate_payout(
    operation: &RedemptionOperation,
    config: &StreamConfig,
    attempt: &TransferAttempt,
) -> Result<(), String> {
    let OwnTransferIntent::Icrc1 {
        ledger,
        from_subaccount,
        to,
        amount,
        fee,
        memo,
        ..
    } = &attempt.intent;
    if *ledger != config.icp_ledger
        || *from_subaccount != config.liquid_icp.canonical()?.subaccount
        || !to.effective_eq(&operation.source_account)?
        || *amount != operation.net_icp_e8s
        || *fee != operation.icp_fee_e8s
        || *memo
            != deterministic_memo(
                b"io-redemption-pay-v2",
                Principal::from_slice(&operation.source_io_block.to_be_bytes()),
                operation.sequence.0,
            )
    {
        return Err("ICP payout intent does not match staged redemption".into());
    }
    Ok(())
}

fn validate_sweep(
    operation: &RedemptionOperation,
    config: &StreamConfig,
    attempt: &TransferAttempt,
) -> Result<(), String> {
    let OwnTransferIntent::Icrc1 {
        ledger,
        from_subaccount,
        to,
        amount,
        fee,
        memo,
        ..
    } = &attempt.intent;
    if *ledger != config.io_ledger
        || *from_subaccount != io_accounts::REDEMPTION_STAGING_SUBACCOUNT
        || !to.effective_eq(&config.io_reserve)?
        || amount.checked_add(*fee) != Some(operation.staged_io_amount_e8s)
        || *fee != operation.io_sweep_fee_e8s
        || *memo
            != deterministic_memo(
                b"io-redemption-sweep-v1",
                Principal::from_slice(&operation.source_io_block.to_be_bytes()),
                operation.sequence.0,
            )
    {
        return Err("reserve sweep intent does not match staged redemption".into());
    }
    Ok(())
}

pub fn quote_for_amount(
    amount_e8s: u128,
    snapshot: &ClaimSnapshot,
) -> Result<io_core_model::RedemptionQuote, String> {
    if snapshot.claim_supply_e8s == 0
        || snapshot.total_claim_backing_e8s < snapshot.claim_supply_e8s
    {
        return Err("canonical claim snapshot is not redemption-ready".into());
    }
    io_core_model::redemption_quote(
        io_core_model::EconomicState {
            backing: io_core_model::Backing {
                liquid: snapshot.liquid_icp_e8s,
                pooled: snapshot.pooled_principal_e8s,
                unwinding: snapshot.unwinding_net_backing_e8s,
                transit: snapshot.transit_backing_e8s,
            },
            claims: snapshot.claim_supply_e8s,
            active_backing: 0,
            active_reward: 0,
        },
        amount_e8s,
        snapshot.icp_fee_e8s,
    )
    .map_err(|error| format!("redemption quote failed: {error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_uses_current_total_backing_and_claim_supply() {
        let snapshot = ClaimSnapshot {
            claim_supply_e8s: 1_000,
            total_claim_backing_e8s: 2_000,
            liquid_icp_e8s: 2_000,
            icp_fee_e8s: 10,
            ..Default::default()
        };
        let quote = quote_for_amount(100, &snapshot).unwrap();
        assert_eq!(quote.gross_icp, 200);
        assert_eq!(quote.net_icp, 190);
    }

    #[test]
    fn backing_without_claims_is_not_redemption_ready() {
        let snapshot = ClaimSnapshot {
            claim_supply_e8s: 0,
            total_claim_backing_e8s: 2_000,
            liquid_icp_e8s: 2_000,
            icp_fee_e8s: 10,
            ..Default::default()
        };
        assert_eq!(
            quote_for_amount(100, &snapshot),
            Err("canonical claim snapshot is not redemption-ready".into())
        );
    }
}
