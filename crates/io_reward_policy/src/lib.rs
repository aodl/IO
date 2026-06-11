//! Pure IO SNS staking entitlement policy.
//!
//! The policy rewards productive governance staking, not passive lockup. Native
//! SNS maturity is expected to be disabled; this crate allocates protocol-backed
//! IO released by the stream manager.

use io_governance_types::SnsNeuronId;

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
    Ok(hash)
}

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

#[cfg(test)]
mod sns_governance_allocation_tests {
    use super::*;
    use io_governance_types::{
        snapshot_sns_eligibility, summarize_sns_participation, SnsBallot, SnsDissolveState,
        SnsEligibilityPolicy, SnsNeuron, SnsNeuronEligibility, SnsNeuronId, SnsParticipationPolicy,
        SnsParticipationSummary, SnsProposal, SnsProposalId, SnsProposalRewardStatus,
        SnsProposalStatus, SnsVote,
    };
    use std::collections::BTreeSet;

    #[test]
    fn equal_eligible_neurons_get_equal_backed_io_allocation() {
        let neurons = vec![
            sns_neuron(1, 1_000, false, false),
            sns_neuron(2, 1_000, false, false),
        ];
        let snapshots = snapshots_from_governance(
            &neurons,
            &[
                proposal(1, 10, &[(1, SnsVote::Yes), (2, SnsVote::No)]),
                proposal(
                    2,
                    20,
                    &[(1, SnsVote::FollowedYes), (2, SnsVote::FollowedNo)],
                ),
            ],
        );
        let out = allocate_rewards(200, &snapshots);
        assert_eq!(out.allocations[0].io_e8s, 100);
        assert_eq!(out.allocations[1].io_e8s, 100);
    }

    #[test]
    fn eligible_sns_staking_increases_io_reward_entitlement() {
        let no_staking = allocate_rewards(500, &snapshots_from_governance(&[], &[]));
        assert!(no_staking.allocations.is_empty());
        assert_eq!(no_staking.dust_e8s, 500);

        let eligible = snapshots_from_governance(
            &[sns_neuron(1, 1_000, false, false)],
            &[proposal(1, 10, &[(1, SnsVote::Yes)])],
        );
        let staking = allocate_rewards(500, &eligible);
        assert_eq!(
            staking.allocations,
            vec![RewardAllocation {
                neuron_id: 1,
                io_e8s: 500
            }]
        );
    }

    #[test]
    fn increasing_staked_io_increases_reward_weight_without_double_counting() {
        let before = snapshots_from_governance(
            &[sns_neuron(1, 1_000, false, false)],
            &[proposal(1, 10, &[(1, SnsVote::Yes)])],
        );
        let after = snapshots_from_governance(
            &[sns_neuron(1, 3_000, false, false)],
            &[proposal(1, 10, &[(1, SnsVote::Yes)])],
        );

        assert_eq!(before.len(), 1);
        assert_eq!(after.len(), 1);
        assert_eq!(before[0].staked_io_e8s, 1_000);
        assert_eq!(after[0].staked_io_e8s, 3_000);
        assert_eq!(reward_weight(&after[0]), reward_weight(&before[0]) * 3);
    }

    #[test]
    fn ineligible_sns_staking_does_not_increase_reward_entitlement() {
        let mut short_delay = sns_neuron(1, 1_000, false, false);
        short_delay.dissolve_delay_seconds = 60;
        short_delay.dissolve_state = SnsDissolveState::NotDissolving {
            dissolve_delay_seconds: 60,
        };
        let mut dissolving = sns_neuron(2, 1_000, false, false);
        dissolving.dissolve_state = SnsDissolveState::Dissolving {
            when_dissolved_timestamp_seconds: 200,
        };
        let mut liquid_only = sns_neuron(3, 0, false, false);
        liquid_only.cached_neuron_stake_e8s = 0;

        let snapshots = snapshots_from_governance(
            &[short_delay, dissolving, liquid_only],
            &[proposal(
                1,
                10,
                &[(1, SnsVote::Yes), (2, SnsVote::Yes), (3, SnsVote::Yes)],
            )],
        );
        let out = allocate_rewards(500, &snapshots);
        assert!(out.allocations.is_empty());
        assert_eq!(out.dust_e8s, 500);
        assert_eq!(active_staked_io_e8s(&snapshots), 0);
    }

