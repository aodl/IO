use candid::{CandidType, Nat, Principal};
use io_ledger_types::Account;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, CandidType, Deserialize)]
pub struct NnsNeuronId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsNeuron {
    pub id: NnsNeuronId,
    pub controller: Option<Principal>,
    pub stake_e8s: u128,
    pub maturity_e8s_equivalent: u128,
    pub dissolve_delay_seconds: u64,
    pub dissolve_state: NnsDissolveState,
    pub known_neuron_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsDissolveState {
    NotDissolving {
        dissolve_delay_seconds: u64,
    },
    Dissolving {
        when_dissolved_timestamp_seconds: u64,
    },
    Dissolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsNeuronCommand {
    Spawn,
    DisburseMaturity,
    Split,
    StartDissolving,
    StopDissolving,
    Merge,
    Disburse,
    Follow,
    RefreshVotingPower,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsCommandResult {
    pub command: NnsNeuronCommand,
    pub neuron_id: NnsNeuronId,
    pub amount_e8s: Option<u128>,
    pub child_neuron_id: Option<NnsNeuronId>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsGovernanceError {
    TemporarilyUnavailable,
    GovernanceReject { code: i32, message: String },
    NeuronNotFound,
    NotAuthorized,
    InsufficientStake,
    InvalidCommand { message: String },
    DuplicateOrAlreadyInProgress,
    CanisterCallFailed { method: String, message: String },
    DecodeError { message: String },
    Unsupported,
}

impl NnsGovernanceError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::TemporarilyUnavailable
                | Self::DuplicateOrAlreadyInProgress
                | Self::CanisterCallFailed { .. }
                | Self::DecodeError { .. }
        )
    }
}

pub trait NnsGovernanceClient {
    fn get_neuron<'a>(
        &'a self,
        id: NnsNeuronId,
    ) -> Pin<Box<dyn Future<Output = Result<NnsNeuron, NnsGovernanceError>> + 'a>>;

