use candid::{CandidType, Principal};
use serde::Deserialize;

use crate::{
    state::{Account, OperationSequence, StreamConfig},
    transfer::{deterministic_memo, OwnTransferIntent, TransferAttempt, TransferState},
};

pub const AUTOMATIC_POLL_MIN_SECONDS: u64 = 10;
pub const AUTOMATIC_POLL_MAX_SECONDS: u64 = 3_600;
pub const MANUAL_WORK_COOLDOWN_NANOS: u64 = 10_000_000_000;
pub const MAX_INDEX_PAGES_PER_RUN: u64 = 1;
pub const MAX_INDEX_TRANSACTIONS_PER_PAGE: u64 = 32;
pub const MAX_PENDING_CANDIDATES: u64 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RedemptionPhase {
    PayoutPrepared,
    PayoutSubmitted,
    PayoutSucceeded,
    SweepPrepared,
    SweepSubmitted,
    Stuck,
}

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
pub struct RedemptionOperation {
    pub sequence: OperationSequence,
    pub source_io_block: u128,
    pub source_account: Account,
    pub staged_io_amount_e8s: u128,
    pub gross_icp_e8s: u128,
    pub net_icp_e8s: u128,
    pub icp_fee_e8s: u128,
    pub io_sweep_fee_e8s: u128,
    pub icp_payout: TransferAttempt,
    pub reserve_sweep: Option<TransferAttempt>,
    pub last_external_call_started_at_nanos: u64,
    pub phase: RedemptionPhase,
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
        validate_payout(self, config)?;
        if let Some(sweep) = &self.reserve_sweep {
            validate_sweep(self, config, sweep)?;
        }
        match self.phase {
            RedemptionPhase::PayoutPrepared
                if !matches!(self.icp_payout.state, TransferState::Prepared)
                    || self.reserve_sweep.is_some() =>
            {
                Err("prepared payout phase is inconsistent".into())
            }
            RedemptionPhase::PayoutSubmitted
                if !matches!(self.icp_payout.state, TransferState::Submitted { .. })
                    || self.reserve_sweep.is_some() =>
            {
                Err("submitted payout phase is inconsistent".into())
            }
            RedemptionPhase::PayoutSucceeded
                if self.icp_payout.succeeded_block().is_err() || self.reserve_sweep.is_some() =>
            {
                Err("successful payout phase is inconsistent".into())
            }
            RedemptionPhase::SweepPrepared
                if self.icp_payout.succeeded_block().is_err()
                    || !matches!(
                        self.reserve_sweep.as_ref().map(|value| &value.state),
                        Some(TransferState::Prepared)
                    ) =>
            {
                Err("prepared reserve sweep phase is inconsistent".into())
            }
            RedemptionPhase::SweepSubmitted
                if self.icp_payout.succeeded_block().is_err()
                    || !matches!(
                        self.reserve_sweep.as_ref().map(|value| &value.state),
                        Some(TransferState::Submitted { .. })
                    ) =>
            {
                Err("submitted reserve sweep phase is inconsistent".into())
            }
            RedemptionPhase::Stuck
                if !matches!(self.icp_payout.state, TransferState::Stuck { .. })
                    && !matches!(
                        self.reserve_sweep.as_ref().map(|value| &value.state),
                        Some(TransferState::Stuck { .. })
                    ) =>
            {
                Err("stuck redemption lacks a stuck exact transfer".into())
            }
            _ => Ok(()),
        }
    }
}

fn validate_payout(operation: &RedemptionOperation, config: &StreamConfig) -> Result<(), String> {
    operation.icp_payout.validate()?;
    let OwnTransferIntent::Icrc1 {
        ledger,
        from_subaccount,
        to,
        amount,
        fee,
        memo,
        ..
    } = &operation.icp_payout.intent;
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
    sweep: &TransferAttempt,
) -> Result<(), String> {
    sweep.validate()?;
    let OwnTransferIntent::Icrc1 {
        ledger,
        from_subaccount,
        to,
        amount,
        fee,
        memo,
        ..
    } = &sweep.intent;
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
    amount: u128,
    snapshot: &ClaimSnapshot,
) -> Result<io_core_model::RedemptionQuote, String> {
    if io_core_model::claim_backing(io_core_model::Backing {
        liquid: snapshot.liquid_icp_e8s,
        pooled: snapshot.pooled_principal_e8s,
        unwinding: snapshot.unwinding_net_backing_e8s,
        transit: snapshot.transit_backing_e8s,
    })
    .map_err(|error| format!("claim backing failed: {error:?}"))?
        != snapshot.total_claim_backing_e8s
    {
        return Err("canonical total claim backing is inconsistent".into());
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
        amount,
        snapshot.icp_fee_e8s,
    )
    .map_err(|error| format!("redemption quote failed: {error:?}"))
}

