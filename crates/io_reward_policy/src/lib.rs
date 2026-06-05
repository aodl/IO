//! Pure IO SNS staking entitlement policy.
//!
//! The policy rewards productive governance staking, not passive lockup. Native
//! SNS maturity is expected to be disabled; this crate allocates protocol-backed
//! IO released by the stream manager.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeuronSnapshot {
    pub neuron_id: u64,
    pub staked_io_e8s: u128,
    pub eligible_seconds: u64,
    pub eligible_closed_proposals: u64,
    pub voted_closed_proposals: u64,
    pub is_genesis_governance_neuron: bool,
    pub is_protocol_owned: bool,
    pub is_dissolving: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewardAllocation {
    pub neuron_id: u64,
    pub io_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllocationOutcome {
    pub allocations: Vec<RewardAllocation>,
    pub dust_e8s: u128,
    pub total_weight: u128,
}

pub fn eligible(n: &NeuronSnapshot) -> bool {
    !n.is_genesis_governance_neuron
        && !n.is_protocol_owned
        && !n.is_dissolving
        && n.staked_io_e8s > 0
        && n.eligible_seconds > 0
}

/// Returns a rational numerator/denominator for voting participation.
/// If no eligible proposals closed while the neuron was eligible, participation is 1.
pub fn participation_ratio(n: &NeuronSnapshot) -> (u128, u128) {
    if n.eligible_closed_proposals == 0 {
        (1, 1)
    } else {
        (
            u128::from(n.voted_closed_proposals.min(n.eligible_closed_proposals)),
            u128::from(n.eligible_closed_proposals),
        )
    }
}

pub fn reward_weight(n: &NeuronSnapshot) -> u128 {
    if !eligible(n) {
        return 0;
    }
    let (num, den) = participation_ratio(n);
    let stake_time = n
        .staked_io_e8s
        .saturating_mul(u128::from(n.eligible_seconds));
    if num >= den {
        stake_time
    } else {
        // Divide before the final multiplication when possible to reduce overflow risk,
        // while preserving the intended conservative floor rounding.
        let quotient = stake_time / den;
        let remainder = stake_time % den;
        quotient
            .saturating_mul(num)
            .saturating_add(remainder.saturating_mul(num) / den)
    }
}

pub fn allocate_rewards(reward_pool_io_e8s: u128, neurons: &[NeuronSnapshot]) -> AllocationOutcome {
    let weights: Vec<(u64, u128)> = neurons
        .iter()
        .map(|n| (n.neuron_id, reward_weight(n)))
        .collect();
    let total_weight: u128 = weights
        .iter()
        .map(|(_, w)| *w)
        .fold(0u128, |acc, w| acc.saturating_add(w));
    if reward_pool_io_e8s == 0 || total_weight == 0 {
        return AllocationOutcome {
            allocations: vec![],
            dust_e8s: reward_pool_io_e8s,
            total_weight,
        };
    }
    let mut issued = 0u128;
    let mut allocations = Vec::new();
    for (neuron_id, weight) in weights {
        if weight == 0 {
            continue;
        }
        let amount = reward_pool_io_e8s.saturating_mul(weight) / total_weight;
        issued = issued.saturating_add(amount);
        if amount > 0 {
            allocations.push(RewardAllocation {
                neuron_id,
                io_e8s: amount,
            });
        }
    }
    AllocationOutcome {
        allocations,
        dust_e8s: reward_pool_io_e8s.saturating_sub(issued),
        total_weight,
    }
}

pub fn active_staked_io_e8s(neurons: &[NeuronSnapshot]) -> u128 {
    neurons
        .iter()
        .filter(|n| eligible(n))
        .map(|n| n.staked_io_e8s)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn n(id: u64, stake: u128, voted: u64, total: u64) -> NeuronSnapshot {
        NeuronSnapshot {
            neuron_id: id,
            staked_io_e8s: stake,
            eligible_seconds: 100,
            eligible_closed_proposals: total,
            voted_closed_proposals: voted,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: false,
        }
    }

    #[test]
    fn non_voter_gets_zero_when_proposals_closed() {
        assert_eq!(reward_weight(&n(1, 1_000, 0, 4)), 0);
    }

    #[test]
    fn half_voter_gets_half_stake_time_weight() {
        assert_eq!(reward_weight(&n(1, 1_000, 2, 4)), 50_000);
    }

    #[test]
    fn no_closed_proposals_does_not_penalise() {
        assert_eq!(reward_weight(&n(1, 1_000, 0, 0)), 100_000);
    }

    #[test]
    fn votes_are_capped_at_eligible_closed_proposals() {
        assert_eq!(participation_ratio(&n(1, 1_000, 9, 4)), (4, 4));
        assert_eq!(reward_weight(&n(1, 1_000, 9, 4)), 100_000);
    }

    #[test]
    fn genesis_protocol_owned_dissolving_zero_stake_and_zero_time_neurons_are_excluded() {
        let mut g = n(1, 1_000, 1, 1);
        g.is_genesis_governance_neuron = true;
        let mut p = n(2, 1_000, 1, 1);
        p.is_protocol_owned = true;
        let mut d = n(3, 1_000, 1, 1);
        d.is_dissolving = true;
        let z = n(4, 0, 1, 1);
        let mut t = n(5, 1_000, 1, 1);
        t.eligible_seconds = 0;
        for neuron in [&g, &p, &d, &z, &t] {
            assert!(!eligible(neuron));
            assert_eq!(reward_weight(neuron), 0);
        }
    }

    #[test]
    fn allocation_respects_participation_weighting() {
        let neurons = vec![n(1, 1_000, 4, 4), n(2, 1_000, 2, 4), n(3, 1_000, 0, 4)];
        let out = allocate_rewards(150, &neurons);
        assert_eq!(
            out.allocations,
            vec![
                RewardAllocation {
                    neuron_id: 1,
                    io_e8s: 100
                },
                RewardAllocation {
                    neuron_id: 2,
                    io_e8s: 50
                }
            ]
        );
        assert_eq!(out.dust_e8s, 0);
    }

    #[test]
    fn allocation_respects_stake_time_not_snapshot_only() {
        let mut a = n(1, 1_000, 1, 1);
        let mut b = n(2, 1_000, 1, 1);
        a.eligible_seconds = 200;
        b.eligible_seconds = 100;
        let out = allocate_rewards(300, &[a, b]);
        assert_eq!(
            out.allocations,
            vec![
                RewardAllocation {
                    neuron_id: 1,
                    io_e8s: 200
                },
                RewardAllocation {
                    neuron_id: 2,
                    io_e8s: 100
                }
            ]
        );
    }

    #[test]
    fn dust_is_reported_not_lost() {
        let neurons = vec![n(1, 1, 1, 1), n(2, 1, 1, 1), n(3, 1, 1, 1)];
        let out = allocate_rewards(100, &neurons);
        assert_eq!(out.allocations.iter().map(|a| a.io_e8s).sum::<u128>(), 99);
        assert_eq!(out.dust_e8s, 1);
    }

    #[test]
    fn no_eligible_neurons_leaves_entire_pool_as_dust() {
        let mut g = n(1, 1_000, 1, 1);
        g.is_genesis_governance_neuron = true;
        let out = allocate_rewards(123, &[g]);
        assert!(out.allocations.is_empty());
        assert_eq!(out.dust_e8s, 123);
        assert_eq!(out.total_weight, 0);
    }

    #[test]
    fn active_staked_supply_excludes_ineligible_neurons() {
        let mut g = n(1, 10, 1, 1);
        g.is_genesis_governance_neuron = true;
        let mut d = n(2, 20, 1, 1);
        d.is_dissolving = true;
        let e = n(3, 30, 1, 1);
        assert_eq!(active_staked_io_e8s(&[g, d, e]), 30);
    }
}

#[cfg(test)]
mod additional_reward_tests {
    use super::*;

    fn n(id: u64, stake: u128, seconds: u64, voted: u64, total: u64) -> NeuronSnapshot {
        NeuronSnapshot {
            neuron_id: id,
            staked_io_e8s: stake,
            eligible_seconds: seconds,
            eligible_closed_proposals: total,
            voted_closed_proposals: voted,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: false,
        }
    }

    #[test]
    fn allocation_order_is_stable_for_historian_replay() {
        let neurons = vec![n(42, 10, 10, 1, 1), n(7, 10, 10, 1, 1), n(99, 10, 10, 1, 1)];
        let out = allocate_rewards(30, &neurons);
        assert_eq!(
            out.allocations
                .iter()
                .map(|a| a.neuron_id)
                .collect::<Vec<_>>(),
            vec![42, 7, 99]
        );
    }

    #[test]
    fn tiny_reward_pool_reports_dust_when_each_share_rounds_to_zero() {
        let neurons = vec![n(1, 1, 1, 1, 1), n(2, 1, 1, 1, 1), n(3, 1, 1, 1, 1)];
        let out = allocate_rewards(2, &neurons);
        assert!(out.allocations.is_empty());
        assert_eq!(out.dust_e8s, 2);
    }

    #[test]
    fn participation_penalty_is_applied_after_stake_time_not_before() {
        let full = n(1, 1_000, 200, 4, 4);
        let half = n(2, 1_000, 200, 2, 4);
        assert_eq!(reward_weight(&full), 200_000);
        assert_eq!(reward_weight(&half), 100_000);
    }

    #[test]
    fn new_neuron_is_judged_only_against_proposals_it_was_eligible_for() {
        let old = n(1, 1_000, 100, 10, 10);
        let new = n(2, 1_000, 50, 2, 2);
        let out = allocate_rewards(300, &[old, new]);
        assert_eq!(
            out.allocations,
            vec![
                RewardAllocation {
                    neuron_id: 1,
                    io_e8s: 200
                },
                RewardAllocation {
                    neuron_id: 2,
                    io_e8s: 100
                },
            ]
        );
    }

    #[test]
    fn over_voting_count_does_not_boost_above_full_participation() {
        let normal = n(1, 1_000, 100, 4, 4);
        let impossible = n(2, 1_000, 100, 40, 4);
        assert_eq!(reward_weight(&normal), reward_weight(&impossible));
    }

    #[test]
    fn active_staked_io_uses_eligibility_not_participation() {
        let active_non_voter = n(1, 1_000, 100, 0, 10);
        assert_eq!(
            active_staked_io_e8s(std::slice::from_ref(&active_non_voter)),
            1_000
        );
        assert_eq!(reward_weight(&active_non_voter), 0);
    }

    #[test]
    fn zero_reward_pool_produces_no_allocations_even_with_weights() {
        let out = allocate_rewards(0, &[n(1, 1_000, 100, 1, 1)]);
        assert!(out.allocations.is_empty());
        assert_eq!(out.dust_e8s, 0);
        assert!(out.total_weight > 0);
    }
}

#[cfg(test)]
mod additional_policy_safety_tests {
    use super::*;

    fn neuron(id: u64, stake: u128, seconds: u64, voted: u64, total: u64) -> NeuronSnapshot {
        NeuronSnapshot {
            neuron_id: id,
            staked_io_e8s: stake,
            eligible_seconds: seconds,
            eligible_closed_proposals: total,
            voted_closed_proposals: voted,
            is_genesis_governance_neuron: false,
            is_protocol_owned: false,
            is_dissolving: false,
        }
    }

    #[test]
    fn zero_eligible_seconds_is_ineligible_even_if_it_voted() {
        let n = neuron(1, 1_000, 0, 10, 10);
        assert!(!eligible(&n));
        assert_eq!(reward_weight(&n), 0);
    }

    #[test]
    fn saturating_weight_calculation_does_not_panic_on_huge_values() {
        let n = neuron(1, u128::MAX, u64::MAX, u64::MAX, u64::MAX);
        assert_eq!(
            participation_ratio(&n),
            (u128::from(u64::MAX), u128::from(u64::MAX))
        );
        assert_eq!(reward_weight(&n), u128::MAX);
    }

    #[test]
    fn allocation_dust_is_less_than_number_of_positive_weight_neurons() {
        let neurons = vec![
            neuron(1, 1, 1, 1, 1),
            neuron(2, 1, 1, 1, 1),
            neuron(3, 1, 1, 1, 1),
        ];
        let out = allocate_rewards(10, &neurons);
        assert_eq!(out.dust_e8s, 1);
        assert!(out.dust_e8s < out.allocations.len() as u128);
    }

    #[test]
    fn excluded_neurons_do_not_receive_allocations_even_when_reward_pool_is_large() {
        let mut genesis = neuron(1, 1_000, 1_000, 1, 1);
        genesis.is_genesis_governance_neuron = true;
        let mut protocol = neuron(2, 1_000, 1_000, 1, 1);
        protocol.is_protocol_owned = true;
        let mut dissolving = neuron(3, 1_000, 1_000, 1, 1);
        dissolving.is_dissolving = true;
        let out = allocate_rewards(1_000_000, &[genesis, protocol, dissolving]);
        assert!(out.allocations.is_empty());
        assert_eq!(out.dust_e8s, 1_000_000);
    }
}
