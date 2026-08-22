use candid::Principal;

use crate::{
    api::ApiError,
    state::{
        Account, RewardEntitlementEntry, RewardEventClassification, RewardEventCredit,
        RewardEventId,
    },
};
use io_sns_reward_boundary::{
    self as reward_governance, Error, EventId, EventSequence, EventSequenceError, Neuron,
    RewardEvent,
};

fn governance_error(error: Error) -> ApiError {
    match error {
        Error::Retryable { method, message } => {
            ApiError::Pending(format!("SNS {method} failed: {message}"))
        }
        Error::Invalid { method, message } => {
            ApiError::Invalid(format!("SNS {method} failed: {message}"))
        }
    }
}

pub(crate) async fn installed_governance(
    root: Principal,
    governance: Principal,
) -> Result<reward_governance::InstalledGovernance, ApiError> {
    reward_governance::installed_governance(root, governance)
        .await
        .map_err(governance_error)
}

pub(crate) async fn latest_reward_event(governance: Principal) -> Result<RewardEvent, ApiError> {
    reward_governance::latest_reward_event(governance)
        .await
        .map_err(governance_error)
}

pub(crate) fn event_id(event: &RewardEvent) -> Result<RewardEventId, ApiError> {
    let EventId {
        end_timestamp_seconds,
        round,
    } = reward_governance::event_id(event).map_err(event_sequence_error)?;
    Ok(RewardEventId {
        end_timestamp_seconds,
        round,
    })
}

pub(crate) fn classify_sequence(
    previous: Option<RewardEventId>,
    next: &RewardEvent,
) -> Result<EventSequence, ApiError> {
    reward_governance::classify_event_sequence(
        previous.map(|event| EventId {
            end_timestamp_seconds: event.end_timestamp_seconds,
            round: event.round,
        }),
        next,
    )
    .map_err(event_sequence_error)
}

fn event_sequence_error(error: EventSequenceError) -> ApiError {
    match error {
        EventSequenceError::MissingEndTimestamp => {
            ApiError::Pending("SNS latest reward event has no end timestamp".into())
        }
        EventSequenceError::Invalid(message) => ApiError::Invalid(format!(
            "invalid canonical reward-event sequence: {message}"
        )),
    }
}

pub(crate) fn require_consistent_event(
    before: &RewardEvent,
    after: &RewardEvent,
) -> Result<(), ApiError> {
    if before == after {
        Ok(())
    } else {
        Err(ApiError::Pending(
            "SNS reward event changed during neuron pagination; discarded transient pages".into(),
        ))
    }
}

pub(crate) async fn list_all_neurons(governance: Principal) -> Result<Vec<Neuron>, ApiError> {
    reward_governance::list_all_neurons(governance)
        .await
        .map_err(governance_error)
}

fn require_unique_neurons(neurons: &[Neuron]) -> Result<(), ApiError> {
    let mut seen = std::collections::BTreeSet::new();
    if neurons.iter().any(|neuron| !seen.insert(&neuron.id)) {
        return Err(ApiError::Invalid(
            "SNS list_neurons returned a duplicate neuron ID".into(),
        ));
    }
    Ok(())
}

fn eligible_destination(
    governance: Principal,
    nonredeemable_governance_io_accounts: &[Account],
    neuron: &Neuron,
) -> Result<Option<Account>, ApiError> {
    if !neuron.is_non_dissolving_for(io_core_model::TWO_WEEK_SECONDS) {
        return Ok(None);
    }
    if neuron.id.len() != 32 {
        return Err(ApiError::Invalid(
            "eligible SNS neuron ID is not a canonical staking subaccount".into(),
        ));
    }
    let destination = Account {
        owner: governance,
        subaccount: Some(neuron.id.clone()),
    };
    let nonredeemable =
        nonredeemable_governance_io_accounts
            .iter()
            .try_fold(false, |matched, account| {
                destination
                    .effective_eq(account)
                    .map(|same| matched || same)
            });
    nonredeemable
        .map(|nonredeemable| (!nonredeemable).then_some(destination))
        .map_err(ApiError::Invalid)
}

#[cfg(test)]
pub(crate) fn event_credits(
    governance: Principal,
    nonredeemable_governance_io_accounts: &[Account],
    event: &RewardEvent,
    neurons: &[Neuron],
) -> Result<(RewardEventClassification, Vec<RewardEventCredit>), ApiError> {
    event_credits_for(
        governance,
        nonredeemable_governance_io_accounts,
        event,
        neurons,
        None,
    )
}

