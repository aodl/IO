//! Pure IO SNS staking entitlement policy.
//!
//! The policy allocates protocol-backed IO from exact SNS Governance reward
//! shares. Native SNS maturity is expected to be disabled.

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
pub struct RewardParticipant {
    pub sns_neuron_id: SnsNeuronId,
    pub neuron_id: u64,
    pub frozen_stake_e8s: u128,
    pub reward_shares: u128,
    pub destination_is_currently_eligible: bool,
}

pub fn participant_from_bytes(
    sns_neuron_id: Vec<u8>,
    frozen_stake_e8s: u128,
    reward_shares: u128,
    destination_is_currently_eligible: bool,
) -> Result<RewardParticipant, SnsNeuronIdConversionError> {
    let sns_neuron_id = SnsNeuronId(sns_neuron_id);
    let neuron_id = sns_neuron_id_to_u64(&sns_neuron_id)?;
    Ok(RewardParticipant {
        sns_neuron_id,
        neuron_id,
        frozen_stake_e8s,
        reward_shares,
        destination_is_currently_eligible,
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
    pub forfeited_reward_e8s: u128,
    pub dust_e8s: u128,
    pub total_weight: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewardPolicyError {
    ArithmeticOverflow,
    InvalidDenominator,
}

pub fn eligible(n: &RewardParticipant) -> bool {
    n.destination_is_currently_eligible && n.frozen_stake_e8s > 0
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

pub fn reward_weight_for_event(n: &RewardParticipant, no_settled_proposals: bool) -> u128 {
    if no_settled_proposals {
        n.frozen_stake_e8s
    } else {
        n.reward_shares
    }
}

pub fn reward_weight(n: &RewardParticipant) -> Result<u128, RewardPolicyError> {
    Ok(reward_weight_for_event(n, false))
}

pub fn allocate_rewards(
    reward_pool_io_e8s: u128,
    participants: &[RewardParticipant],
) -> Result<AllocationOutcome, RewardPolicyError> {
    allocate_rewards_for_event(reward_pool_io_e8s, participants, 1)
}

pub fn allocate_rewards_for_event(
    reward_pool_io_e8s: u128,
    participants: &[RewardParticipant],
    settled_proposal_count: u64,
) -> Result<AllocationOutcome, RewardPolicyError> {
    let no_settled_proposals = settled_proposal_count == 0;
    let weights: Vec<(SnsNeuronId, u64, bool, u128)> = participants
        .iter()
        .map(|n| {
            Ok((
                n.sns_neuron_id.clone(),
                n.neuron_id,
                n.destination_is_currently_eligible,
                reward_weight_for_event(n, no_settled_proposals),
            ))
        })
        .collect::<Result<_, RewardPolicyError>>()?;
    let total_weight = weights
        .iter()
        .map(|(_, _, _, w)| *w)
        .try_fold(0u128, |acc, w| acc.checked_add(w))
        .ok_or(RewardPolicyError::ArithmeticOverflow)?;
    if reward_pool_io_e8s == 0 || total_weight == 0 {
        return Ok(AllocationOutcome {
            allocations: vec![],
            rounding_dust_e8s: reward_pool_io_e8s,
            forfeited_reward_e8s: 0,
            dust_e8s: reward_pool_io_e8s,
            total_weight,
        });
    }

    let mut issued = 0u128;
    let mut forfeited_reward_e8s = 0u128;
    let mut allocations = Vec::new();
    for (sns_neuron_id, neuron_id, destination_is_currently_eligible, weight) in weights {
        if weight == 0 {
            continue;
        }
        let amount = mul_div_floor(reward_pool_io_e8s, weight, total_weight)?;
        if destination_is_currently_eligible {
            issued = issued
                .checked_add(amount)
                .ok_or(RewardPolicyError::ArithmeticOverflow)?;
            if amount > 0 {
                allocations.push(RewardAllocation {
                    sns_neuron_id,
                    neuron_id,
                    io_e8s: amount,
                });
            }
        } else {
            forfeited_reward_e8s = forfeited_reward_e8s
                .checked_add(amount)
                .ok_or(RewardPolicyError::ArithmeticOverflow)?;
        }
    }
    let rounding_dust_e8s = reward_pool_io_e8s
        .checked_sub(issued)
        .and_then(|remaining| remaining.checked_sub(forfeited_reward_e8s))
        .ok_or(RewardPolicyError::ArithmeticOverflow)?;
    let dust_e8s = rounding_dust_e8s
        .checked_add(forfeited_reward_e8s)
        .ok_or(RewardPolicyError::ArithmeticOverflow)?;
    Ok(AllocationOutcome {
        allocations,
        rounding_dust_e8s,
        forfeited_reward_e8s,
        dust_e8s,
        total_weight,
    })
}

pub fn active_staked_io_e8s(participants: &[RewardParticipant]) -> Result<u128, RewardPolicyError> {
    participants
        .iter()
        .filter(|n| eligible(n))
        .map(|n| n.frozen_stake_e8s)
        .try_fold(0u128, |sum, stake| sum.checked_add(stake))
        .ok_or(RewardPolicyError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(id: u64, stake: u128, voted: u64, total: u64) -> RewardParticipant {
        let reward_shares = if total == 0 {
            stake
        } else {
            mul_div_floor(stake, u128::from(voted.min(total)), u128::from(total)).unwrap()
        };
        RewardParticipant {
            sns_neuron_id: SnsNeuronId(id.to_be_bytes().to_vec()),
            neuron_id: id,
            frozen_stake_e8s: stake,
            reward_shares,
            destination_is_currently_eligible: true,
        }
    }

    fn sum_allocations(out: &AllocationOutcome) -> u128 {
        out.allocations.iter().map(|a| a.io_e8s).sum()
    }

    #[test]
    fn equal_stake_equal_participation_has_equal_weight_without_time() {
        assert_eq!(reward_weight(&n(1, 1_000, 4, 4)).unwrap(), 1_000);
        assert_eq!(reward_weight(&n(2, 1_000, 4, 4)).unwrap(), 1_000);
        let out = allocate_rewards(200, &[n(1, 1_000, 4, 4), n(2, 1_000, 4, 4)]).unwrap();
        assert_eq!(out.allocations[0].io_e8s, 100);
        assert_eq!(out.allocations[1].io_e8s, 100);
    }

    #[test]
    fn double_stake_has_double_weight_without_time() {
        let out = allocate_rewards(300, &[n(1, 2_000, 1, 1), n(2, 1_000, 1, 1)]).unwrap();
        assert_eq!(out.allocations[0].io_e8s, 200);
        assert_eq!(out.allocations[1].io_e8s, 100);
    }

    #[test]
    fn half_participation_has_half_weight() {
        assert_eq!(reward_weight(&n(1, 1_000, 2, 4)).unwrap(), 500);
    }

    #[test]
    fn no_closed_proposals_has_full_participation() {
        assert_eq!(reward_weight(&n(1, 1_000, 0, 0)).unwrap(), 1_000);
    }

    #[test]
    fn non_voter_has_zero_participation_weight() {
        assert_eq!(reward_weight(&n(1, 1_000, 0, 4)).unwrap(), 0);
    }

    #[test]
    fn over_voting_is_capped() {
        assert_eq!(reward_weight(&n(1, 1_000, 9, 4)).unwrap(), 1_000);
    }

    #[test]
    fn zero_current_destination_eligibility_means_no_transfer() {
        let mut participant = n(1, 1_000, 1, 1);
        participant.destination_is_currently_eligible = false;
        assert!(!eligible(&participant));
        let out = allocate_rewards(100, &[participant]).unwrap();
        assert!(out.allocations.is_empty());
        assert_eq!(out.forfeited_reward_e8s, 100);
        assert_eq!(out.dust_e8s, 100);
    }

    #[test]
    fn zero_stake_has_zero_weight() {
        assert_eq!(reward_weight(&n(1, 0, 1, 1)).unwrap(), 0);
    }

    #[test]
    fn forfeited_destination_share_becomes_dust_not_redistribution() {
        let eligible = n(1, 1_000, 1, 1);
        let mut forfeited = n(2, 1_000, 1, 1);
        forfeited.destination_is_currently_eligible = false;
        let out = allocate_rewards(101, &[eligible, forfeited]).unwrap();
        assert_eq!(out.allocations[0].io_e8s, 50);
        assert_eq!(out.forfeited_reward_e8s, 50);
        assert_eq!(out.rounding_dust_e8s, 1);
        assert_eq!(out.dust_e8s, 51);
    }

    #[test]
    fn allocations_plus_all_dust_equal_backed_pool() {
        let mut forfeited = n(3, 1, 1, 1);
        forfeited.destination_is_currently_eligible = false;
        let out = allocate_rewards(100, &[n(1, 1, 1, 1), n(2, 1, 1, 1), forfeited]).unwrap();
        assert_eq!(sum_allocations(&out) + out.dust_e8s, 100);
        assert_eq!(out.rounding_dust_e8s, 1);
        assert_eq!(out.forfeited_reward_e8s, 33);
    }

    #[test]
    fn deterministic_order_preserves_input_order() {
        let out =
            allocate_rewards(30, &[n(42, 10, 1, 1), n(7, 10, 1, 1), n(99, 10, 1, 1)]).unwrap();
        assert_eq!(
            out.allocations
                .iter()
                .map(|a| a.neuron_id)
                .collect::<Vec<_>>(),
            vec![42, 7, 99]
        );
    }

    #[test]
    fn tiny_reward_pool_reports_rounding_dust() {
        let out = allocate_rewards(2, &[n(1, 1, 1, 1), n(2, 1, 1, 1), n(3, 1, 1, 1)]).unwrap();
        assert!(out.allocations.is_empty());
        assert_eq!(out.rounding_dust_e8s, 2);
        assert_eq!(out.dust_e8s, 2);
    }

    #[test]
    fn max_value_weight_does_not_panic() {
        let weight = reward_weight(&n(1, u128::MAX, 1, 2)).unwrap();
        assert_eq!(weight, u128::MAX / 2);
    }

    #[test]
    fn max_value_allocation_fails_closed_or_computes_exactly() {
        let outcome = allocate_rewards(u128::MAX, &[n(1, u128::MAX, 1, 1)]).unwrap();
        assert_eq!(outcome.allocations[0].io_e8s, u128::MAX);
        assert_eq!(outcome.dust_e8s, 0);
    }

    #[test]
    fn allocation_never_exceeds_pool() {
        let outcome = allocate_rewards(
            u128::MAX - 1,
            &[
                n(1, u128::MAX, 1, 3),
                n(2, u128::MAX - 1, 2, 3),
                n(3, u128::MAX - 2, 0, 3),
            ],
        )
        .unwrap();
        assert!(sum_allocations(&outcome) < u128::MAX);
    }

    #[test]
    fn allocations_plus_dust_always_equal_pool() {
        let mut forfeited = n(4, u128::MAX / 8, 1, 2);
        forfeited.destination_is_currently_eligible = false;
        let pool = u128::MAX - 9;
        let outcome = allocate_rewards(
            pool,
            &[
                n(1, u128::MAX / 8, 1, 1),
                n(2, u128::MAX / 8 - 1, 1, 2),
                n(3, 10_000_000_000, 1, 3),
                forfeited,
            ],
        )
        .unwrap();
        assert_eq!(sum_allocations(&outcome) + outcome.dust_e8s, pool);
    }

    #[test]
    fn overflowing_weight_total_fails_closed() {
        assert_eq!(
            allocate_rewards(100, &[n(1, u128::MAX, 1, 1), n(2, 1, 1, 1)]),
            Err(RewardPolicyError::ArithmeticOverflow)
        );
    }

    #[test]
    fn active_staked_io_uses_current_destination_eligibility_not_participation() {
        let active_non_voter = n(1, 1_000, 0, 10);
        assert_eq!(active_staked_io_e8s(&[active_non_voter]), Ok(1_000));
    }
}
