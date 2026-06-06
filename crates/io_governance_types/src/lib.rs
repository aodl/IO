use candid::{CandidType, Nat, Principal};
use io_ledger_types::Account;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::convert::TryFrom;
use std::future::Future;
use std::pin::Pin;

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct EmptyRecord {}

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
    ConfigureDissolveDelay,
    StartDissolving,
    StopDissolving,
    Merge,
    MergeMaturity,
    StakeMaturity,
    Disburse,
    Follow,
    RefreshVotingPower,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsCommandResult {
    pub command: NnsNeuronCommand,
    pub neuron_id: NnsNeuronId,
    pub amount_e8s: Option<u128>,
    pub transfer_block_height: Option<u64>,
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
    PaginationDidNotProgress,
    MalformedNeuronId { message: String },
    MalformedProposalId { message: String },
    NumericOverflow { field: String },
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
    Configure(NnsProductionConfigure),
    DisburseMaturity(NnsProductionDisburseMaturity),
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

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsProductionListNeuronsRequest {
    pub neuron_ids: Vec<u64>,
    pub include_neurons_readable_by_caller: bool,
    pub include_empty_neurons_readable_by_caller: Option<bool>,
    pub include_public_neurons_in_full_neurons: Option<bool>,
    pub page_number: Option<u64>,
    pub page_size: Option<u64>,
    pub neuron_subaccounts: Option<Vec<NnsNeuronSubaccount>>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsNeuronSubaccount {
    pub subaccount: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsProductionListNeuronsResponse {
    pub neuron_infos: Vec<(u64, NnsNeuronInfoRecord)>,
    pub full_neurons: Vec<NnsNeuronRecord>,
    pub total_pages_available: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsNeuronInfoRecord {
    pub id: Option<NnsNeuronIdRecord>,
    pub dissolve_delay_seconds: u64,
    pub recent_ballots: Vec<NnsBallotInfoRecord>,
    pub neuron_type: Option<i32>,
    pub created_timestamp_seconds: u64,
    pub state: i32,
    pub stake_e8s: u64,
    pub joined_community_fund_timestamp_seconds: Option<u64>,
    pub retrieved_at_timestamp_seconds: u64,
    pub visibility: Option<i32>,
    pub known_neuron_data: Option<NnsKnownNeuronData>,
    pub age_seconds: u64,
    pub voting_power: u64,
    pub voting_power_refreshed_timestamp_seconds: Option<u64>,
    pub deciding_voting_power: Option<u64>,
    pub potential_voting_power: Option<u64>,
    pub eight_year_gang_bonus_base_e8s: Option<u64>,
    pub staked_maturity_e8s_equivalent: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsBallotInfoRecord {
    pub vote: i32,
    pub proposal_id: Option<NnsProposalIdRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsKnownNeuronData {
    pub name: String,
    pub description: Option<String>,
    pub links: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsNeuronRecord {
    pub id: Option<NnsNeuronIdRecord>,
    pub staked_maturity_e8s_equivalent: Option<u64>,
    pub controller: Option<Principal>,
    pub recent_ballots: Vec<NnsBallotInfoRecord>,
    pub kyc_verified: bool,
    pub neuron_type: Option<i32>,
    pub not_for_profit: bool,
    pub maturity_e8s_equivalent: u64,
    pub cached_neuron_stake_e8s: u64,
    pub created_timestamp_seconds: u64,
    pub auto_stake_maturity: Option<bool>,
    pub aging_since_timestamp_seconds: u64,
    pub hot_keys: Vec<Principal>,
    pub account: Vec<u8>,
    pub joined_community_fund_timestamp_seconds: Option<u64>,
    pub dissolve_state: Option<NnsDissolveStateRecord>,
    pub followees: Vec<(i32, NnsFollowees)>,
    pub neuron_fees_e8s: u64,
    pub visibility: Option<i32>,
    pub transfer: Option<NnsNeuronStakeTransfer>,
    pub known_neuron_data: Option<NnsKnownNeuronData>,
    pub spawn_at_timestamp_seconds: Option<u64>,
    pub voting_power_refreshed_timestamp_seconds: Option<u64>,
    pub deciding_voting_power: Option<u64>,
    pub potential_voting_power: Option<u64>,
    pub eight_year_gang_bonus_base_e8s: Option<u64>,
    pub maturity_disbursements_in_progress: Option<Vec<NnsMaturityDisbursement>>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsDissolveStateRecord {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsFollowees {
    pub followees: Vec<NnsNeuronIdRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsNeuronStakeTransfer {
    pub to_subaccount: Vec<u8>,
    pub neuron_stake_e8s: u64,
    pub from: Option<Principal>,
    pub memo: u64,
    pub from_subaccount: Vec<u8>,
    pub transfer_timestamp: u64,
    pub block_height: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsMaturityDisbursement {
    pub timestamp_of_disbursement_seconds: Option<u64>,
    pub amount_e8s: Option<u64>,
    pub account_to_disburse_to: Option<NnsAccount>,
    pub finalize_disbursement_timestamp_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsAccount {
    pub owner: Option<Principal>,
    pub subaccount: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsProposalIdRecord {
    pub id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsProductionListProposalInfoRequest {
    pub include_reward_status: Vec<i32>,
    pub omit_large_fields: Option<bool>,
    pub before_proposal: Option<NnsProposalIdRecord>,
    pub limit: u32,
    pub exclude_topic: Vec<i32>,
    pub include_all_manage_neuron_proposals: Option<bool>,
    pub include_status: Vec<i32>,
    pub return_self_describing_action: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsProductionListProposalInfoResponse {
    pub proposal_info: Vec<NnsProposalInfoRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsProposalInfoRecord {
    pub id: Option<NnsProposalIdRecord>,
    pub topic: i32,
    pub status: i32,
    pub reward_status: i32,
    pub decided_timestamp_seconds: u64,
    pub executed_timestamp_seconds: u64,
    pub failed_timestamp_seconds: u64,
    pub ballots: Vec<(u64, NnsBallotRecord)>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsBallotRecord {
    pub vote: i32,
    pub voting_power: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsProductionManageNeuronRequest {
    pub neuron_id_or_subaccount: Option<NnsNeuronIdOrSubaccount>,
    pub command: Option<NnsManageNeuronCommandRequest>,
    pub id: Option<NnsNeuronIdRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsNeuronIdOrSubaccount {
    Subaccount(Vec<u8>),
    NeuronId(NnsNeuronIdRecord),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsManageNeuronCommandRequest {
    Spawn(NnsSpawn),
    Split(NnsSplit),
    Follow(NnsFollow),
    ClaimOrRefresh(NnsClaimOrRefresh),
    Configure(NnsProductionConfigure),
    RegisterVote(NnsRegisterVote),
    Merge(NnsMerge),
    DisburseToNeuron(NnsDisburseToNeuron),
    MakeProposal(NnsProposalRequest),
    StakeMaturity(NnsStakeMaturity),
    MergeMaturity(NnsMergeMaturity),
    Disburse(NnsDisburse),
    RefreshVotingPower(NnsRefreshVotingPower),
    DisburseMaturity(NnsProductionDisburseMaturity),
    SetFollowing(NnsSetFollowing),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsSpawn {
    pub percentage_to_spawn: Option<u32>,
    pub new_controller: Option<Principal>,
    pub nonce: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsFollow {
    pub topic: i32,
    pub followees: Vec<NnsNeuronIdRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsClaimOrRefresh {
    pub by: Option<NnsClaimOrRefreshBy>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsClaimOrRefreshBy {
    NeuronIdOrSubaccount(EmptyRecord),
    MemoAndController(NnsClaimOrRefreshNeuronFromAccount),
    Memo(u64),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsClaimOrRefreshNeuronFromAccount {
    pub controller: Option<Principal>,
    pub memo: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsRegisterVote {
    pub vote: i32,
    pub proposal: Option<NnsProposalIdRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsMerge {
    pub source_neuron_id: Option<NnsNeuronIdRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsDisburseToNeuron {
    pub dissolve_delay_seconds: u64,
    pub kyc_verified: bool,
    pub amount_e8s: u64,
    pub new_controller: Option<Principal>,
    pub nonce: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsProposalRequest {
    pub url: String,
    pub title: Option<String>,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsStakeMaturity {
    pub percentage_to_stake: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsMergeMaturity {
    pub percentage_to_merge: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsRefreshVotingPower {}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsSetFollowing {
    pub topic_following: Option<Vec<NnsFolloweesForTopic>>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsFolloweesForTopic {
    pub followees: Option<Vec<NnsNeuronIdRecord>>,
    pub topic: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsProductionConfigure {
    pub operation: Option<NnsProductionConfigureOperation>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsProductionConfigureOperation {
    RemoveHotKey(NnsRemoveHotKey),
    AddHotKey(NnsAddHotKey),
    ChangeAutoStakeMaturity(NnsChangeAutoStakeMaturity),
    StopDissolving(EmptyRecord),
    StartDissolving(EmptyRecord),
    IncreaseDissolveDelay(NnsIncreaseDissolveDelay),
    SetVisibility(NnsSetVisibility),
    JoinCommunityFund(EmptyRecord),
    LeaveCommunityFund(EmptyRecord),
    SetDissolveTimestamp(NnsSetDissolveTimestamp),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsRemoveHotKey {
    pub hot_key_to_remove: Option<Principal>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsAddHotKey {
    pub new_hot_key: Option<Principal>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsChangeAutoStakeMaturity {
    pub requested_setting_for_auto_stake_maturity: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsIncreaseDissolveDelay {
    pub additional_dissolve_delay_seconds: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsSetVisibility {
    pub visibility: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsSetDissolveTimestamp {
    pub dissolve_timestamp_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsProductionDisburseMaturity {
    pub percentage_to_disburse: u32,
    pub to_account: Option<NnsAccount>,
    pub to_account_identifier: Option<NnsAccountIdentifier>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsProductionManageNeuronResponse {
    pub command: Option<NnsManageNeuronResponseCommandRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsManageNeuronResponseCommandRecord {
    Error(NnsGovernanceErrorRecord),
    Spawn(NnsSpawnResponse),
    Split(NnsSpawnResponse),
    Follow(EmptyRecord),
    ClaimOrRefresh(NnsClaimOrRefreshResponse),
    Configure(EmptyRecord),
    RegisterVote(EmptyRecord),
    Merge(Box<NnsMergeResponse>),
    DisburseToNeuron(NnsSpawnResponse),
    MakeProposal(NnsMakeProposalResponse),
    StakeMaturity(NnsStakeMaturityResponse),
    MergeMaturity(NnsMergeMaturityResponse),
    Disburse(NnsDisburseResponse),
    RefreshVotingPower(NnsRefreshVotingPowerResponse),
    DisburseMaturity(NnsProductionDisburseMaturityResponse),
    SetFollowing(NnsSetFollowingResponse),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsSpawnResponse {
    pub created_neuron_id: Option<NnsNeuronIdRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsClaimOrRefreshResponse {
    pub refreshed_neuron_id: Option<NnsNeuronIdRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsMergeResponse {
    pub target_neuron: Option<NnsNeuronRecord>,
    pub source_neuron: Option<NnsNeuronRecord>,
    pub target_neuron_info: Option<NnsNeuronInfoRecord>,
    pub source_neuron_info: Option<NnsNeuronInfoRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsMakeProposalResponse {
    pub message: Option<String>,
    pub proposal_id: Option<NnsProposalIdRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsStakeMaturityResponse {
    pub maturity_e8s: u64,
    pub staked_maturity_e8s: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsMergeMaturityResponse {
    pub merged_maturity_e8s: u64,
    pub new_stake_e8s: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsDisburseResponse {
    pub transfer_block_height: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsRefreshVotingPowerResponse {}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsProductionDisburseMaturityResponse {
    pub amount_disbursed_e8s: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsSetFollowingResponse {}

impl TryFrom<NnsNeuronRecord> for NnsNeuron {
    type Error = NnsGovernanceError;

    fn try_from(value: NnsNeuronRecord) -> Result<Self, Self::Error> {
        let id = value.id.map(|id| NnsNeuronId(id.id)).ok_or_else(|| {
            NnsGovernanceError::MalformedNeuronId {
                message: "NNS neuron record missing id".to_string(),
            }
        })?;
        let (dissolve_delay_seconds, dissolve_state) = match value.dissolve_state {
            Some(NnsDissolveStateRecord::DissolveDelaySeconds(delay)) => (
                delay,
                NnsDissolveState::NotDissolving {
                    dissolve_delay_seconds: delay,
                },
            ),
            Some(NnsDissolveStateRecord::WhenDissolvedTimestampSeconds(when)) => (
                0,
                NnsDissolveState::Dissolving {
                    when_dissolved_timestamp_seconds: when,
                },
            ),
            None => (0, NnsDissolveState::Dissolved),
        };
        Ok(Self {
            id,
            controller: value.controller,
            stake_e8s: u128::from(value.cached_neuron_stake_e8s),
            maturity_e8s_equivalent: u128::from(value.maturity_e8s_equivalent),
            dissolve_delay_seconds,
            dissolve_state,
            known_neuron_name: value.known_neuron_data.map(|known| known.name),
        })
    }
}

impl NnsNeuronInfoRecord {
    pub fn to_domain_neuron(&self) -> Result<NnsNeuron, NnsGovernanceError> {
        let id = self.id.map(|id| NnsNeuronId(id.id)).ok_or_else(|| {
            NnsGovernanceError::MalformedNeuronId {
                message: "NNS neuron info missing id".to_string(),
            }
        })?;
        let dissolve_state = if self.state == 2 {
            NnsDissolveState::Dissolving {
                when_dissolved_timestamp_seconds: self
                    .retrieved_at_timestamp_seconds
                    .saturating_add(self.dissolve_delay_seconds),
            }
        } else if self.dissolve_delay_seconds == 0 {
            NnsDissolveState::Dissolved
        } else {
            NnsDissolveState::NotDissolving {
                dissolve_delay_seconds: self.dissolve_delay_seconds,
            }
        };
        Ok(NnsNeuron {
            id,
            controller: None,
            stake_e8s: u128::from(self.stake_e8s),
            maturity_e8s_equivalent: 0,
            dissolve_delay_seconds: self.dissolve_delay_seconds,
            dissolve_state,
            known_neuron_name: self.known_neuron_data.clone().map(|known| known.name),
        })
    }
}

pub fn nns_manage_neuron_request(
    neuron_id: NnsNeuronId,
    command: NnsManageNeuronCommandRequest,
) -> NnsProductionManageNeuronRequest {
    NnsProductionManageNeuronRequest {
        neuron_id_or_subaccount: Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
            id: neuron_id.0,
        })),
        command: Some(command),
        id: Some(NnsNeuronIdRecord { id: neuron_id.0 }),
    }
}

pub fn nns_split_request(
    neuron_id: NnsNeuronId,
    amount_e8s: u128,
) -> Result<NnsProductionManageNeuronRequest, NnsGovernanceError> {
    Ok(nns_manage_neuron_request(
        neuron_id,
        NnsManageNeuronCommandRequest::Split(NnsSplit {
            amount_e8s: u64::try_from(amount_e8s).map_err(|_| {
                NnsGovernanceError::NumericOverflow {
                    field: "split.amount_e8s".to_string(),
                }
            })?,
        }),
    ))
}

pub fn nns_configure_dissolve_delay_request(
    neuron_id: NnsNeuronId,
    additional_dissolve_delay_seconds: u32,
) -> NnsProductionManageNeuronRequest {
    nns_manage_neuron_request(
        neuron_id,
        NnsManageNeuronCommandRequest::Configure(NnsProductionConfigure {
            operation: Some(NnsProductionConfigureOperation::IncreaseDissolveDelay(
                NnsIncreaseDissolveDelay {
                    additional_dissolve_delay_seconds,
                },
            )),
        }),
    )
}

pub fn nns_start_dissolving_request(neuron_id: NnsNeuronId) -> NnsProductionManageNeuronRequest {
    nns_manage_neuron_request(
        neuron_id,
        NnsManageNeuronCommandRequest::Configure(NnsProductionConfigure {
            operation: Some(NnsProductionConfigureOperation::StartDissolving(
                EmptyRecord {},
            )),
        }),
    )
}

pub fn nns_stop_dissolving_request(neuron_id: NnsNeuronId) -> NnsProductionManageNeuronRequest {
    nns_manage_neuron_request(
        neuron_id,
        NnsManageNeuronCommandRequest::Configure(NnsProductionConfigure {
            operation: Some(NnsProductionConfigureOperation::StopDissolving(
                EmptyRecord {},
            )),
        }),
    )
}

pub fn nns_merge_request(
    target_neuron_id: NnsNeuronId,
    source_neuron_id: NnsNeuronId,
) -> NnsProductionManageNeuronRequest {
    nns_manage_neuron_request(
        target_neuron_id,
        NnsManageNeuronCommandRequest::Merge(NnsMerge {
            source_neuron_id: Some(NnsNeuronIdRecord {
                id: source_neuron_id.0,
            }),
        }),
    )
}

pub fn nns_merge_maturity_request(
    neuron_id: NnsNeuronId,
    percentage_to_merge: u32,
) -> NnsProductionManageNeuronRequest {
    nns_manage_neuron_request(
        neuron_id,
        NnsManageNeuronCommandRequest::MergeMaturity(NnsMergeMaturity {
            percentage_to_merge,
        }),
    )
}

pub fn nns_stake_maturity_request(
    neuron_id: NnsNeuronId,
    percentage_to_stake: Option<u32>,
) -> NnsProductionManageNeuronRequest {
    nns_manage_neuron_request(
        neuron_id,
        NnsManageNeuronCommandRequest::StakeMaturity(NnsStakeMaturity {
            percentage_to_stake,
        }),
    )
}

pub fn nns_disburse_maturity_request(
    neuron_id: NnsNeuronId,
    percentage_to_disburse: u32,
    to: Account,
) -> NnsProductionManageNeuronRequest {
    nns_manage_neuron_request(
        neuron_id,
        NnsManageNeuronCommandRequest::DisburseMaturity(NnsProductionDisburseMaturity {
            percentage_to_disburse,
            to_account: None,
            to_account_identifier: Some(NnsAccountIdentifier {
                hash: to.icp_account_identifier_bytes().to_vec(),
            }),
        }),
    )
}

pub fn nns_disburse_request(
    neuron_id: NnsNeuronId,
    to: Account,
) -> NnsProductionManageNeuronRequest {
    nns_manage_neuron_request(
        neuron_id,
        NnsManageNeuronCommandRequest::Disburse(NnsDisburse {
            to_account: Some(NnsAccountIdentifier {
                hash: to.icp_account_identifier_bytes().to_vec(),
            }),
            amount: None,
        }),
    )
}

pub fn nns_refresh_voting_power_request(
    neuron_id: NnsNeuronId,
) -> NnsProductionManageNeuronRequest {
    nns_manage_neuron_request(
        neuron_id,
        NnsManageNeuronCommandRequest::RefreshVotingPower(NnsRefreshVotingPower {}),
    )
}

pub fn nns_command_result_from_response(
    expected: NnsNeuronCommand,
    neuron_id: NnsNeuronId,
    response: NnsProductionManageNeuronResponse,
) -> Result<NnsCommandResult, NnsGovernanceError> {
    let command = response
        .command
        .ok_or_else(|| NnsGovernanceError::DecodeError {
            message: "manage_neuron response missing command".to_string(),
        })?;
    match command {
        NnsManageNeuronResponseCommandRecord::Error(err) => Err(err.into()),
        NnsManageNeuronResponseCommandRecord::Split(response)
            if expected == NnsNeuronCommand::Split =>
        {
            let child = response
                .created_neuron_id
                .map(|id| NnsNeuronId(id.id))
                .ok_or_else(|| NnsGovernanceError::MalformedNeuronId {
                    message: "split response missing created_neuron_id".to_string(),
                })?;
            Ok(NnsCommandResult {
                command: expected,
                neuron_id,
                amount_e8s: None,
                transfer_block_height: None,
                child_neuron_id: Some(child),
            })
        }
        NnsManageNeuronResponseCommandRecord::Configure(_)
            if expected == NnsNeuronCommand::StartDissolving
                || expected == NnsNeuronCommand::StopDissolving
                || expected == NnsNeuronCommand::ConfigureDissolveDelay =>
        {
            Ok(NnsCommandResult {
                command: expected,
                neuron_id,
                amount_e8s: None,
                transfer_block_height: None,
                child_neuron_id: None,
            })
        }
        NnsManageNeuronResponseCommandRecord::Merge(_) if expected == NnsNeuronCommand::Merge => {
            Ok(NnsCommandResult {
                command: expected,
                neuron_id,
                amount_e8s: None,
                transfer_block_height: None,
                child_neuron_id: None,
            })
        }
        NnsManageNeuronResponseCommandRecord::MergeMaturity(response)
            if expected == NnsNeuronCommand::MergeMaturity =>
        {
            Ok(NnsCommandResult {
                command: expected,
                neuron_id,
                amount_e8s: Some(u128::from(response.merged_maturity_e8s)),
                transfer_block_height: None,
                child_neuron_id: None,
            })
        }
        NnsManageNeuronResponseCommandRecord::StakeMaturity(response)
            if expected == NnsNeuronCommand::StakeMaturity =>
        {
            Ok(NnsCommandResult {
                command: expected,
                neuron_id,
                amount_e8s: Some(u128::from(response.staked_maturity_e8s)),
                transfer_block_height: None,
                child_neuron_id: None,
            })
        }
        NnsManageNeuronResponseCommandRecord::DisburseMaturity(response)
            if expected == NnsNeuronCommand::DisburseMaturity =>
        {
            Ok(NnsCommandResult {
                command: expected,
                neuron_id,
                amount_e8s: response.amount_disbursed_e8s.map(u128::from),
                transfer_block_height: None,
                child_neuron_id: None,
            })
        }
        NnsManageNeuronResponseCommandRecord::Disburse(response)
            if expected == NnsNeuronCommand::Disburse =>
        {
            Ok(NnsCommandResult {
                command: expected,
                neuron_id,
                amount_e8s: None,
                transfer_block_height: Some(response.transfer_block_height),
                child_neuron_id: None,
            })
        }
        NnsManageNeuronResponseCommandRecord::RefreshVotingPower(_)
            if expected == NnsNeuronCommand::RefreshVotingPower =>
        {
            Ok(NnsCommandResult {
                command: expected,
                neuron_id,
                amount_e8s: None,
                transfer_block_height: None,
                child_neuron_id: None,
            })
        }
        other => Err(NnsGovernanceError::DecodeError {
            message: format!(
                "manage_neuron response command {other:?} did not match expected {expected:?}"
            ),
        }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NnsGovernanceCanisterClient {
    pub canister: Principal,
}

impl NnsGovernanceClient for NnsGovernanceCanisterClient {
    fn get_neuron<'a>(
        &'a self,
        id: NnsNeuronId,
    ) -> Pin<Box<dyn Future<Output = Result<NnsNeuron, NnsGovernanceError>> + 'a>> {
        Box::pin(async move { nns_canister_get_neuron(self.canister, id).await })
    }

    fn disburse_maturity<'a>(
        &'a self,
        id: NnsNeuronId,
        percentage_to_disburse: u32,
        to: Account,
    ) -> Pin<Box<dyn Future<Output = Result<NnsCommandResult, NnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            let request = nns_disburse_maturity_request(id, percentage_to_disburse, to);
            let response = nns_canister_manage_neuron(self.canister, request).await?;
            nns_command_result_from_response(NnsNeuronCommand::DisburseMaturity, id, response)
        })
    }

    fn split_neuron<'a>(
        &'a self,
        id: NnsNeuronId,
        amount_e8s: u128,
    ) -> Pin<Box<dyn Future<Output = Result<NnsNeuronId, NnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            let request = nns_split_request(id, amount_e8s)?;
            let response = nns_canister_manage_neuron(self.canister, request).await?;
            nns_command_result_from_response(NnsNeuronCommand::Split, id, response)?
                .child_neuron_id
                .ok_or_else(|| NnsGovernanceError::MalformedNeuronId {
                    message: "split command result missing child neuron id".to_string(),
                })
        })
    }

    fn start_dissolving<'a>(
        &'a self,
        id: NnsNeuronId,
    ) -> Pin<Box<dyn Future<Output = Result<NnsCommandResult, NnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            let response =
                nns_canister_manage_neuron(self.canister, nns_start_dissolving_request(id)).await?;
            nns_command_result_from_response(NnsNeuronCommand::StartDissolving, id, response)
        })
    }

    fn stop_dissolving<'a>(
        &'a self,
        id: NnsNeuronId,
    ) -> Pin<Box<dyn Future<Output = Result<NnsCommandResult, NnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            let response =
                nns_canister_manage_neuron(self.canister, nns_stop_dissolving_request(id)).await?;
            nns_command_result_from_response(NnsNeuronCommand::StopDissolving, id, response)
        })
    }

    fn disburse_neuron<'a>(
        &'a self,
        id: NnsNeuronId,
        to: Account,
    ) -> Pin<Box<dyn Future<Output = Result<NnsCommandResult, NnsGovernanceError>> + 'a>> {
        Box::pin(async move {
            let response =
                nns_canister_manage_neuron(self.canister, nns_disburse_request(id, to)).await?;
            nns_command_result_from_response(NnsNeuronCommand::Disburse, id, response)
        })
    }
}

async fn nns_canister_get_neuron(
    canister: Principal,
    id: NnsNeuronId,
) -> Result<NnsNeuron, NnsGovernanceError> {
    #[cfg(target_family = "wasm")]
    {
        let request = NnsProductionListNeuronsRequest {
            neuron_ids: vec![id.0],
            include_neurons_readable_by_caller: true,
            include_empty_neurons_readable_by_caller: Some(false),
            include_public_neurons_in_full_neurons: Some(true),
            page_number: None,
            page_size: Some(1),
            neuron_subaccounts: None,
        };
        let response = ic_cdk::call::Call::bounded_wait(canister, "list_neurons")
            .with_arg(request)
            .await
            .map_err(|err| NnsGovernanceError::CanisterCallFailed {
                method: "list_neurons".to_string(),
                message: format!("{err:?}"),
            })?
            .candid::<NnsProductionListNeuronsResponse>()
            .map_err(|err| NnsGovernanceError::DecodeError {
                message: format!("{err:?}"),
            })?;
        if let Some(neuron) = response.full_neurons.into_iter().next() {
            return neuron.try_into();
        }
        response
            .neuron_infos
            .into_iter()
            .find(|(record_id, _)| *record_id == id.0)
            .ok_or(NnsGovernanceError::NeuronNotFound)?
            .1
            .to_domain_neuron()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = (canister, id);
        Err(NnsGovernanceError::Unsupported)
    }
}

async fn nns_canister_manage_neuron(
    canister: Principal,
    request: NnsProductionManageNeuronRequest,
) -> Result<NnsProductionManageNeuronResponse, NnsGovernanceError> {
    #[cfg(target_family = "wasm")]
    {
        ic_cdk::call::Call::bounded_wait(canister, "manage_neuron")
            .with_arg(request)
            .await
            .map_err(|err| NnsGovernanceError::CanisterCallFailed {
                method: "manage_neuron".to_string(),
                message: format!("{err:?}"),
            })?
            .candid::<NnsProductionManageNeuronResponse>()
            .map_err(|err| NnsGovernanceError::DecodeError {
                message: format!("{err:?}"),
            })
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = (canister, request);
        Err(NnsGovernanceError::Unsupported)
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
    NotAuthorized,
    InvalidCommand { message: String },
    NotFound,
    DecodeError { message: String },
    CanisterCallFailed { method: String, message: String },
    PaginationDidNotProgress,
    MalformedNeuronId { message: String },
    MalformedProposalId { message: String },
    NumericOverflow { field: String },
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

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsGovernanceErrorRecord {
    pub error_message: String,
    pub error_type: i32,
}

impl From<SnsGovernanceErrorRecord> for SnsGovernanceError {
    fn from(value: SnsGovernanceErrorRecord) -> Self {
        match value.error_type {
            2 => Self::NotFound,
            3 => Self::NotAuthorized,
            4 => Self::InvalidCommand {
                message: value.error_message,
            },
            5 | 6 => Self::TemporarilyUnavailable,
            code => Self::GovernanceReject {
                code,
                message: value.error_message,
            },
        }
    }
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
pub struct SnsProductionListNeuronsRequest {
    pub of_principal: Option<Principal>,
    pub limit: u32,
    pub start_page_at: Option<SnsNeuronIdRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProductionListNeuronsResponse {
    pub neurons: Vec<SnsNeuronRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProductionGetNeuronRequest {
    pub neuron_id: Option<SnsNeuronIdRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProductionGetNeuronResponse {
    pub result: Option<SnsGetNeuronResult>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum SnsGetNeuronResult {
    Error(SnsGovernanceErrorRecord),
    Neuron(Box<SnsNeuronRecord>),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProductionListProposalsRequest {
    pub include_reward_status: Vec<i32>,
    pub before_proposal: Option<SnsProposalIdRecord>,
    pub limit: u32,
    pub exclude_type: Vec<u64>,
    pub include_status: Vec<i32>,
    pub include_topics: Option<Vec<SnsTopicSelector>>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsTopicSelector {
    pub topic: Option<SnsTopic>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum SnsTopic {
    DaoCommunitySettings,
    SnsFrameworkManagement,
    DappCanisterManagement,
    ApplicationBusinessLogic,
    Governance,
    TreasuryAssetManagement,
    CriticalDappOperations,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProductionListProposalsResponse {
    pub include_ballots_by_caller: Option<bool>,
    pub include_topic_filtering: Option<bool>,
    pub proposals: Vec<SnsProposalRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProductionGetProposalRequest {
    pub proposal_id: Option<SnsProposalIdRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProductionGetProposalResponse {
    pub result: Option<SnsGetProposalResult>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum SnsGetProposalResult {
    Error(SnsGovernanceErrorRecord),
    Proposal(Box<SnsProposalRecord>),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsNeuronRecord {
    pub id: Option<SnsNeuronIdRecord>,
    pub staked_maturity_e8s_equivalent: Option<u64>,
    pub cached_neuron_stake_e8s: u64,
    pub maturity_e8s_equivalent: u64,
    pub created_timestamp_seconds: u64,
    pub source_nns_neuron_id: Option<u64>,
    pub auto_stake_maturity: Option<bool>,
    pub aging_since_timestamp_seconds: u64,
    pub dissolve_state: Option<SnsDissolveStateRecord>,
    pub voting_power_percentage_multiplier: u64,
    pub vesting_period_seconds: Option<u64>,
    pub disburse_maturity_in_progress: Vec<SnsDisburseMaturityInProgress>,
    pub followees: Vec<(u64, SnsFollowees)>,
    pub neuron_fees_e8s: u64,
    pub permissions: Vec<SnsNeuronPermissionRecord>,
    pub topic_followees: Option<SnsTopicFollowees>,
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
pub struct SnsDisburseMaturityInProgress {
    pub timestamp_of_disbursement_seconds: u64,
    pub amount_e8s: u64,
    pub account_to_disburse_to: Option<SnsAccount>,
    pub finalize_disbursement_timestamp_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsAccount {
    pub owner: Option<Principal>,
    pub subaccount: Option<SnsSubaccount>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsSubaccount {
    pub subaccount: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsFollowees {
    pub followees: Vec<SnsNeuronIdRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsTopicFollowees {
    pub topic_id_to_followees: Vec<(i32, SnsFolloweesForTopic)>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsFolloweesForTopic {
    pub followees: Vec<SnsFollowee>,
    pub topic: Option<SnsTopic>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsFollowee {
    pub neuron_id: Option<SnsNeuronIdRecord>,
    pub alias: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProposalRecord {
    pub id: Option<SnsProposalIdRecord>,
    pub payload_text_rendering: Option<String>,
    pub action: u64,
    pub failure_reason: Option<SnsGovernanceErrorRecord>,
    pub ballots: Vec<(String, SnsBallotRecord)>,
    pub minimum_yes_proportion_of_total: Option<SnsPercentage>,
    pub reward_event_round: u64,
    pub failed_timestamp_seconds: u64,
    pub reward_event_end_timestamp_seconds: Option<u64>,
    pub proposal_creation_timestamp_seconds: u64,
    pub initial_voting_period_seconds: u64,
    pub reject_cost_e8s: u64,
    pub latest_tally: Option<SnsTally>,
    pub wait_for_quiet_deadline_increase_seconds: u64,
    pub decided_timestamp_seconds: u64,
    pub proposal: Option<SnsProposalPayload>,
    pub proposer: Option<SnsNeuronIdRecord>,
    pub wait_for_quiet_state: Option<SnsWaitForQuietState>,
    pub minimum_yes_proportion_of_exercised: Option<SnsPercentage>,
    pub is_eligible_for_rewards: bool,
    pub executed_timestamp_seconds: u64,
    pub topic: Option<SnsTopic>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsPercentage {
    pub basis_points: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsTally {
    pub no: u64,
    pub yes: u64,
    pub total: u64,
    pub timestamp_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsProposalPayload {
    pub url: String,
    pub title: String,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum SnsProposalAction {
    Unspecified(EmptyRecord),
    Motion(SnsMotion),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsMotion {
    pub motion_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsWaitForQuietState {
    pub current_deadline_timestamp_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsLegacyProposalRecord {
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

impl TryFrom<SnsNeuronRecord> for SnsNeuron {
    type Error = SnsGovernanceError;

    fn try_from(value: SnsNeuronRecord) -> Result<Self, Self::Error> {
        let id = value.id.map(|id| SnsNeuronId(id.id)).ok_or_else(|| {
            SnsGovernanceError::MalformedNeuronId {
                message: "SNS neuron record missing id".to_string(),
            }
        })?;
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
        let controller = sns_controller_from_permissions(&value.permissions);
        Ok(Self {
            id,
            controller,
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
        })
    }
}

const SNS_PERMISSION_MANAGE_PRINCIPALS: i32 = 3;

fn sns_controller_from_permissions(permissions: &[SnsNeuronPermissionRecord]) -> Option<Principal> {
    permissions
        .iter()
        .find(|permission| {
            permission.principal.is_some()
                && permission
                    .permission_type
                    .contains(&SNS_PERMISSION_MANAGE_PRINCIPALS)
        })
        .and_then(|permission| permission.principal)
        .or_else(|| {
            permissions
                .iter()
                .find_map(|permission| permission.principal)
        })
}

impl TryFrom<SnsProposalRecord> for SnsProposal {
    type Error = SnsGovernanceError;

    fn try_from(value: SnsProposalRecord) -> Result<Self, Self::Error> {
        let id = value.id.map(|id| SnsProposalId(id.id)).ok_or_else(|| {
            SnsGovernanceError::MalformedProposalId {
                message: "SNS proposal record missing id".to_string(),
            }
        })?;
        let status = sns_proposal_status_from_record(&value);
        Ok(Self {
            id,
            topic: value.topic.map(sns_topic_code),
            status,
            reward_status: sns_reward_status_from_record(&value, status),
            decided_timestamp_seconds: (value.decided_timestamp_seconds > 0)
                .then_some(value.decided_timestamp_seconds),
            ballots: sns_ballots_from_records(value.ballots),
        })
    }
}

impl From<SnsLegacyProposalRecord> for SnsProposal {
    fn from(value: SnsLegacyProposalRecord) -> Self {
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
            ballots: sns_ballots_from_records(value.ballots),
        }
    }
}

fn sns_ballots_from_records(ballots: Vec<(String, SnsBallotRecord)>) -> Vec<SnsBallot> {
    ballots
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
        .collect()
}

fn sns_proposal_status_from_record(value: &SnsProposalRecord) -> SnsProposalStatus {
    if value.decided_timestamp_seconds == 0 {
        SnsProposalStatus::Open
    } else if value.failed_timestamp_seconds > 0 {
        SnsProposalStatus::Failed
    } else if value.executed_timestamp_seconds > 0 {
        SnsProposalStatus::Executed
    } else if value
        .latest_tally
        .as_ref()
        .is_some_and(|tally| tally.no >= tally.yes)
    {
        SnsProposalStatus::Rejected
    } else {
        SnsProposalStatus::Adopted
    }
}

fn sns_reward_status_from_record(
    value: &SnsProposalRecord,
    status: SnsProposalStatus,
) -> SnsProposalRewardStatus {
    if !value.is_eligible_for_rewards {
        SnsProposalRewardStatus::Ineligible
    } else if value.reward_event_round > 0 || value.reward_event_end_timestamp_seconds.is_some() {
        SnsProposalRewardStatus::Settled
    } else if status.is_closed() {
        SnsProposalRewardStatus::ReadyToSettle
    } else {
        SnsProposalRewardStatus::AcceptVotes
    }
}

fn sns_topic_code(topic: SnsTopic) -> u64 {
    match topic {
        SnsTopic::DaoCommunitySettings => 1,
        SnsTopic::SnsFrameworkManagement => 2,
        SnsTopic::DappCanisterManagement => 3,
        SnsTopic::ApplicationBusinessLogic => 4,
        SnsTopic::Governance => 5,
        SnsTopic::TreasuryAssetManagement => 6,
        SnsTopic::CriticalDappOperations => 7,
    }
}

pub fn sns_neuron_page_from_production_response(
    request: &SnsNeuronPageRequest,
    response: SnsProductionListNeuronsResponse,
) -> Result<SnsNeuronPage, SnsGovernanceError> {
    let mut neurons = Vec::with_capacity(response.neurons.len());
    let mut seen = BTreeSet::new();
    for record in response.neurons {
        let neuron = SnsNeuron::try_from(record)?;
        if !seen.insert(neuron.id.clone()) {
            return Err(SnsGovernanceError::PaginationDidNotProgress);
        }
        neurons.push(neuron);
    }
    let next_page_at = sns_next_neuron_cursor(request, &neurons)?;
    Ok(SnsNeuronPage {
        neurons,
        next_page_at,
    })
}

pub fn sns_proposal_page_from_production_response(
    request: &SnsProposalPageRequest,
    response: SnsProductionListProposalsResponse,
) -> Result<SnsProposalPage, SnsGovernanceError> {
    let mut proposals = Vec::with_capacity(response.proposals.len());
    let mut seen = BTreeSet::new();
    for record in response.proposals {
        let proposal = SnsProposal::try_from(record)?;
        if !seen.insert(proposal.id) {
            return Err(SnsGovernanceError::PaginationDidNotProgress);
        }
        proposals.push(proposal);
    }
    let next_before_proposal = sns_next_proposal_cursor(request, &proposals)?;
    Ok(SnsProposalPage {
        proposals,
        next_before_proposal,
    })
}

fn sns_next_neuron_cursor(
    request: &SnsNeuronPageRequest,
    neurons: &[SnsNeuron],
) -> Result<Option<SnsNeuronId>, SnsGovernanceError> {
    if request.limit == 0 {
        return Err(SnsGovernanceError::InvalidCommand {
            message: "SNS neuron page limit must be greater than zero".to_string(),
        });
    }
    if neurons.len() < request.limit as usize {
        return Ok(None);
    }
    let Some(last) = neurons.last().map(|neuron| neuron.id.clone()) else {
        return Ok(None);
    };
    if request
        .start_page_at
        .as_ref()
        .is_some_and(|cursor| *cursor >= last)
    {
        return Err(SnsGovernanceError::PaginationDidNotProgress);
    }
    Ok(Some(last))
}

fn sns_next_proposal_cursor(
    request: &SnsProposalPageRequest,
    proposals: &[SnsProposal],
) -> Result<Option<SnsProposalId>, SnsGovernanceError> {
    if request.limit == 0 {
        return Err(SnsGovernanceError::InvalidCommand {
            message: "SNS proposal page limit must be greater than zero".to_string(),
        });
    }
    if proposals.len() < request.limit as usize {
        return Ok(None);
    }
    let Some(last) = proposals.last().map(|proposal| proposal.id) else {
        return Ok(None);
    };
    if request.before_proposal.is_some_and(|cursor| last >= cursor) {
        return Err(SnsGovernanceError::PaginationDidNotProgress);
    }
    Ok(Some(last))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnsGovernanceCanisterClient {
    pub canister: Principal,
}

impl SnsGovernanceClient for SnsGovernanceCanisterClient {
    fn list_neurons<'a>(
        &'a self,
        page: SnsNeuronPageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SnsNeuronPage, SnsGovernanceError>> + 'a>> {
        Box::pin(async move { sns_canister_list_neurons(self.canister, page).await })
    }

    fn get_neuron<'a>(
        &'a self,
        id: SnsNeuronId,
    ) -> Pin<Box<dyn Future<Output = Result<SnsNeuron, SnsGovernanceError>> + 'a>> {
        Box::pin(async move { sns_canister_get_neuron(self.canister, id).await })
    }

    fn list_proposals<'a>(
        &'a self,
        request: SnsProposalPageRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SnsProposalPage, SnsGovernanceError>> + 'a>> {
        Box::pin(async move { sns_canister_list_proposals(self.canister, request).await })
    }

    fn get_proposal<'a>(
        &'a self,
        id: SnsProposalId,
    ) -> Pin<Box<dyn Future<Output = Result<SnsProposal, SnsGovernanceError>> + 'a>> {
        Box::pin(async move { sns_canister_get_proposal(self.canister, id).await })
    }
}

async fn sns_canister_list_neurons(
    canister: Principal,
    page: SnsNeuronPageRequest,
) -> Result<SnsNeuronPage, SnsGovernanceError> {
    let limit = u32::try_from(page.limit).map_err(|_| SnsGovernanceError::NumericOverflow {
        field: "list_neurons.limit".to_string(),
    })?;
    #[cfg(target_family = "wasm")]
    {
        let request = SnsProductionListNeuronsRequest {
            of_principal: None,
            limit,
            start_page_at: page
                .start_page_at
                .as_ref()
                .map(|id| SnsNeuronIdRecord { id: id.0.clone() }),
        };
        let response = ic_cdk::call::Call::bounded_wait(canister, "list_neurons")
            .with_arg(request)
            .await
            .map_err(|err| SnsGovernanceError::CanisterCallFailed {
                method: "list_neurons".to_string(),
                message: format!("{err:?}"),
            })?
            .candid::<SnsProductionListNeuronsResponse>()
            .map_err(|err| SnsGovernanceError::DecodeError {
                message: format!("{err:?}"),
            })?;
        sns_neuron_page_from_production_response(&page, response)
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = (canister, limit);
        Err(SnsGovernanceError::Unsupported)
    }
}

async fn sns_canister_get_neuron(
    canister: Principal,
    id: SnsNeuronId,
) -> Result<SnsNeuron, SnsGovernanceError> {
    #[cfg(target_family = "wasm")]
    {
        let request = SnsProductionGetNeuronRequest {
            neuron_id: Some(SnsNeuronIdRecord { id: id.0 }),
        };
        let response = ic_cdk::call::Call::bounded_wait(canister, "get_neuron")
            .with_arg(request)
            .await
            .map_err(|err| SnsGovernanceError::CanisterCallFailed {
                method: "get_neuron".to_string(),
                message: format!("{err:?}"),
            })?
            .candid::<SnsProductionGetNeuronResponse>()
            .map_err(|err| SnsGovernanceError::DecodeError {
                message: format!("{err:?}"),
            })?;
        match response.result {
            Some(SnsGetNeuronResult::Neuron(neuron)) => (*neuron).try_into(),
            Some(SnsGetNeuronResult::Error(err)) => Err(err.into()),
            None => Err(SnsGovernanceError::DecodeError {
                message: "get_neuron response missing result".to_string(),
            }),
        }
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = (canister, id);
        Err(SnsGovernanceError::Unsupported)
    }
}

async fn sns_canister_list_proposals(
    canister: Principal,
    request: SnsProposalPageRequest,
) -> Result<SnsProposalPage, SnsGovernanceError> {
    let limit = u32::try_from(request.limit).map_err(|_| SnsGovernanceError::NumericOverflow {
        field: "list_proposals.limit".to_string(),
    })?;
    #[cfg(target_family = "wasm")]
    {
        let production_request = SnsProductionListProposalsRequest {
            include_reward_status: Vec::new(),
            before_proposal: request
                .before_proposal
                .map(|proposal| SnsProposalIdRecord { id: proposal.0 }),
            limit,
            exclude_type: Vec::new(),
            include_status: Vec::new(),
            include_topics: None,
        };
        let response = ic_cdk::call::Call::bounded_wait(canister, "list_proposals")
            .with_arg(production_request)
            .await
            .map_err(|err| SnsGovernanceError::CanisterCallFailed {
                method: "list_proposals".to_string(),
                message: format!("{err:?}"),
            })?
            .candid::<SnsProductionListProposalsResponse>()
            .map_err(|err| SnsGovernanceError::DecodeError {
                message: format!("{err:?}"),
            })?;
        sns_proposal_page_from_production_response(&request, response)
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = (canister, limit);
        Err(SnsGovernanceError::Unsupported)
    }
}

async fn sns_canister_get_proposal(
    canister: Principal,
    id: SnsProposalId,
) -> Result<SnsProposal, SnsGovernanceError> {
    #[cfg(target_family = "wasm")]
    {
        let request = SnsProductionGetProposalRequest {
            proposal_id: Some(SnsProposalIdRecord { id: id.0 }),
        };
        let response = ic_cdk::call::Call::bounded_wait(canister, "get_proposal")
            .with_arg(request)
            .await
            .map_err(|err| SnsGovernanceError::CanisterCallFailed {
                method: "get_proposal".to_string(),
                message: format!("{err:?}"),
            })?
            .candid::<SnsProductionGetProposalResponse>()
            .map_err(|err| SnsGovernanceError::DecodeError {
                message: format!("{err:?}"),
            })?;
        match response.result {
            Some(SnsGetProposalResult::Proposal(proposal)) => (*proposal).try_into(),
            Some(SnsGetProposalResult::Error(err)) => Err(err.into()),
            None => Err(SnsGovernanceError::DecodeError {
                message: "get_proposal response missing result".to_string(),
            }),
        }
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let _ = (canister, id);
        Err(SnsGovernanceError::Unsupported)
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

    fn account() -> Account {
        Account::new(principal(), None)
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

    fn nns_neuron_record(id: u64) -> NnsNeuronRecord {
        NnsNeuronRecord {
            id: Some(NnsNeuronIdRecord { id }),
            staked_maturity_e8s_equivalent: Some(7),
            controller: Some(principal()),
            recent_ballots: Vec::new(),
            kyc_verified: true,
            neuron_type: None,
            not_for_profit: false,
            maturity_e8s_equivalent: 11,
            cached_neuron_stake_e8s: 100,
            created_timestamp_seconds: 1,
            auto_stake_maturity: Some(false),
            aging_since_timestamp_seconds: 1,
            hot_keys: vec![principal()],
            account: vec![0; 32],
            joined_community_fund_timestamp_seconds: None,
            dissolve_state: Some(NnsDissolveStateRecord::DissolveDelaySeconds(1_209_600)),
            followees: Vec::new(),
            neuron_fees_e8s: 0,
            visibility: Some(1),
            transfer: None,
            known_neuron_data: Some(NnsKnownNeuronData {
                name: "io".to_string(),
                description: None,
                links: None,
            }),
            spawn_at_timestamp_seconds: None,
            voting_power_refreshed_timestamp_seconds: Some(2),
            deciding_voting_power: Some(100),
            potential_voting_power: Some(100),
            eight_year_gang_bonus_base_e8s: Some(0),
            maturity_disbursements_in_progress: Some(Vec::new()),
        }
    }

    fn sns_neuron_record(
        id: Vec<u8>,
        stake: u64,
        dissolve_state: SnsDissolveStateRecord,
    ) -> SnsNeuronRecord {
        SnsNeuronRecord {
            id: Some(SnsNeuronIdRecord { id }),
            staked_maturity_e8s_equivalent: Some(0),
            cached_neuron_stake_e8s: stake,
            maturity_e8s_equivalent: 0,
            created_timestamp_seconds: 1,
            source_nns_neuron_id: None,
            auto_stake_maturity: Some(false),
            aging_since_timestamp_seconds: 1,
            dissolve_state: Some(dissolve_state),
            voting_power_percentage_multiplier: 100,
            vesting_period_seconds: None,
            disburse_maturity_in_progress: Vec::new(),
            followees: Vec::new(),
            neuron_fees_e8s: 0,
            permissions: vec![SnsNeuronPermissionRecord {
                principal: Some(principal()),
                permission_type: vec![1],
            }],
            topic_followees: None,
        }
    }

    struct SnsProposalFixture {
        id: u64,
        topic: Option<SnsTopic>,
        decided: u64,
        executed: u64,
        failed: u64,
        eligible: bool,
        ballot_id: &'static str,
        vote: i32,
    }

    fn sns_proposal_record(fixture: SnsProposalFixture) -> SnsProposalRecord {
        SnsProposalRecord {
            id: Some(SnsProposalIdRecord { id: fixture.id }),
            payload_text_rendering: Some("payload".to_string()),
            action: 10,
            failure_reason: None,
            ballots: vec![(
                fixture.ballot_id.to_string(),
                SnsBallotRecord { vote: fixture.vote },
            )],
            minimum_yes_proportion_of_total: Some(SnsPercentage {
                basis_points: Some(5_000),
            }),
            reward_event_round: if fixture.eligible && fixture.decided > 0 {
                1
            } else {
                0
            },
            failed_timestamp_seconds: fixture.failed,
            reward_event_end_timestamp_seconds: None,
            proposal_creation_timestamp_seconds: 1,
            initial_voting_period_seconds: 10,
            reject_cost_e8s: 1,
            latest_tally: Some(SnsTally {
                no: 0,
                yes: 1,
                total: 1,
                timestamp_seconds: fixture.decided,
            }),
            wait_for_quiet_deadline_increase_seconds: 0,
            decided_timestamp_seconds: fixture.decided,
            proposal: Some(SnsProposalPayload {
                url: "https://example.com".to_string(),
                title: "proposal".to_string(),
                summary: "summary".to_string(),
            }),
            proposer: Some(SnsNeuronIdRecord { id: vec![9] }),
            wait_for_quiet_state: None,
            minimum_yes_proportion_of_exercised: None,
            is_eligible_for_rewards: fixture.eligible,
            executed_timestamp_seconds: fixture.executed,
            topic: fixture.topic,
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
    fn nns_production_neuron_and_proposal_list_fixtures_decode_and_map() {
        let response = NnsProductionListNeuronsResponse {
            neuron_infos: vec![(
                42,
                NnsNeuronInfoRecord {
                    id: Some(NnsNeuronIdRecord { id: 42 }),
                    dissolve_delay_seconds: 1_209_600,
                    recent_ballots: vec![NnsBallotInfoRecord {
                        vote: 1,
                        proposal_id: Some(NnsProposalIdRecord { id: 7 }),
                    }],
                    neuron_type: None,
                    created_timestamp_seconds: 1,
                    state: 1,
                    stake_e8s: 100,
                    joined_community_fund_timestamp_seconds: None,
                    retrieved_at_timestamp_seconds: 2,
                    visibility: Some(1),
                    known_neuron_data: None,
                    age_seconds: 0,
                    voting_power: 100,
                    voting_power_refreshed_timestamp_seconds: Some(2),
                    deciding_voting_power: Some(100),
                    potential_voting_power: Some(100),
                    eight_year_gang_bonus_base_e8s: Some(0),
                    staked_maturity_e8s_equivalent: Some(0),
                },
            )],
            full_neurons: vec![nns_neuron_record(42)],
            total_pages_available: Some(1),
        };
        let bytes = Encode!(&response).unwrap();
        let decoded = Decode!(&bytes, NnsProductionListNeuronsResponse).unwrap();
        assert_eq!(
            NnsNeuron::try_from(decoded.full_neurons[0].clone())
                .unwrap()
                .id,
            NnsNeuronId(42)
        );

        let proposals = NnsProductionListProposalInfoResponse {
            proposal_info: vec![NnsProposalInfoRecord {
                id: Some(NnsProposalIdRecord { id: 99 }),
                topic: 1,
                status: 2,
                reward_status: 3,
                decided_timestamp_seconds: 10,
                executed_timestamp_seconds: 11,
                failed_timestamp_seconds: 0,
                ballots: vec![(
                    42,
                    NnsBallotRecord {
                        vote: 1,
                        voting_power: 100,
                    },
                )],
            }],
        };
        let bytes = Encode!(&proposals).unwrap();
        assert_eq!(
            Decode!(&bytes, NnsProductionListProposalInfoResponse)
                .unwrap()
                .proposal_info[0]
                .id,
            Some(NnsProposalIdRecord { id: 99 })
        );
    }

    #[test]
    fn nns_production_manage_neuron_commands_and_results_map() {
        let requests = vec![
            nns_split_request(NnsNeuronId(1), 100).unwrap(),
            nns_configure_dissolve_delay_request(NnsNeuronId(1), 60),
            nns_start_dissolving_request(NnsNeuronId(1)),
            nns_stop_dissolving_request(NnsNeuronId(1)),
            nns_merge_request(NnsNeuronId(1), NnsNeuronId(2)),
            nns_merge_maturity_request(NnsNeuronId(1), 100),
            nns_stake_maturity_request(NnsNeuronId(1), Some(100)),
            nns_disburse_maturity_request(NnsNeuronId(1), 100, account()),
            nns_disburse_request(NnsNeuronId(1), account()),
            nns_refresh_voting_power_request(NnsNeuronId(1)),
        ];
        for request in requests {
            let bytes = Encode!(&request).unwrap();
            Decode!(&bytes, NnsProductionManageNeuronRequest).unwrap();
        }

        let split = nns_command_result_from_response(
            NnsNeuronCommand::Split,
            NnsNeuronId(1),
            NnsProductionManageNeuronResponse {
                command: Some(NnsManageNeuronResponseCommandRecord::Split(
                    NnsSpawnResponse {
                        created_neuron_id: Some(NnsNeuronIdRecord { id: 10 }),
                    },
                )),
            },
        )
        .unwrap();
        assert_eq!(split.child_neuron_id, Some(NnsNeuronId(10)));
        assert_eq!(split.amount_e8s, None);
        assert_eq!(split.transfer_block_height, None);

        let merge_maturity = nns_command_result_from_response(
            NnsNeuronCommand::MergeMaturity,
            NnsNeuronId(1),
            NnsProductionManageNeuronResponse {
                command: Some(NnsManageNeuronResponseCommandRecord::MergeMaturity(
                    NnsMergeMaturityResponse {
                        merged_maturity_e8s: 88,
                        new_stake_e8s: 188,
                    },
                )),
            },
        )
        .unwrap();
        assert_eq!(merge_maturity.amount_e8s, Some(88));
        assert_eq!(merge_maturity.transfer_block_height, None);

        let stake_maturity = nns_command_result_from_response(
            NnsNeuronCommand::StakeMaturity,
            NnsNeuronId(1),
            NnsProductionManageNeuronResponse {
                command: Some(NnsManageNeuronResponseCommandRecord::StakeMaturity(
                    NnsStakeMaturityResponse {
                        maturity_e8s: 44,
                        staked_maturity_e8s: 33,
                    },
                )),
            },
        )
        .unwrap();
        assert_eq!(stake_maturity.amount_e8s, Some(33));
        assert_eq!(stake_maturity.transfer_block_height, None);

        let disburse_maturity = nns_command_result_from_response(
            NnsNeuronCommand::DisburseMaturity,
            NnsNeuronId(1),
            NnsProductionManageNeuronResponse {
                command: Some(NnsManageNeuronResponseCommandRecord::DisburseMaturity(
                    NnsProductionDisburseMaturityResponse {
                        amount_disbursed_e8s: Some(77),
                    },
                )),
            },
        )
        .unwrap();
        assert_eq!(disburse_maturity.amount_e8s, Some(77));
        assert_eq!(disburse_maturity.transfer_block_height, None);

        let disburse = nns_command_result_from_response(
            NnsNeuronCommand::Disburse,
            NnsNeuronId(1),
            NnsProductionManageNeuronResponse {
                command: Some(NnsManageNeuronResponseCommandRecord::Disburse(
                    NnsDisburseResponse {
                        transfer_block_height: 55,
                    },
                )),
            },
        )
        .unwrap();
        assert_eq!(disburse.amount_e8s, None);
        assert_eq!(disburse.transfer_block_height, Some(55));

        let err = nns_command_result_from_response(
            NnsNeuronCommand::Disburse,
            NnsNeuronId(1),
            NnsProductionManageNeuronResponse {
                command: Some(NnsManageNeuronResponseCommandRecord::Error(
                    NnsGovernanceErrorRecord {
                        error_type: 3,
                        error_message: "not authorized".to_string(),
                    },
                )),
            },
        )
        .unwrap_err();
        assert_eq!(err, NnsGovernanceError::NotAuthorized);
    }

    #[test]
    fn nns_merge_request_keeps_target_and_source_distinct() {
        let request = nns_merge_request(NnsNeuronId(10), NnsNeuronId(20));
        assert_eq!(
            request.neuron_id_or_subaccount,
            Some(NnsNeuronIdOrSubaccount::NeuronId(NnsNeuronIdRecord {
                id: 10
            }))
        );
        assert_eq!(request.id, Some(NnsNeuronIdRecord { id: 10 }));
        assert_eq!(
            request.command,
            Some(NnsManageNeuronCommandRequest::Merge(NnsMerge {
                source_neuron_id: Some(NnsNeuronIdRecord { id: 20 })
            }))
        );
    }

    #[test]
    fn nns_numeric_overflow_is_explicit() {
        assert_eq!(
            nns_split_request(NnsNeuronId(1), u128::MAX),
            Err(NnsGovernanceError::NumericOverflow {
                field: "split.amount_e8s".to_string()
            })
        );
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
        let record = sns_neuron_record(
            vec![1, 2],
            123,
            SnsDissolveStateRecord::DissolveDelaySeconds(1_209_600),
        );
        let bytes = Encode!(&record).unwrap();
        let decoded = Decode!(&bytes, SnsNeuronRecord).unwrap();
        let neuron = SnsNeuron::try_from(decoded).unwrap();
        assert_eq!(neuron.id, SnsNeuronId(vec![1, 2]));
        assert_eq!(neuron.cached_neuron_stake_e8s, 123);
        assert!(matches!(
            neuron.dissolve_state,
            SnsDissolveState::NotDissolving { .. }
        ));
        assert_eq!(neuron.controller, Some(principal()));
        assert_eq!(
            neuron.permissions,
            vec![SnsNeuronPermission {
                principal: Some(principal()),
                permission_type: vec![1],
            }]
        );
    }

    #[derive(Clone, Debug, PartialEq, Eq, CandidType)]
    struct OfficialSnsNeuronRecordWithoutController {
        id: Option<SnsNeuronIdRecord>,
        staked_maturity_e8s_equivalent: Option<u64>,
        cached_neuron_stake_e8s: u64,
        maturity_e8s_equivalent: u64,
        created_timestamp_seconds: u64,
        source_nns_neuron_id: Option<u64>,
        auto_stake_maturity: Option<bool>,
        aging_since_timestamp_seconds: u64,
        dissolve_state: Option<SnsDissolveStateRecord>,
        voting_power_percentage_multiplier: u64,
        vesting_period_seconds: Option<u64>,
        disburse_maturity_in_progress: Vec<SnsDisburseMaturityInProgress>,
        followees: Vec<(u64, SnsFollowees)>,
        neuron_fees_e8s: u64,
        permissions: Vec<SnsNeuronPermissionRecord>,
        topic_followees: Option<SnsTopicFollowees>,
    }

    #[test]
    fn sns_official_neuron_record_without_controller_decodes_and_derives_owner() {
        let controller = principal();
        let fallback = Principal::from_slice(&[2]);
        let record = OfficialSnsNeuronRecordWithoutController {
            id: Some(SnsNeuronIdRecord { id: vec![9] }),
            staked_maturity_e8s_equivalent: Some(0),
            cached_neuron_stake_e8s: 1_000,
            maturity_e8s_equivalent: 0,
            created_timestamp_seconds: 1,
            source_nns_neuron_id: None,
            auto_stake_maturity: Some(false),
            aging_since_timestamp_seconds: 1,
            dissolve_state: Some(SnsDissolveStateRecord::DissolveDelaySeconds(1_209_600)),
            voting_power_percentage_multiplier: 100,
            vesting_period_seconds: None,
            disburse_maturity_in_progress: Vec::new(),
            followees: Vec::new(),
            neuron_fees_e8s: 0,
            permissions: vec![
                SnsNeuronPermissionRecord {
                    principal: Some(fallback),
                    permission_type: vec![4],
                },
                SnsNeuronPermissionRecord {
                    principal: Some(controller),
                    permission_type: vec![3, 4],
                },
            ],
            topic_followees: None,
        };
        let bytes = Encode!(&record).unwrap();
        let decoded = Decode!(&bytes, SnsNeuronRecord).unwrap();
        assert_eq!(decoded.permissions, record.permissions);

        let neuron = SnsNeuron::try_from(decoded).unwrap();
        assert_eq!(neuron.controller, Some(controller));
        assert_eq!(
            neuron.permissions,
            record
                .permissions
                .into_iter()
                .map(|permission| SnsNeuronPermission {
                    principal: permission.principal,
                    permission_type: permission.permission_type,
                })
                .collect::<Vec<_>>()
        );

        let policy = SnsEligibilityPolicy {
            protocol_neuron_ids: BTreeSet::new(),
            jupiter_governance_neuron_ids: BTreeSet::new(),
            minimum_dissolve_delay_seconds: 1_209_600,
            require_non_dissolving: true,
            current_timestamp_seconds: 10,
        };
        let eligibility = snapshot_sns_eligibility(&[neuron], &policy);
        assert_eq!(eligibility[0].owner, Some(controller));
        assert_eq!(eligibility[0].excluded_reason, None);
    }

    #[test]
    fn sns_production_list_neurons_fixture_maps_page_and_malformed_id() {
        let response = SnsProductionListNeuronsResponse {
            neurons: vec![
                sns_neuron_record(
                    vec![1],
                    100,
                    SnsDissolveStateRecord::DissolveDelaySeconds(100),
                ),
                sns_neuron_record(
                    vec![2],
                    200,
                    SnsDissolveStateRecord::DissolveDelaySeconds(100),
                ),
            ],
        };
        let bytes = Encode!(&response).unwrap();
        let decoded = Decode!(&bytes, SnsProductionListNeuronsResponse).unwrap();
        let page = sns_neuron_page_from_production_response(
            &SnsNeuronPageRequest {
                limit: 2,
                start_page_at: None,
            },
            decoded,
        )
        .unwrap();
        assert_eq!(page.neurons.len(), 2);
        assert_eq!(page.next_page_at, Some(SnsNeuronId(vec![2])));

        let mut malformed = sns_neuron_record(
            vec![3],
            100,
            SnsDissolveStateRecord::DissolveDelaySeconds(100),
        );
        malformed.id = None;
        assert!(matches!(
            SnsNeuron::try_from(malformed),
            Err(SnsGovernanceError::MalformedNeuronId { .. })
        ));
    }

    #[test]
    fn sns_production_list_proposals_fixture_maps_participation_inputs() {
        let response = SnsProductionListProposalsResponse {
            include_ballots_by_caller: Some(false),
            include_topic_filtering: Some(true),
            proposals: vec![
                sns_proposal_record(SnsProposalFixture {
                    id: 10,
                    topic: Some(SnsTopic::Governance),
                    decided: 60,
                    executed: 61,
                    failed: 0,
                    eligible: true,
                    ballot_id: "\u{1}",
                    vote: 1,
                }),
                sns_proposal_record(SnsProposalFixture {
                    id: 9,
                    topic: Some(SnsTopic::ApplicationBusinessLogic),
                    decided: 70,
                    executed: 0,
                    failed: 0,
                    eligible: false,
                    ballot_id: "\u{1}",
                    vote: 3,
                }),
            ],
        };
        let bytes = Encode!(&response).unwrap();
        let decoded = Decode!(&bytes, SnsProductionListProposalsResponse).unwrap();
        let page = sns_proposal_page_from_production_response(
            &SnsProposalPageRequest {
                limit: 2,
                before_proposal: None,
            },
            decoded,
        )
        .unwrap();
        assert_eq!(page.next_before_proposal, Some(SnsProposalId(9)));
        assert_eq!(page.proposals[0].status, SnsProposalStatus::Executed);
        assert_eq!(
            page.proposals[0].reward_status,
            SnsProposalRewardStatus::Settled
        );
        assert_eq!(
            page.proposals[1].reward_status,
            SnsProposalRewardStatus::Ineligible
        );

        let eligibility = SnsNeuronEligibility {
            neuron_id: SnsNeuronId("\u{1}".as_bytes().to_vec()),
            owner: Some(principal()),
            eligible_stake_e8s: 100,
            eligible_since_seconds: 0,
            dissolve_delay_seconds: 1_209_600,
            is_non_dissolving: true,
            excluded_reason: None,
        };
        let summary = summarize_sns_participation(
            &[eligibility],
            &page.proposals,
            &SnsParticipationPolicy {
                count_direct_votes: true,
                count_followed_votes: true,
                excluded_topics: BTreeSet::from([4]),
                epoch_start_seconds: 1,
                epoch_end_seconds: 100,
            },
        );
        assert_eq!(summary[0].eligible_closed_proposals_total, 1);
        assert_eq!(summary[0].voted_proposals, 1);
    }

    #[test]
    fn sns_pagination_rejects_duplicates_and_non_progressing_cursors() {
        let duplicate = SnsProductionListNeuronsResponse {
            neurons: vec![
                sns_neuron_record(
                    vec![1],
                    100,
                    SnsDissolveStateRecord::DissolveDelaySeconds(100),
                ),
                sns_neuron_record(
                    vec![1],
                    100,
                    SnsDissolveStateRecord::DissolveDelaySeconds(100),
                ),
            ],
        };
        assert_eq!(
            sns_neuron_page_from_production_response(
                &SnsNeuronPageRequest {
                    limit: 2,
                    start_page_at: None,
                },
                duplicate,
            ),
            Err(SnsGovernanceError::PaginationDidNotProgress)
        );

        let non_progressing = SnsProductionListProposalsResponse {
            include_ballots_by_caller: None,
            include_topic_filtering: None,
            proposals: vec![sns_proposal_record(SnsProposalFixture {
                id: 10,
                topic: Some(SnsTopic::Governance),
                decided: 60,
                executed: 61,
                failed: 0,
                eligible: true,
                ballot_id: "\u{1}",
                vote: 1,
            })],
        };
        assert_eq!(
            sns_proposal_page_from_production_response(
                &SnsProposalPageRequest {
                    limit: 1,
                    before_proposal: Some(SnsProposalId(10)),
                },
                non_progressing,
            ),
            Err(SnsGovernanceError::PaginationDidNotProgress)
        );
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