pub fn coherent_paid_unswept_io(operation: &RedemptionOperation) -> Result<u128, String> {
    match operation.phase {
        RedemptionPhase::PayoutPrepared => Ok(0),
        RedemptionPhase::PayoutSucceeded | RedemptionPhase::SweepPrepared => {
            Ok(operation.staged_io_amount_e8s)
        }
        RedemptionPhase::PayoutSubmitted => {
            Err("redemption payout effect is transitionally ambiguous".into())
        }
        RedemptionPhase::SweepSubmitted => {
            Err("redemption reserve-sweep effect is transitionally ambiguous".into())
        }
        RedemptionPhase::Stuck => Err("redemption exact effect awaits reviewed proof".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::DispatchEpoch;

    fn snapshot(liquid: u128, backing: u128, claims: u128, payout_fee: u128) -> ClaimSnapshot {
        ClaimSnapshot {
            claim_supply_e8s: claims,
            liquid_icp_e8s: liquid,
            pooled_principal_e8s: backing - liquid,
            total_claim_backing_e8s: backing,
            icp_fee_e8s: payout_fee,
            ..ClaimSnapshot::default()
        }
    }

    fn operation(phase: RedemptionPhase) -> RedemptionOperation {
        fn attempt(state: TransferState) -> TransferAttempt {
            TransferAttempt {
                intent: OwnTransferIntent::Icrc1 {
                    ledger: Principal::from_slice(&[1]),
                    from_subaccount: [1; 32],
                    to: Account {
                        owner: Principal::from_slice(&[2]),
                        subaccount: None,
                    },
                    amount: 290,
                    fee: 10,
                    memo: vec![3; 32],
                    created_at_time: 1,
                },
                state,
            }
        }
        let submitted = TransferState::Submitted {
            epoch: DispatchEpoch(1),
            first_submitted_at: 1,
            last_submitted_at: 1,
        };
        let payout_state = match phase {
            RedemptionPhase::PayoutPrepared => TransferState::Prepared,
            RedemptionPhase::PayoutSubmitted => submitted.clone(),
            _ => TransferState::Succeeded { block: 7 },
        };
        let reserve_sweep = match phase {
            RedemptionPhase::SweepPrepared => Some(attempt(TransferState::Prepared)),
            RedemptionPhase::SweepSubmitted => Some(attempt(submitted)),
            _ => None,
        };
        RedemptionOperation {
            sequence: OperationSequence(1),
            source_io_block: 5,
            source_account: Account {
                owner: Principal::from_slice(&[4]),
                subaccount: None,
            },
            staged_io_amount_e8s: 300,
            gross_icp_e8s: 300,
            net_icp_e8s: 290,
            icp_fee_e8s: 10,
            io_sweep_fee_e8s: 10,
            icp_payout: attempt(payout_state),
            reserve_sweep,
            last_external_call_started_at_nanos: 0,
            phase,
        }
    }

    #[test]
    fn staged_principal_is_quoted_at_current_total_backing_without_reconstructing_io_fee() {
        let quote = quote_for_amount(100, &snapshot(1_000, 2_000, 1_000, 10)).unwrap();
        assert_eq!((quote.gross_icp, quote.net_icp), (200, 190));
    }

    #[test]
    fn payout_retirement_cannot_decrease_the_claim_rate() {
        for (backing, claims, amount) in [(1_000, 1_000, 100), (2_001, 1_000, 333)] {
            let quote = quote_for_amount(amount, &snapshot(backing, backing, claims, 1)).unwrap();
            let after_backing = backing - quote.gross_icp;
            let after_claims = claims - amount;
            assert!(after_backing * claims >= backing * after_claims);
        }
    }

    #[test]
    fn sweep_physical_conservation_exactly_replaces_temporary_retirement() {
        let (supply, reserve, staged, fee) = (2_000u128, 500u128, 300u128, 10u128);
        let economic_before_sweep = supply - reserve - staged;
        let supply_after = supply - fee;
        let reserve_after = reserve + staged - fee;
        assert_eq!(supply_after - reserve_after, economic_before_sweep);
    }

    #[test]
    fn canonical_claim_observation_fails_closed_at_both_ambiguous_boundaries() {
        assert_eq!(
            coherent_paid_unswept_io(&operation(RedemptionPhase::PayoutPrepared)),
            Ok(0)
        );
        assert!(coherent_paid_unswept_io(&operation(RedemptionPhase::PayoutSubmitted)).is_err());
        assert_eq!(
            coherent_paid_unswept_io(&operation(RedemptionPhase::PayoutSucceeded)),
            Ok(300)
        );
        assert_eq!(
            coherent_paid_unswept_io(&operation(RedemptionPhase::SweepPrepared)),
            Ok(300)
        );
        assert!(coherent_paid_unswept_io(&operation(RedemptionPhase::SweepSubmitted)).is_err());
    }
}
