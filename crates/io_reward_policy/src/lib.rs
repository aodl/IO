//! Pure allocation of one actually backed IO pool over cumulative entitlement credits.

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
}
