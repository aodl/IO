use candid::CandidType;
use io_accounts::Account;
use serde::Deserialize;

pub const MAX_LIVE_UNWIND_COHORTS: usize = 32;

pub fn net_committed_child_backing(
    physical_principal_e8s: u128,
    future_disbursement_fee_e8s: u128,
) -> Result<u128, io_core_model::EconomicsError> {
    physical_principal_e8s
        .checked_sub(future_disbursement_fee_e8s)
        .ok_or(io_core_model::EconomicsError::InsufficientBacking)
}

pub fn expected_split_child_principal(
    gross_e8s: u128,
    split_fee_e8s: u128,
) -> Result<u128, io_core_model::EconomicsError> {
    gross_e8s
        .checked_sub(split_fee_e8s)
        .ok_or(io_core_model::EconomicsError::InsufficientBacking)
}

pub fn net_committed_unwind_backing(
    gross_e8s: u128,
    split_fee_e8s: u128,
    future_disbursement_fee_e8s: u128,
) -> Result<u128, io_core_model::EconomicsError> {
    net_committed_child_backing(
        expected_split_child_principal(gross_e8s, split_fee_e8s)?,
        future_disbursement_fee_e8s,
    )
}

pub fn remaining_parent_transit(
    expected_before: u128,
    expected_credit: u128,
    observed_parent: u128,
) -> Result<u128, io_core_model::EconomicsError> {
    let expected_after = io_core_model::checked_add(expected_before, expected_credit)?;
    if observed_parent < expected_before {
        return Err(io_core_model::EconomicsError::InsufficientBacking);
    }
    Ok(expected_after.saturating_sub(observed_parent))
}
pub const POOLED_PARENT_DELAY_SECONDS: u64 = 1_209_600;

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct FollowPolicy {
    pub followee_neuron_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ParentAssetObservation {
    pub neuron_id: u64,
    pub staking_account: Account,
    pub physical_principal_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ParentPolicyObservation {
    pub neuron_id: u64,
    pub dissolve_delay_seconds: u64,
    pub auto_stake_maturity: bool,
    pub follow_policy: FollowPolicy,
    pub voting_power_refreshed_at_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PoolPolicyObservation {
    pub parent: Option<ParentPolicyObservation>,
    pub control_epoch: u64,
    pub active_operation_sequence: u64,
    pub fingerprint: Vec<u8>,
}

impl PoolPolicyObservation {
    pub fn validate(&self) -> Result<(), String> {
        if self.fingerprint.len() != 32
            || self.parent.as_ref().is_some_and(|parent| {
                parent.neuron_id == 0
                    || parent.dissolve_delay_seconds != POOLED_PARENT_DELAY_SECONDS
                    || parent.auto_stake_maturity
                    || parent.follow_policy.followee_neuron_id == 0
                    || parent.voting_power_refreshed_at_seconds == 0
            })
        {
            return Err("pool policy observation is invalid".into());
        }
        Ok(())
    }
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
    pub physical_principal_e8s: u128,
    pub net_backing_e8s: u128,
    pub committed_fee_e8s: u128,
    pub ready_at_seconds: u64,
    pub proof: CohortProofState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, CandidType, Deserialize)]
pub enum TransitComponentKind {
    PoolTopUp,
    ActiveUnwind,
    ActiveJupiter,
    ActiveMaturity,
    PendingTwoYearMaturity,
    PendingTwoWeekMaturity,
}

impl TransitComponentKind {
    fn requires_fee_basis(self) -> bool {
        !matches!(self, Self::PoolTopUp)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TransitComponentObservation {
    pub kind: TransitComponentKind,
    pub backing_e8s: u128,
    pub fee_basis_e8s: Option<u128>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ClaimAssetObservation {
    pub parent: Option<ParentAssetObservation>,
    pub pool_staking_account: Account,
    pub minimum_parent_stake_e8s: u128,
    pub pooled_parent_principal_e8s: u128,
    pub live_cohorts: Vec<CohortObservation>,
    pub live_child_physical_principal_e8s: u128,
    pub live_child_net_backing_e8s: u128,
    pub live_child_committed_fee_liability_e8s: u128,
    pub transit_components: Vec<TransitComponentObservation>,
    pub transit_backing_e8s: u128,
    pub active_operation_sequence: u64,
    pub last_completed_pool_operation_sequence: Option<u64>,
    pub control_epoch: u64,
    pub fingerprint: Vec<u8>,
    pub oldest_ready_at_seconds: Option<u64>,
}

impl ClaimAssetObservation {
    pub fn validate(&self) -> Result<(), String> {
        self.pool_staking_account.validate()?;
        if self.fingerprint.len() != 32
            || self.live_cohorts.len() > MAX_LIVE_UNWIND_COHORTS
            || self.last_completed_pool_operation_sequence == Some(0)
            || self.minimum_parent_stake_e8s == 0
            || self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.staking_account != self.pool_staking_account)
        {
            return Err("claim-backing observation bounds are invalid".into());
        }
        let mut previous_component = None;
        let transit = self
            .transit_components
            .iter()
            .try_fold(0u128, |total, component| {
                if component.backing_e8s == 0
                    || previous_component
                        .replace(component.kind)
                        .is_some_and(|previous| previous >= component.kind)
                    || component.kind.requires_fee_basis() != component.fee_basis_e8s.is_some()
                    || component.fee_basis_e8s == Some(0)
                {
                    return Err("transit components are malformed or unsorted".to_string());
                }
                total
                    .checked_add(component.backing_e8s)
                    .ok_or_else(|| "transit component total overflow".to_string())
            })?;
        if transit != self.transit_backing_e8s {
            return Err("transit component total is inconsistent".into());
        }
        if self
            .parent
            .as_ref()
            .map_or(self.pooled_parent_principal_e8s != 0, |parent| {
                parent.neuron_id == 0
                    || parent.physical_principal_e8s != self.pooled_parent_principal_e8s
                    || parent.physical_principal_e8s < self.minimum_parent_stake_e8s
            })
        {
            return Err("pooled parent observation is invalid".into());
        }
        let mut previous = None;
        let mut child_ids = std::collections::BTreeSet::new();
        let (physical, net) =
            self.live_cohorts
                .iter()
                .try_fold((0u128, 0u128), |(physical, net), cohort| {
                    if cohort.generation == 0
                        || cohort.child_neuron_id == 0
                        || !child_ids.insert(cohort.child_neuron_id)
                        || cohort.net_backing_e8s > cohort.physical_principal_e8s
                        || cohort.committed_fee_e8s == 0
                        || (cohort.physical_principal_e8s > 0
                            && cohort
                                .physical_principal_e8s
                                .checked_sub(cohort.net_backing_e8s)
                                != Some(cohort.committed_fee_e8s))
                        || (cohort.physical_principal_e8s == 0
                            && matches!(
                                cohort.proof,
                                CohortProofState::Dissolving
                                    | CohortProofState::DisbursementSubmitted
                            ))
                        || previous
                            .replace(cohort.generation)
                            .is_some_and(|old| old >= cohort.generation)
                    {
                        return Err("live cohorts are malformed or unsorted".to_string());
                    }
                    Ok((
                        physical
                            .checked_add(cohort.physical_principal_e8s)
                            .ok_or_else(|| "live cohort physical principal overflow".to_string())?,
                        net.checked_add(cohort.net_backing_e8s)
                            .ok_or_else(|| "live cohort net backing overflow".to_string())?,
                    ))
                })?;
        if physical != self.live_child_physical_principal_e8s
            || net != self.live_child_net_backing_e8s
            || physical.checked_sub(net) != Some(self.live_child_committed_fee_liability_e8s)
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
        target_status: PoolTargetResult,
    },
    CapacityPending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum PoolTargetResult {
    AtTarget,
    OverTarget,
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
                < self
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
        assert_eq!(remaining_parent_transit(1_000, 100, 1_101), Ok(0));
    }

    #[test]
    fn donations_at_each_parent_credit_boundary_never_wedge_or_create_negative_transit() {
        // A donation visible when a freshly claimed bootstrap parent is first observed.
        assert_eq!(remaining_parent_transit(0, 100, 125), Ok(0));

        // A donation after permit preparation but before the refresh is only favourable:
        // the still-unreflected part remains transit until the exact operation credit lands.
        assert_eq!(remaining_parent_transit(1_000, 100, 1_025), Ok(75));
        assert_eq!(remaining_parent_transit(1_000, 100, 1_125), Ok(0));

        // Lost-callback recovery observes the same monotone completion without attributing
        // the excess to the exact operation credit or counting any residual twice.
        let recovered = remaining_parent_transit(1_000, 100, 1_150).unwrap();
        assert_eq!(recovered, 0);
        assert_eq!(1_150_u128.checked_add(recovered), Some(1_150));
    }

    #[test]
    fn committed_child_fee_is_counted_once_and_disbursement_preserves_backing() {
        let physical = 100;
        let fee = 10;
        let net = net_committed_child_backing(physical, fee).unwrap();
        assert_eq!(net, 90);
        let before = io_core_model::claim_backing(io_core_model::Backing {
            liquid: 1_000,
            pooled: 0,
            unwinding: net,
            transit: 0,
        });
        let after = io_core_model::claim_backing(io_core_model::Backing {
            liquid: 1_090,
            pooled: 0,
            unwinding: 0,
            transit: 0,
        });
        assert_eq!(before, after);
        assert_eq!(physical - net, fee);
    }

    #[test]
    fn frozen_redemption_quote_survives_net_child_return_without_a_donation() {
        let physical_child = 120;
        let fee = 10;
        let net_child = net_committed_child_backing(physical_child, fee).unwrap();
        let before = io_core_model::EconomicState {
            backing: io_core_model::Backing {
                liquid: 0,
                pooled: 890,
                unwinding: net_child,
                transit: 0,
            },
            claims: 1_000,
            active_backing: 0,
            active_reward: 0,
        };
        let frozen = io_core_model::redemption_quote(before, 100, 0, fee).unwrap();
        assert!(matches!(
            io_core_model::require_liquidity(frozen, before.backing.liquid),
            Err(io_core_model::EconomicsError::InsufficientLiquidity(_))
        ));

        // The immutable IO pull/quote is still valid when the committed physical child
        // returns exactly its already-net claim value; no protocol donation is needed.
        let after_return = io_core_model::EconomicState {
            backing: io_core_model::Backing {
                liquid: net_child,
                pooled: 890,
                unwinding: 0,
                transit: 0,
            },
            ..before
        };
        assert_eq!(
            io_core_model::claim_backing(before.backing),
            io_core_model::claim_backing(after_return.backing)
        );
        assert_eq!(
            io_core_model::redemption_quote(after_return, 100, 0, fee),
            Ok(frozen)
        );
        assert_eq!(
            io_core_model::require_liquidity(frozen, after_return.backing.liquid),
            Ok(())
        );
        assert_eq!(
            after_return.backing.liquid.checked_sub(frozen.gross_icp),
            Some(10)
        );
    }

    #[test]
    fn child_identified_through_passive_uses_one_net_value() {
        let gross = 200_000_000;
        let fee = 10_000;
        let principal = expected_split_child_principal(gross, fee).unwrap();
        let child_identified = net_committed_unwind_backing(gross, fee, fee).unwrap();
        let split_proved = net_committed_child_backing(principal, fee).unwrap();
        let passive = net_committed_child_backing(principal, fee).unwrap();
        assert_eq!(child_identified, gross - fee - fee);
        assert_eq!(child_identified, split_proved);
        assert_eq!(split_proved, passive);
    }

    #[test]
    fn each_live_child_derives_its_own_committed_liability() {
        let physical = [100, 200, 300];
        let net = physical
            .into_iter()
            .map(|principal| net_committed_child_backing(principal, 10).unwrap())
            .sum::<u128>();
        assert_eq!(physical.into_iter().sum::<u128>() - net, 30);
        assert_eq!(
            net_committed_child_backing(0, 10),
            Err(io_core_model::EconomicsError::InsufficientBacking)
        );
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
    fn completed_pool_credit_accepts_only_monotone_actual_principal() {
        let permit = TopUpPermit {
            generation: 7,
            operation_sequence: 2,
            expected_parent_principal_e8s: 1_000,
            destination: account(1),
            expected_credit_e8s: 100,
            fee_e8s: 10,
            memo: b"IO:POOL:7".to_vec(),
            prepared_at_nanos: 1,
            snapshot_fingerprint: vec![7; 32],
        };
        for principal_e8s in [1_100, 1_101] {
            assert_eq!(
                CompletedPoolCommand {
                    permit: permit.clone(),
                    transfer_block_index: 9,
                    parent_neuron_id: 4,
                    principal_e8s,
                }
                .validate(3),
                Ok(())
            );
        }
        assert!(CompletedPoolCommand {
            permit,
            transfer_block_index: 9,
            parent_neuron_id: 4,
            principal_e8s: 1_099,
        }
        .validate(3)
        .is_err());
    }

    #[test]
    fn returned_principal_is_not_counted_while_cleanup_remains_live() {
        let observation = ClaimAssetObservation {
            parent: None,
            pool_staking_account: account(9),
            minimum_parent_stake_e8s: 100_000_000,
            pooled_parent_principal_e8s: 0,
            live_cohorts: vec![CohortObservation {
                generation: 1,
                child_neuron_id: 2,
                physical_principal_e8s: 0,
                net_backing_e8s: 0,
                committed_fee_e8s: 10,
                ready_at_seconds: 3,
                proof: CohortProofState::PrincipalReturned,
            }],
            live_child_physical_principal_e8s: 0,
            live_child_net_backing_e8s: 0,
            live_child_committed_fee_liability_e8s: 0,
            transit_backing_e8s: 0,
            transit_components: Vec::new(),
            active_operation_sequence: 0,
            last_completed_pool_operation_sequence: None,
            control_epoch: 0,
            fingerprint: vec![1; 32],
            oldest_ready_at_seconds: Some(3),
        };
        assert_eq!(observation.validate(), Ok(()));
    }
}