pub(crate) fn event_credits_for(
    governance: Principal,
    nonredeemable_governance_io_accounts: &[Account],
    event: &RewardEvent,
    neurons: &[Neuron],
    reward_eligible_ids: Option<&std::collections::BTreeSet<Vec<u8>>>,
) -> Result<(RewardEventClassification, Vec<RewardEventCredit>), ApiError> {
    let proposal_count = event.settled_proposal_count().map_err(governance_error)?;
    let event_id = event_id(event)?;
    let no_proposals = proposal_count == 0;
    require_unique_neurons(neurons)?;
    let mut canonical_share_total = 0u128;
    let mut eligible_stake_total = 0u128;
    let mut eligible = Vec::new();
    for neuron in neurons {
        let current_shares = if no_proposals {
            0
        } else {
            match neuron.latest_reward_event_participation {
                Some(participation)
                    if participation.reward_event_end_timestamp_seconds
                        == event_id.end_timestamp_seconds =>
                {
                    participation
                        .exact_reward_shares()
                        .map_err(governance_error)?
                }
                Some(_) | None => 0,
            }
        };
        canonical_share_total = canonical_share_total
            .checked_add(current_shares)
            .ok_or_else(|| ApiError::Invalid("canonical reward-share total overflow".into()))?;
        let Some(destination) =
            eligible_destination(governance, nonredeemable_governance_io_accounts, neuron)?
        else {
            continue;
        };
        if reward_eligible_ids.is_some_and(|ids| !ids.contains(&neuron.id)) {
            continue;
        }
        eligible_stake_total = eligible_stake_total
            .checked_add(neuron.cached_neuron_stake_e8s)
            .ok_or_else(|| ApiError::Invalid("eligible stake total overflow".into()))?;
        eligible.push((
            neuron.id.clone(),
            destination,
            current_shares,
            neuron.cached_neuron_stake_e8s,
        ));
    }
    let denominator = if no_proposals {
        eligible_stake_total
    } else {
        canonical_share_total
    };
    let mut credits = Vec::new();
    for (sns_neuron_id, destination, current_shares, eligible_stake) in eligible {
        let numerator = if no_proposals {
            eligible_stake
        } else {
            current_shares
        };
        let event_credit = if denominator == 0 {
            0
        } else {
            io_reward_policy::mul_div_floor(
                io_reward_policy::DAILY_EVENT_CREDIT,
                numerator,
                denominator,
            )
            .map_err(|error| ApiError::Invalid(format!("daily reward credit failed: {error:?}")))?
        };
        if event_credit > 0 {
            credits.push(RewardEventCredit {
                sns_neuron_id,
                destination,
                event_credit,
            });
        }
    }
    credits.sort_by(|left, right| left.sns_neuron_id.cmp(&right.sns_neuron_id));
    let classification = if no_proposals {
        RewardEventClassification::NoProposalFallback
    } else if credits.is_empty() {
        RewardEventClassification::ZeroEligibleParticipation
    } else {
        RewardEventClassification::ProposalBearing
    };
    Ok((classification, credits))
}

