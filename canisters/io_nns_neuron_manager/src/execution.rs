use crate::{
    api::ApiError,
    jupiter::{NeuronSnapshot, StreamReceiptPermit},
    maturity::{CanonicalDisbursementEvidence, DISBURSEMENT_DELAY_SECONDS},
    state::{Account, NnsConfig},
    transfer::{NnsTransferIntent, TransferOutcomeClassification},
};
use candid::{CandidType, Nat, Principal, Reserved};
use ic_cdk::call::Call;
use io_ledger_boundary::{IcrcTransferArg, IcrcTransferError, IcrcTransferResult};
use io_nns_types::backing::{FollowPolicy, POOLED_PARENT_DELAY_SECONDS};
pub use io_receipt_types::ClaimBackingReceiptProgress as StreamLiquidProgress;
use io_receipt_types::{
    ClaimBackingReceiptKind, ClaimBackingReceiptPermit, PrepareClaimBackingReceiptArgs,
    ProveClaimBackingReceiptArgs,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub enum ExactTransferOutcome {
    Succeeded(u128),
    Paused(TransferOutcomeClassification, String),
}

pub fn parent_staking_account(config: &NnsConfig, memo: u64) -> Account {
    let controller = ic_cdk::api::canister_self();
    let mut hasher = Sha256::new();
    hasher.update([0x0c]);
    hasher.update(b"neuron-stake");
    hasher.update(controller.as_slice());
    hasher.update(memo.to_be_bytes());
    Account {
        owner: config.nns_governance,
        subaccount: Some(hasher.finalize().to_vec()),
    }
}

pub fn classify_transfer(
    result: Result<IcrcTransferResult, String>,
) -> Result<ExactTransferOutcome, ApiError> {
    let paused = |class, reason| Ok(ExactTransferOutcome::Paused(class, reason));
    match result {
        Ok(Ok(block))
        | Ok(Err(IcrcTransferError::Duplicate {
            duplicate_of: block,
        })) => block
            .0
            .try_into()
            .map(ExactTransferOutcome::Succeeded)
            .map_err(|_| ApiError::Invalid("ICP block does not fit u128".into())),
        Err(error) => paused(
            TransferOutcomeClassification::AmbiguousPossibleEffect,
            format!("ICP transfer callback is ambiguous: {error}"),
        ),
        Ok(Err(IcrcTransferError::BadFee { expected_fee })) => paused(
            TransferOutcomeClassification::BadFee,
            format!("ICP transfer BadFee; approved fee update required ({expected_fee})"),
        ),
        Ok(Err(IcrcTransferError::InsufficientFunds { balance })) => paused(
            TransferOutcomeClassification::InsufficientFunds,
            format!("ICP transfer requires staging replenishment ({balance})"),
        ),
        Ok(Err(error)) => paused(
            TransferOutcomeClassification::RejectedNoEffect,
            format!("ICP transfer rejected without effect: {error:?}"),
        ),
    }
}

#[derive(Clone, Debug)]
pub struct NeuronObservation {
    pub snapshot: NeuronSnapshot,
    pub maturity_e8s: u64,
    pub staked_maturity_e8s: u64,
    pub auto_stake_maturity: bool,
    pub maturity_disbursements: Vec<MaturityDisbursement>,
    pub dissolve_state: Option<DissolveState>,
    pub followees: Vec<(i32, Vec<u64>)>,
    pub voting_power_refreshed_timestamp_seconds: Option<u64>,
}

pub const APPROVED_PERMANENT_DISSOLVE_DELAY_SECONDS: u64 = 63_115_200;

pub fn validate_permanent_configuration(observation: &NeuronObservation) -> Result<(), String> {
    if !observation.auto_stake_maturity
        && observation.dissolve_state
            == Some(DissolveState::DissolveDelaySeconds(
                APPROVED_PERMANENT_DISSOLVE_DELAY_SECONDS,
            ))
    {
        Ok(())
    } else {
        Err("configured NNS neuron auto-stake or approved dissolve configuration drifted".into())
    }
}

pub fn validate_parent_configuration(
    observation: &NeuronObservation,
    policy: FollowPolicy,
) -> Result<(), String> {
    let expected = [0, 4, 14];
    let exact_following = observation.followees.len() == expected.len()
        && expected.iter().all(|topic| {
            observation
                .followees
                .iter()
                .any(|(actual, ids)| actual == topic && ids == &[policy.followee_neuron_id])
        });
    if !observation.auto_stake_maturity
        && observation.dissolve_state
            == Some(DissolveState::DissolveDelaySeconds(
                POOLED_PARENT_DELAY_SECONDS,
            ))
        && exact_following
        && observation
            .voting_power_refreshed_timestamp_seconds
            .is_some_and(|timestamp| timestamp > 0)
    {
        Ok(())
    } else {
        Err(format!(
            "pooled parent configuration drifted: dissolve_state={:?}, auto_stake={}, followees={:?}, voting_power_refreshed_at={:?}",
            observation.dissolve_state,
            observation.auto_stake_maturity,
            observation.followees,
            observation.voting_power_refreshed_timestamp_seconds
        ))
    }
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct GovernanceError {
    error_type: i32,
    error_message: String,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Neuron {
    id: Option<NeuronId>,
    account: Vec<u8>,
    cached_neuron_stake_e8s: u64,
    maturity_e8s_equivalent: u64,
    staked_maturity_e8s_equivalent: Option<u64>,
    auto_stake_maturity: Option<bool>,
    maturity_disbursements_in_progress: Option<Vec<MaturityDisbursement>>,
    dissolve_state: Option<DissolveState>,
    followees: Vec<(i32, Followees)>,
    voting_power_refreshed_timestamp_seconds: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Followees {
    followees: Vec<NeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct MaturityDisbursement {
    amount_e8s: Option<u64>,
    timestamp_of_disbursement_seconds: Option<u64>,
    finalize_disbursement_timestamp_seconds: Option<u64>,
    account_to_disburse_to: Option<NnsAccount>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct NnsAccount {
    owner: Option<Principal>,
    subaccount: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum DissolveState {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize)]
struct NeuronId {
    id: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum NeuronIdOrSubaccount {
    NeuronId(NeuronId),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ClaimOrRefresh {
    by: Option<ClaimBy>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum ClaimBy {
    NeuronIdOrSubaccount(Empty),
    MemoAndController(ClaimFromAccount),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ClaimFromAccount {
    controller: Option<Principal>,
    memo: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Empty {}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ClaimOrRefreshResponse {
    refreshed_neuron_id: Option<NeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum Command {
    ClaimOrRefresh(ClaimOrRefresh),
    Configure(Configure),
    Split(Split),
    Merge(Merge),
    Disburse(Disburse),
    StakeMaturity(StakeMaturity),
    DisburseMaturity(DisburseMaturity),
    SetFollowing(SetFollowing),
    RefreshVotingPower(Empty),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Configure {
    operation: Option<ConfigureOperation>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum ConfigureOperation {
    StopDissolving(Empty),
    StartDissolving(Empty),
    IncreaseDissolveDelay(IncreaseDissolveDelay),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct IncreaseDissolveDelay {
    additional_dissolve_delay_seconds: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct SetFollowing {
    topic_following: Option<Vec<FolloweesForTopic>>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct FolloweesForTopic {
    followees: Option<Vec<NeuronId>>,
    topic: Option<i32>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Split {
    amount_e8s: u64,
    memo: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Merge {
    source_neuron_id: Option<NeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Disburse {
    to_account: Option<AccountIdentifier>,
    amount: Option<Amount>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Amount {
    e8s: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct AccountIdentifier {
    hash: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct StakeMaturity {
    percentage_to_stake: Option<u32>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DisburseMaturity {
    percentage_to_disburse: u32,
    to_account: Option<NnsAccount>,
    to_account_identifier: Option<AccountIdentifier>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum CommandResponse {
    Error(GovernanceError),
    ClaimOrRefresh(ClaimOrRefreshResponse),
    Configure(Reserved),
    Split(SpawnResponse),
    Merge(Reserved),
    Disburse(DisburseResponse),
    StakeMaturity(StakeMaturityResponse),
    DisburseMaturity(DisburseMaturityResponse),
    SetFollowing(Reserved),
    RefreshVotingPower(Reserved),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct SpawnResponse {
    created_neuron_id: Option<NeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DisburseResponse {
    transfer_block_height: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct StakeMaturityResponse {
    maturity_e8s: u64,
    staked_maturity_e8s: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DisburseMaturityResponse {
    amount_disbursed_e8s: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ManageNeuron {
    id: Option<NeuronId>,
    neuron_id_or_subaccount: Option<NeuronIdOrSubaccount>,
    command: Option<Command>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ManageNeuronResponse {
    command: Option<CommandResponse>,
}

pub async fn query_neuron(config: &NnsConfig, neuron_id: u64) -> Result<NeuronSnapshot, ApiError> {
    Ok(query_neuron_observation(config, neuron_id).await?.snapshot)
}

pub async fn query_neuron_observation(
    config: &NnsConfig,
    neuron_id: u64,
) -> Result<NeuronObservation, ApiError> {
    let result: Result<Neuron, GovernanceError> =
        Call::bounded_wait(config.nns_governance, "get_full_neuron")
            .with_arg(neuron_id)
            .await
            .map_err(|error| {
                ApiError::Pending(format!("protected neuron query failed: {error:?}"))
            })?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("protected neuron decode failed: {error:?}"))
            })?;
    let neuron = result.map_err(|error| governance_error("protected neuron query", error))?;
    if neuron.id.as_ref().map(|id| id.id) != Some(neuron_id) {
        return Err(ApiError::Invalid(
            "NNS returned the wrong protected neuron".into(),
        ));
    }
    let staking_subaccount: [u8; 32] = neuron
        .account
        .try_into()
        .map_err(|_| ApiError::Invalid("protected neuron account is not 32 bytes".into()))?;
    Ok(NeuronObservation {
        snapshot: NeuronSnapshot {
            neuron_id,
            staking_subaccount,
            cached_stake_e8s: neuron.cached_neuron_stake_e8s.into(),
        },
        maturity_e8s: neuron.maturity_e8s_equivalent,
        staked_maturity_e8s: neuron.staked_maturity_e8s_equivalent.unwrap_or(0),
        auto_stake_maturity: neuron.auto_stake_maturity.unwrap_or(false),
        maturity_disbursements: neuron
            .maturity_disbursements_in_progress
            .unwrap_or_default(),
        dissolve_state: neuron.dissolve_state,
        followees: neuron
            .followees
            .into_iter()
            .map(|(topic, followees)| {
                (
                    topic,
                    followees.followees.into_iter().map(|id| id.id).collect(),
                )
            })
            .collect(),
        voting_power_refreshed_timestamp_seconds: neuron.voting_power_refreshed_timestamp_seconds,
    })
}

pub async fn claim_parent(config: &NnsConfig, memo: u64) -> Result<u64, ApiError> {
    let response = manage(
        config,
        0,
        Command::ClaimOrRefresh(ClaimOrRefresh {
            by: Some(ClaimBy::MemoAndController(ClaimFromAccount {
                controller: Some(ic_cdk::api::canister_self()),
                memo,
            })),
        }),
    )
    .await?;
    match response {
        Some(CommandResponse::ClaimOrRefresh(value)) => value
            .refreshed_neuron_id
            .map(|id| id.id)
            .ok_or_else(|| ApiError::Invalid("ClaimOrRefresh returned no parent ID".into())),
        Some(CommandResponse::Error(error)) => Err(governance_error("ClaimOrRefresh", error)),
        _ => Err(ApiError::Invalid(
            "parent claim returned the wrong response".into(),
        )),
    }
}

pub async fn increase_delay(
    config: &NnsConfig,
    neuron_id: u64,
    additional_seconds: u32,
) -> Result<(), ApiError> {
    configure(
        config,
        neuron_id,
        ConfigureOperation::IncreaseDissolveDelay(IncreaseDissolveDelay {
            additional_dissolve_delay_seconds: additional_seconds,
        }),
    )
    .await
}

pub async fn set_following(
    config: &NnsConfig,
    neuron_id: u64,
    policy: FollowPolicy,
) -> Result<(), ApiError> {
    let topic_following = [0, 4, 14]
        .into_iter()
        .map(|topic| FolloweesForTopic {
            followees: Some(vec![NeuronId {
                id: policy.followee_neuron_id,
            }]),
            topic: Some(topic),
        })
        .collect();
    match manage(
        config,
        neuron_id,
        Command::SetFollowing(SetFollowing {
            topic_following: Some(topic_following),
        }),
    )
    .await?
    {
        Some(CommandResponse::SetFollowing(_)) => Ok(()),
        Some(CommandResponse::Error(error)) => Err(governance_error("SetFollowing", error)),
        _ => Err(ApiError::Invalid(
            "SetFollowing returned the wrong response".into(),
        )),
    }
}

pub async fn refresh_voting_power(config: &NnsConfig, neuron_id: u64) -> Result<(), ApiError> {
    match manage(config, neuron_id, Command::RefreshVotingPower(Empty {})).await? {
        Some(CommandResponse::RefreshVotingPower(_)) => Ok(()),
        Some(CommandResponse::Error(error)) => Err(governance_error("RefreshVotingPower", error)),
        _ => Err(ApiError::Invalid(
            "RefreshVotingPower returned the wrong response".into(),
        )),
    }
}

pub async fn refresh_neuron(config: &NnsConfig, neuron_id: u64) -> Result<(), ApiError> {
    let response = manage(
        config,
        neuron_id,
        Command::ClaimOrRefresh(ClaimOrRefresh {
            by: Some(ClaimBy::NeuronIdOrSubaccount(Empty {})),
        }),
    )
    .await?;
    match response {
        Some(CommandResponse::ClaimOrRefresh(_)) => Ok(()),
        Some(CommandResponse::Error(error)) => Err(ApiError::Invalid(format!(
            "claim/refresh rejected ({}): {}",
            error.error_type, error.error_message
        ))),
        Some(_) => Err(ApiError::Invalid(
            "claim/refresh returned the wrong command result".into(),
        )),
        None => Err(ApiError::Invalid(
            "claim/refresh returned no command result".into(),
        )),
    }
}

pub async fn stake_maturity(config: &NnsConfig, neuron_id: u64) -> Result<(u64, u64), ApiError> {
    let response = manage(
        config,
        neuron_id,
        Command::StakeMaturity(StakeMaturity {
            percentage_to_stake: Some(40),
        }),
    )
    .await?;
    match response {
        Some(CommandResponse::StakeMaturity(value)) => {
            Ok((value.maturity_e8s, value.staked_maturity_e8s))
        }
        Some(CommandResponse::Error(error)) => Err(governance_error("StakeMaturity", error)),
        _ => Err(ApiError::Invalid(
            "StakeMaturity returned the wrong command response".into(),
        )),
    }
}

pub async fn disburse_maturity(
    config: &NnsConfig,
    neuron_id: u64,
    destination: &Account,
) -> Result<u64, ApiError> {
    let response = manage(
        config,
        neuron_id,
        Command::DisburseMaturity(DisburseMaturity {
            percentage_to_disburse: 100,
            to_account: Some(NnsAccount {
                owner: Some(destination.owner),
                subaccount: destination.subaccount.clone(),
            }),
            to_account_identifier: None,
        }),
    )
    .await?;
    match response {
        Some(CommandResponse::DisburseMaturity(value)) => value
            .amount_disbursed_e8s
            .filter(|amount| *amount > 0)
            .ok_or_else(|| ApiError::Invalid("DisburseMaturity returned no amount".into())),
        Some(CommandResponse::Error(error)) => Err(governance_error("DisburseMaturity", error)),
        _ => Err(ApiError::Invalid(
            "DisburseMaturity returned the wrong command response".into(),
        )),
    }
}

pub async fn split_neuron(
    config: &NnsConfig,
    parent_neuron_id: u64,
    amount_e8s: u128,
    memo: u64,
) -> Result<u64, ApiError> {
    let amount_e8s = u64::try_from(amount_e8s)
        .map_err(|_| ApiError::Invalid("unwind excess does not fit NNS nat64".into()))?;
    match manage(
        config,
        parent_neuron_id,
        Command::Split(Split {
            amount_e8s,
            memo: Some(memo),
        }),
    )
    .await?
    {
        Some(CommandResponse::Split(value)) => value
            .created_neuron_id
            .map(|id| id.id)
            .ok_or_else(|| ApiError::Invalid("NNS Split returned no child neuron".into())),
        Some(CommandResponse::Error(error)) => Err(governance_error("Split", error)),
        _ => Err(ApiError::Invalid(
            "Split returned the wrong command response".into(),
        )),
    }
}

pub async fn set_dissolving(
    config: &NnsConfig,
    neuron_id: u64,
    start: bool,
) -> Result<(), ApiError> {
    let operation = if start {
        ConfigureOperation::StartDissolving(Empty {})
    } else {
        ConfigureOperation::StopDissolving(Empty {})
    };
    configure(config, neuron_id, operation).await
}

async fn configure(
    config: &NnsConfig,
    neuron_id: u64,
    operation: ConfigureOperation,
) -> Result<(), ApiError> {
    match manage(
        config,
        neuron_id,
        Command::Configure(Configure {
            operation: Some(operation),
        }),
    )
    .await?
    {
        Some(CommandResponse::Configure(_)) => Ok(()),
        Some(CommandResponse::Error(error)) => Err(governance_error("Configure", error)),
        _ => Err(ApiError::Invalid(
            "Configure returned the wrong command response".into(),
        )),
    }
}

pub async fn merge_neuron(
    config: &NnsConfig,
    parent_neuron_id: u64,
    child_neuron_id: u64,
) -> Result<(), ApiError> {
    match manage(
        config,
        parent_neuron_id,
        Command::Merge(Merge {
            source_neuron_id: Some(NeuronId {
                id: child_neuron_id,
            }),
        }),
    )
    .await?
    {
        Some(CommandResponse::Merge(_)) => Ok(()),
        Some(CommandResponse::Error(error)) => Err(governance_error("Merge", error)),
        _ => Err(ApiError::Invalid(
            "Merge returned the wrong command response".into(),
        )),
    }
}

pub async fn disburse_neuron(
    config: &NnsConfig,
    child_neuron_id: u64,
    destination: &Account,
) -> Result<u128, ApiError> {
    let hash =
        io_ledger_boundary::icp_account_identifier(destination).map_err(ApiError::Invalid)?;
    match manage(
        config,
        child_neuron_id,
        Command::Disburse(Disburse {
            to_account: Some(AccountIdentifier { hash }),
            amount: None,
        }),
    )
    .await?
    {
        Some(CommandResponse::Disburse(value)) => Ok(value.transfer_block_height.into()),
        Some(CommandResponse::Error(error)) => Err(governance_error("Disburse", error)),
        _ => Err(ApiError::Invalid(
            "Disburse returned the wrong command response".into(),
        )),
    }
}

async fn manage(
    config: &NnsConfig,
    neuron_id: u64,
    command: Command,
) -> Result<Option<CommandResponse>, ApiError> {
    Call::bounded_wait(config.nns_governance, "manage_neuron")
        .with_arg(ManageNeuron {
            id: None,
            neuron_id_or_subaccount: Some(NeuronIdOrSubaccount::NeuronId(NeuronId {
                id: neuron_id,
            })),
            command: Some(command),
        })
        .await
        .map_err(|error| ApiError::Pending(format!("NNS governance command ambiguous: {error:?}")))?
        .candid::<ManageNeuronResponse>()
        .map(|response| response.command)
        .map_err(|error| ApiError::Invalid(format!("NNS governance decode failed: {error:?}")))
}

fn governance_error(method: &str, error: GovernanceError) -> ApiError {
    ApiError::Invalid(format!(
        "{method} rejected ({}): {}",
        error.error_type, error.error_message
    ))
}

pub fn exact_maturity_disbursement(
    observation: &NeuronObservation,
    amount_e8s: u64,
    destination: &Account,
    submitted_at_seconds: u64,
) -> Result<CanonicalDisbursementEvidence, ApiError> {
    let matching = observation
        .maturity_disbursements
        .iter()
        .filter(|entry| {
            entry.amount_e8s == Some(amount_e8s)
                && entry
                    .timestamp_of_disbursement_seconds
                    .is_some_and(|timestamp| timestamp >= submitted_at_seconds)
                && entry
                    .account_to_disburse_to
                    .as_ref()
                    .is_some_and(|account| {
                        account.owner == Some(destination.owner)
                            && Account {
                                owner: destination.owner,
                                subaccount: account.subaccount.clone(),
                            }
                            .effective_eq(destination)
                            .unwrap_or(false)
                    })
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(ApiError::Invalid(
            "NNS neuron lacks one exact pending maturity disbursement".into(),
        ));
    }
    let entry = matching[0];
    let initiated_at_seconds = entry
        .timestamp_of_disbursement_seconds
        .ok_or_else(|| ApiError::Invalid("maturity initiation timestamp is absent".into()))?;
    let scheduled_finalization_timestamp_seconds = entry
        .finalize_disbursement_timestamp_seconds
        .filter(|timestamp| *timestamp > 0)
        .ok_or_else(|| ApiError::Invalid("maturity finalization timestamp is absent".into()))?;
    if scheduled_finalization_timestamp_seconds
        != initiated_at_seconds
            .checked_add(DISBURSEMENT_DELAY_SECONDS)
            .ok_or_else(|| ApiError::Invalid("maturity finalization overflow".into()))?
    {
        return Err(ApiError::Invalid(
            "maturity finalization is not the pinned seven-day delay".into(),
        ));
    }
    Ok(CanonicalDisbursementEvidence {
        initiated_at_seconds,
        scheduled_finalization_timestamp_seconds,
    })
}

pub fn has_exact_maturity_disbursement(
    observation: &NeuronObservation,
    amount_e8s: u64,
    destination: &Account,
    initiated_at_seconds: u64,
    finalization_timestamp_seconds: u64,
) -> bool {
    observation.maturity_disbursements.iter().any(|entry| {
        entry.amount_e8s == Some(amount_e8s)
            && entry.timestamp_of_disbursement_seconds == Some(initiated_at_seconds)
            && entry.finalize_disbursement_timestamp_seconds == Some(finalization_timestamp_seconds)
            && entry
                .account_to_disburse_to
                .as_ref()
                .is_some_and(|account| {
                    account.owner == Some(destination.owner)
                        && Account {
                            owner: destination.owner,
                            subaccount: account.subaccount.clone(),
                        }
                        .effective_eq(destination)
                        .unwrap_or(false)
                })
    })
}

pub async fn submit_transfer(intent: &NnsTransferIntent) -> Result<IcrcTransferResult, String> {
    Call::bounded_wait(intent.ledger, "icrc1_transfer")
        .with_arg(IcrcTransferArg {
            from_subaccount: Some(intent.source_subaccount.to_vec()),
            to: intent.destination.clone(),
            amount: Nat::from(intent.amount_e8s),
            fee: Some(Nat::from(intent.fee_e8s)),
            memo: Some(intent.memo.clone()),
            created_at_time: Some(intent.created_at_time_nanos),
        })
        .await
        .map_err(|error| format!("ICP ICRC-1 transfer call ambiguous: {error:?}"))?
        .candid()
        .map_err(|error| format!("ICP ICRC-1 transfer decode failed: {error:?}"))
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum StreamProgress {
    Redemption(Reserved),
    ClaimReceipt(StreamLiquidProgress),
    BackingReconciliation,
    Idle,
}

pub async fn prepare_claim_receipt(
    config: &NnsConfig,
    args: PrepareClaimBackingReceiptArgs,
) -> Result<ClaimBackingReceiptPermit, ApiError> {
    let result: Result<ClaimBackingReceiptPermit, Reserved> =
        Call::bounded_wait(config.stream_manager, "prepare_claim_backing_receipt")
            .with_arg(args)
            .await
            .map_err(|error| {
                ApiError::Pending(format!("claim-receipt prepare ambiguous: {error:?}"))
            })?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("claim-receipt permit decode failed: {error:?}"))
            })?;
    result.map_err(|_| ApiError::Invalid("Stream rejected claim receipt".into()))
}

pub async fn prove_claim_receipt(
    config: &NnsConfig,
    args: ProveClaimBackingReceiptArgs,
) -> Result<StreamLiquidProgress, ApiError> {
    let result: Result<StreamLiquidProgress, Reserved> =
        Call::bounded_wait(config.stream_manager, "prove_claim_backing_receipt")
            .with_arg(args)
            .await
            .map_err(|error| {
                ApiError::Pending(format!("claim-receipt proof ambiguous: {error:?}"))
            })?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("claim-receipt proof decode failed: {error:?}"))
            })?;
    result.map_err(|_| ApiError::Invalid("Stream rejected claim-receipt proof".into()))
}

pub async fn resume_claim_receipt(config: &NnsConfig) -> Result<StreamLiquidProgress, ApiError> {
    let result: Result<StreamProgress, Reserved> =
        Call::bounded_wait(config.stream_manager, "resume")
            .with_arg(())
            .await
            .map_err(|error| ApiError::Pending(format!("Stream resume ambiguous: {error:?}")))?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("Stream resume decode failed: {error:?}"))
            })?;
    match result.map_err(|_| ApiError::Invalid("Stream resume rejected".into()))? {
        StreamProgress::ClaimReceipt(progress) => Ok(progress),
        StreamProgress::Redemption(_) | StreamProgress::BackingReconciliation => {
            Err(ApiError::Busy)
        }
        StreamProgress::Idle => Err(ApiError::Invalid("Stream lost the claim receipt".into())),
    }
}

pub async fn prepare_jupiter_claim_receipt(
    config: &NnsConfig,
    deposit_block: u128,
    liquid_e8s: u128,
) -> Result<StreamReceiptPermit, ApiError> {
    let observation = crate::api::observe_claim_assets().await?;
    prepare_claim_receipt(
        config,
        PrepareClaimBackingReceiptArgs {
            source_operation_id: deposit_block.to_be_bytes().to_vec(),
            kind: ClaimBackingReceiptKind::Jupiter,
            source_account: config.jupiter_staging.clone(),
            source_block: deposit_block,
            net_liquid_credit_e8s: liquid_e8s,
            nns_fingerprint: observation.fingerprint,
        },
    )
    .await
}

pub async fn icp_balance(config: &NnsConfig, account: &Account) -> Result<u128, ApiError> {
    let balance: Nat = Call::bounded_wait(config.icp_ledger, "icrc1_balance_of")
        .with_arg(account.clone())
        .await
        .map_err(|error| ApiError::Pending(format!("ICP balance query failed: {error:?}")))?
        .candid()
        .map_err(|error| ApiError::Invalid(format!("ICP balance decode failed: {error:?}")))?;
    balance
        .0
        .try_into()
        .map_err(|_| ApiError::Invalid("ICP balance does not fit u128".into()))
}

pub async fn complete_jupiter_receipt(
    config: &NnsConfig,
    permit: &StreamReceiptPermit,
    block_index: u128,
) -> Result<StreamLiquidProgress, ApiError> {
    let result: Result<StreamLiquidProgress, Reserved> =
        Call::bounded_wait(config.stream_manager, "prove_claim_backing_receipt")
            .with_arg(ProveClaimBackingReceiptArgs {
                stream_operation_sequence: permit.stream_operation_sequence,
                block_index,
            })
            .await
            .map_err(|error| {
                ApiError::Pending(format!("receipt completion call ambiguous: {error:?}"))
            })?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("receipt completion decode failed: {error:?}"))
            })?;
    result.map_err(|error| ApiError::Invalid(format!("stream rejected receipt proof: {error:?}")))
}

pub async fn resume_stream(config: &NnsConfig) -> Result<StreamLiquidProgress, ApiError> {
    let result: Result<StreamProgress, Reserved> =
        Call::bounded_wait(config.stream_manager, "resume")
            .with_arg(())
            .await
            .map_err(|error| ApiError::Pending(format!("stream resume call ambiguous: {error:?}")))?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("stream resume decode failed: {error:?}"))
            })?;
    match result.map_err(|error| ApiError::Invalid(format!("stream resume failed: {error:?}")))? {
        StreamProgress::ClaimReceipt(progress) => Ok(progress),
        StreamProgress::Idle => Err(ApiError::Invalid("stream lost the Jupiter receipt".into())),
        StreamProgress::Redemption(_) => Err(ApiError::Busy),
        StreamProgress::BackingReconciliation => Err(ApiError::Busy),
    }
}

pub fn staking_account(config: &NnsConfig, neuron: &NeuronSnapshot) -> Account {
    Account {
        owner: config.nns_governance,
        subaccount: Some(neuron.staking_subaccount.to_vec()),
    }
}

#[cfg(test)]
pub(crate) fn placeholder_maturity_disbursement() -> MaturityDisbursement {
    MaturityDisbursement {
        amount_e8s: None,
        timestamp_of_disbursement_seconds: None,
        finalize_disbursement_timestamp_seconds: None,
        account_to_disburse_to: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> NeuronObservation {
        NeuronObservation {
            snapshot: NeuronSnapshot {
                neuron_id: 1,
                staking_subaccount: [1; 32],
                cached_stake_e8s: 1,
            },
            maturity_e8s: 1,
            staked_maturity_e8s: u64::MAX,
            auto_stake_maturity: false,
            maturity_disbursements: vec![],
            dissolve_state: Some(DissolveState::DissolveDelaySeconds(
                APPROVED_PERMANENT_DISSOLVE_DELAY_SECONDS,
            )),
            followees: vec![],
            voting_power_refreshed_timestamp_seconds: None,
        }
    }

    #[test]
    fn transfer_outcomes_distinguish_ambiguity_from_no_effect() {
        assert!(matches!(
            classify_transfer(Err("transport".into())).unwrap(),
            ExactTransferOutcome::Paused(TransferOutcomeClassification::AmbiguousPossibleEffect, _)
        ));
        assert!(matches!(
            classify_transfer(Ok(Err(IcrcTransferError::BadFee {
                expected_fee: Nat::from(10u8),
            })))
            .unwrap(),
            ExactTransferOutcome::Paused(TransferOutcomeClassification::BadFee, _)
        ));
        assert!(matches!(
            classify_transfer(Ok(Err(IcrcTransferError::InsufficientFunds {
                balance: Nat::from(0u8),
            })))
            .unwrap(),
            ExactTransferOutcome::Paused(TransferOutcomeClassification::InsufficientFunds, _)
        ));
    }

    #[test]
    fn later_retained_staked_maturity_is_valid_but_configuration_drift_is_not() {
        let valid = observation();
        assert_eq!(validate_permanent_configuration(&valid), Ok(()));
        let mut auto = valid.clone();
        auto.auto_stake_maturity = true;
        assert!(validate_permanent_configuration(&auto).is_err());
        let mut dissolving = valid;
        dissolving.dissolve_state = Some(DissolveState::WhenDissolvedTimestampSeconds(u64::MAX));
        assert!(validate_permanent_configuration(&dissolving).is_err());
    }
}