    fn disburse_maturity<'a>(
        &'a self,
        id: NnsNeuronId,
        percentage_to_disburse: u32,
        to: Account,
    ) -> Pin<Box<dyn Future<Output = Result<NnsCommandResult, NnsGovernanceError>> + 'a>>;

    fn split_neuron<'a>(
        &'a self,
        id: NnsNeuronId,
        amount_e8s: u128,
    ) -> Pin<Box<dyn Future<Output = Result<NnsNeuronId, NnsGovernanceError>> + 'a>>;

    fn start_dissolving<'a>(
        &'a self,
        id: NnsNeuronId,
    ) -> Pin<Box<dyn Future<Output = Result<NnsCommandResult, NnsGovernanceError>> + 'a>>;

    fn stop_dissolving<'a>(
        &'a self,
        id: NnsNeuronId,
    ) -> Pin<Box<dyn Future<Output = Result<NnsCommandResult, NnsGovernanceError>> + 'a>>;

    fn disburse_neuron<'a>(
        &'a self,
        id: NnsNeuronId,
        to: Account,
    ) -> Pin<Box<dyn Future<Output = Result<NnsCommandResult, NnsGovernanceError>> + 'a>>;
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsManageNeuron {
    pub id: Option<NnsNeuronIdRecord>,
    pub command: Option<NnsManageNeuronCommand>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsNeuronIdRecord {
    pub id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsManageNeuronCommand {
    Disburse(NnsDisburse),
    Split(NnsSplit),
    Configure(NnsConfigure),
    DisburseMaturity(NnsDisburseMaturity),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsDisburse {
    pub to_account: Option<NnsAccountIdentifier>,
    pub amount: Option<NnsTokens>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsSplit {
    pub amount_e8s: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsConfigure {
    pub operation: Option<NnsConfigureOperation>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsConfigureOperation {
    StartDissolving(()),
    StopDissolving(()),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsDisburseMaturity {
    pub percentage_to_disburse: u32,
    pub to_account: Option<NnsAccountIdentifier>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsAccountIdentifier {
    pub hash: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsTokens {
    pub e8s: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsManageNeuronResponse {
    Command(NnsManageNeuronResponseCommand),
    Error(NnsGovernanceErrorRecord),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsManageNeuronResponseCommand {
    Disburse(()),
    Split(NnsNeuronIdRecord),
    Configure(()),
    DisburseMaturity(NnsDisburseMaturityResponse),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsDisburseMaturityResponse {
    pub amount_disbursed_e8s: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsGovernanceErrorRecord {
    pub error_type: i32,
    pub error_message: String,
}

impl From<NnsGovernanceErrorRecord> for NnsGovernanceError {
    fn from(value: NnsGovernanceErrorRecord) -> Self {
        match value.error_type {
            2 => Self::NeuronNotFound,
            3 => Self::NotAuthorized,
            4 => Self::InvalidCommand {
                message: value.error_message,
            },
            5 => Self::InsufficientStake,
            6 => Self::DuplicateOrAlreadyInProgress,
            code => Self::GovernanceReject {
                code,
                message: value.error_message,
            },
        }
    }
}

pub fn nns_split_child_id(
    response: NnsManageNeuronResponse,
) -> Result<NnsNeuronId, NnsGovernanceError> {
    match response {
        NnsManageNeuronResponse::Command(NnsManageNeuronResponseCommand::Split(child)) => {
            Ok(NnsNeuronId(child.id))
        }
        NnsManageNeuronResponse::Error(err) => Err(err.into()),
        _ => Err(NnsGovernanceError::DecodeError {
            message: "manage_neuron response did not contain a split command".to_string(),
        }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, CandidType, Deserialize)]
pub struct SnsNeuronId(pub Vec<u8>);

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsNeuron {
    pub id: SnsNeuronId,
    pub controller: Option<Principal>,
    pub stake_e8s: u128,
    pub dissolve_delay_seconds: u64,
    pub dissolve_state: SnsDissolveState,
    pub cached_neuron_stake_e8s: u128,
    pub voting_power: u128,
    pub permissions: Vec<SnsNeuronPermission>,
    pub is_io_protocol_neuron: bool,
    pub is_jupiter_governance_neuron: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum SnsDissolveState {
    NotDissolving {
        dissolve_delay_seconds: u64,
    },
    Dissolving {
        when_dissolved_timestamp_seconds: u64,
    },
    Dissolved,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsNeuronPermission {
    pub principal: Option<Principal>,
    pub permission_type: Vec<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, CandidType, Deserialize)]
pub struct SnsProposalId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProposal {
    pub id: SnsProposalId,
    pub topic: Option<u64>,
    pub status: SnsProposalStatus,
    pub reward_status: SnsProposalRewardStatus,
    pub decided_timestamp_seconds: Option<u64>,
    pub ballots: Vec<SnsBallot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum SnsProposalStatus {
    Open,
    Adopted,
    Rejected,
    Executed,
    Failed,
    Unknown,
}

impl SnsProposalStatus {
    pub fn is_closed(self) -> bool {
        !matches!(self, Self::Open | Self::Unknown)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum SnsProposalRewardStatus {
    AcceptVotes,
    ReadyToSettle,
    Settled,
    Ineligible,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsBallot {
    pub neuron_id: SnsNeuronId,
    pub vote: SnsVote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum SnsVote {
    Unspecified,
    Yes,
    No,
    FollowedYes,
    FollowedNo,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsNeuronEligibility {
    pub neuron_id: SnsNeuronId,
    pub owner: Option<Principal>,
    pub eligible_stake_e8s: u128,
    pub eligible_since_seconds: u64,
    pub dissolve_delay_seconds: u64,
    pub is_non_dissolving: bool,
    pub excluded_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsParticipationSummary {
    pub neuron_id: SnsNeuronId,
    pub eligible_closed_proposals_total: u64,
    pub voted_proposals: u64,
    pub participation_bps: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsNeuronPageRequest {
    pub limit: u64,
    pub start_page_at: Option<SnsNeuronId>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsNeuronPage {
    pub neurons: Vec<SnsNeuron>,
    pub next_page_at: Option<SnsNeuronId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProposalPageRequest {
    pub limit: u64,
    pub before_proposal: Option<SnsProposalId>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProposalPage {
    pub proposals: Vec<SnsProposal>,
    pub next_before_proposal: Option<SnsProposalId>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum SnsGovernanceError {
    TemporarilyUnavailable,
    GovernanceReject { code: i32, message: String },
    NotFound,
    DecodeError { message: String },
    CanisterCallFailed { method: String, message: String },
    Unsupported,
}

impl SnsGovernanceError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::TemporarilyUnavailable
                | Self::CanisterCallFailed { .. }
                | Self::DecodeError { .. }
        )
    }
}

pub trait SnsGovernanceClient {
    fn list_neurons<'a>(
        &'a self,
        page: SnsNeuronPageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SnsNeuronPage, SnsGovernanceError>> + 'a>>;

    fn get_neuron<'a>(
        &'a self,
        id: SnsNeuronId,
    ) -> Pin<Box<dyn Future<Output = Result<SnsNeuron, SnsGovernanceError>> + 'a>>;

    fn list_proposals<'a>(
        &'a self,
        request: SnsProposalPageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SnsProposalPage, SnsGovernanceError>> + 'a>>;

    fn get_proposal<'a>(
        &'a self,
        id: SnsProposalId,
    ) -> Pin<Box<dyn Future<Output = Result<SnsProposal, SnsGovernanceError>> + 'a>>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnsEligibilityPolicy {
    pub protocol_neuron_ids: BTreeSet<SnsNeuronId>,
    pub jupiter_governance_neuron_ids: BTreeSet<SnsNeuronId>,
    pub minimum_dissolve_delay_seconds: u64,
    pub require_non_dissolving: bool,
    pub current_timestamp_seconds: u64,
}

pub fn snapshot_sns_eligibility(
    neurons: &[SnsNeuron],
    policy: &SnsEligibilityPolicy,
) -> Vec<SnsNeuronEligibility> {
    neurons
        .iter()
        .map(|neuron| {
            let is_non_dissolving = matches!(
                neuron.dissolve_state,
                SnsDissolveState::NotDissolving { .. }
            );
            let owner = neuron.controller;
            let excluded_reason = if policy.protocol_neuron_ids.contains(&neuron.id)
                || neuron.is_io_protocol_neuron
            {
                Some("protocol-owned neuron".to_string())
            } else if policy.jupiter_governance_neuron_ids.contains(&neuron.id)
                || neuron.is_jupiter_governance_neuron
            {
                Some("jupiter governance neuron".to_string())
            } else if neuron.cached_neuron_stake_e8s == 0 || neuron.stake_e8s == 0 {
                Some("zero stake".to_string())
            } else if neuron.dissolve_delay_seconds < policy.minimum_dissolve_delay_seconds {
                Some("dissolve delay below minimum".to_string())
            } else if policy.require_non_dissolving && !is_non_dissolving {
                Some("neuron is dissolving".to_string())
            } else {
                None
            };
            SnsNeuronEligibility {
                neuron_id: neuron.id.clone(),
                owner,
                eligible_stake_e8s: if excluded_reason.is_none() {
                    neuron.cached_neuron_stake_e8s
                } else {
                    0
                },
                eligible_since_seconds: policy.current_timestamp_seconds,
                dissolve_delay_seconds: neuron.dissolve_delay_seconds,
                is_non_dissolving,
                excluded_reason,
            }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnsParticipationPolicy {
    pub count_direct_votes: bool,
    pub count_followed_votes: bool,
    pub excluded_topics: BTreeSet<u64>,
    pub epoch_start_seconds: u64,
    pub epoch_end_seconds: u64,
}

pub fn summarize_sns_participation(
    eligibilities: &[SnsNeuronEligibility],
    proposals: &[SnsProposal],
    policy: &SnsParticipationPolicy,
) -> Vec<SnsParticipationSummary> {
    let eligible_by_id: BTreeMap<SnsNeuronId, &SnsNeuronEligibility> = eligibilities
        .iter()
        .filter(|e| e.excluded_reason.is_none())
        .map(|e| (e.neuron_id.clone(), e))
        .collect();
    eligible_by_id
        .iter()
        .map(|(neuron_id, eligibility)| {
            let mut total = 0u64;
            let mut voted = 0u64;
            for proposal in proposals {
                let Some(decided) = proposal.decided_timestamp_seconds else {
                    continue;
                };
                if !proposal.status.is_closed()
                    || decided < policy.epoch_start_seconds
                    || decided > policy.epoch_end_seconds
                    || decided < eligibility.eligible_since_seconds
                    || proposal
                        .topic
                        .is_some_and(|topic| policy.excluded_topics.contains(&topic))
                    || matches!(proposal.reward_status, SnsProposalRewardStatus::Ineligible)
                {
                    continue;
                }
                total = total.saturating_add(1);
                let counted = proposal
                    .ballots
                    .iter()
                    .find(|b| &b.neuron_id == neuron_id)
                    .map(|b| match b.vote {
                        SnsVote::Yes | SnsVote::No => policy.count_direct_votes,
                        SnsVote::FollowedYes | SnsVote::FollowedNo => policy.count_followed_votes,
                        SnsVote::Unspecified => false,
                    })
                    .unwrap_or(false);
                if counted {
                    voted = voted.saturating_add(1);
                }
            }
            let participation_bps = voted
                .saturating_mul(10_000)
                .checked_div(total)
                .map(|bps| bps.min(10_000) as u16)
                .unwrap_or(10_000);
            SnsParticipationSummary {
                neuron_id: neuron_id.clone(),
                eligible_closed_proposals_total: total,
                voted_proposals: voted,
                participation_bps,
            }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsNeuronRecord {
    pub id: Option<SnsNeuronIdRecord>,
    pub controller: Option<Principal>,
    pub cached_neuron_stake_e8s: u64,
    pub dissolve_state: Option<SnsDissolveStateRecord>,
    pub permissions: Vec<SnsNeuronPermissionRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsNeuronIdRecord {
    pub id: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum SnsDissolveStateRecord {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsNeuronPermissionRecord {
    pub principal: Option<Principal>,
    pub permission_type: Vec<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProposalRecord {
    pub id: Option<SnsProposalIdRecord>,
    pub topic: u64,
    pub status: i32,
    pub reward_status: i32,
    pub decided_timestamp_seconds: u64,
    pub ballots: Vec<(String, SnsBallotRecord)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProposalIdRecord {
    pub id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsBallotRecord {
    pub vote: i32,
}

impl From<SnsNeuronRecord> for SnsNeuron {
    fn from(value: SnsNeuronRecord) -> Self {
        let (dissolve_delay_seconds, dissolve_state) = match value.dissolve_state {
            Some(SnsDissolveStateRecord::DissolveDelaySeconds(delay)) => (
                delay,
                SnsDissolveState::NotDissolving {
                    dissolve_delay_seconds: delay,
                },
            ),
            Some(SnsDissolveStateRecord::WhenDissolvedTimestampSeconds(when)) => (
                0,
                SnsDissolveState::Dissolving {
                    when_dissolved_timestamp_seconds: when,
                },
            ),
            None => (0, SnsDissolveState::Dissolved),
        };
        Self {
            id: SnsNeuronId(value.id.map(|id| id.id).unwrap_or_default()),
            controller: value.controller,
            stake_e8s: u128::from(value.cached_neuron_stake_e8s),
            dissolve_delay_seconds,
            dissolve_state,
            cached_neuron_stake_e8s: u128::from(value.cached_neuron_stake_e8s),
            voting_power: u128::from(value.cached_neuron_stake_e8s),
            permissions: value
                .permissions
                .into_iter()
                .map(|p| SnsNeuronPermission {
                    principal: p.principal,
                    permission_type: p.permission_type,
                })
                .collect(),
            is_io_protocol_neuron: false,
            is_jupiter_governance_neuron: false,
        }
    }
}

impl From<SnsProposalRecord> for SnsProposal {
    fn from(value: SnsProposalRecord) -> Self {
        Self {
            id: SnsProposalId(value.id.map(|id| id.id).unwrap_or_default()),
            topic: Some(value.topic),
            status: match value.status {
                1 => SnsProposalStatus::Open,
                2 => SnsProposalStatus::Adopted,
                3 => SnsProposalStatus::Rejected,
                4 => SnsProposalStatus::Executed,
                5 => SnsProposalStatus::Failed,
                _ => SnsProposalStatus::Unknown,
            },
            reward_status: match value.reward_status {
                1 => SnsProposalRewardStatus::AcceptVotes,
                2 => SnsProposalRewardStatus::ReadyToSettle,
                3 => SnsProposalRewardStatus::Settled,
                4 => SnsProposalRewardStatus::Ineligible,
                _ => SnsProposalRewardStatus::Unknown,
            },
            decided_timestamp_seconds: (value.decided_timestamp_seconds > 0)
                .then_some(value.decided_timestamp_seconds),
            ballots: value
                .ballots
                .into_iter()
                .map(|(id, ballot)| SnsBallot {
                    neuron_id: SnsNeuronId(id.into_bytes()),
                    vote: match ballot.vote {
                        1 => SnsVote::Yes,
                        2 => SnsVote::No,
                        3 => SnsVote::FollowedYes,
                        4 => SnsVote::FollowedNo,
                        _ => SnsVote::Unspecified,
                    },
                })
                .collect(),
        }
    }
}

pub fn nat_to_u128(value: &Nat, field: &str) -> Result<u128, String> {
    value
        .0
        .clone()
        .try_into()
        .map_err(|_| format!("{field}: candid nat exceeds u128"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::{Decode, Encode};

    fn principal() -> Principal {
        Principal::from_text("oae4c-3iaaa-aaaar-qb5qq-cai").unwrap()
    }

    fn sns_neuron(id: u8, stake: u128, delay: u64) -> SnsNeuron {
        SnsNeuron {
            id: SnsNeuronId(vec![id]),
            controller: Some(principal()),
            stake_e8s: stake,
            dissolve_delay_seconds: delay,
            dissolve_state: SnsDissolveState::NotDissolving {
                dissolve_delay_seconds: delay,
            },
            cached_neuron_stake_e8s: stake,
            voting_power: stake,
            permissions: vec![SnsNeuronPermission {
                principal: Some(principal()),
                permission_type: vec![1, 2, 3],
            }],
            is_io_protocol_neuron: false,
            is_jupiter_governance_neuron: false,
        }
    }

    #[test]
    fn governance_domain_types_are_candid_serializable() {
        let nns = NnsNeuron {
            id: NnsNeuronId(42),
            controller: Some(principal()),
            stake_e8s: 100,
            maturity_e8s_equivalent: 5,
            dissolve_delay_seconds: 1_209_600,
            dissolve_state: NnsDissolveState::NotDissolving {
                dissolve_delay_seconds: 1_209_600,
            },
            known_neuron_name: Some("io two week pool".to_string()),
        };
        let bytes = Encode!(&nns).unwrap();
        assert_eq!(Decode!(&bytes, NnsNeuron).unwrap(), nns);

        let sns = sns_neuron(1, 100, 1_209_600);
        let bytes = Encode!(&sns).unwrap();
        assert_eq!(Decode!(&bytes, SnsNeuron).unwrap(), sns);
    }

    #[test]
    fn nns_manage_neuron_fixtures_round_trip_and_map_split_child() {
        let request = NnsManageNeuron {
            id: Some(NnsNeuronIdRecord { id: 2 }),
            command: Some(NnsManageNeuronCommand::Split(NnsSplit { amount_e8s: 100 })),
        };
        let bytes = Encode!(&request).unwrap();
        assert_eq!(Decode!(&bytes, NnsManageNeuron).unwrap(), request);

        let response = NnsManageNeuronResponse::Command(NnsManageNeuronResponseCommand::Split(
            NnsNeuronIdRecord { id: 10_000 },
        ));
        assert_eq!(nns_split_child_id(response).unwrap(), NnsNeuronId(10_000));
    }

    #[test]
    fn nns_governance_errors_map_and_classify_retryability() {
        assert_eq!(
            NnsGovernanceError::from(NnsGovernanceErrorRecord {
                error_type: 2,
                error_message: "missing".to_string()
            }),
            NnsGovernanceError::NeuronNotFound
        );
        assert!(NnsGovernanceError::TemporarilyUnavailable.is_retryable());
        assert!(!NnsGovernanceError::NotAuthorized.is_retryable());
    }

    #[test]
    fn sns_candid_records_convert_to_domain() {
        let record = SnsNeuronRecord {
            id: Some(SnsNeuronIdRecord { id: vec![1, 2] }),
            controller: Some(principal()),
            cached_neuron_stake_e8s: 123,
            dissolve_state: Some(SnsDissolveStateRecord::DissolveDelaySeconds(1_209_600)),
            permissions: vec![SnsNeuronPermissionRecord {
                principal: Some(principal()),
                permission_type: vec![1],
            }],
        };
        let bytes = Encode!(&record).unwrap();
        let decoded = Decode!(&bytes, SnsNeuronRecord).unwrap();
        let neuron = SnsNeuron::from(decoded);
        assert_eq!(neuron.id, SnsNeuronId(vec![1, 2]));
        assert_eq!(neuron.cached_neuron_stake_e8s, 123);
        assert!(matches!(
            neuron.dissolve_state,
            SnsDissolveState::NotDissolving { .. }
        ));
    }

    #[test]
    fn sns_eligibility_excludes_expected_neurons() {
        let mut protocol = sns_neuron(2, 10_000, 1_209_600);
        protocol.is_io_protocol_neuron = true;
        let mut jupiter = sns_neuron(3, 10_000, 1_209_600);
        jupiter.is_jupiter_governance_neuron = true;
        let mut dissolving = sns_neuron(4, 10_000, 1_209_600);
        dissolving.dissolve_state = SnsDissolveState::Dissolving {
            when_dissolved_timestamp_seconds: 999,
        };
        let neurons = vec![
            sns_neuron(1, 10_000, 1_209_600),
            protocol,
            jupiter,
            dissolving,
            sns_neuron(5, 10_000, 1),
            sns_neuron(6, 0, 1_209_600),
        ];
        let policy = SnsEligibilityPolicy {
            protocol_neuron_ids: BTreeSet::new(),
            jupiter_governance_neuron_ids: BTreeSet::new(),
            minimum_dissolve_delay_seconds: 1_209_600,
            require_non_dissolving: true,
            current_timestamp_seconds: 10,
        };
        let out = snapshot_sns_eligibility(&neurons, &policy);
        assert_eq!(out[0].excluded_reason, None);
        assert_eq!(out[0].eligible_stake_e8s, 10_000);
        assert_eq!(
            out[1].excluded_reason.as_deref(),
            Some("protocol-owned neuron")
        );
        assert_eq!(
            out[2].excluded_reason.as_deref(),
            Some("jupiter governance neuron")
        );
        assert_eq!(
            out[3].excluded_reason.as_deref(),
            Some("neuron is dissolving")
        );
        assert_eq!(
            out[4].excluded_reason.as_deref(),
            Some("dissolve delay below minimum")
        );
        assert_eq!(out[5].excluded_reason.as_deref(), Some("zero stake"));
    }

    #[test]
    fn sns_participation_counts_direct_followed_and_epoch_filters() {
        let eligibility = SnsNeuronEligibility {
            neuron_id: SnsNeuronId(vec![1]),
            owner: Some(principal()),
            eligible_stake_e8s: 100,
            eligible_since_seconds: 50,
            dissolve_delay_seconds: 1_209_600,
            is_non_dissolving: true,
            excluded_reason: None,
        };
        let proposals = vec![
            proposal(1, 10, SnsProposalStatus::Open, SnsVote::Yes),
            proposal(2, 60, SnsProposalStatus::Adopted, SnsVote::Yes),
            proposal(3, 70, SnsProposalStatus::Rejected, SnsVote::FollowedNo),
            proposal(4, 80, SnsProposalStatus::Rejected, SnsVote::Unspecified),
            proposal(5, 101, SnsProposalStatus::Rejected, SnsVote::Yes),
            proposal(6, 40, SnsProposalStatus::Rejected, SnsVote::Yes),
        ];
        let summary = summarize_sns_participation(
            &[eligibility],
            &proposals,
            &SnsParticipationPolicy {
                count_direct_votes: true,
                count_followed_votes: true,
                excluded_topics: BTreeSet::new(),
                epoch_start_seconds: 0,
                epoch_end_seconds: 100,
            },
        );
        assert_eq!(summary[0].eligible_closed_proposals_total, 3);
        assert_eq!(summary[0].voted_proposals, 2);
        assert_eq!(summary[0].participation_bps, 6_666);
    }

    #[test]
    fn sns_participation_defaults_to_full_when_no_proposals() {
        let eligibility = SnsNeuronEligibility {
            neuron_id: SnsNeuronId(vec![1]),
            owner: Some(principal()),
            eligible_stake_e8s: 100,
            eligible_since_seconds: 0,
            dissolve_delay_seconds: 1_209_600,
            is_non_dissolving: true,
            excluded_reason: None,
        };
        let summary = summarize_sns_participation(
            &[eligibility],
            &[],
            &SnsParticipationPolicy {
                count_direct_votes: true,
                count_followed_votes: true,
                excluded_topics: BTreeSet::new(),
                epoch_start_seconds: 0,
                epoch_end_seconds: 100,
            },
        );
        assert_eq!(summary[0].participation_bps, 10_000);
    }

    fn proposal(id: u64, decided: u64, status: SnsProposalStatus, vote: SnsVote) -> SnsProposal {
        SnsProposal {
            id: SnsProposalId(id),
            topic: Some(1),
            status,
            reward_status: SnsProposalRewardStatus::Settled,
            decided_timestamp_seconds: Some(decided),
            ballots: vec![SnsBallot {
                neuron_id: SnsNeuronId(vec![1]),
                vote,
            }],
        }
    }
}
