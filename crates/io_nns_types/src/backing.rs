use candid::CandidType;
use io_accounts::Account;
use serde::Deserialize;

pub const MAX_LIVE_UNWIND_COHORTS: usize = 32;
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
    pub pooled_principal_e8s: u128,
    pub live_cohorts: Vec<CohortObservation>,
    pub unwinding_principal_e8s: u128,
    pub transit_backing_e8s: u128,
    pub active_operation_sequence: u64,
    pub control_epoch: u64,
    pub fingerprint: Vec<u8>,
    pub oldest_ready_at_seconds: Option<u64>,
}

impl ClaimBackingObservation {
    pub fn validate(&self) -> Result<(), String> {
        if self.fingerprint.len() != 32 || self.live_cohorts.len() > MAX_LIVE_UNWIND_COHORTS {
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
    pub parent_neuron_id: Option<u64>,
    pub phase: PoolCommandPhase,
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
        {
            return Err("pooled-parent command is inconsistent".into());
        }
        Ok(())
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
            control_epoch: 0,
            fingerprint: vec![1; 32],
            oldest_ready_at_seconds: Some(3),
        };
        assert_eq!(observation.validate(), Ok(()));
    }
}