    #[test]
    fn half_participation_gets_half_weight() {
        let neurons = vec![
            sns_neuron(1, 1_000, false, false),
            sns_neuron(2, 1_000, false, false),
        ];
        let snapshots = snapshots_from_governance(
            &neurons,
            &[
                proposal(1, 10, &[(1, SnsVote::Yes), (2, SnsVote::Yes)]),
                proposal(2, 20, &[(1, SnsVote::Yes)]),
            ],
        );
        let out = allocate_rewards(300, &snapshots);
        assert_eq!(out.allocations[0].io_e8s, 200);
        assert_eq!(out.allocations[1].io_e8s, 100);
    }

    #[test]
    fn halfway_eligible_neuron_gets_half_stake_time_weight() {
        let neurons = vec![
            sns_neuron(1, 1_000, false, false),
            sns_neuron(2, 1_000, false, false),
        ];
        let mut eligibilities = base_eligibilities(&neurons);
        eligibilities[0].eligible_since_seconds = 0;
        eligibilities[1].eligible_since_seconds = 50;
        let summaries = summarize_sns_participation(
            &eligibilities,
            &[proposal(1, 75, &[(1, SnsVote::Yes), (2, SnsVote::Yes)])],
            &participation_policy(),
        );
        let snapshots = snapshots_from_eligibility(&eligibilities, &summaries, 100);
        let out = allocate_rewards(300, &snapshots);
        assert_eq!(out.allocations[0].io_e8s, 200);
        assert_eq!(out.allocations[1].io_e8s, 100);
    }

    #[test]
    fn jupiter_and_protocol_governance_neurons_are_excluded() {
        let neurons = vec![
            sns_neuron(1, 10_000, true, false),
            sns_neuron(2, 10_000, false, true),
            sns_neuron(3, 1_000, false, false),
        ];
        let snapshots = snapshots_from_governance(
            &neurons,
            &[proposal(
                1,
                10,
                &[(1, SnsVote::Yes), (2, SnsVote::Yes), (3, SnsVote::Yes)],
            )],
        );
        let out = allocate_rewards(100, &snapshots);
        assert_eq!(
            out.allocations,
            vec![RewardAllocation {
                neuron_id: 3,
                io_e8s: 100
            }]
        );
    }

    #[test]
    fn no_closed_proposals_gives_full_participation_and_dust_does_not_over_issue() {
        let neurons = vec![
            sns_neuron(1, 1, false, false),
            sns_neuron(2, 1, false, false),
            sns_neuron(3, 1, false, false),
        ];
        let snapshots = snapshots_from_governance(&neurons, &[]);
        let out = allocate_rewards(100, &snapshots);
        assert_eq!(out.allocations.iter().map(|a| a.io_e8s).sum::<u128>(), 99);
        assert_eq!(out.dust_e8s, 1);
    }

    #[test]
    fn eight_byte_sns_neuron_id_converts_correctly() {
        let id = SnsNeuronId(42u64.to_be_bytes().to_vec());
        assert_eq!(sns_neuron_id_to_u64(&id), Ok(42));
    }

    #[test]
    fn non_eight_byte_sns_neuron_id_does_not_convert_to_zero() {
        let id = SnsNeuronId(vec![0]);
        assert_ne!(sns_neuron_id_to_u64(&id), Ok(0));
    }

    #[test]
    fn different_non_eight_byte_sns_neuron_ids_do_not_collide_as_zero() {
        let one_byte_id = SnsNeuronId(vec![1]);
        let nine_byte_id = SnsNeuronId(vec![0; 9]);
        let one_byte_key = sns_neuron_id_to_u64(&one_byte_id).unwrap();
        let nine_byte_key = sns_neuron_id_to_u64(&nine_byte_id).unwrap();
        assert_ne!(one_byte_key, 0);
        assert_ne!(nine_byte_key, 0);
        assert_ne!(one_byte_key, nine_byte_key);
    }

