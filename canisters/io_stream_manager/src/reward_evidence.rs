use candid::Principal;

use crate::{
    api::ApiError,
    state::{Account, RewardCohort, RewardEventId, RewardMember, RewardShareSnapshot},
};
use io_sns_reward_boundary::{
    self as reward_governance, Error, EventId, EventSequenceError, Neuron, RewardEvent,
};

fn governance_error(error: Error) -> ApiError {
    match error {
        Error::Retryable { method, message } => {
            ApiError::Pending(format!("SNS {method} failed: {message}"))
        }
        Error::Invalid { method, message } => {
            ApiError::Invalid(format!("SNS {method} failed: {message}"))
        }
        Error::NotFound => ApiError::Invalid("SNS neuron was not found".into()),
    }
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

pub(crate) fn require_next_event(
    previous: RewardEventId,
    next: &RewardEvent,
) -> Result<(), ApiError> {
    reward_governance::require_next_event(
        EventId {
            end_timestamp_seconds: previous.end_timestamp_seconds,
            round: previous.round,
        },
        next,
    )
    .map_err(event_sequence_error)
}

fn event_sequence_error(error: EventSequenceError) -> ApiError {
    match error {
        EventSequenceError::MissingEndTimestamp => {
            ApiError::Pending("SNS latest reward event has no end timestamp".into())
        }
        EventSequenceError::Pending => ApiError::Pending("SNS reward event has not advanced".into()),
        EventSequenceError::Missed { previous, next } => ApiError::Invalid(format!(
            "RewardEventMissed: captured round {} at {}, latest round {} at {}; backed pool remains in reserve",
            previous.round, previous.end_timestamp_seconds, next.round, next.end_timestamp_seconds
        )),
        EventSequenceError::Invalid(message) => {
            ApiError::Invalid(format!("invalid canonical reward-event sequence: {message}"))
        }
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

pub(crate) fn eligible_members(
    governance: Principal,
    excluded_io_accounts: &[Account],
    neurons: &[Neuron],
) -> Result<Vec<RewardMember>, ApiError> {
    let mut ids = std::collections::BTreeSet::new();
    for neuron in neurons {
        if !ids.insert(neuron.id.clone()) {
            return Err(ApiError::Invalid(
                "SNS list_neurons returned a duplicate neuron ID".into(),
            ));
        }
    }
    neurons
        .iter()
        .filter(|neuron| canonical_eligible(neuron))
        .filter_map(|neuron| {
            let id = neuron.id.clone();
            if id.len() != 32 {
                return Some(Err(ApiError::Invalid(
                    "eligible SNS neuron ID is not a canonical staking subaccount".into(),
                )));
            }
            let account = Account {
                owner: governance,
                subaccount: Some(id.clone()),
            };
            let excluded = excluded_io_accounts.iter().any(|excluded| {
                account
                    .effective_eq(excluded)
                    .expect("validated accounts have canonical subaccounts")
            });
            if excluded {
                return None;
            }
            Some(Ok(RewardMember {
                account,
                sns_neuron_id: id,
                frozen_stake_e8s: neuron.cached_neuron_stake_e8s,
                observed_stake_e8s: neuron.cached_neuron_stake_e8s,
                reward_shares: None,
                reward_event_end_timestamp_seconds: None,
                destination_is_currently_eligible: true,
            }))
        })
        .collect()
}

pub(crate) fn canonical_eligible(neuron: &Neuron) -> bool {
    neuron.is_non_dissolving_for(io_core_model::TWO_WEEK_SECONDS)
}

pub(crate) async fn exact_neuron(
    governance: Principal,
    id: &[u8],
) -> Result<Option<Neuron>, ApiError> {
    reward_governance::get_exact_neuron(governance, id)
        .await
        .map_err(governance_error)
}

pub(crate) async fn list_all_neurons(governance: Principal) -> Result<Vec<Neuron>, ApiError> {
    reward_governance::list_all_neurons(governance)
        .await
        .map_err(governance_error)
}

pub(crate) fn apply_reward_share_snapshot(
    cohort: &mut RewardCohort,
    event: &RewardEvent,
    neurons: &[Neuron],
    captured_at_nanos: u64,
) -> Result<(), ApiError> {
    let settled_proposal_count = event.settled_proposal_count().map_err(governance_error)?;
    let event = event_id(event)?;
    let mut total_eligible_reward_shares = 0u128;
    for member in &mut cohort.members {
        let current = neurons
            .iter()
            .find(|neuron| neuron.id == member.sns_neuron_id);
        member.destination_is_currently_eligible = current.is_some_and(canonical_eligible);
        member.observed_stake_e8s = current
            .map(|neuron| neuron.cached_neuron_stake_e8s)
            .unwrap_or(member.frozen_stake_e8s);
        member.reward_shares = if settled_proposal_count == 0 {
            Some(member.frozen_stake_e8s)
        } else {
            Some(
                match current.and_then(|neuron| neuron.latest_reward_event_participation) {
                    Some(participation) => {
                        let shares = participation
                            .exact_reward_shares()
                            .map_err(governance_error)?;
                        (participation.reward_event_end_timestamp_seconds
                            == event.end_timestamp_seconds)
                            .then_some(shares)
                            .unwrap_or(0)
                    }
                    None => 0,
                },
            )
        };
        member.reward_event_end_timestamp_seconds = Some(event.end_timestamp_seconds);
        total_eligible_reward_shares = total_eligible_reward_shares
            .checked_add(member.reward_shares.unwrap_or(0))
            .ok_or_else(|| ApiError::Invalid("eligible reward-share total overflow".into()))?;
    }
    cohort.reward_share_snapshot = Some(RewardShareSnapshot {
        event,
        settled_proposal_count,
        total_eligible_reward_shares,
        captured_at_nanos,
        no_proposal_fallback: Some(settled_proposal_count == 0),
        no_eligible_participation: Some(
            settled_proposal_count > 0 && total_eligible_reward_shares == 0,
        ),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use io_sns_reward_boundary::{DissolveState, RewardEventParticipation, Uint128};

    fn event(round: u64, end: u64, span: u64) -> RewardEvent {
        RewardEvent {
            rounds_since_last_distribution: Some(span),
            actual_timestamp_seconds: end,
            end_timestamp_seconds: Some(end),
            round,
            settled_proposals: Vec::new(),
        }
    }

    fn neuron(id: u8, shares: Option<(u64, Uint128)>) -> Neuron {
        Neuron {
            id: vec![id; 32],
            dissolve_state: DissolveState::NotDissolving {
                dissolve_delay_seconds: io_core_model::TWO_WEEK_SECONDS,
            },
            cached_neuron_stake_e8s: 100,
            latest_reward_event_participation: shares.map(|(timestamp, reward_shares)| {
                RewardEventParticipation {
                    reward_event_end_timestamp_seconds: timestamp,
                    reward_shares: Some(reward_shares),
                }
            }),
        }
    }

    #[test]
    fn event_change_discards_pages_and_skipped_round_is_missed() {
        assert!(require_consistent_event(&event(1, 10, 1), &event(2, 20, 1)).is_err());
        assert!(matches!(
            require_next_event(
                RewardEventId {
                    round: 1,
                    end_timestamp_seconds: 10
                },
                &event(3, 30, 1)
            ),
            Err(ApiError::Invalid(message)) if message.contains("RewardEventMissed")
        ));
    }

    #[test]
    fn delayed_periodic_work_consumes_one_multi_round_event() {
        let previous = RewardEventId {
            round: 4,
            end_timestamp_seconds: 40,
        };
        assert_eq!(require_next_event(previous, &event(7, 70, 3)), Ok(()));
        assert!(matches!(
            require_next_event(previous, &event(6, 60, 3)),
            Err(ApiError::Invalid(message)) if message.contains("invalid canonical")
        ));
    }

    #[test]
    fn exact_high_low_reward_shares_are_preserved() {
        let value = Uint128 { high: 1, low: 7 };
        assert_eq!(value.exact(), (1_u128 << 64) | 7);
        let n = neuron(1, Some((20, value)));
        assert_eq!(
            n.latest_reward_event_participation
                .unwrap()
                .exact_reward_shares()
                .unwrap(),
            (1_u128 << 64) | 7
        );
    }
}
