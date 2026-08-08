//! Pure allocation of one actually backed IO pool over cumulative entitlement weights.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnsNeuronId(pub Vec<u8>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnsNeuronIdConversionError {
    Empty,
}

pub fn sns_neuron_id_to_u64(id: &SnsNeuronId) -> Result<u64, SnsNeuronIdConversionError> {
    if id.0.is_empty() {
        return Err(SnsNeuronIdConversionError::Empty);
    }
    if let Ok(bytes) = <[u8; 8]>::try_from(id.0.as_slice()) {
        return Ok(u64::from_be_bytes(bytes));
    }
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in &id.0 {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Ok(hash.max(1))
}

pub fn sns_neuron_id_is_valid(id: &SnsNeuronId) -> bool {
    sns_neuron_id_to_u64(id).is_ok()
}

pub fn sns_neuron_id_is_canonical_staking_subaccount(id: &SnsNeuronId) -> bool {
    id.0.len() == 32
}

pub fn compatibility_sns_neuron_id_from_u64(id: u64) -> SnsNeuronId {
    SnsNeuronId(id.to_be_bytes().to_vec())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntitlementWeight {
    pub sns_neuron_id: SnsNeuronId,
    pub neuron_id: u64,
    pub accumulated_weight: u128,
}

pub fn entitlement_from_bytes(
    sns_neuron_id: Vec<u8>,
    accumulated_weight: u128,
) -> Result<EntitlementWeight, SnsNeuronIdConversionError> {
    let sns_neuron_id = SnsNeuronId(sns_neuron_id);
    let neuron_id = sns_neuron_id_to_u64(&sns_neuron_id)?;
    Ok(EntitlementWeight {
        sns_neuron_id,
        neuron_id,
        accumulated_weight,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewardAllocation {
    pub sns_neuron_id: SnsNeuronId,
    pub neuron_id: u64,
    pub io_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllocationOutcome {
    pub allocations: Vec<RewardAllocation>,
    pub rounding_dust_e8s: u128,
    pub total_weight: u128,
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

fn mul_div_floor(
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
    entitlements: &[EntitlementWeight],
) -> Result<AllocationOutcome, RewardPolicyError> {
    let total_weight = entitlements
        .iter()
        .map(|entry| entry.accumulated_weight)
        .try_fold(0u128, |sum, weight| sum.checked_add(weight))
        .ok_or(RewardPolicyError::ArithmeticOverflow)?;
    if reward_pool_io_e8s == 0 || total_weight == 0 {
        return Ok(AllocationOutcome {
            allocations: Vec::new(),
            rounding_dust_e8s: reward_pool_io_e8s,
            total_weight,
        });
    }
    let mut issued = 0u128;
    let mut allocations = Vec::new();
    for entitlement in entitlements {
        if entitlement.accumulated_weight == 0 {
            continue;
        }
        let amount = mul_div_floor(
            reward_pool_io_e8s,
            entitlement.accumulated_weight,
            total_weight,
        )?;
        issued = issued
            .checked_add(amount)
            .ok_or(RewardPolicyError::ArithmeticOverflow)?;
        if amount > 0 {
            allocations.push(RewardAllocation {
                sns_neuron_id: entitlement.sns_neuron_id.clone(),
                neuron_id: entitlement.neuron_id,
                io_e8s: amount,
            });
        }
    }
    Ok(AllocationOutcome {
        allocations,
        rounding_dust_e8s: reward_pool_io_e8s
            .checked_sub(issued)
            .ok_or(RewardPolicyError::ArithmeticOverflow)?,
        total_weight,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weight(id: u64, accumulated_weight: u128) -> EntitlementWeight {
        EntitlementWeight {
            sns_neuron_id: SnsNeuronId(id.to_be_bytes().to_vec()),
            neuron_id: id,
            accumulated_weight,
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
    fn unequal_weights_allocate_a_large_pool_one_to_two_to_three() {
        let outcome =
            allocate_rewards(600_000, &[weight(1, 100), weight(2, 200), weight(3, 300)]).unwrap();
        assert_eq!(
            outcome
                .allocations
                .iter()
                .map(|allocation| allocation.io_e8s)
                .collect::<Vec<_>>(),
            vec![100_000, 200_000, 300_000]
        );
        assert_eq!(outcome.rounding_dust_e8s, 0);
    }

    #[test]
    fn tiny_pool_has_deterministic_dust_and_conserves_the_pool() {
        let outcome = allocate_rewards(2, &[weight(1, 1), weight(2, 1), weight(3, 1)]).unwrap();
        assert!(outcome.allocations.is_empty());
        assert_eq!(outcome.rounding_dust_e8s, 2);
        assert_eq!(sum_allocations(&outcome) + outcome.rounding_dust_e8s, 2);
    }

    #[test]
    fn zero_weight_batch_keeps_the_full_pool_as_dust() {
        let outcome = allocate_rewards(100, &[]).unwrap();
        assert!(outcome.allocations.is_empty());
        assert_eq!(outcome.total_weight, 0);
        assert_eq!(outcome.rounding_dust_e8s, 100);
    }

    #[test]
    fn deterministic_order_preserves_canonical_input_order() {
        let outcome =
            allocate_rewards(30, &[weight(7, 10), weight(42, 10), weight(99, 10)]).unwrap();
        assert_eq!(
            outcome
                .allocations
                .iter()
                .map(|allocation| allocation.neuron_id)
                .collect::<Vec<_>>(),
            vec![7, 42, 99]
        );
    }

    #[test]
    fn max_value_allocation_is_exact_and_overflowing_total_fails_closed() {
        let exact = allocate_rewards(u128::MAX, &[weight(1, u128::MAX)]).unwrap();
        assert_eq!(exact.allocations[0].io_e8s, u128::MAX);
        assert_eq!(exact.rounding_dust_e8s, 0);
        assert_eq!(
            allocate_rewards(100, &[weight(1, u128::MAX), weight(2, 1)]),
            Err(RewardPolicyError::ArithmeticOverflow)
        );
    }
}