    #[test]
    fn empty_sns_neuron_id_is_rejected() {
        let id = SnsNeuronId(Vec::new());
        assert_eq!(
            sns_neuron_id_to_u64(&id),
            Err(SnsNeuronIdConversionError::Empty)
        );
    }

    fn sns_neuron(
        id: u64,
        stake: u128,
        jupiter_governance: bool,
        protocol_owned: bool,
    ) -> SnsNeuron {
        SnsNeuron {
            id: SnsNeuronId(id.to_be_bytes().to_vec()),
            controller: None,
            stake_e8s: stake,
            dissolve_delay_seconds: 14 * 24 * 60 * 60,
            dissolve_state: SnsDissolveState::NotDissolving {
                dissolve_delay_seconds: 14 * 24 * 60 * 60,
            },
            cached_neuron_stake_e8s: stake,
            voting_power: stake,
            permissions: Vec::new(),
            is_io_protocol_neuron: protocol_owned,
            is_jupiter_governance_neuron: jupiter_governance,
        }
    }

    fn base_eligibilities(neurons: &[SnsNeuron]) -> Vec<SnsNeuronEligibility> {
        snapshot_sns_eligibility(
            neurons,
            &SnsEligibilityPolicy {
                protocol_neuron_ids: BTreeSet::new(),
                jupiter_governance_neuron_ids: BTreeSet::new(),
                minimum_dissolve_delay_seconds: 14 * 24 * 60 * 60,
                require_non_dissolving: true,
                current_timestamp_seconds: 0,
            },
        )
    }

    fn snapshots_from_governance(
        neurons: &[SnsNeuron],
        proposals: &[SnsProposal],
    ) -> Vec<NeuronSnapshot> {
        let eligibilities = base_eligibilities(neurons);
        let summaries =
            summarize_sns_participation(&eligibilities, proposals, &participation_policy());
        snapshots_from_eligibility(&eligibilities, &summaries, 100)
    }

    fn snapshots_from_eligibility(
        eligibilities: &[SnsNeuronEligibility],
        summaries: &[SnsParticipationSummary],
        epoch_seconds: u64,
    ) -> Vec<NeuronSnapshot> {
        eligibilities
            .iter()
            .filter(|eligibility| eligibility.excluded_reason.is_none())
            .filter_map(|eligibility| {
                let summary = summaries
                    .iter()
                    .find(|summary| summary.neuron_id == eligibility.neuron_id)?;
                Some(NeuronSnapshot {
                    neuron_id: eight_byte_fixture_sns_neuron_id_to_u64(&eligibility.neuron_id),
                    staked_io_e8s: eligibility.eligible_stake_e8s,
                    eligible_seconds: epoch_seconds
                        .saturating_sub(eligibility.eligible_since_seconds.min(epoch_seconds)),
                    eligible_closed_proposals: summary.eligible_closed_proposals_total,
                    voted_closed_proposals: summary.voted_proposals,
                    is_genesis_governance_neuron: false,
                    is_protocol_owned: false,
                    is_dissolving: !eligibility.is_non_dissolving,
                })
            })
            .collect()
    }

    fn eight_byte_fixture_sns_neuron_id_to_u64(id: &SnsNeuronId) -> u64 {
        sns_neuron_id_to_u64(id).expect("SNS governance allocation fixtures use 8-byte IDs")
    }

    fn participation_policy() -> SnsParticipationPolicy {
        SnsParticipationPolicy {
            count_direct_votes: true,
            count_followed_votes: true,
            excluded_topics: BTreeSet::new(),
            epoch_start_seconds: 0,
            epoch_end_seconds: 100,
        }
    }

    fn proposal(id: u64, decided: u64, votes: &[(u64, SnsVote)]) -> SnsProposal {
        SnsProposal {
            id: SnsProposalId(id),
            topic: Some(1),
            status: SnsProposalStatus::Adopted,
            reward_status: SnsProposalRewardStatus::Settled,
            decided_timestamp_seconds: Some(decided),
            ballots: votes
                .iter()
                .map(|(neuron_id, vote)| SnsBallot {
                    neuron_id: SnsNeuronId(neuron_id.to_be_bytes().to_vec()),
                    vote: *vote,
                })
                .collect(),
        }
    }
}
