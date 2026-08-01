use candid::{CandidType, Nat, Principal, Reserved};
use ic_cdk::call::Call;
use io_ledger_boundary::{IcrcTransferArg, IcrcTransferResult};
use serde::Deserialize;

use crate::{
    api::ApiError,
    jupiter::{NeuronSnapshot, StreamReceiptPermit},
    maturity::{
        CanonicalDisbursementEvidence, PendingMaturityDisbursement, DISBURSEMENT_DELAY_SECONDS,
    },
    state::{Account, NnsConfig},
    transfer::NnsTransferIntent,
};

#[derive(Clone, Debug)]
pub struct NeuronObservation {
    pub snapshot: NeuronSnapshot,
    pub maturity_e8s: u64,
    pub staked_maturity_e8s: u64,
    pub maturity_disbursements: Vec<MaturityDisbursement>,
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
    maturity_disbursements_in_progress: Option<Vec<MaturityDisbursement>>,
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

#[derive(Clone, Debug, CandidType, Deserialize)]
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
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Empty {}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum Command {
    ClaimOrRefresh(ClaimOrRefresh),
    StakeMaturity(StakeMaturity),
    DisburseMaturity(DisburseMaturity),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ManageNeuron {
    id: Option<NeuronId>,
    neuron_id_or_subaccount: Option<NeuronIdOrSubaccount>,
    command: Option<Command>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum CommandResponse {
    Error(GovernanceError),
    ClaimOrRefresh(Reserved),
    StakeMaturity(StakeMaturityResponse),
    DisburseMaturity(DisburseMaturityResponse),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct StakeMaturity {
    percentage_to_stake: Option<u32>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct StakeMaturityResponse {
    maturity_e8s: u64,
    staked_maturity_e8s: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DisburseMaturity {
    percentage_to_disburse: u32,
    to_account: Option<NnsAccount>,
    to_account_identifier: Option<NnsAccountIdentifier>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct NnsAccountIdentifier {
    hash: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct DisburseMaturityResponse {
    amount_disbursed_e8s: Option<u64>,
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
    let neuron = result.map_err(|error| {
        ApiError::Invalid(format!(
            "protected neuron query rejected ({}): {}",
            error.error_type, error.error_message
        ))
    })?;
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
        maturity_disbursements: neuron
            .maturity_disbursements_in_progress
            .unwrap_or_default(),
    })
}

pub async fn refresh_neuron(config: &NnsConfig, neuron_id: u64) -> Result<(), ApiError> {
    let response: ManageNeuronResponse = Call::bounded_wait(config.nns_governance, "manage_neuron")
        .with_arg(ManageNeuron {
            id: Some(NeuronId { id: neuron_id }),
            neuron_id_or_subaccount: Some(NeuronIdOrSubaccount::NeuronId(NeuronId {
                id: neuron_id,
            })),
            command: Some(Command::ClaimOrRefresh(ClaimOrRefresh {
                by: Some(ClaimBy::NeuronIdOrSubaccount(Empty {})),
            })),
        })
        .await
        .map_err(|error| ApiError::Pending(format!("claim/refresh call ambiguous: {error:?}")))?
        .candid()
        .map_err(|error| ApiError::Invalid(format!("claim/refresh decode failed: {error:?}")))?;
    match response.command {
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
    let response = manage_neuron(
        config,
        neuron_id,
        Command::StakeMaturity(StakeMaturity {
            percentage_to_stake: Some(40),
        }),
    )
    .await?;
    match response.command {
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
    let response = manage_neuron(
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
    match response.command {
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

async fn manage_neuron(
    config: &NnsConfig,
    neuron_id: u64,
    command: Command,
) -> Result<ManageNeuronResponse, ApiError> {
    Call::bounded_wait(config.nns_governance, "manage_neuron")
        .with_arg(ManageNeuron {
            id: Some(NeuronId { id: neuron_id }),
            neuron_id_or_subaccount: Some(NeuronIdOrSubaccount::NeuronId(NeuronId {
                id: neuron_id,
            })),
            command: Some(command),
        })
        .await
        .map_err(|error| ApiError::Pending(format!("NNS governance call ambiguous: {error:?}")))?
        .candid()
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
enum ReceiptKind {
    Jupiter,
    TwoWeekMaturity,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct PrepareLiquidReceiptArgs {
    receipt_sequence: u64,
    receipt_kind: ReceiptKind,
    source_operation_id: Vec<u8>,
    liquid_amount_e8s: u128,
    cohort_generation: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct CompleteLiquidReceiptArgs {
    receipt_sequence: u64,
    block_index: u128,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct StreamStatus {
    next_nns_receipt_sequence: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum StreamError {
    Anonymous,
    Unauthorized,
    Paused,
    Busy,
    WrongNonce { expected: u64 },
    NonceAlreadyUsed,
    Invalid(String),
    Ledger(String),
    Pending(String),
    Stuck(String),
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct JupiterReceiptResult {
    pub request_fingerprint: Vec<u8>,
    pub receipt_block: u128,
    pub backed_io_e8s: u128,
    pub io_transfer_block: u128,
    pub io_fee_e8s: u128,
    pub completed_at_nanos: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub struct TwoWeekReceiptResult {
    pub request_fingerprint: Vec<u8>,
    pub receipt_block: u128,
    pub backed_io_pool_e8s: u128,
    pub distributed_io_e8s: u128,
    pub dust_io_e8s: u128,
    pub completed_at_nanos: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize, PartialEq, Eq)]
pub enum CompletedReceiptResult {
    Jupiter(JupiterReceiptResult),
    TwoWeek(TwoWeekReceiptResult),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum StreamLiquidProgress {
    AwaitingReceipt,
    ReceiptProved,
    Settling,
    Completed(CompletedReceiptResult),
    Stuck(String),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum StreamProgress {
    Redemption(Reserved),
    LiquidReceipt(StreamLiquidProgress),
    Idle,
}

pub async fn prepare_jupiter_receipt(
    config: &NnsConfig,
    deposit_block: u128,
    liquid_e8s: u128,
) -> Result<StreamReceiptPermit, ApiError> {
    let status: StreamStatus = Call::bounded_wait(config.stream_manager, "get_status")
        .with_arg(())
        .await
        .map_err(|error| ApiError::Pending(format!("stream status query failed: {error:?}")))?
        .candid()
        .map_err(|error| ApiError::Invalid(format!("stream status decode failed: {error:?}")))?;
    let result: Result<StreamReceiptPermit, StreamError> =
        Call::bounded_wait(config.stream_manager, "prepare_liquid_receipt")
            .with_arg(PrepareLiquidReceiptArgs {
                receipt_sequence: status.next_nns_receipt_sequence,
                receipt_kind: ReceiptKind::Jupiter,
                source_operation_id: deposit_block.to_be_bytes().to_vec(),
                liquid_amount_e8s: liquid_e8s,
                cohort_generation: None,
            })
            .await
            .map_err(|error| {
                ApiError::Pending(format!("receipt prepare call ambiguous: {error:?}"))
            })?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("receipt permit decode failed: {error:?}"))
            })?;
    result.map_err(|error| ApiError::Invalid(format!("stream rejected receipt: {error:?}")))
}

fn two_week_source_operation_id(pending: &PendingMaturityDisbursement) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(
        candid::encode_one((
            pending.neuron_id,
            pending.initiation_timestamp_seconds,
            pending.scheduled_finalization_timestamp_seconds,
            pending.nominal_disbursed_maturity_e8s,
            &pending.destination,
        ))
        .expect("two-week source operation must encode"),
    )
    .to_vec()
}

fn two_week_request(
    sequence: u64,
    pending: &PendingMaturityDisbursement,
    actual_minted_e8s: u128,
) -> Result<PrepareLiquidReceiptArgs, ApiError> {
    Ok(PrepareLiquidReceiptArgs {
        receipt_sequence: sequence,
        receipt_kind: ReceiptKind::TwoWeekMaturity,
        source_operation_id: two_week_source_operation_id(pending),
        liquid_amount_e8s: actual_minted_e8s,
        cohort_generation: Some(pending.stake_evidence.plan.cohort_generation.ok_or_else(
            || ApiError::Invalid("two-week maturity lacks cohort generation".into()),
        )?),
    })
}

pub async fn prepare_two_week_receipt(
    config: &NnsConfig,
    pending: &PendingMaturityDisbursement,
    actual_minted_e8s: u128,
) -> Result<StreamReceiptPermit, ApiError> {
    let status: StreamStatus = Call::bounded_wait(config.stream_manager, "get_status")
        .with_arg(())
        .await
        .map_err(|error| ApiError::Pending(format!("stream status query failed: {error:?}")))?
        .candid()
        .map_err(|error| ApiError::Invalid(format!("stream status decode failed: {error:?}")))?;
    let request = two_week_request(status.next_nns_receipt_sequence, pending, actual_minted_e8s)?;
    let result: Result<StreamReceiptPermit, StreamError> =
        Call::bounded_wait(config.stream_manager, "prepare_liquid_receipt")
            .with_arg(request)
            .await
            .map_err(|error| {
                ApiError::Pending(format!(
                    "two-week receipt prepare call ambiguous: {error:?}"
                ))
            })?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("two-week receipt permit decode failed: {error:?}"))
            })?;
    result
        .map_err(|error| ApiError::Invalid(format!("stream rejected two-week receipt: {error:?}")))
}

pub fn two_week_receipt_fingerprint(
    sequence: u64,
    pending: &PendingMaturityDisbursement,
    actual_minted_e8s: u128,
) -> Result<Vec<u8>, ApiError> {
    use sha2::{Digest, Sha256};
    let request = two_week_request(sequence, pending, actual_minted_e8s)?;
    let mut hasher = Sha256::new();
    hasher.update(b"io-liquid-receipt-request-v1\0");
    hasher.update(candid::encode_one(request).expect("two-week receipt request must encode"));
    Ok(hasher.finalize().to_vec())
}

pub fn jupiter_receipt_fingerprint(
    sequence: u64,
    deposit_block: u128,
    liquid_e8s: u128,
) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let request = PrepareLiquidReceiptArgs {
        receipt_sequence: sequence,
        receipt_kind: ReceiptKind::Jupiter,
        source_operation_id: deposit_block.to_be_bytes().to_vec(),
        liquid_amount_e8s: liquid_e8s,
        cohort_generation: None,
    };
    let mut hasher = Sha256::new();
    hasher.update(b"io-liquid-receipt-request-v1\0");
    hasher.update(candid::encode_one(request).expect("Jupiter receipt request must encode"));
    hasher.finalize().to_vec()
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
    let result: Result<StreamLiquidProgress, StreamError> =
        Call::bounded_wait(config.stream_manager, "complete_liquid_receipt")
            .with_arg(CompleteLiquidReceiptArgs {
                receipt_sequence: permit.sequence,
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

pub async fn complete_two_week_receipt(
    config: &NnsConfig,
    permit: &StreamReceiptPermit,
    block_index: u128,
) -> Result<StreamLiquidProgress, ApiError> {
    complete_jupiter_receipt(config, permit, block_index).await
}

pub async fn resume_stream(config: &NnsConfig) -> Result<StreamLiquidProgress, ApiError> {
    let result: Result<StreamProgress, StreamError> =
        Call::bounded_wait(config.stream_manager, "resume")
            .with_arg(())
            .await
            .map_err(|error| ApiError::Pending(format!("stream resume call ambiguous: {error:?}")))?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("stream resume decode failed: {error:?}"))
            })?;
    match result.map_err(|error| ApiError::Invalid(format!("stream resume failed: {error:?}")))? {
        StreamProgress::LiquidReceipt(progress) => Ok(progress),
        StreamProgress::Idle => Err(ApiError::Invalid("stream lost the Jupiter receipt".into())),
        StreamProgress::Redemption(_) => Err(ApiError::Busy),
    }
}

pub fn staking_account(config: &NnsConfig, neuron: &NeuronSnapshot) -> Account {
    Account {
        owner: config.nns_governance,
        subaccount: Some(neuron.staking_subaccount.to_vec()),
    }
}