pub(crate) fn merge_event_credits(
    existing: &[RewardEntitlementEntry],
    event: &[RewardEventCredit],
) -> Result<Vec<RewardEntitlementEntry>, ApiError> {
    let mut merged = std::collections::BTreeMap::<Vec<u8>, RewardEntitlementEntry>::new();
    for entry in existing {
        if merged
            .insert(entry.sns_neuron_id.clone(), entry.clone())
            .is_some()
        {
            return Err(ApiError::Invalid(
                "entitlement accumulator contains a duplicate neuron ID".into(),
            ));
        }
    }
    let mut event_ids = std::collections::BTreeSet::new();
    for credit in event {
        if !event_ids.insert(credit.sns_neuron_id.clone()) {
            return Err(ApiError::Invalid(
                "reward event contains a duplicate neuron ID".into(),
            ));
        }
        match merged.get_mut(&credit.sns_neuron_id) {
            Some(entry) => {
                if !entry
                    .destination
                    .effective_eq(&credit.destination)
                    .map_err(ApiError::Invalid)?
                {
                    return Err(ApiError::Invalid(
                        "reward destination changed for an accumulated neuron".into(),
                    ));
                }
                entry.accumulated_eligible_credit = entry
                    .accumulated_eligible_credit
                    .checked_add(credit.event_credit)
                    .ok_or_else(|| ApiError::Invalid("entitlement credit overflow".into()))?;
            }
            None if credit.event_credit > 0 => {
                merged.insert(
                    credit.sns_neuron_id.clone(),
                    RewardEntitlementEntry {
                        sns_neuron_id: credit.sns_neuron_id.clone(),
                        destination: credit.destination.clone(),
                        accumulated_eligible_credit: credit.event_credit,
                    },
                );
            }
            None => {}
        }
    }
    if merged.len() > crate::state::RewardEntitlementAccumulator::MAX_ENTRIES {
        return Err(ApiError::Invalid(
            "entitlement accumulator exceeds 1,000 entries".into(),
        ));
    }
    Ok(merged.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use io_sns_reward_boundary::{DissolveState, ProposalId, RewardEventParticipation, Uint128};

    fn principal(value: u8) -> Principal {
        Principal::from_slice(&[value; 29])
    }

    fn event(round: u64, end: u64, proposals: usize) -> RewardEvent {
        RewardEvent {
            rounds_since_last_distribution: Some(1),
            actual_timestamp_seconds: end,
            end_timestamp_seconds: Some(end),
            round,
            settled_proposals: (0..proposals).map(|_| ProposalId { id: 1 }).collect(),
        }
    }

    fn neuron(
        id: u8,
        stake: u128,
        delay: u64,
        participation: Option<(u64, Option<Uint128>)>,
    ) -> Neuron {
        Neuron {
            id: vec![id; 32],
            dissolve_state: DissolveState::NotDissolving {
                dissolve_delay_seconds: delay,
            },
            cached_neuron_stake_e8s: stake,
            latest_reward_event_participation: participation.map(|(timestamp, reward_shares)| {
                RewardEventParticipation {
                    reward_event_end_timestamp_seconds: timestamp,
                    reward_shares,
                }
            }),
        }
    }

    fn no_exclusions() -> Vec<Account> {
        Vec::new()
    }

    fn daily_fraction(numerator: u128, denominator: u128) -> u128 {
        io_reward_policy::mul_div_floor(
            io_reward_policy::DAILY_EVENT_CREDIT,
            numerator,
            denominator,
        )
        .unwrap()
    }

    #[test]
    fn no_proposal_equal_and_unequal_stakes_receive_full_credit() {
        let governance = principal(1);
        let equal = vec![
            neuron(1, 100, io_core_model::TWO_WEEK_SECONDS, None),
            neuron(2, 100, io_core_model::TWO_WEEK_SECONDS, None),
            neuron(3, 100, io_core_model::TWO_WEEK_SECONDS, None),
        ];
        let (classification, weights) =
            event_credits(governance, &no_exclusions(), &event(1, 10, 0), &equal).unwrap();
        assert_eq!(
            classification,
            RewardEventClassification::NoProposalFallback
        );
        assert_eq!(
            weights
                .iter()
                .map(|weight| weight.event_credit)
                .collect::<Vec<_>>(),
            vec![daily_fraction(1, 3); 3]
        );

        let unequal = vec![
            neuron(1, 100, io_core_model::TWO_WEEK_SECONDS, None),
            neuron(2, 200, io_core_model::TWO_WEEK_SECONDS, None),
            neuron(3, 300, io_core_model::TWO_WEEK_SECONDS, None),
        ];
        let (_, weights) =
            event_credits(governance, &no_exclusions(), &event(2, 20, 0), &unequal).unwrap();
        assert_eq!(
            weights
                .iter()
                .map(|weight| weight.event_credit)
                .collect::<Vec<_>>(),
            vec![
                daily_fraction(1, 6),
                daily_fraction(2, 6),
                daily_fraction(3, 6)
            ]
        );
    }

    #[test]
    fn no_proposal_ignores_stale_fields() {
        let neurons = vec![
            neuron(
                1,
                100,
                io_core_model::TWO_WEEK_SECONDS,
                Some((
                    10,
                    Some(Uint128 {
                        high: 0,
                        low: 9_999,
                    }),
                )),
            ),
            neuron(2, 200, io_core_model::TWO_WEEK_SECONDS, None),
        ];
        let (_, weights) =
            event_credits(principal(1), &no_exclusions(), &event(2, 20, 0), &neurons).unwrap();
        assert_eq!(
            weights
                .iter()
                .map(|weight| weight.event_credit)
                .collect::<Vec<_>>(),
            vec![daily_fraction(1, 3), daily_fraction(2, 3)]
        );
    }

    #[test]
    fn proposals_with_no_current_eligible_shares_do_not_fallback() {
        let neurons = vec![
            neuron(1, 100, io_core_model::TWO_WEEK_SECONDS, None),
            neuron(
                2,
                200,
                io_core_model::TWO_WEEK_SECONDS,
                Some((10, Some(Uint128 { high: 0, low: 900 }))),
            ),
            neuron(
                3,
                300,
                io_core_model::TWO_WEEK_SECONDS,
                Some((20, Some(Uint128 { high: 0, low: 0 }))),
            ),
        ];
        let (classification, weights) =
            event_credits(principal(1), &no_exclusions(), &event(2, 20, 1), &neurons).unwrap();
        assert_eq!(
            classification,
            RewardEventClassification::ZeroEligibleParticipation
        );
        assert!(weights.is_empty());
    }

    #[test]
    fn no_proposal_excludes_protocol_jupiter_and_ineligible_neurons() {
        let governance = principal(1);
        let nonredeemable = vec![
            Account {
                owner: governance,
                subaccount: Some(vec![1; 32]),
            },
            Account {
                owner: governance,
                subaccount: Some(vec![2; 32]),
            },
        ];
        let mut dissolving = neuron(3, 100, io_core_model::TWO_WEEK_SECONDS, None);
        dissolving.dissolve_state = DissolveState::Dissolving;
        let neurons = vec![
            neuron(1, 100, io_core_model::TWO_WEEK_SECONDS, None),
            neuron(2, 100, io_core_model::TWO_WEEK_SECONDS, None),
            dissolving,
            neuron(4, 100, io_core_model::TWO_WEEK_SECONDS - 1, None),
            neuron(5, 100, io_core_model::TWO_WEEK_SECONDS + 1, None),
            neuron(6, 0, io_core_model::TWO_WEEK_SECONDS, None),
            neuron(7, 700, io_core_model::TWO_WEEK_SECONDS, None),
        ];
        let (_, weights) =
            event_credits(governance, &nonredeemable, &event(1, 10, 0), &neurons).unwrap();
        assert_eq!(weights.len(), 1);
        assert_eq!(weights[0].sns_neuron_id, vec![7; 32]);
        assert_eq!(
            weights[0].event_credit,
            io_reward_policy::DAILY_EVENT_CREDIT
        );
    }

    #[test]
    fn no_eligible_neurons_consumes_as_zero_weight_without_a_denominator() {
        let neurons = vec![neuron(1, 0, io_core_model::TWO_WEEK_SECONDS, None)];
        let (classification, weights) =
            event_credits(principal(1), &no_exclusions(), &event(1, 10, 0), &neurons).unwrap();
        assert_eq!(
            classification,
            RewardEventClassification::NoProposalFallback
        );
        assert!(weights.is_empty());
    }

    #[test]
    fn current_malformed_shares_fail_closed_but_stale_malformed_shares_are_zero() {
        let malformed = neuron(1, 100, io_core_model::TWO_WEEK_SECONDS, Some((20, None)));
        assert!(event_credits(
            principal(1),
            &no_exclusions(),
            &event(2, 20, 1),
            &[malformed],
        )
        .is_err());
        let stale = neuron(1, 100, io_core_model::TWO_WEEK_SECONDS, Some((10, None)));
        let (_, weights) =
            event_credits(principal(1), &no_exclusions(), &event(2, 20, 1), &[stale]).unwrap();
        assert!(weights.is_empty());
    }

    #[test]
    fn accumulation_is_checked_sorted_and_non_overlapping() {
        let governance = principal(1);
        let destination = |id| Account {
            owner: governance,
            subaccount: Some(vec![id; 32]),
        };
        let first = vec![RewardEventCredit {
            sns_neuron_id: vec![2; 32],
            destination: destination(2),
            event_credit: 200,
        }];
        let accumulated = merge_event_credits(&[], &first).unwrap();
        let next = vec![
            RewardEventCredit {
                sns_neuron_id: vec![1; 32],
                destination: destination(1),
                event_credit: 100,
            },
            RewardEventCredit {
                sns_neuron_id: vec![2; 32],
                destination: destination(2),
                event_credit: 200,
            },
        ];
        let accumulated = merge_event_credits(&accumulated, &next).unwrap();
        assert_eq!(accumulated[0].sns_neuron_id, vec![1; 32]);
        assert_eq!(accumulated[0].accumulated_eligible_credit, 100);
        assert_eq!(accumulated[1].accumulated_eligible_credit, 400);

        let overflow = vec![RewardEntitlementEntry {
            sns_neuron_id: vec![1; 32],
            destination: destination(1),
            accumulated_eligible_credit: u128::MAX,
        }];
        assert!(merge_event_credits(
            &overflow,
            &[RewardEventCredit {
                sns_neuron_id: vec![1; 32],
                destination: destination(1),
                event_credit: 1,
            }]
        )
        .is_err());
    }

    #[test]
    fn fourteen_daily_events_accumulate_without_window_eviction() {
        let governance = principal(1);
        let mut accumulated = Vec::new();
        let cases = [
            (
                event(1, 10, 1),
                vec![
                    neuron(
                        1,
                        100,
                        io_core_model::TWO_WEEK_SECONDS,
                        Some((10, Some(Uint128 { high: 0, low: 100 }))),
                    ),
                    neuron(2, 200, io_core_model::TWO_WEEK_SECONDS, None),
                ],
                [io_reward_policy::DAILY_EVENT_CREDIT, 0],
            ),
            (
                event(2, 20, 0),
                vec![
                    neuron(
                        1,
                        100,
                        io_core_model::TWO_WEEK_SECONDS,
                        Some((10, Some(Uint128 { high: 0, low: 100 }))),
                    ),
                    neuron(2, 200, io_core_model::TWO_WEEK_SECONDS, None),
                ],
                [
                    io_reward_policy::DAILY_EVENT_CREDIT + daily_fraction(1, 3),
                    daily_fraction(2, 3),
                ],
            ),
            (
                event(3, 30, 1),
                vec![
                    neuron(
                        1,
                        100,
                        io_core_model::TWO_WEEK_SECONDS,
                        Some((10, Some(Uint128 { high: 0, low: 100 }))),
                    ),
                    neuron(
                        2,
                        200,
                        io_core_model::TWO_WEEK_SECONDS,
                        Some((30, Some(Uint128 { high: 0, low: 200 }))),
                    ),
                ],
                [
                    io_reward_policy::DAILY_EVENT_CREDIT + daily_fraction(1, 3),
                    daily_fraction(2, 3) + io_reward_policy::DAILY_EVENT_CREDIT,
                ],
            ),
        ];
        for (event, neurons, expected) in cases {
            let (_, weights) =
                event_credits(governance, &no_exclusions(), &event, &neurons).unwrap();
            accumulated = merge_event_credits(&accumulated, &weights).unwrap();
            let actual = [1_u8, 2].map(|id| {
                accumulated
                    .iter()
                    .find(|entry| entry.sns_neuron_id == vec![id; 32])
                    .map_or(0, |entry| entry.accumulated_eligible_credit)
            });
            assert_eq!(
                actual, expected,
                "unexpected accumulation at day {}",
                event.round
            );
        }
        for day in 4_u64..=14 {
            let (_, weights) = event_credits(
                governance,
                &no_exclusions(),
                &event(day, day * 10, 0),
                &[
                    neuron(1, 100, io_core_model::TWO_WEEK_SECONDS, None),
                    neuron(2, 200, io_core_model::TWO_WEEK_SECONDS, None),
                ],
            )
            .unwrap();
            accumulated = merge_event_credits(&accumulated, &weights).unwrap();
            assert_eq!(
                accumulated[0].accumulated_eligible_credit,
                io_reward_policy::DAILY_EVENT_CREDIT
                    + daily_fraction(1, 3)
                    + (day - 3) as u128 * daily_fraction(1, 3)
            );
            assert_eq!(
                accumulated[1].accumulated_eligible_credit,
                daily_fraction(2, 3)
                    + io_reward_policy::DAILY_EVENT_CREDIT
                    + (day - 3) as u128 * daily_fraction(2, 3)
            );
        }
        assert_eq!(
            accumulated[0].accumulated_eligible_credit,
            io_reward_policy::DAILY_EVENT_CREDIT + 12 * daily_fraction(1, 3)
        );
        assert_eq!(
            accumulated[1].accumulated_eligible_credit,
            io_reward_policy::DAILY_EVENT_CREDIT + 12 * daily_fraction(2, 3)
        );
    }

    #[test]
    fn equal_daily_opportunities_produce_three_to_one_cumulative_credit() {
        let governance = principal(1);
        let day_one = vec![
            neuron(
                1,
                100,
                io_core_model::TWO_WEEK_SECONDS,
                Some((10, Some(Uint128 { high: 0, low: 100 }))),
            ),
            neuron(2, 100, io_core_model::TWO_WEEK_SECONDS, None),
        ];
        let day_two = vec![
            neuron(
                1,
                100,
                io_core_model::TWO_WEEK_SECONDS,
                Some((
                    20,
                    Some(Uint128 {
                        high: 0,
                        low: 5_000,
                    }),
                )),
            ),
            neuron(
                2,
                100,
                io_core_model::TWO_WEEK_SECONDS,
                Some((
                    20,
                    Some(Uint128 {
                        high: 0,
                        low: 5_000,
                    }),
                )),
            ),
        ];
        let (_, weights) = event_credits(governance, &[], &event(1, 10, 1), &day_one).unwrap();
        let accumulated = merge_event_credits(&[], &weights).unwrap();
        let (_, weights) = event_credits(governance, &[], &event(2, 20, 100), &day_two).unwrap();
        let accumulated = merge_event_credits(&accumulated, &weights).unwrap();
        assert_eq!(
            accumulated[0].accumulated_eligible_credit,
            io_reward_policy::DAILY_EVENT_CREDIT + daily_fraction(1, 2)
        );
        assert_eq!(
            accumulated[1].accumulated_eligible_credit,
            daily_fraction(1, 2)
        );

        let entitlements = accumulated
            .iter()
            .map(|entry| {
                io_reward_policy::entitlement_credit_from_bytes(
                    entry.sns_neuron_id.clone(),
                    entry.accumulated_eligible_credit,
                )
            })
            .collect::<Vec<_>>();
        let allocation = io_reward_policy::allocate_rewards(
            10_000,
            2 * io_reward_policy::DAILY_EVENT_CREDIT,
            &entitlements,
        )
        .unwrap();
        assert_eq!(
            allocation
                .allocations
                .iter()
                .map(|allocation| allocation.io_e8s)
                .collect::<Vec<_>>(),
            vec![7_500, 2_500]
        );
    }

    #[test]
    fn excluded_current_event_share_is_forfeited() {
        let governance = principal(1);
        let nonredeemable = Account {
            owner: governance,
            subaccount: Some(vec![9; 32]),
        };
        let neurons = vec![
            neuron(
                1,
                100,
                io_core_model::TWO_WEEK_SECONDS,
                Some((10, Some(Uint128 { high: 0, low: 50 }))),
            ),
            neuron(
                9,
                100,
                io_core_model::TWO_WEEK_SECONDS,
                Some((10, Some(Uint128 { high: 0, low: 50 }))),
            ),
        ];
        let (_, weights) =
            event_credits(governance, &[nonredeemable], &event(1, 10, 1), &neurons).unwrap();
        assert_eq!(weights.len(), 1);
        assert_eq!(weights[0].event_credit, daily_fraction(1, 2));
        let allocation = io_reward_policy::allocate_rewards(
            1_000,
            io_reward_policy::DAILY_EVENT_CREDIT,
            &[io_reward_policy::entitlement_credit_from_bytes(
                weights[0].sns_neuron_id.clone(),
                weights[0].event_credit,
            )],
        )
        .unwrap();
        assert_eq!(allocation.allocations[0].io_e8s, 500);
        assert_eq!(allocation.forfeited_io_e8s, 500);
        assert_eq!(allocation.rounding_dust_e8s, 0);
    }

    #[test]
    fn gap_first_observed_event_is_credit_bearing() {
        let current = event(7, 70, 1);
        assert_eq!(
            classify_sequence(None, &current).unwrap(),
            EventSequence::First
        );
        let (_, weights) = event_credits(
            principal(1),
            &[],
            &current,
            &[neuron(
                1,
                100,
                io_core_model::TWO_WEEK_SECONDS,
                Some((70, Some(Uint128 { high: 0, low: 100 }))),
            )],
        )
        .unwrap();
        assert_eq!(
            weights[0].event_credit,
            io_reward_policy::DAILY_EVENT_CREDIT
        );
    }
}
