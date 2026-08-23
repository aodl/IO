use candid::CandidType;
use io_accounts::Account;
use serde::Deserialize;

pub const MAX_LIVE_UNWIND_COHORTS: usize = 32;

pub fn remaining_parent_transit(
    expected_before: u128,
    expected_credit: u128,
    observed_parent: u128,
) -> Result<u128, io_core_model::EconomicsError> {
    let expected_after = io_core_model::checked_add(expected_before, expected_credit)?;
    if observed_parent < expected_before || observed_parent > expected_after {
        return Err(io_core_model::EconomicsError::InsufficientBacking);
    }
    Ok(expected_after - observed_parent)
}
pub const POOLED_PARENT_DELAY_SECONDS: u64 = 1_209_600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct FollowPolicy {
    pub followee_neuron_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ParentObservation {
    pub neuron_id: u64,
    pub staking_account: Account,
    pub principal_e8s: u128,
    pub dissolve_delay_seconds: u64,
    pub auto_stake_maturity: bool,
    pub follow_policy: FollowPolicy,
    pub voting_power_refreshed_at_seconds: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum CohortProofState {
    Dissolving,
    DisbursementSubmitted,
    PrincipalReturned,
    MaturityHandled,
    CleanupComplete,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CohortObservation {
    pub generation: u64,
    pub child_neuron_id: u64,
    pub principal_e8s: u128,
    pub ready_at_seconds: u64,
    pub proof: CohortProofState,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ClaimBackingObservation {
    pub parent: Option<ParentObservation>,
    pub permanent_staking_account: Account,
    pub pool_staking_account: Account,
    pub minimum_parent_stake_e8s: u128,
    pub pooled_principal_e8s: u128,
    pub live_cohorts: Vec<CohortObservation>,
    pub unwinding_principal_e8s: u128,
    pub transit_backing_e8s: u128,
    pub active_operation_sequence: u64,
    pub last_completed_pool_operation_sequence: Option<u64>,
    pub active_unwind_generation: Option<u64>,
    pub control_epoch: u64,
    pub fingerprint: Vec<u8>,
    pub oldest_ready_at_seconds: Option<u64>,
}

impl ClaimBackingObservation {
    pub fn validate(&self) -> Result<(), String> {
        self.permanent_staking_account.validate()?;
        self.pool_staking_account.validate()?;
        if self.fingerprint.len() != 32
            || self.live_cohorts.len() > MAX_LIVE_UNWIND_COHORTS
            || self.active_unwind_generation == Some(0)
            || self.last_completed_pool_operation_sequence == Some(0)
            || self.minimum_parent_stake_e8s == 0
            || self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.staking_account != self.pool_staking_account)
        {
            return Err("claim-backing observation bounds are invalid".into());
        }
        if self
            .parent
            .as_ref()
            .map_or(self.pooled_principal_e8s != 0, |parent| {
                parent.neuron_id == 0
                    || parent.principal_e8s != self.pooled_principal_e8s
                    || parent.dissolve_delay_seconds != POOLED_PARENT_DELAY_SECONDS
                    || parent.auto_stake_maturity
                    || parent.follow_policy.followee_neuron_id == 0
                    || parent.voting_power_refreshed_at_seconds == 0
            })
        {
            return Err("pooled parent observation is invalid".into());
        }
        let mut previous = None;
        let sum = self.live_cohorts.iter().try_fold(0u128, |sum, cohort| {
            if cohort.generation == 0
                || cohort.child_neuron_id == 0
                || (cohort.principal_e8s == 0
                    && matches!(
                        cohort.proof,
                        CohortProofState::Dissolving | CohortProofState::DisbursementSubmitted
                    ))
                || previous
                    .replace(cohort.generation)
                    .is_some_and(|old| old >= cohort.generation)
            {
                return Err("live cohorts are malformed or unsorted".to_string());
            }
            sum.checked_add(cohort.principal_e8s)
                .ok_or_else(|| "live cohort principal overflow".to_string())
        })?;
        if sum != self.unwinding_principal_e8s
            || self.oldest_ready_at_seconds
                != self
                    .live_cohorts
                    .iter()
                    .map(|cohort| cohort.ready_at_seconds)
                    .min()
        {
            return Err("live cohort totals are inconsistent".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TopUpPermit {
    pub generation: u64,
    pub operation_sequence: u64,
    pub expected_parent_principal_e8s: u128,
    pub destination: Account,
    pub expected_credit_e8s: u128,
    pub fee_e8s: u128,
    pub memo: Vec<u8>,
    pub prepared_at_nanos: u64,
    pub snapshot_fingerprint: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PreparePoolReconciliationArgs {
    pub generation: u64,
    pub target_e8s: u128,
    pub action: PoolReconciliationAction,
    pub fee_e8s: u128,
    pub snapshot_fingerprint: Vec<u8>,
    pub memo: Vec<u8>,
    pub created_at_time_nanos: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum PoolReconciliationAction {
    Hold,
    TopUp { expected_credit_e8s: u128 },
    Unwind { expected_gross_e8s: u128 },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum PoolProgress {
    Held {
        principal_e8s: u128,
    },
    AwaitingTransfer(TopUpPermit),
    UnwindPrepared {
        generation: u64,
        gross_e8s: u128,
    },
    UnwindCommitted {
        generation: u64,
        principal_e8s: u128,
    },
    AwaitingProof,
    Completed {
        parent_neuron_id: u64,
        principal_e8s: u128,
    },
    CapacityPending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum PoolCommandKind {
    Bootstrap,
    TopUp,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum PoolCommandPhase {
    AwaitingTransfer,
    TransferProved { block_index: u128 },
    ClaimSubmitted { block_index: u128 },
    ParentIdentified,
    DelaySubmitted { expected_delay_seconds: u64 },
    FollowingSubmitted,
    RefreshSubmitted,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PoolCommand {
    pub kind: PoolCommandKind,
    pub permit: TopUpPermit,
    pub transfer_block_index: Option<u128>,
    pub parent_neuron_id: Option<u64>,
    pub phase: PoolCommandPhase,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CompletedPoolCommand {
    pub permit: TopUpPermit,
    pub transfer_block_index: u128,
    pub parent_neuron_id: u64,
    pub principal_e8s: u128,
}

impl PoolCommand {
    pub fn validate(&self, next_operation_sequence: u64) -> Result<(), String> {
        self.permit.destination.validate()?;
        if self.permit.generation == 0
            || self.permit.operation_sequence == 0
            || self.permit.operation_sequence >= next_operation_sequence
            || self.permit.expected_credit_e8s == 0
            || self.permit.fee_e8s == 0
            || self.permit.memo.is_empty()
            || self.permit.memo.len() > 32
            || self.permit.prepared_at_nanos == 0
            || self.permit.snapshot_fingerprint.len() != 32
            || (self.kind == PoolCommandKind::Bootstrap
                && self.permit.expected_parent_principal_e8s != 0)
            || (self.kind == PoolCommandKind::TopUp
                && (self.permit.expected_parent_principal_e8s == 0
                    || self.parent_neuron_id.is_none()))
            || self.transfer_block_index.is_some()
                != !matches!(self.phase, PoolCommandPhase::AwaitingTransfer)
        {
            return Err("pooled-parent command is inconsistent".into());
        }
        Ok(())
    }
}

impl CompletedPoolCommand {
    pub fn validate(&self, next_operation_sequence: u64) -> Result<(), String> {
        if self.transfer_block_index == 0
            || self.parent_neuron_id == 0
            || self.principal_e8s
                != self
                    .permit
                    .expected_parent_principal_e8s
                    .checked_add(self.permit.expected_credit_e8s)
                    .ok_or("completed pool principal overflow")?
        {
            return Err("completed pool command evidence is inconsistent".into());
        }
        PoolCommand {
            kind: if self.permit.expected_parent_principal_e8s == 0 {
                PoolCommandKind::Bootstrap
            } else {
                PoolCommandKind::TopUp
            },
            permit: self.permit.clone(),
            transfer_block_index: Some(self.transfer_block_index),
            parent_neuron_id: Some(self.parent_neuron_id),
            phase: PoolCommandPhase::RefreshSubmitted,
        }
        .validate(next_operation_sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;

    fn account(byte: u8) -> Account {
        Account {
            owner: Principal::from_slice(&[byte; 29]),
            subaccount: Some(vec![byte; 32]),
        }
    }

    #[test]
    fn parent_principal_and_transit_partition_one_committed_credit() {
        assert_eq!(remaining_parent_transit(1_000, 100, 1_000), Ok(100));
        assert_eq!(remaining_parent_transit(1_000, 100, 1_040), Ok(60));
        assert_eq!(remaining_parent_transit(1_000, 100, 1_100), Ok(0));
        assert!(remaining_parent_transit(1_000, 100, 999).is_err());
        assert!(remaining_parent_transit(1_000, 100, 1_101).is_err());
    }

    #[test]
    fn exact_pool_command_is_bounded_to_one_generation_and_intent() {
        let command = PoolCommand {
            kind: PoolCommandKind::Bootstrap,
            permit: TopUpPermit {
                generation: 7,
                operation_sequence: 2,
                expected_parent_principal_e8s: 0,
                destination: account(1),
                expected_credit_e8s: 100_000_000,
                fee_e8s: 10_000,
                memo: b"IO:POOL:7".to_vec(),
                prepared_at_nanos: 1,
                snapshot_fingerprint: vec![7; 32],
            },
            transfer_block_index: None,
            parent_neuron_id: None,
            phase: PoolCommandPhase::AwaitingTransfer,
        };
        assert_eq!(command.validate(3), Ok(()));
        let mut invalid = command;
        invalid.kind = PoolCommandKind::TopUp;
        assert!(invalid.validate(3).is_err());
    }

    #[test]
    fn returned_principal_is_not_counted_while_cleanup_remains_live() {
        let observation = ClaimBackingObservation {
            parent: None,
            permanent_staking_account: account(8),
            pool_staking_account: account(9),
            minimum_parent_stake_e8s: 100_000_000,
            pooled_principal_e8s: 0,
            live_cohorts: vec![CohortObservation {
                generation: 1,
                child_neuron_id: 2,
                principal_e8s: 0,
                ready_at_seconds: 3,
                proof: CohortProofState::PrincipalReturned,
            }],
            unwinding_principal_e8s: 0,
            transit_backing_e8s: 0,
            active_operation_sequence: 0,
            last_completed_pool_operation_sequence: None,
            active_unwind_generation: None,
            control_epoch: 0,
            fingerprint: vec![1; 32],
            oldest_ready_at_seconds: Some(3),
        };
        assert_eq!(observation.validate(), Ok(()));
    }
}
