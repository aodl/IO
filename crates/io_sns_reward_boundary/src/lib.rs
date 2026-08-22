use candid::{CandidType, Principal};
use serde::Deserialize;

pub const MAX_NUMBER_OF_NEURONS: u64 = 1_000;
const NEURON_PAGE_SIZE: u32 = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Retryable {
        method: &'static str,
        message: String,
    },
    Invalid {
        method: &'static str,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct Uint128 {
    pub high: u64,
    pub low: u64,
}

impl Uint128 {
    pub fn exact(self) -> u128 {
        (u128::from(self.high) << 64) | u128::from(self.low)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardEventParticipation {
    pub reward_event_end_timestamp_seconds: u64,
    pub reward_shares: Option<Uint128>,
}

impl RewardEventParticipation {
    pub fn exact_reward_shares(self) -> Result<u128, Error> {
        self.reward_shares
            .map(Uint128::exact)
            .ok_or_else(|| Error::Invalid {
                method: "list_neurons",
                message: "latest reward-event participation lacks reward_shares".into(),
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ProposalId {
    pub id: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardEvent {
    pub rounds_since_last_distribution: Option<u64>,
    pub actual_timestamp_seconds: u64,
    pub end_timestamp_seconds: Option<u64>,
    pub round: u64,
    pub settled_proposals: Vec<ProposalId>,
}

impl RewardEvent {
    pub fn settled_proposal_count(&self) -> Result<u64, Error> {
        u64::try_from(self.settled_proposals.len()).map_err(|_| Error::Invalid {
            method: "get_latest_reward_event",
            message: "settled proposal count overflow".into(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventId {
    pub end_timestamp_seconds: u64,
    pub round: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventSequenceError {
    MissingEndTimestamp,
    Invalid(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventSequence {
    First,
    Next,
    Same,
    Skipped {
        previous: Option<EventId>,
        next: EventId,
        ambiguous_event_count: u64,
        rounds_since_last_distribution: u64,
    },
}

pub fn event_id(event: &RewardEvent) -> Result<EventId, EventSequenceError> {
    Ok(EventId {
        end_timestamp_seconds: event
            .end_timestamp_seconds
            .filter(|timestamp| *timestamp > 0)
            .ok_or(EventSequenceError::MissingEndTimestamp)?,
        round: event.round,
    })
}

pub fn classify_event_sequence(
    previous: Option<EventId>,
    next: &RewardEvent,
) -> Result<EventSequence, EventSequenceError> {
    let next_id = event_id(next)?;
    let span = next
        .rounds_since_last_distribution
        .filter(|span| *span > 0)
        .ok_or(EventSequenceError::Invalid(
            "rounds_since_last_distribution is missing or zero",
        ))?;
    let Some(previous) = previous else {
        return if span == 1 {
            Ok(EventSequence::First)
        } else {
            Ok(EventSequence::Skipped {
                previous: None,
                next: next_id,
                ambiguous_event_count: span,
                rounds_since_last_distribution: span,
            })
        };
    };
    let delta = next_id
        .round
        .checked_sub(previous.round)
        .ok_or(EventSequenceError::Invalid("round regressed"))?;
    if delta == 0 {
        return if next_id.end_timestamp_seconds == previous.end_timestamp_seconds {
            Ok(EventSequence::Same)
        } else {
            Err(EventSequenceError::Invalid(
                "unchanged round changed end timestamp",
            ))
        };
    }
    if next_id.end_timestamp_seconds <= previous.end_timestamp_seconds {
        return Err(EventSequenceError::Invalid("end timestamp did not advance"));
    }
    if delta == 1 && span == 1 {
        Ok(EventSequence::Next)
    } else {
        Ok(EventSequence::Skipped {
            previous: Some(previous),
            next: next_id,
            ambiguous_event_count: delta.max(span),
            rounds_since_last_distribution: span,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DissolveState {
    NotDissolving { dissolve_delay_seconds: u64 },
    Dissolving,
    Dissolved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Neuron {
    pub id: Vec<u8>,
    pub cached_neuron_stake_e8s: u128,
    pub dissolve_state: DissolveState,
    pub latest_reward_event_participation: Option<RewardEventParticipation>,
}

impl Neuron {
    pub fn is_non_dissolving_for(&self, dissolve_delay_seconds: u64) -> bool {
        self.cached_neuron_stake_e8s > 0
            && matches!(
                self.dissolve_state,
                DissolveState::NotDissolving {
                    dissolve_delay_seconds: actual
                } if actual == dissolve_delay_seconds
            )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsNeuronIdRecord {
    pub id: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum DissolveStateRecord {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct NeuronRecord {
    id: Option<SnsNeuronIdRecord>,
    cached_neuron_stake_e8s: u64,
    dissolve_state: Option<DissolveStateRecord>,
    latest_reward_event_participation: Option<RewardEventParticipation>,
}

impl TryFrom<NeuronRecord> for Neuron {
    type Error = Error;

    fn try_from(value: NeuronRecord) -> Result<Self, Self::Error> {
        let id = value.id.ok_or_else(|| Error::Invalid {
            method: "list_neurons",
            message: "neuron record lacks an ID".into(),
        })?;
        let dissolve_state = match value.dissolve_state {
            Some(DissolveStateRecord::DissolveDelaySeconds(delay)) => {
                DissolveState::NotDissolving {
                    dissolve_delay_seconds: delay,
                }
            }
            Some(DissolveStateRecord::WhenDissolvedTimestampSeconds(_)) => {
                DissolveState::Dissolving
            }
            None => DissolveState::Dissolved,
        };
        Ok(Self {
            id: id.id,
            cached_neuron_stake_e8s: u128::from(value.cached_neuron_stake_e8s),
            dissolve_state,
            latest_reward_event_participation: value.latest_reward_event_participation,
        })
    }
}

#[derive(Clone, Debug, CandidType)]
struct ListNeuronsRequest {
    of_principal: Option<Principal>,
    limit: u32,
    start_page_at: Option<SnsNeuronIdRecord>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ListNeuronsResponse {
    neurons: Vec<NeuronRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProductionManageNeuronRequest {
    pub subaccount: Vec<u8>,
    pub command: Option<SnsManageNeuronCommand>,
}
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum SnsManageNeuronCommand {
    ClaimOrRefresh(SnsClaimOrRefresh),
}
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsClaimOrRefresh {
    pub by: Option<SnsClaimOrRefreshBy>,
}
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum SnsClaimOrRefreshBy {
    NeuronId(EmptyRecord),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct EmptyRecord {}
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProductionManageNeuronResponse {
    pub command: Option<SnsManageNeuronCommandResponse>,
}
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum SnsManageNeuronCommandResponse {
    Error(SnsGovernanceErrorRecord),
    ClaimOrRefresh(SnsClaimOrRefreshResponse),
}
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsGovernanceErrorRecord {
    pub error_type: i32,
    pub error_message: String,
}
#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsClaimOrRefreshResponse {
    pub refreshed_neuron_id: Option<SnsNeuronIdRecord>,
}

#[derive(Clone, Debug, CandidType)]
struct SummaryRequest {
    update_canister_list: Option<bool>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct SummaryResponse {
    governance: Option<CanisterSummary>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct CanisterSummary {
    canister_id: Option<Principal>,
    status: Option<CanisterStatus>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct CanisterStatus {
    module_hash: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct NervousSystemParameters {
    voting_rewards_parameters: Option<VotingRewardsParameters>,
    max_number_of_neurons: Option<u64>,
    max_dissolve_delay_bonus_percentage: Option<u64>,
    max_age_bonus_percentage: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct VotingRewardsParameters {
    final_reward_rate_basis_points: Option<u64>,
    initial_reward_rate_basis_points: Option<u64>,
    round_duration_seconds: Option<u64>,
}

pub struct InstalledGovernance {
    pub canister: Principal,
    pub module_hash: Vec<u8>,
    pub initial_reward_rate_basis_points: u64,
    pub final_reward_rate_basis_points: u64,
    pub round_duration_seconds: u64,
    pub max_number_of_neurons: u64,
    pub max_dissolve_delay_bonus_percentage: u64,
    pub max_age_bonus_percentage: u64,
}

pub async fn installed_governance(
    root: Principal,
    expected_governance: Principal,
) -> Result<InstalledGovernance, Error> {
    let summary: SummaryResponse = call(
        root,
        "get_sns_canisters_summary",
        SummaryRequest {
            update_canister_list: Some(false),
        },
    )
    .await?;
    let governance = summary.governance.ok_or_else(|| Error::Invalid {
        method: "get_sns_canisters_summary",
        message: "SNS Root summary lacks Governance".into(),
    })?;
    let canister = governance.canister_id.ok_or_else(|| Error::Invalid {
        method: "get_sns_canisters_summary",
        message: "SNS Root summary lacks Governance canister ID".into(),
    })?;
    if canister != expected_governance {
        return Err(Error::Invalid {
            method: "get_sns_canisters_summary",
            message: "SNS Root returned an unexpected Governance canister".into(),
        });
    }
    let module_hash = governance
        .status
        .and_then(|status| status.module_hash)
        .ok_or_else(|| Error::Invalid {
            method: "get_sns_canisters_summary",
            message: "SNS Root summary lacks Governance module hash".into(),
        })?;
    let parameters: NervousSystemParameters =
        call(canister, "get_nervous_system_parameters", ()).await?;
    let rewards = parameters
        .voting_rewards_parameters
        .ok_or_else(|| Error::Invalid {
            method: "get_nervous_system_parameters",
            message: "Governance lacks voting reward parameters".into(),
        })?;
    Ok(InstalledGovernance {
        canister,
        module_hash,
        initial_reward_rate_basis_points: required_parameter(
            rewards.initial_reward_rate_basis_points,
            "initial_reward_rate_basis_points",
        )?,
        final_reward_rate_basis_points: required_parameter(
            rewards.final_reward_rate_basis_points,
            "final_reward_rate_basis_points",
        )?,
        round_duration_seconds: required_parameter(
            rewards.round_duration_seconds,
            "round_duration_seconds",
        )?,
        max_number_of_neurons: required_parameter(
            parameters.max_number_of_neurons,
            "max_number_of_neurons",
        )?,
        max_dissolve_delay_bonus_percentage: required_parameter(
            parameters.max_dissolve_delay_bonus_percentage,
            "max_dissolve_delay_bonus_percentage",
        )?,
        max_age_bonus_percentage: required_parameter(
            parameters.max_age_bonus_percentage,
            "max_age_bonus_percentage",
        )?,
    })
}

pub async fn latest_reward_event(governance: Principal) -> Result<RewardEvent, Error> {
    call(governance, "get_latest_reward_event", ()).await
}

pub async fn list_neurons(
    governance: Principal,
    limit: u32,
    start_page_at: Option<Vec<u8>>,
) -> Result<Vec<Neuron>, Error> {
    let response: ListNeuronsResponse = call(
        governance,
        "list_neurons",
        ListNeuronsRequest {
            of_principal: None,
            limit,
            start_page_at: start_page_at.map(|id| SnsNeuronIdRecord { id }),
        },
    )
    .await?;
    response.neurons.into_iter().map(Neuron::try_from).collect()
}

pub async fn list_all_neurons(governance: Principal) -> Result<Vec<Neuron>, Error> {
    const MAX_PAGES: usize = MAX_NUMBER_OF_NEURONS as usize / NEURON_PAGE_SIZE as usize;
    let mut neurons = Vec::new();
    let mut cursor = None;
    for page_index in 0..=MAX_PAGES {
        let page = list_neurons(governance, NEURON_PAGE_SIZE, cursor.clone()).await?;
        if page.len() > NEURON_PAGE_SIZE as usize {
            return Err(Error::Invalid {
                method: "list_neurons",
                message: "neuron page exceeds bound".into(),
            });
        }
        if page_index == MAX_PAGES {
            return if page.is_empty() {
                Ok(neurons)
            } else {
                Err(Error::Invalid {
                    method: "list_neurons",
                    message: "neuron evidence exceeds 1,000 members".into(),
                })
            };
        }
        let next = page.last().map(|neuron| neuron.id.clone());
        if page.len() == NEURON_PAGE_SIZE as usize && next == cursor {
            return Err(Error::Invalid {
                method: "list_neurons",
                message: "pagination did not progress".into(),
            });
        }
        let count = page.len();
        neurons.extend(page);
        if count < NEURON_PAGE_SIZE as usize {
            return Ok(neurons);
        }
        cursor = next;
    }
    unreachable!("bounded pagination returns from every terminal path")
}

fn required_parameter(value: Option<u64>, field: &'static str) -> Result<u64, Error> {
    value.ok_or_else(|| Error::Invalid {
        method: "get_nervous_system_parameters",
        message: format!("Governance lacks {field}"),
    })
}

async fn call<A: CandidType, R: for<'de> Deserialize<'de> + CandidType>(
    canister: Principal,
    method: &'static str,
    arg: A,
) -> Result<R, Error> {
    #[cfg(target_family = "wasm")]
    {
        ic_cdk::call::Call::bounded_wait(canister, method)
            .with_arg(arg)
            .await
            .map_err(|error| Error::Retryable {
                method,
                message: format!("{error:?}"),
            })?
            .candid::<R>()
            .map_err(|error| Error::Invalid {
                method,
                message: format!("Candid decode failed: {error:?}"),
            })
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = (canister, method, arg);
        Err(Error::Retryable {
            method,
            message: "canister calls are unavailable on host".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(round: u64, end: Option<u64>, span: Option<u64>) -> RewardEvent {
        RewardEvent {
            rounds_since_last_distribution: span,
            actual_timestamp_seconds: end.unwrap_or_default(),
            end_timestamp_seconds: end,
            round,
            settled_proposals: Vec::new(),
        }
    }

    fn previous() -> EventId {
        EventId {
            end_timestamp_seconds: 10,
            round: 1,
        }
    }

    #[test]
    fn uint128_is_exact() {
        assert_eq!(Uint128 { high: 1, low: 7 }.exact(), (1_u128 << 64) | 7);
    }

    #[test]
    fn old_neuron_without_additive_field_decodes() {
        #[derive(CandidType)]
        struct OldNeuron {
            id: Option<SnsNeuronIdRecord>,
            cached_neuron_stake_e8s: u64,
            dissolve_state: Option<DissolveStateRecord>,
        }
        let bytes = candid::encode_one(OldNeuron {
            id: Some(SnsNeuronIdRecord { id: vec![1; 32] }),
            cached_neuron_stake_e8s: 10,
            dissolve_state: Some(DissolveStateRecord::DissolveDelaySeconds(10)),
        })
        .unwrap();
        let decoded: NeuronRecord = candid::decode_one(&bytes).unwrap();
        assert_eq!(decoded.latest_reward_event_participation, None);
    }

    #[test]
    fn same_event_is_pending_without_mutation() {
        assert_eq!(
            classify_event_sequence(Some(previous()), &event(1, Some(10), Some(1))),
            Ok(EventSequence::Same)
        );
    }

    #[test]
    fn exact_next_single_round_event_is_accepted() {
        assert_eq!(
            classify_event_sequence(Some(previous()), &event(2, Some(20), Some(1))),
            Ok(EventSequence::Next)
        );
    }

    #[test]
    fn one_or_several_missed_events_are_typed_skips() {
        assert!(matches!(
            classify_event_sequence(Some(previous()), &event(3, Some(30), Some(1))),
            Ok(EventSequence::Skipped {
                ambiguous_event_count: 2,
                rounds_since_last_distribution: 1,
                ..
            })
        ));
        assert!(matches!(
            classify_event_sequence(Some(previous()), &event(5, Some(50), Some(1))),
            Ok(EventSequence::Skipped {
                ambiguous_event_count: 4,
                ..
            })
        ));
    }

    #[test]
    fn catch_up_span_is_a_typed_skip() {
        assert!(matches!(
            classify_event_sequence(Some(previous()), &event(4, Some(40), Some(3))),
            Ok(EventSequence::Skipped {
                ambiguous_event_count: 3,
                rounds_since_last_distribution: 3,
                ..
            })
        ));
    }

    #[test]
    fn first_single_round_event_is_processable_and_first_catch_up_is_skipped() {
        assert!(matches!(
            classify_event_sequence(None, &event(7, Some(70), Some(1))),
            Ok(EventSequence::First)
        ));
        assert!(matches!(
            classify_event_sequence(None, &event(7, Some(70), Some(7))),
            Ok(EventSequence::Skipped {
                ambiguous_event_count: 7,
                ..
            })
        ));
    }

    #[test]
    fn regressed_and_malformed_events_are_rejected() {
        assert!(matches!(
            classify_event_sequence(Some(previous()), &event(0, Some(5), Some(1))),
            Err(EventSequenceError::Invalid("round regressed"))
        ));
        assert!(matches!(
            classify_event_sequence(Some(previous()), &event(2, Some(20), None)),
            Err(EventSequenceError::Invalid(_))
        ));
    }

    #[test]
    fn gap_reader_capacity_is_exactly_one_thousand_total_neurons() {
        let complete_pages = MAX_NUMBER_OF_NEURONS / u64::from(NEURON_PAGE_SIZE);
        assert_eq!(complete_pages, 10);
        assert!(complete_pages * u64::from(NEURON_PAGE_SIZE) <= MAX_NUMBER_OF_NEURONS);
        assert!(complete_pages * u64::from(NEURON_PAGE_SIZE) + 1 > MAX_NUMBER_OF_NEURONS);
    }
}
