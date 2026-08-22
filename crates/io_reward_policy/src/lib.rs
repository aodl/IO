//! Pure allocation of one actually backed IO pool over cumulative entitlement credits.

use io_core_model::{
    backed_io, checked_add, claim_backing, split_40_60, target, EconomicState, EconomicsError,
};

pub const DAILY_EVENT_CREDIT: u128 = 1_000_000_000_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntitlementCredit {
    pub sns_neuron_id: Vec<u8>,
    pub accumulated_eligible_credit: u128,
}

pub fn entitlement_credit_from_bytes(
    sns_neuron_id: Vec<u8>,
    accumulated_eligible_credit: u128,
) -> EntitlementCredit {
    EntitlementCredit {
        sns_neuron_id,
        accumulated_eligible_credit,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewardAllocation {
    pub sns_neuron_id: Vec<u8>,
    pub io_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllocationOutcome {
    pub allocations: Vec<RewardAllocation>,
    pub forfeited_io_e8s: u128,
    pub rounding_dust_e8s: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewardPolicyError {
    ArithmeticOverflow,
    InvalidDenominator,
}

fn mul_u128_wide(a: u128, b: u128) -> (u128, u128) {
    const MASK: u128 = u64::MAX as u128;
    let a0 = a & MASK;
    let a1 = a >> 64;
    let b0 = b & MASK;
    let b1 = b >> 64;
    let p0 = a0 * b0;
    let p1 = a0 * b1;
    let p2 = a1 * b0;
    let p3 = a1 * b1;
    let lo_low = p0 & MASK;
    let carry = p0 >> 64;
    let middle = (p1 & MASK) + (p2 & MASK) + carry;
    let lo_high = middle & MASK;
    let hi = p3 + (p1 >> 64) + (p2 >> 64) + (middle >> 64);
    (hi, (lo_high << 64) | lo_low)
}

fn doubled_remainder_minus_denominator(
    remainder: u128,
    bit: u128,
    denominator: u128,
) -> Option<u128> {
    let half = denominator / 2;
    if denominator.is_multiple_of(2) {
        (remainder >= half).then(|| (remainder - half) * 2 + bit)
    } else if remainder > half {
        Some((remainder - half) * 2 - 1 + bit)
    } else if remainder == half && bit == 1 {
        Some(0)
    } else {
        None
    }
}

pub fn mul_div_floor(
    value: u128,
    numerator: u128,
    denominator: u128,
) -> Result<u128, RewardPolicyError> {
    if denominator == 0 {
        return Err(RewardPolicyError::InvalidDenominator);
    }
    if numerator == 0 || value == 0 {
        return Ok(0);
    }
    let (hi, lo) = mul_u128_wide(value, numerator);
    let mut quotient = 0u128;
    let mut remainder = 0u128;
    for bit_index in (0..256).rev() {
        let bit = if bit_index >= 128 {
            (hi >> (bit_index - 128)) & 1
        } else {
            (lo >> bit_index) & 1
        };
        if let Some(next) = doubled_remainder_minus_denominator(remainder, bit, denominator) {
            remainder = next;
            if bit_index >= 128 {
                return Err(RewardPolicyError::ArithmeticOverflow);
            }
            quotient |= 1u128 << bit_index;
        } else {
            remainder = remainder
                .checked_mul(2)
                .and_then(|value| value.checked_add(bit))
                .ok_or(RewardPolicyError::ArithmeticOverflow)?;
        }
    }
    Ok(quotient)
}

pub fn allocate_rewards(
    reward_pool_io_e8s: u128,
    policy_credit_total: u128,
    entitlements: &[EntitlementCredit],
) -> Result<AllocationOutcome, RewardPolicyError> {
    if policy_credit_total == 0 {
        return Err(RewardPolicyError::InvalidDenominator);
    }
    let eligible_credit_total = entitlements
        .iter()
        .map(|entry| entry.accumulated_eligible_credit)
        .try_fold(0u128, |sum, credit| sum.checked_add(credit))
        .ok_or(RewardPolicyError::ArithmeticOverflow)?;
    if eligible_credit_total > policy_credit_total {
        return Err(RewardPolicyError::InvalidDenominator);
    }
    let eligible_pool_e8s = mul_div_floor(
        reward_pool_io_e8s,
        eligible_credit_total,
        policy_credit_total,
    )?;
    let forfeited_io_e8s = reward_pool_io_e8s
        .checked_sub(eligible_pool_e8s)
        .ok_or(RewardPolicyError::ArithmeticOverflow)?;
    if eligible_pool_e8s == 0 || eligible_credit_total == 0 {
        return Ok(AllocationOutcome {
            allocations: Vec::new(),
            forfeited_io_e8s,
            rounding_dust_e8s: eligible_pool_e8s,
        });
    }
    let mut issued = 0u128;
    let mut allocations = Vec::new();
    for entitlement in entitlements {
        if entitlement.accumulated_eligible_credit == 0 {
            continue;
        }
        let amount = mul_div_floor(
            eligible_pool_e8s,
            entitlement.accumulated_eligible_credit,
            eligible_credit_total,
        )?;
        issued = issued
            .checked_add(amount)
            .ok_or(RewardPolicyError::ArithmeticOverflow)?;
        if amount > 0 {
            allocations.push(RewardAllocation {
                sns_neuron_id: entitlement.sns_neuron_id.clone(),
                io_e8s: amount,
            });
        }
    }
    Ok(AllocationOutcome {
        allocations,
        forfeited_io_e8s,
        rounding_dust_e8s: eligible_pool_e8s
            .checked_sub(issued)
            .ok_or(RewardPolicyError::ArithmeticOverflow)?,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimRoute {
    AllLiquid,
    AllPool,
    Mixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimRoutePlan {
    pub route: ClaimRoute,
    pub fee_count: u8,
    pub claim_credit: u128,
    pub liquid_credit: u128,
    pub pooled_credit: u128,
    pub target: u128,
    pub under_target: u128,
    pub over_target: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwoWeekSettlementPlan {
    pub route: ClaimRoutePlan,
    pub permanent_credit: u128,
    pub maximum_io_pool: u128,
    pub rewards: AllocationOutcome,
    pub distributed_io: u128,
    pub recipient_io_fees: u128,
    pub post_backing: u128,
    pub post_claims: u128,
    pub post_active_backing: u128,
    pub post_active_reward: u128,
    pub reward_target: u128,
    pub snapshot_fingerprint: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanningError {
    Economics(EconomicsError),
    Rewards(RewardPolicyError),
    InsufficientPermanentCredit,
    InsufficientClaimCredit,
    InsufficientIoReserve,
    NoSafeRoute,
}

impl From<EconomicsError> for PlanningError {
    fn from(value: EconomicsError) -> Self {
        Self::Economics(value)
    }
}

impl From<RewardPolicyError> for PlanningError {
    fn from(value: RewardPolicyError) -> Self {
        Self::Rewards(value)
    }
}

#[derive(Clone, Copy)]
struct Candidate {
    credit: u128,
    target: u128,
    reward_target: u128,
}

fn executable(parent_exists: bool, credit: u128, minimum: u128) -> bool {
    parent_exists || credit >= minimum
}

fn finish_route(
    pooled: u128,
    fee: u128,
    parent_exists: bool,
    minimum: u128,
    one: Candidate,
    two: Option<Candidate>,
) -> Result<ClaimRoutePlan, PlanningError> {
    let one_need = one.target.saturating_sub(pooled);
    let one_liquid_ok = pooled >= one.reward_target;
    let make = |route, fee_count, candidate: Candidate, pool| ClaimRoutePlan {
        route,
        fee_count,
        claim_credit: candidate.credit,
        liquid_credit: candidate.credit - pool,
        pooled_credit: pool,
        target: candidate.target,
        under_target: candidate.target.saturating_sub(pooled + pool),
        over_target: (pooled + pool).saturating_sub(candidate.target),
    };
    if one_need == 0 && one_liquid_ok {
        return Ok(make(ClaimRoute::AllLiquid, 1, one, 0));
    }
    if one_need >= one.credit
        && executable(parent_exists, one.credit, minimum)
        && pooled + one.credit >= one.reward_target
    {
        return Ok(make(ClaimRoute::AllPool, 1, one, one.credit));
    }
    let Some(two) = two else {
        return one_liquid_ok
            .then(|| make(ClaimRoute::AllLiquid, 1, one, 0))
            .ok_or(PlanningError::NoSafeRoute);
    };
    let pool = two.credit.min(two.target.saturating_sub(pooled));
    let liquid = two.credit - pool;
    if pool <= fee || !executable(parent_exists, pool, minimum) {
        return one_liquid_ok
            .then(|| make(ClaimRoute::AllLiquid, 1, one, 0))
            .ok_or(PlanningError::NoSafeRoute);
    }
    if liquid == 0 {
        if executable(parent_exists, one.credit, minimum)
            && pooled + one.credit >= one.reward_target
        {
            return Ok(make(ClaimRoute::AllPool, 1, one, one.credit));
        }
        return one_liquid_ok
            .then(|| make(ClaimRoute::AllLiquid, 1, one, 0))
            .ok_or(PlanningError::NoSafeRoute);
    }
    if pooled + pool < two.reward_target {
        return Err(PlanningError::NoSafeRoute);
    }
    Ok(make(ClaimRoute::Mixed, 2, two, pool))
}

fn route_candidate(
    state: EconomicState,
    credit: u128,
    claims_delta: u128,
    active_delta: u128,
    reward_delta: u128,
) -> Result<Candidate, PlanningError> {
    let backing = claim_backing(state.backing)?
        .checked_add(credit)
        .ok_or(EconomicsError::ArithmeticOverflow)?;
    let claims = state
        .claims
        .checked_add(claims_delta)
        .ok_or(EconomicsError::ArithmeticOverflow)?;
    let active = state
        .active_backing
        .checked_add(active_delta)
        .ok_or(EconomicsError::ArithmeticOverflow)?;
    let reward = state
        .active_reward
        .checked_add(reward_delta)
        .ok_or(EconomicsError::ArithmeticOverflow)?;
    Ok(Candidate {
        credit,
        target: target(active, backing, claims)?,
        reward_target: target(reward, backing, claims)?,
    })
}

pub fn plan_permanent_maturity(
    state: EconomicState,
    actual_mint: u128,
    fee: u128,
    parent_exists: bool,
    minimum_parent_credit: u128,
) -> Result<ClaimRoutePlan, PlanningError> {
    let one_credit = actual_mint
        .checked_sub(fee)
        .ok_or(PlanningError::InsufficientClaimCredit)?;
    let one = route_candidate(state, one_credit, 0, 0, 0)?;
    let two = one_credit
        .checked_sub(fee)
        .map(|credit| route_candidate(state, credit, 0, 0, 0))
        .transpose()?;
    finish_route(
        state.backing.pooled,
        fee,
        parent_exists,
        minimum_parent_credit,
        one,
        two,
    )
}

pub struct TwoWeekSettlementInput<'a> {
    pub state: EconomicState,
    pub actual_mint: u128,
    pub permanent_transfer_fee: u128,
    pub claim_transfer_fee: u128,
    pub parent_exists: bool,
    pub minimum_parent_credit: u128,
    pub policy_credit_total: u128,
    pub entitlements: &'a [EntitlementCredit],
    pub reward_eligible_ids: &'a [Vec<u8>],
    pub reserve_io_capacity: u128,
    pub io_fee: u128,
    pub snapshot_fingerprint: [u8; 32],
}

struct SettlementCandidate {
    route: Candidate,
    maximum_io_pool: u128,
    rewards: AllocationOutcome,
    distributed: u128,
    recipient_fees: u128,
    reward_delta: u128,
}

fn settlement_candidate(
    input: &TwoWeekSettlementInput<'_>,
    pre_backing: u128,
    credit: u128,
) -> Result<SettlementCandidate, PlanningError> {
    let maximum_io_pool = backed_io(credit, pre_backing, input.state.claims)?;
    let rewards = allocate_rewards(
        maximum_io_pool,
        input.policy_credit_total,
        input.entitlements,
    )?;
    let distributed = rewards.allocations.iter().try_fold(0u128, |sum, item| {
        sum.checked_add(item.io_e8s).ok_or(PlanningError::Rewards(
            RewardPolicyError::ArithmeticOverflow,
        ))
    })?;
    let recipients = u128::try_from(rewards.allocations.len())
        .map_err(|_| PlanningError::InsufficientIoReserve)?;
    let recipient_fees = input
        .io_fee
        .checked_mul(recipients)
        .ok_or(PlanningError::InsufficientIoReserve)?;
    let reserve_debit = distributed
        .checked_add(recipient_fees)
        .ok_or(PlanningError::InsufficientIoReserve)?;
    if reserve_debit > input.reserve_io_capacity {
        return Err(PlanningError::InsufficientIoReserve);
    }
    let reward_delta = rewards.allocations.iter().try_fold(0u128, |sum, item| {
        if input.reward_eligible_ids.contains(&item.sns_neuron_id) {
            sum.checked_add(item.io_e8s).ok_or(PlanningError::Rewards(
                RewardPolicyError::ArithmeticOverflow,
            ))
        } else {
            Ok(sum)
        }
    })?;
    Ok(SettlementCandidate {
        route: route_candidate(input.state, credit, distributed, distributed, reward_delta)?,
        maximum_io_pool,
        rewards,
        distributed,
        recipient_fees,
        reward_delta,
    })
}

pub fn plan_two_week_settlement(
    input: TwoWeekSettlementInput<'_>,
) -> Result<TwoWeekSettlementPlan, PlanningError> {
    let split = split_40_60(input.actual_mint)?;
    let permanent_credit = split
        .permanent
        .checked_sub(input.permanent_transfer_fee)
        .ok_or(PlanningError::InsufficientPermanentCredit)?;
    let one_credit = split
        .claim
        .checked_sub(input.claim_transfer_fee)
        .ok_or(PlanningError::InsufficientClaimCredit)?;
    let pre_backing = claim_backing(input.state.backing)?;
    let one = settlement_candidate(&input, pre_backing, one_credit)?;
    let two = one_credit
        .checked_sub(input.claim_transfer_fee)
        .map(|credit| settlement_candidate(&input, pre_backing, credit))
        .transpose()?;
    let route = finish_route(
        input.state.backing.pooled,
        input.claim_transfer_fee,
        input.parent_exists,
        input.minimum_parent_credit,
        one.route,
        two.as_ref().map(|candidate| candidate.route),
    )?;
    let selected = if route.fee_count == 1 {
        one
    } else {
        two.unwrap()
    };
    Ok(TwoWeekSettlementPlan {
        post_backing: checked_add(pre_backing, route.claim_credit)?,
        post_claims: checked_add(input.state.claims, selected.distributed)?,
        post_active_backing: checked_add(input.state.active_backing, selected.distributed)?,
        post_active_reward: checked_add(input.state.active_reward, selected.reward_delta)?,
        reward_target: selected.route.reward_target,
        maximum_io_pool: selected.maximum_io_pool,
        rewards: selected.rewards,
        distributed_io: selected.distributed,
        recipient_io_fees: selected.recipient_fees,
        snapshot_fingerprint: input.snapshot_fingerprint,
        permanent_credit,
        route,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credit(id: u64, accumulated_eligible_credit: u128) -> EntitlementCredit {
        EntitlementCredit {
            sns_neuron_id: vec![id as u8; 32],
            accumulated_eligible_credit,
        }
    }

    fn sum_allocations(outcome: &AllocationOutcome) -> u128 {
        outcome
            .allocations
            .iter()
            .map(|allocation| allocation.io_e8s)
            .sum()
    }

    #[test]
    fn unequal_credits_allocate_a_large_pool_one_to_two_to_three() {
        let outcome = allocate_rewards(
            600_000,
            600,
            &[credit(1, 100), credit(2, 200), credit(3, 300)],
        )
        .unwrap();
        assert_eq!(
            outcome
                .allocations
                .iter()
                .map(|allocation| allocation.io_e8s)
                .collect::<Vec<_>>(),
            vec![100_000, 200_000, 300_000]
        );
        assert_eq!(outcome.forfeited_io_e8s, 0);
        assert_eq!(outcome.rounding_dust_e8s, 0);
    }

    #[test]
    fn tiny_pool_has_deterministic_dust_and_conserves_the_pool() {
        let outcome = allocate_rewards(2, 3, &[credit(1, 1), credit(2, 1), credit(3, 1)]).unwrap();
        assert!(outcome.allocations.is_empty());
        assert_eq!(outcome.rounding_dust_e8s, 2);
        assert_eq!(outcome.forfeited_io_e8s, 0);
        assert_eq!(sum_allocations(&outcome) + outcome.rounding_dust_e8s, 2);
    }

    #[test]
    fn zero_eligible_credit_forfeits_the_full_pool() {
        let outcome = allocate_rewards(100, DAILY_EVENT_CREDIT, &[]).unwrap();
        assert!(outcome.allocations.is_empty());
        assert_eq!(outcome.forfeited_io_e8s, 100);
        assert_eq!(outcome.rounding_dust_e8s, 0);
    }

    #[test]
    fn deterministic_order_preserves_canonical_input_order() {
        let outcome =
            allocate_rewards(30, 30, &[credit(7, 10), credit(42, 10), credit(99, 10)]).unwrap();
        assert_eq!(
            outcome
                .allocations
                .iter()
                .map(|allocation| allocation.sns_neuron_id[0])
                .collect::<Vec<_>>(),
            vec![7, 42, 99]
        );
    }

    #[test]
    fn max_value_allocation_is_exact_and_overflowing_total_fails_closed() {
        let exact = allocate_rewards(u128::MAX, u128::MAX, &[credit(1, u128::MAX)]).unwrap();
        assert_eq!(exact.allocations[0].io_e8s, u128::MAX);
        assert_eq!(exact.rounding_dust_e8s, 0);
        assert_eq!(
            allocate_rewards(100, u128::MAX, &[credit(1, u128::MAX), credit(2, 1)]),
            Err(RewardPolicyError::ArithmeticOverflow)
        );
    }

    #[test]
    fn excluded_half_is_forfeited_without_redistribution() {
        let outcome = allocate_rewards(
            1_000,
            DAILY_EVENT_CREDIT,
            &[credit(1, DAILY_EVENT_CREDIT / 2)],
        )
        .unwrap();
        assert_eq!(sum_allocations(&outcome), 500);
        assert_eq!(outcome.forfeited_io_e8s, 500);
        assert_eq!(outcome.rounding_dust_e8s, 0);
    }

    #[test]
    fn distributed_forfeited_and_dust_conserve_the_backed_pool() {
        let outcome = allocate_rewards(101, 6, &[credit(1, 1), credit(2, 2)]).unwrap();
        assert_eq!(outcome.forfeited_io_e8s, 51);
        assert_eq!(outcome.rounding_dust_e8s, 1);
        assert_eq!(sum_allocations(&outcome), 49);
        assert_eq!(49 + 51 + 1, 101);
    }

    fn economic_state(pooled: u128, active: u128) -> EconomicState {
        EconomicState {
            backing: io_core_model::Backing {
                liquid: 100_000_000_000 - pooled,
                pooled,
                unwinding: 0,
                transit: 0,
            },
            claims: 100_000_000_000,
            active_backing: active,
            active_reward: 0,
        }
    }

    fn settlement(
        pooled: u128,
        active: u128,
        actual_mint: u128,
        fee: u128,
        parent_exists: bool,
        minimum: u128,
    ) -> TwoWeekSettlementPlan {
        plan_two_week_settlement(TwoWeekSettlementInput {
            state: economic_state(pooled, active),
            actual_mint,
            permanent_transfer_fee: fee,
            claim_transfer_fee: fee,
            parent_exists,
            minimum_parent_credit: minimum,
            policy_credit_total: 1,
            entitlements: &[],
            reward_eligible_ids: &[],
            reserve_io_capacity: u128::MAX,
            io_fee: fee,
            snapshot_fingerprint: [7; 32],
        })
        .unwrap()
    }

    #[test]
    fn joint_planner_selects_each_physical_route() {
        assert_eq!(
            settlement(60_000_000_000, 50_000_000_000, 100_000_000, 10_000, true, 1)
                .route
                .route,
            ClaimRoute::AllLiquid
        );
        assert_eq!(
            settlement(49_900_000_000, 50_000_000_000, 100_000_000, 10_000, true, 1)
                .route
                .route,
            ClaimRoute::AllPool
        );
        assert_eq!(
            settlement(49_980_000_000, 50_000_000_000, 100_000_000, 10_000, true, 1)
                .route
                .route,
            ClaimRoute::Mixed
        );
    }

    #[test]
    fn two_fee_all_pool_candidate_becomes_direct_one_fee_all_pool() {
        let plan = settlement(49_970_010_000, 50_000_000_000, 100_000_000, 10_000, true, 1);
        assert_eq!(plan.route.route, ClaimRoute::AllPool);
        assert_eq!(plan.route.fee_count, 1);
        assert_eq!(plan.route.pooled_credit, 59_990_000);
        assert_eq!(plan.route.liquid_credit, 0);
        assert_eq!(plan.route.over_target, 5_000);
        assert_eq!(plan.route.claim_credit - 59_980_000, 10_000);
    }

    #[test]
    fn optional_second_fee_and_absent_parent_fall_back_to_liquid() {
        let sub_fee = settlement(0, 1, 25, 10, true, 1);
        assert_eq!(sub_fee.route.route, ClaimRoute::AllLiquid);
        assert_eq!(sub_fee.route.fee_count, 1);
        let absent = settlement(0, 1_000, 100_000_000, 10_000, false, 100_000_000);
        assert_eq!(absent.route.route, ClaimRoute::AllLiquid);
        assert_eq!(absent.route.pooled_credit, 0);
    }

    #[test]
    fn settlement_freezes_reward_effects_and_io_fees() {
        let eligible = vec![vec![1; 32]];
        let plan = plan_two_week_settlement(TwoWeekSettlementInput {
            state: economic_state(60_000_000_000, 50_000_000_000),
            actual_mint: 100_000_000,
            permanent_transfer_fee: 10_000,
            claim_transfer_fee: 10_000,
            parent_exists: true,
            minimum_parent_credit: 1,
            policy_credit_total: 2,
            entitlements: &[credit(1, 1)],
            reward_eligible_ids: &eligible,
            reserve_io_capacity: 100_000_000,
            io_fee: 1_000,
            snapshot_fingerprint: [9; 32],
        })
        .unwrap();
        assert!(plan.distributed_io > 0);
        assert_eq!(plan.recipient_io_fees, 1_000);
        assert_eq!(plan.post_active_reward, plan.distributed_io);
        assert_eq!(plan.snapshot_fingerprint, [9; 32]);
        assert_eq!(
            plan.distributed_io + plan.rewards.forfeited_io_e8s + plan.rewards.rounding_dust_e8s,
            plan.maximum_io_pool
        );
    }
}
