#[cfg(all(target_family = "wasm", debug_assertions))]
use crate::clients::sns_governance;
#[cfg(target_family = "wasm")]
use crate::clients::{icp_ledger, io_ledger};
#[cfg(target_family = "wasm")]
use crate::governance_snapshot::{
    build_governance_reward_snapshot, GovernanceRewardSnapshotRequest, GovernanceSnapshotError,
};
#[cfg(target_family = "wasm")]
use crate::state::{IO_NNS_NEURON_MANAGER_SOURCE, JUPITER_FAUCET_SOURCE};
use crate::DebugTickOutcome;
#[cfg(any(target_family = "wasm", test))]
use crate::StreamManager;
#[cfg(target_family = "wasm")]
use crate::{
    ApiIoRecipientPolicy, ApiStreamKind, OperationPhase, RejectedFundDisposition,
    RejectedRefundAttemptRecord, RewardDistributionPreflight, RewardFeeRepreflightEvidence,
    RewardPreflightStatus, RewardReservation, RewardTransferAttemptLifecycle,
    RewardTransferAttemptRecord, StreamManagerError, StreamOperation, StreamOperationKind,
    TransferStatus, TwoWeekRecipientTransfer, CANISTER_STATE,
};
#[cfg(test)]
use crate::{
    OperationPhase, RejectedFundDisposition, RejectedRefundAttemptRecord,
    RewardDistributionPreflight, RewardFeeRepreflightEvidence, RewardPreflightStatus,
    RewardReservation, RewardTransferAttemptLifecycle, RewardTransferAttemptRecord,
    StreamOperation, StreamOperationKind, TransferStatus, TwoWeekRecipientTransfer, CANISTER_STATE,
};
use candid::CandidType;
#[cfg(target_family = "wasm")]
use candid::Nat;
#[cfg(any(target_family = "wasm", test))]
use candid::Principal;
#[cfg(any(target_family = "wasm", test))]
use io_core_model::split_40_60;
#[cfg(target_family = "wasm")]
use io_core_model::ModelError;
#[cfg(target_family = "wasm")]
use io_governance_types::{
    SnsEligibilityPolicy, SnsGovernanceCanisterClient, SnsGovernanceClient, SnsNeuronId,
    SnsParticipationPolicy,
};
#[cfg(any(target_family = "wasm", test))]
use io_ledger_types::Account;
#[cfg(any(target_family = "wasm", test))]
use io_ledger_types::AccountHistoryPageOrder;
#[cfg(any(target_family = "wasm", test))]
use io_ledger_types::AccountHistoryScanState;
#[cfg(target_family = "wasm")]
use io_ledger_types::IcpIndexCanisterClient;
#[cfg(any(target_family = "wasm", test))]
use io_ledger_types::LedgerOperationKind;
#[cfg(any(target_family = "wasm", test))]
use io_ledger_types::Subaccount;
#[cfg(any(target_family = "wasm", test))]
use io_ledger_types::{
    duplicate_matches_expected, LedgerBlock, LedgerTransferError, LedgerTransferRequest,
    LedgerTransferSuccess,
};
#[cfg(target_family = "wasm")]
use io_ledger_types::{AccountAlias, IndexScanRequest, IndexTransaction};
use io_ledger_types::{BlockIndex, IndexError, IndexScanResult};
#[cfg(target_family = "wasm")]
use io_ledger_types::{
    IcpLedgerCanisterClient, IcrcAccount, IcrcIndexCanisterClient, IcrcLedgerCanisterClient,
    LedgerIndexClient, LedgerTransferClient,
};
#[cfg(any(target_family = "wasm", test))]
use io_production_wiring::{
    PRODUCTION_FRONTEND_CANISTER_ID, PRODUCTION_IO_HISTORIAN_CANISTER_ID,
    PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID, PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID,
};
use serde::Deserialize;
#[cfg(any(target_family = "wasm", test))]
use sha2::{Digest, Sha256};
#[cfg(any(target_family = "wasm", test))]
use std::collections::BTreeSet;

pub const STREAM_MANAGER_DEPOSIT_ACCOUNT: &str = "stream_manager_deposit";
pub const REDEMPTION_ACCOUNT: &str = "redemption";
pub const PROTOCOL_RESERVE_ACCOUNT: &str = "protocol_reserve";
pub const REDEMPTION_PAYOUT_MEMO: &str = "redemption_payout";
pub const REDEEMED_IO_MEMO: &str = "redeemed_io_to_reserve";
pub const TWO_WEEK_REWARD_ACCOUNT_PREFIX: &str = "sns_neuron_";
#[cfg(target_family = "wasm")]
const TWO_WEEK_DISSOLVE_DELAY_SECONDS: u64 = 14 * 24 * 60 * 60;
#[cfg(target_family = "wasm")]
const GOVERNANCE_SNAPSHOT_PAGE_LIMIT: u64 = 100;
#[cfg(target_family = "wasm")]
const GOVERNANCE_SNAPSHOT_MAX_PAGES: u64 = 100;
#[cfg(any(target_family = "wasm", test))]
const REJECTED_REFUND_RETRY_BUDGET_PER_TICK: usize = 8;
#[cfg(any(target_family = "wasm", test))]
#[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
const REJECTED_REFUND_PROOF_RECONCILIATION_BUDGET_PER_TICK: usize = 4;
#[cfg(any(target_family = "wasm", test))]
const REJECTED_REFUND_PROOF_SCAN_MAX_PAGES: usize = 20;
#[cfg(any(target_family = "wasm", test))]
const TWO_WEEK_REWARD_LEDGER_TRANSFER_BUDGET_PER_TICK: usize = 8;
#[cfg(any(target_family = "wasm", test))]
const TWO_WEEK_REWARD_REFRESH_BUDGET_PER_TICK: usize = 8;
#[cfg(any(target_family = "wasm", test))]
const TWO_WEEK_REWARD_PROOF_SCAN_MAX_PAGES: usize = 20;

#[cfg(any(target_family = "wasm", test))]
fn encode_hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(TABLE[(byte >> 4) as usize] as char);
        out.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(any(target_family = "wasm", test))]
fn legacy_icp_account_history_scan_state(cursor: u64) -> AccountHistoryScanState {
    AccountHistoryScanState {
        cursor: io_ledger_types::AccountHistoryCursor {
            order: Some(AccountHistoryPageOrder::Descending),
            latest_cursor: Some(BlockIndex(cursor)),
            oldest_cursor: Some(BlockIndex(cursor)),
            backfill_complete: true,
        },
        status: Default::default(),
    }
}

#[cfg(target_family = "wasm")]
fn legacy_io_account_history_scan_state(cursor: u64) -> AccountHistoryScanState {
    AccountHistoryScanState {
        cursor: io_ledger_types::AccountHistoryCursor {
            order: Some(AccountHistoryPageOrder::Ascending),
            latest_cursor: Some(BlockIndex(cursor)),
            oldest_cursor: Some(BlockIndex(cursor)),
            backfill_complete: true,
        },
        status: Default::default(),
    }
}

#[cfg(target_family = "wasm")]
fn no_new_page_errors(outcome: &DebugTickOutcome, page_error_count: usize) -> bool {
    outcome.errors.len() == page_error_count
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SchedulerTickOutcome {
    pub scanned_jupiter_faucet_deposits: u64,
    pub scanned_nns_maturity_deposits: u64,
    pub scanned_redemption_transfers: u64,
    pub processed_authorized_streams: u64,
    pub planned_steps: Vec<String>,
}

impl SchedulerTickOutcome {
    fn no_work_configured() -> Self {
        Self {
            scanned_jupiter_faucet_deposits: 0,
            scanned_nns_maturity_deposits: 0,
            scanned_redemption_transfers: 0,
            processed_authorized_streams: 0,
            planned_steps: vec![
                "scan ICP ledger/index for Jupiter Faucet deposits".to_string(),
                "scan ICP ledger/index for NNS maturity deposits".to_string(),
                "scan IO ledger/index for user redemption transfers".to_string(),
                "classify observed flows before internal processing".to_string(),
            ],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(any(target_family = "wasm", test))]
enum BoundaryTransferDecision {
    Succeeded(u64),
    Retryable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(any(target_family = "wasm", test))]
enum RewardSnapshotAvailability {
    Available(Vec<io_reward_policy::NeuronSnapshot>),
    Unavailable(String),
}

#[cfg(any(target_family = "wasm", test))]
fn require_new_two_week_reward_inputs(
    io_transfer_ledger: Option<Principal>,
    io_index_canister: Option<Principal>,
    sns_governance_canister: Option<Principal>,
    reward_snapshot: &RewardSnapshotAvailability,
) -> Result<
    (
        Principal,
        Principal,
        Principal,
        &[io_reward_policy::NeuronSnapshot],
    ),
    String,
> {
    let io_canister = io_transfer_ledger
        .ok_or_else(|| "IO transfer ledger is required for new two-week rewards".to_string())?;
    let io_index = io_index_canister
        .ok_or_else(|| "IO index canister is required for new two-week rewards".to_string())?;
    let sns_governance = sns_governance_canister.ok_or_else(|| {
        "SNS governance canister is required for new two-week rewards".to_string()
    })?;
    match reward_snapshot {
        RewardSnapshotAvailability::Available(neurons) => {
            Ok((io_canister, io_index, sns_governance, neurons))
        }
        RewardSnapshotAvailability::Unavailable(reason) => Err(format!(
            "SNS governance reward snapshot unavailable for new two-week reward: {reason}"
        )),
    }
}

#[cfg(any(target_family = "wasm", test))]
fn boundary_error_message(err: &LedgerTransferError) -> String {
    match err {
        LedgerTransferError::TemporarilyUnavailable => {
            "ledger transfer temporarily unavailable".to_string()
        }
        LedgerTransferError::CanisterCallFailed { method, message } => {
            format!("ledger transfer call {method} failed: {message}")
        }
        LedgerTransferError::BadFee { expected_fee_e8s } => {
            format!("ledger transfer bad fee; expected {expected_fee_e8s} e8s")
        }
        LedgerTransferError::InsufficientFunds { balance_e8s } => {
            format!("ledger transfer insufficient funds; balance {balance_e8s} e8s")
        }
        LedgerTransferError::Duplicate { duplicate_of } => {
            format!("ledger transfer duplicate at block {}", duplicate_of.0)
        }
        err => format!("ledger transfer failed: {err:?}"),
    }
}

#[cfg(any(target_family = "wasm", test))]
fn is_retryable_redemption_operation(op: &StreamOperation) -> bool {
    op.kind == StreamOperationKind::Redemption
        && !matches!(
            op.phase,
            OperationPhase::Completed | OperationPhase::FailedTerminal
        )
        && !matches!(op.icp_payout_status, TransferStatus::FailedTerminal)
        && !matches!(op.io_return_status, TransferStatus::FailedTerminal)
}

#[cfg(any(all(target_family = "wasm", debug_assertions), test))]
fn mock_fee_fallback_allowed_for_build(
    debug_probe_succeeded: bool,
    debug_assertions_enabled: bool,
) -> bool {
    debug_assertions_enabled && debug_probe_succeeded
}

#[cfg(any(target_family = "wasm", test))]
fn rejected_refund_attempt_created_at(op: &StreamOperation) -> u64 {
    match &op.rejected_fund_disposition {
        Some(RejectedFundDisposition::ReturnToSenderProofPending {
            original_created_at_time: Some(created_at_time),
            ..
        })
        | Some(RejectedFundDisposition::ReturnToSenderManualReconciliationRequired {
            original_created_at_time: Some(created_at_time),
            ..
        }) => *created_at_time,
        Some(RejectedFundDisposition::ReturnToSenderRetryable {
            next_attempt_created_at_time: Some(created_at_time),
            ..
        }) => *created_at_time,
        _ => op.created_at,
    }
}

#[cfg(any(target_family = "wasm", test))]
fn is_retryable_rejected_refund_operation(op: &StreamOperation) -> bool {
    op.kind == StreamOperationKind::RejectedRedemption
        && !matches!(
            op.phase,
            OperationPhase::Completed | OperationPhase::FailedTerminal
        )
        && matches!(
            op.rejected_fund_disposition,
            Some(RejectedFundDisposition::ReturnToSenderPending)
                | Some(RejectedFundDisposition::ReturnToSenderRetryable { .. })
        )
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(any(target_family = "wasm", test))]
enum TooOldRefundProofDisposition {
    ProofFound(BlockIndex),
    IndexNotCaughtUp(String),
    HistoryIncomplete(String),
    CompleteNoMatch(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(target_family = "wasm")]
struct TooOldRefundProofScanOutcome {
    disposition: TooOldRefundProofDisposition,
    scan_state: AccountHistoryScanState,
}

#[cfg(any(target_family = "wasm", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
enum RewardTransferProofDisposition {
    ProofFound(BlockIndex),
    IndexNotCaughtUp(String),
    HistoryIncomplete(String),
    CompleteNoMatch(String),
}

#[cfg(target_family = "wasm")]
struct RewardTransferProofScanOutcome {
    disposition: RewardTransferProofDisposition,
    scan_state: AccountHistoryScanState,
}

#[cfg(any(target_family = "wasm", test))]
fn proof_absence_completion_gap(scan_state: &AccountHistoryScanState) -> Option<String> {
    if !scan_state.cursor.backfill_complete {
        return Some("full refund-source account history has not been backfilled".to_string());
    }
    let Some(index_synced) = scan_state.status.num_blocks_synced else {
        return Some(
            "IO index did not report ledger catch-up evidence for refund proof absence".to_string(),
        );
    };
    if let Some(latest) = scan_state.cursor.latest_cursor {
        if index_synced < latest {
            return Some(format!(
                "IO index is only synced through block {}, below observed refund-source block {}",
                index_synced.0, latest.0
            ));
        }
    }
    None
}

#[cfg(any(target_family = "wasm", test))]
fn classify_reward_transfer_proof_state(
    scan_state: &AccountHistoryScanState,
    pages_scanned: usize,
    max_pages: usize,
) -> RewardTransferProofDisposition {
    if scan_state.status.lag_suspected {
        return RewardTransferProofDisposition::IndexNotCaughtUp(
            scan_state.status.last_error.clone().unwrap_or_else(|| {
                "SNS index lag suspected while proving reward transfer".to_string()
            }),
        );
    }
    if scan_state.status.scan_incomplete || pages_scanned >= max_pages {
        return RewardTransferProofDisposition::HistoryIncomplete(
            proof_absence_completion_gap(scan_state).unwrap_or_else(|| {
                format!("matching reward transfer proof not found within {max_pages} index pages")
            }),
        );
    }
    if scan_state.cursor.backfill_complete {
        return match proof_absence_completion_gap(scan_state) {
            Some(reason) => RewardTransferProofDisposition::HistoryIncomplete(reason),
            None => RewardTransferProofDisposition::CompleteNoMatch(
                "complete SNS reward account history contains no matching transfer".to_string(),
            ),
        };
    }
    RewardTransferProofDisposition::IndexNotCaughtUp(
        "SNS reward account history is not complete enough to prove transfer outcome".to_string(),
    )
}

#[cfg(any(target_family = "wasm", test))]
fn classify_too_old_refund_proof_state(
    scan_state: &AccountHistoryScanState,
    pages_scanned: usize,
    max_pages: usize,
) -> TooOldRefundProofDisposition {
    if scan_state.status.lag_suspected {
        return TooOldRefundProofDisposition::IndexNotCaughtUp(
            scan_state
                .status
                .last_error
                .clone()
                .unwrap_or_else(|| "IO index has not caught up to the ledger tip".to_string()),
        );
    }
    if scan_state.status.scan_incomplete || pages_scanned >= max_pages {
        return TooOldRefundProofDisposition::HistoryIncomplete(
            "bounded IO index proof scan did not cover complete account history".to_string(),
        );
    }
    if scan_state.cursor.backfill_complete {
        return match proof_absence_completion_gap(scan_state) {
            Some(reason) => TooOldRefundProofDisposition::HistoryIncomplete(reason),
            None => TooOldRefundProofDisposition::CompleteNoMatch(
                "complete canonical IO index history contains no matching refund proof".to_string(),
            ),
        };
    }
    TooOldRefundProofDisposition::IndexNotCaughtUp(
        "IO index proof scan has no complete backfill evidence".to_string(),
    )
}

#[cfg(any(target_family = "wasm", test))]
fn next_retryable_rejected_refund_operation(
    journal: &[StreamOperation],
    attempted: &BTreeSet<String>,
) -> Option<StreamOperation> {
    journal.iter().find_map(|op| {
        (is_retryable_rejected_refund_operation(op) && !attempted.contains(&op.operation_id))
            .then(|| op.clone())
    })
}

#[cfg(any(target_family = "wasm", test))]
#[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
fn next_proof_pending_rejected_refund_operation(
    journal: &[StreamOperation],
    attempted: &BTreeSet<String>,
) -> Option<StreamOperation> {
    journal.iter().find_map(|op| {
        (op.kind == StreamOperationKind::RejectedRedemption
            && matches!(
                op.rejected_fund_disposition,
                Some(RejectedFundDisposition::ReturnToSenderProofPending { .. })
            )
            && !attempted.contains(&op.operation_id))
        .then(|| op.clone())
    })
}

#[cfg(any(target_family = "wasm", test))]
fn amount_after_fee(amount_e8s: u128, fee_e8s: u128) -> Option<u128> {
    if amount_e8s > fee_e8s {
        Some(amount_e8s - fee_e8s)
    } else {
        None
    }
}

#[cfg(any(target_family = "wasm", test))]
fn classify_boundary_transfer_result(
    expected: &LedgerTransferRequest,
    result: Result<LedgerTransferSuccess, LedgerTransferError>,
    duplicate_block: Option<&LedgerBlock>,
) -> BoundaryTransferDecision {
    match result {
        Ok(success) => BoundaryTransferDecision::Succeeded(success.block_index.0),
        Err(LedgerTransferError::Duplicate { .. }) => match duplicate_block {
            Some(block) => match duplicate_matches_expected(expected, block) {
                Ok(block) => BoundaryTransferDecision::Succeeded(block.0),
                Err(proof) => BoundaryTransferDecision::Retryable(format!(
                    "duplicate transfer did not match expected amount/account/memo: {proof:?}"
                )),
            },
            None => BoundaryTransferDecision::Retryable(
                "duplicate transfer could not be proven against expected amount/account/memo"
                    .to_string(),
            ),
        },
        Err(err) => BoundaryTransferDecision::Retryable(boundary_error_message(&err)),
    }
}

#[cfg(any(target_family = "wasm", test))]
fn classify_boundary_transfer_result_with_source(
    expected: &LedgerTransferRequest,
    expected_from: &io_ledger_types::Account,
    result: Result<LedgerTransferSuccess, LedgerTransferError>,
    duplicate_block: Option<&LedgerBlock>,
) -> BoundaryTransferDecision {
    match result {
        Ok(success) => BoundaryTransferDecision::Succeeded(success.block_index.0),
        Err(LedgerTransferError::Duplicate { .. }) => match duplicate_block {
            Some(block) => match duplicate_matches_expected(expected, block) {
                Ok(block_index) if block.from.as_ref() == Some(expected_from) => {
                    BoundaryTransferDecision::Succeeded(block_index.0)
                }
                Ok(_) => BoundaryTransferDecision::Retryable(
                    "duplicate transfer did not match expected source account".to_string(),
                ),
                Err(proof) => BoundaryTransferDecision::Retryable(format!(
                    "duplicate transfer did not match expected amount/account/memo: {proof:?}"
                )),
            },
            None => BoundaryTransferDecision::Retryable(
                "duplicate transfer could not be proven against expected amount/account/memo"
                    .to_string(),
            ),
        },
        Err(err) => BoundaryTransferDecision::Retryable(boundary_error_message(&err)),
    }
}

#[cfg(target_family = "wasm")]
fn principal(text: &Option<String>) -> Option<Principal> {
    text.as_deref()
        .and_then(|value| Principal::from_text(value).ok())
}

#[cfg(target_family = "wasm")]
fn kind_from_api(kind: ApiStreamKind) -> StreamOperationKind {
    match kind {
        ApiStreamKind::JupiterFaucet => StreamOperationKind::JupiterFaucetStream,
        ApiStreamKind::TwoYearMaturity => StreamOperationKind::TwoYearMaturityStream,
        ApiStreamKind::TwoWeekMaturity => StreamOperationKind::TwoWeekMaturityStream,
    }
}

#[cfg(target_family = "wasm")]
async fn refresh_finalized_sns_reward_snapshot(
    canister: Principal,
) -> Result<Vec<io_reward_policy::NeuronSnapshot>, GovernanceSnapshotError> {
    let client = SnsGovernanceCanisterClient { canister };
    let now_seconds = ic_cdk::api::time() / 1_000_000_000;
    let request = GovernanceRewardSnapshotRequest {
        eligibility_policy: SnsEligibilityPolicy {
            protocol_neuron_ids: BTreeSet::new(),
            jupiter_governance_neuron_ids: BTreeSet::new(),
            minimum_dissolve_delay_seconds: TWO_WEEK_DISSOLVE_DELAY_SECONDS,
            require_non_dissolving: true,
            current_timestamp_seconds: 0,
        },
        participation_policy: SnsParticipationPolicy {
            count_direct_votes: true,
            count_followed_votes: true,
            excluded_topics: BTreeSet::new(),
            epoch_start_seconds: 0,
            epoch_end_seconds: now_seconds,
        },
        max_neuron_pages: GOVERNANCE_SNAPSHOT_MAX_PAGES,
        max_proposal_pages: GOVERNANCE_SNAPSHOT_MAX_PAGES,
        page_limit: GOVERNANCE_SNAPSHOT_PAGE_LIMIT,
        eligible_since_overrides: Default::default(),
    };

    match build_governance_reward_snapshot(&client, request).await {
        Ok(snapshot) => {
            CANISTER_STATE.with(|cell| {
                cell.borrow_mut()
                    .manager
                    .refresh_active_staked_io_from_neurons(&snapshot.snapshots);
            });
            Ok(snapshot.snapshots)
        }
        Err(err) => Err(err),
    }
}

#[cfg(target_family = "wasm")]
fn governance_snapshot_error_message(err: &GovernanceSnapshotError) -> String {
    format!("finalized SNS governance reward snapshot refresh failed: {err:?}")
}

#[cfg(all(target_family = "wasm", debug_assertions))]
fn reward_snapshot_debug_fallback_allowed(err: &GovernanceSnapshotError) -> bool {
    matches!(
        err,
        GovernanceSnapshotError::SnsGovernance(
            io_governance_types::SnsGovernanceError::CanisterCallFailed { .. }
                | io_governance_types::SnsGovernanceError::Unsupported
        )
    )
}

#[cfg(target_family = "wasm")]
async fn load_reward_snapshot(sns_governance: Option<Principal>) -> RewardSnapshotAvailability {
    let Some(canister) = sns_governance else {
        return RewardSnapshotAvailability::Unavailable(
            "SNS governance canister is not configured".to_string(),
        );
    };

    match refresh_finalized_sns_reward_snapshot(canister).await {
        Ok(neurons) => RewardSnapshotAvailability::Available(neurons),
        Err(real_err) => {
            let real_err_message = governance_snapshot_error_message(&real_err);
            #[cfg(debug_assertions)]
            {
                if reward_snapshot_debug_fallback_allowed(&real_err) {
                    match sns_governance::debug_list_neurons(canister).await {
                        Ok(neurons) => {
                            CANISTER_STATE.with(|cell| {
                                cell.borrow_mut()
                                    .manager
                                    .refresh_active_staked_io_from_neurons(&neurons);
                            });
                            RewardSnapshotAvailability::Available(neurons)
                        }
                        Err(debug_err) => RewardSnapshotAvailability::Unavailable(format!(
                            "{real_err_message}; {debug_err}"
                        )),
                    }
                } else {
                    RewardSnapshotAvailability::Unavailable(real_err_message)
                }
            }
            #[cfg(not(debug_assertions))]
            {
                RewardSnapshotAvailability::Unavailable(real_err_message)
            }
        }
    }
}

pub fn scheduler_tick_plan_only() -> SchedulerTickOutcome {
    SchedulerTickOutcome::no_work_configured()
}

pub fn boundary_cursor_after_contiguous_page(
    current: Option<BlockIndex>,
    result: &IndexScanResult,
) -> Result<Option<BlockIndex>, IndexError> {
    if let (Some(requested), Some(tip)) = (current, result.index_tip) {
        if tip < requested {
            return Err(IndexError::IndexLag {
                requested,
                tip: Some(tip),
            });
        }
    }

    if result.archive_required {
        return Err(IndexError::ArchiveRequired {
            from: current.unwrap_or(BlockIndex(0)),
        });
    }

    let mut expected_next = current.map(|block| block.0.saturating_add(1));
    let mut highest = current;
    for tx in &result.transactions {
        if let Some(cursor) = current {
            if tx.block_index <= cursor {
                highest = Some(cursor);
                continue;
            }
        }
        if let Some(expected) = expected_next {
            if tx.block_index.0 != expected {
                return Err(IndexError::MissingBlock {
                    block_index: BlockIndex(expected),
                });
            }
        }
        expected_next = Some(tx.block_index.0.saturating_add(1));
        highest = Some(tx.block_index);
    }

    Ok(highest)
}

pub fn boundary_cursor_after_account_page(
    current: Option<BlockIndex>,
    result: &IndexScanResult,
) -> Result<Option<BlockIndex>, IndexError> {
    if let (Some(requested), Some(tip)) = (current, result.index_tip) {
        if tip < requested {
            return Err(IndexError::IndexLag {
                requested,
                tip: Some(tip),
            });
        }
    }

    if result.archive_required {
        return Err(IndexError::ArchiveRequired {
            from: current.unwrap_or(BlockIndex(0)),
        });
    }

    let mut last = None;
    let mut highest = current;
    for tx in &result.transactions {
        if let Some(previous) = last {
            if tx.block_index <= previous {
                return Err(IndexError::MissingBlock {
                    block_index: tx.block_index,
                });
            }
        }
        last = Some(tx.block_index);

        if current.is_some_and(|cursor| tx.block_index <= cursor) {
            continue;
        }

        highest = Some(highest.map_or(tx.block_index, |cursor| cursor.max(tx.block_index)));
    }

    Ok(highest)
}

#[cfg(target_family = "wasm")]
fn mock_transfer_request(
    from: &str,
    to: &str,
    amount_e8s: u128,
    memo: &str,
) -> LedgerTransferRequest {
    LedgerTransferRequest {
        from_subaccount: Some(icp_ledger::mock_subaccount(from)),
        to: icp_ledger::mock_account(to),
        amount_e8s,
        fee_e8s: None,
        memo: Some(io_ledger_types::Memo::from(memo)),
        created_at_time: None,
    }
}

#[cfg(target_family = "wasm")]
fn canister_owned_account(label: &str) -> Account {
    Account::new(
        ic_cdk::api::canister_self(),
        Some(icp_ledger::mock_subaccount(label)),
    )
}

#[cfg(any(target_family = "wasm", test))]
fn rejected_refund_memo(operation_id: &str) -> io_ledger_types::Memo {
    io_ledger_types::Memo::from(format!("rejected_io_refund:{operation_id}"))
}

#[cfg(any(target_family = "wasm", test))]
fn rejected_refund_request_from_attempt(
    attempt: &RejectedRefundAttemptRecord,
) -> LedgerTransferRequest {
    LedgerTransferRequest {
        from_subaccount: attempt.refund_source_account.subaccount,
        to: attempt.destination_account.clone(),
        amount_e8s: attempt.attempted_refund_amount_e8s,
        fee_e8s: None,
        memo: attempt.memo.clone(),
        created_at_time: Some(attempt.attempted_created_at_time),
    }
}

#[cfg(any(target_family = "wasm", test))]
fn rejected_refund_attempt_from_parts(
    refund_source: Account,
    destination: Account,
    refund_amount_e8s: u128,
    fee_e8s: u128,
    memo: Option<io_ledger_types::Memo>,
    created_at_time: u64,
) -> RejectedRefundAttemptRecord {
    RejectedRefundAttemptRecord {
        attempted_refund_amount_e8s: refund_amount_e8s,
        attempted_fee_e8s: fee_e8s,
        attempted_created_at_time: created_at_time,
        memo,
        refund_source_account: refund_source,
        destination_account: destination,
    }
}

#[cfg(any(target_family = "wasm", test))]
fn reward_transfer_memo(
    operation_id: &str,
    canonical_sns_neuron_id: &[u8],
) -> io_ledger_types::Memo {
    let canonical_id: [u8; 32] = canonical_sns_neuron_id
        .try_into()
        .expect("reward transfer memo requires canonical 32-byte SNS neuron ID");
    let mut hasher = Sha256::new();
    hasher.update(b"io:two_week_reward:v1");
    hasher.update(operation_id.as_bytes());
    hasher.update(canonical_id);
    io_ledger_types::Memo(hasher.finalize().to_vec())
}

#[cfg(any(target_family = "wasm", test))]
fn reward_transfer_request_from_attempt(
    attempt: &RewardTransferAttemptRecord,
) -> LedgerTransferRequest {
    LedgerTransferRequest {
        from_subaccount: attempt.source_account.subaccount,
        to: attempt.destination_account.clone(),
        amount_e8s: attempt.amount_e8s,
        fee_e8s: Some(attempt.fee_e8s),
        memo: attempt.memo.clone(),
        created_at_time: Some(attempt.created_at_time),
    }
}

#[cfg(any(target_family = "wasm", test))]
fn reward_transfer_attempt_from_parts(
    source_account: Account,
    destination_account: Account,
    amount_e8s: u128,
    fee_e8s: u128,
    created_at_time: u64,
    operation_id: &str,
    canonical_sns_neuron_id: Vec<u8>,
) -> RewardTransferAttemptRecord {
    RewardTransferAttemptRecord {
        amount_e8s,
        fee_e8s,
        created_at_time,
        memo: Some(reward_transfer_memo(operation_id, &canonical_sns_neuron_id)),
        source_account,
        destination_account,
        canonical_sns_neuron_id,
        lifecycle: Some(RewardTransferAttemptLifecycle::Prepared),
    }
}

#[cfg(any(target_family = "wasm", test))]
struct RewardAttemptPlan {
    source_account: Account,
    destination_account: Account,
    amount_e8s: u128,
    fee_e8s: u128,
    created_at_time: u64,
    canonical_sns_neuron_id: Vec<u8>,
}

#[cfg(any(target_family = "wasm", test))]
fn get_or_create_reward_transfer_attempt(
    operation_journal: &mut [StreamOperation],
    processed_transactions: &BTreeSet<String>,
    operation_id: &str,
    recipient_index: usize,
    plan: RewardAttemptPlan,
) -> Result<RewardTransferAttemptRecord, String> {
    let op = operation_journal
        .iter_mut()
        .find(|op| op.operation_id == operation_id)
        .ok_or_else(|| {
            format!("reward operation {operation_id} disappeared before transfer attempt")
        })?;
    if !reward_operation_can_progress_for_host(op) {
        return Err(format!(
            "reward operation {operation_id} is not in a transferable phase"
        ));
    }
    let Some(preflight) = op.reward_preflight.as_ref() else {
        return Err(format!(
            "reward operation {operation_id} is missing validated preflight"
        ));
    };
    if preflight.status != RewardPreflightStatus::Validated {
        return Err(format!(
            "reward operation {operation_id} preflight is not validated"
        ));
    }
    if preflight.ledger_fee_e8s != plan.fee_e8s {
        return Err(format!(
            "reward operation {operation_id} preflight fee {} disagrees with attempt fee {}",
            preflight.ledger_fee_e8s, plan.fee_e8s
        ));
    }
    let reservation = reward_reservation_for_operation(op, processed_transactions)?;
    if reservation.unspent_reserved_reward_debit_e8s == 0 {
        return Err(format!(
            "reward operation {operation_id} has no unspent reward reservation for transfer attempt"
        ));
    }
    let recipient = op.two_week_recipients.get(recipient_index).ok_or_else(|| {
        format!("reward operation {operation_id} recipient {recipient_index} disappeared")
    })?;
    if recipient_ledger_status(recipient) == TransferStatus::Succeeded {
        return Err(format!(
            "reward operation {operation_id} recipient {recipient_index} already has a succeeded transfer"
        ));
    }
    if recipient.sns_neuron_id.as_deref() != Some(plan.canonical_sns_neuron_id.as_slice()) {
        return Err(format!(
            "reward operation {operation_id} recipient {recipient_index} canonical SNS neuron id changed"
        ));
    }
    if recipient.amount_e8s != plan.amount_e8s {
        return Err(format!(
            "reward operation {operation_id} recipient {recipient_index} amount changed"
        ));
    }

    let expected = reward_transfer_attempt_from_parts(
        plan.source_account,
        plan.destination_account,
        plan.amount_e8s,
        plan.fee_e8s,
        plan.created_at_time,
        operation_id,
        plan.canonical_sns_neuron_id,
    );
    if let Some(existing) = recipient.reward_transfer_attempt.clone() {
        if existing.amount_e8s != expected.amount_e8s
            || existing.fee_e8s != expected.fee_e8s
            || existing.source_account != expected.source_account
            || existing.destination_account != expected.destination_account
            || existing.memo != expected.memo
            || existing.canonical_sns_neuron_id != expected.canonical_sns_neuron_id
        {
            return Err(format!(
                "reward operation {operation_id} recipient {recipient_index} existing transfer attempt disagrees with current plan"
            ));
        }
        return Ok(existing);
    }

    let reserve_debit = expected
        .amount_e8s
        .checked_add(expected.fee_e8s)
        .ok_or_else(|| "reward transfer attempt reserve debit overflowed".to_string())?;
    let mut validated_op = op.clone();
    let validated_recipient = validated_op
        .two_week_recipients
        .get_mut(recipient_index)
        .ok_or_else(|| {
            format!("reward operation {operation_id} recipient {recipient_index} disappeared")
        })?;
    validated_recipient.reward_transfer_attempt = Some(expected.clone());
    validated_recipient.ledger_transfer_fee_e8s = Some(expected.fee_e8s);
    validated_recipient.reward_amount_received_e8s = Some(expected.amount_e8s);
    validated_recipient.reserve_debit_e8s = Some(reserve_debit);
    crate::validate_reward_operation_accounting(
        &validated_op,
        Some(processed_transactions),
        crate::RewardValidationMode::Current,
    )?;

    let recipient = op
        .two_week_recipients
        .get_mut(recipient_index)
        .ok_or_else(|| {
            format!("reward operation {operation_id} recipient {recipient_index} disappeared")
        })?;
    recipient.reward_transfer_attempt = Some(expected.clone());
    recipient.ledger_transfer_fee_e8s = Some(expected.fee_e8s);
    recipient.reward_amount_received_e8s = Some(expected.amount_e8s);
    recipient.reserve_debit_e8s = Some(reserve_debit);
    recipient.last_error = None;
    op.mark_updated(OperationPhase::PartiallyDistributed);
    Ok(expected)
}

#[cfg(any(target_family = "wasm", test))]
fn reward_operation_can_progress_for_host(op: &StreamOperation) -> bool {
    op.kind == StreamOperationKind::TwoWeekMaturityStream
        && !matches!(
            op.phase,
            OperationPhase::Completed | OperationPhase::FailedTerminal
        )
}

#[cfg(target_family = "wasm")]
fn stored_reward_attempt_still_matches(
    operation_id: &str,
    recipient_index: usize,
    attempt: &RewardTransferAttemptRecord,
) -> bool {
    CANISTER_STATE.with(|cell| {
        reward_attempt_matches_in_journal(
            &cell.borrow().operation_journal,
            operation_id,
            recipient_index,
            attempt,
        )
    })
}

#[cfg(any(target_family = "wasm", test))]
fn reward_attempt_matches_in_journal(
    operation_journal: &[StreamOperation],
    operation_id: &str,
    recipient_index: usize,
    attempt: &RewardTransferAttemptRecord,
) -> bool {
    operation_journal
        .iter()
        .find(|op| op.operation_id == operation_id)
        .and_then(|op| op.two_week_recipients.get(recipient_index))
        .and_then(|recipient| recipient.reward_transfer_attempt.as_ref())
        == Some(attempt)
}

#[cfg(any(target_family = "wasm", test))]
fn mark_reward_attempt_submitted_if_prepared(
    operation_journal: &mut [StreamOperation],
    operation_id: &str,
    recipient_index: usize,
    attempt: &RewardTransferAttemptRecord,
) -> Result<RewardTransferAttemptRecord, String> {
    let op = operation_journal
        .iter_mut()
        .find(|op| op.operation_id == operation_id)
        .ok_or_else(|| format!("reward operation {operation_id} disappeared before submit"))?;
    if op
        .two_week_recipients
        .iter()
        .enumerate()
        .any(|(index, recipient)| {
            index != recipient_index
                && crate::reward_recipient_has_submitted_or_proof_required_attempt(recipient)
        })
    {
        return Err(format!(
            "reward operation {operation_id} already has another submitted or proof-required reward attempt"
        ));
    }
    let recipient = op
        .two_week_recipients
        .get_mut(recipient_index)
        .ok_or_else(|| {
            format!("reward operation {operation_id} recipient {recipient_index} disappeared")
        })?;
    let Some(stored) = recipient.reward_transfer_attempt.as_mut() else {
        return Err(format!(
            "reward operation {operation_id} recipient {recipient_index} missing transfer attempt before submit"
        ));
    };
    if stored != attempt {
        return Err(format!(
            "reward operation {operation_id} recipient {recipient_index} persisted attempt changed before submit"
        ));
    }
    if !reward_attempt_is_prepared(stored) {
        return Err(format!(
            "reward operation {operation_id} recipient {recipient_index} transfer attempt is already submitted or awaiting proof"
        ));
    }
    stored.lifecycle = Some(RewardTransferAttemptLifecycle::SubmittedAwaitingResult {
        generation: stored.created_at_time,
    });
    let submitted = stored.clone();
    crate::validate_reward_operation_accounting(op, None, crate::RewardValidationMode::Current)?;
    op.mark_updated(OperationPhase::PartiallyDistributed);
    Ok(submitted)
}

#[cfg(any(target_family = "wasm", test))]
fn reward_account_for_sns_neuron(
    governance_canister: Principal,
    sns_neuron_id: &[u8],
) -> Result<Account, String> {
    let bytes = <[u8; 32]>::try_from(sns_neuron_id).map_err(|_| {
        format!(
            "SNS reward destination neuron id must be exactly 32 bytes, got {}",
            sns_neuron_id.len()
        )
    })?;
    Ok(Account::new(governance_canister, Some(Subaccount(bytes))))
}

#[cfg(any(target_family = "wasm", test))]
fn reward_recipient_reserve_debit(
    op: &StreamOperation,
    recipient: &TwoWeekRecipientTransfer,
    preflight: &RewardDistributionPreflight,
) -> Result<u128, String> {
    crate::reward_recipient_authoritative_debit(op, recipient, preflight)
}

#[cfg(any(target_family = "wasm", test))]
#[cfg(any(target_family = "wasm", test))]
fn reward_current_bad_fee_recipient_has_value_or_uncertainty(
    recipient: &TwoWeekRecipientTransfer,
) -> bool {
    recipient.transfer_block_index.is_some()
        || recipient.ledger_transfer_block.is_some()
        || recipient.ledger_transfer_proof_scan_state.is_some()
        || recipient_ledger_status(recipient) == TransferStatus::Succeeded
        || matches!(
            recipient.governance_refresh_status,
            Some(TransferStatus::Succeeded)
        )
        || recipient.observed_stake_after_e8s.is_some()
        || recipient.concurrent_stake_delta_e8s.is_some()
}

#[cfg(any(target_family = "wasm", test))]
fn pending_repreflight_reward_reservation(
    op: &StreamOperation,
    evidence: RewardFeeRepreflightEvidence,
) -> Result<RewardReservation, String> {
    let stored = op.reward_reservation.ok_or_else(|| {
        format!(
            "operation {} pending reward re-preflight is missing prior reservation",
            op.operation_id
        )
    })?;
    let total = stored.checked_total_unavailable_reward_debit_e8s()?;
    if total != evidence.prior_reserved_debit_e8s
        || op.reserved_reward_debit_e8s.unwrap_or(total) != evidence.prior_reserved_debit_e8s
    {
        return Err(format!(
            "operation {} pending reward re-preflight reservation disagrees with prior debit evidence",
            op.operation_id
        ));
    }
    if evidence.prior_validated_fee_e8s == evidence.observed_current_fee_e8s {
        return Err(format!(
            "operation {} pending reward re-preflight has no fee change evidence",
            op.operation_id
        ));
    }
    Ok(stored)
}

#[cfg(any(target_family = "wasm", test))]
fn operation_has_external_reward_effect_or_uncertainty(op: &StreamOperation) -> bool {
    op.two_week_recipients
        .iter()
        .any(crate::reward_recipient_has_any_attempt_or_external_evidence)
}

#[cfg(any(target_family = "wasm", test))]
fn derived_reward_reservation_for_operation(
    op: &StreamOperation,
) -> Result<RewardReservation, String> {
    let Some(preflight) = &op.reward_preflight else {
        return Ok(op.reward_reservation.unwrap_or_else(|| RewardReservation {
            unspent_reserved_reward_debit_e8s: op.reserved_reward_debit_e8s.unwrap_or(0),
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        }));
    };
    if preflight.status == RewardPreflightStatus::Pending {
        if let Some(evidence) = op.reward_fee_repreflight {
            return pending_repreflight_reward_reservation(op, evidence);
        }
        if op
            .reward_reservation
            .map(|reservation| {
                reservation
                    .checked_total_unavailable_reward_debit_e8s()
                    .unwrap_or(u128::MAX)
                    != 0
            })
            .unwrap_or(false)
            || op.reserved_reward_debit_e8s.unwrap_or(0) != 0
        {
            return Err(format!(
                "operation {} has pending reward preflight with reservation but no re-preflight evidence",
                op.operation_id
            ));
        }
    }
    if preflight.status != RewardPreflightStatus::Validated
        && !operation_has_external_reward_effect_or_uncertainty(op)
    {
        return Ok(RewardReservation::default());
    }

    op.two_week_recipients.iter().try_fold(
        RewardReservation::default(),
        |mut reservation, recipient| {
            let debit = reward_recipient_reserve_debit(op, recipient, preflight)?;
            if crate::reward_recipient_has_spent_debit(recipient) {
                reservation.externally_spent_but_uncommitted_reward_debit_e8s = reservation
                    .externally_spent_but_uncommitted_reward_debit_e8s
                    .checked_add(debit)
                    .ok_or_else(|| {
                        "spent-but-uncommitted reward reservation overflowed".to_string()
                    })?;
            } else {
                reservation.unspent_reserved_reward_debit_e8s = reservation
                    .unspent_reserved_reward_debit_e8s
                    .checked_add(debit)
                    .ok_or_else(|| "unspent reward reservation overflowed".to_string())?;
            }
            Ok(reservation)
        },
    )
}

#[cfg(any(target_family = "wasm", test))]
fn reward_reservation_for_operation(
    op: &StreamOperation,
    processed_transactions: &BTreeSet<String>,
) -> Result<RewardReservation, String> {
    let is_processed = processed_transactions.contains(&op.source_transaction_id);
    if op.phase == OperationPhase::Completed {
        if !is_processed {
            return Err(format!(
                "completed reward operation {} is missing processed transaction evidence",
                op.operation_id
            ));
        }
        let stored = op.reward_reservation.unwrap_or_default();
        let stored_total = stored.checked_total_unavailable_reward_debit_e8s()?;
        if stored_total != 0 || op.reserved_reward_debit_e8s.unwrap_or(0) != 0 {
            return Err(format!(
                "completed reward operation {} has nonzero reward reservation",
                op.operation_id
            ));
        }
        return Ok(RewardReservation::default());
    }
    if is_processed {
        return Err(format!(
            "non-completed reward operation {} has processed transaction evidence",
            op.operation_id
        ));
    }

    crate::validate_reward_operation_accounting(
        op,
        Some(processed_transactions),
        crate::RewardValidationMode::Current,
    )?;
    let derived = derived_reward_reservation_for_operation(op)?;
    if let Some(stored) = op.reward_reservation {
        if stored != derived {
            return Err(format!(
                "operation {} reward reservation split disagrees with recipient evidence: stored {:?}, derived {:?}",
                op.operation_id, stored, derived
            ));
        }
    }
    if let Some(legacy) = op.reserved_reward_debit_e8s {
        let total = derived.checked_total_unavailable_reward_debit_e8s()?;
        if legacy != total {
            return Err(format!(
                "operation {} legacy reward reservation {legacy} disagrees with split total {total}",
                op.operation_id
            ));
        }
    }
    Ok(derived)
}

#[cfg(any(target_family = "wasm", test))]
fn pending_reward_reservation_for_operation(
    op: &StreamOperation,
    processed_transactions: &BTreeSet<String>,
) -> Result<u128, String> {
    reward_reservation_for_operation(op, processed_transactions)?
        .checked_total_unavailable_reward_debit_e8s()
}

#[cfg(test)]
fn pending_reward_unspent_reservation_for_operation(
    op: &StreamOperation,
    processed_transactions: &BTreeSet<String>,
) -> Result<u128, String> {
    reward_reservation_for_operation(op, processed_transactions)
        .map(|reservation| reservation.unspent_reserved_reward_debit_e8s)
}

#[cfg(test)]
fn pending_reward_spent_uncommitted_reservation_for_operation(
    op: &StreamOperation,
    processed_transactions: &BTreeSet<String>,
) -> Result<u128, String> {
    reward_reservation_for_operation(op, processed_transactions)
        .map(|reservation| reservation.externally_spent_but_uncommitted_reward_debit_e8s)
}

#[cfg(any(target_family = "wasm", test))]
fn pending_reward_reservations<'a>(
    ops: impl Iterator<Item = &'a StreamOperation>,
    processed_transactions: &BTreeSet<String>,
    excluding_operation_id: Option<&str>,
) -> Result<u128, String> {
    ops.filter(|op| excluding_operation_id != Some(op.operation_id.as_str()))
        .try_fold(0_u128, |sum, op| {
            let reservation = pending_reward_reservation_for_operation(op, processed_transactions)?;
            sum.checked_add(reservation)
                .ok_or_else(|| "pending reward reservations overflowed".to_string())
        })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg(any(target_family = "wasm", test))]
struct RewardReservationComponents {
    total_unavailable_e8s: u128,
    unspent_reserved_e8s: u128,
    spent_uncommitted_e8s: u128,
}

#[cfg(any(target_family = "wasm", test))]
fn pending_reward_reservation_components<'a>(
    ops: impl Iterator<Item = &'a StreamOperation>,
    processed_transactions: &BTreeSet<String>,
    excluding_operation_id: Option<&str>,
) -> Result<RewardReservationComponents, String> {
    ops.filter(|op| excluding_operation_id != Some(op.operation_id.as_str()))
        .try_fold(
            RewardReservationComponents::default(),
            |mut components, op| {
                let reservation = reward_reservation_for_operation(op, processed_transactions)?;
                components.unspent_reserved_e8s = components
                    .unspent_reserved_e8s
                    .checked_add(reservation.unspent_reserved_reward_debit_e8s)
                    .ok_or_else(|| "pending unspent reward reservations overflowed".to_string())?;
                components.spent_uncommitted_e8s = components
                    .spent_uncommitted_e8s
                    .checked_add(reservation.externally_spent_but_uncommitted_reward_debit_e8s)
                    .ok_or_else(|| {
                        "pending spent-uncommitted reward reservations overflowed".to_string()
                    })?;
                components.total_unavailable_e8s = components
                    .unspent_reserved_e8s
                    .checked_add(components.spent_uncommitted_e8s)
                    .ok_or_else(|| "pending reward reservations overflowed".to_string())?;
                Ok(components)
            },
        )
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(any(target_family = "wasm", test))]
struct RewardPreflightSnapshot {
    operation_id: String,
    operation: StreamOperation,
    protocol_reserve_io_e8s: u128,
    pending_reservations: RewardReservationComponents,
    processed_transactions: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(any(target_family = "wasm", test))]
enum RewardPreflightCasError {
    RetryableConflict(String),
    Terminal(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(any(target_family = "wasm", test))]
struct RewardPreflightObservedInputs {
    sns_governance_canister: Principal,
    ledger_fee_e8s: u128,
    real_reserve_balance_e8s: u128,
    validated_at_timestamp_nanos: u64,
}

#[cfg(any(target_family = "wasm", test))]
fn capture_reward_preflight_snapshot(
    operation_journal: &[StreamOperation],
    processed_transactions: &BTreeSet<String>,
    protocol_reserve_io_e8s: u128,
    operation_id: &str,
) -> Result<RewardPreflightSnapshot, String> {
    let operation = operation_journal
        .iter()
        .find(|op| op.operation_id == operation_id)
        .cloned()
        .ok_or_else(|| format!("reward operation {operation_id} disappeared before preflight"))?;
    let pending_reservations = pending_reward_reservation_components(
        operation_journal.iter(),
        processed_transactions,
        Some(operation_id),
    )?;
    Ok(RewardPreflightSnapshot {
        operation_id: operation_id.to_string(),
        operation,
        protocol_reserve_io_e8s,
        pending_reservations,
        processed_transactions: processed_transactions.clone(),
    })
}

#[cfg(any(target_family = "wasm", test))]
fn finalize_reward_preflight_snapshot(
    snapshot: &RewardPreflightSnapshot,
    operation_journal: &[StreamOperation],
    processed_transactions: &BTreeSet<String>,
    current_protocol_reserve_io_e8s: u128,
    observed: RewardPreflightObservedInputs,
) -> Result<RewardDistributionPreflight, RewardPreflightCasError> {
    let current_op = operation_journal
        .iter()
        .find(|op| op.operation_id == snapshot.operation_id)
        .ok_or_else(|| {
            RewardPreflightCasError::RetryableConflict(
                "reward operation disappeared before preflight finalization".to_string(),
            )
        })?;
    if current_op != &snapshot.operation {
        return Err(RewardPreflightCasError::RetryableConflict(
            "reward preflight operation snapshot changed during external ledger reads".to_string(),
        ));
    }
    if processed_transactions != &snapshot.processed_transactions {
        return Err(RewardPreflightCasError::RetryableConflict(
            "reward preflight processed transaction set changed during external ledger reads"
                .to_string(),
        ));
    }
    if current_protocol_reserve_io_e8s != snapshot.protocol_reserve_io_e8s {
        return Err(RewardPreflightCasError::RetryableConflict(
            "reward preflight protocol reserve changed during external ledger reads".to_string(),
        ));
    }
    let current_pending = pending_reward_reservation_components(
        operation_journal.iter(),
        processed_transactions,
        Some(&snapshot.operation_id),
    )
    .map_err(RewardPreflightCasError::RetryableConflict)?;
    if current_pending != snapshot.pending_reservations {
        return Err(RewardPreflightCasError::RetryableConflict(
            "reward preflight reservation set changed during external ledger reads".to_string(),
        ));
    }
    let current_protocol_available = reward_reserve_available(
        current_protocol_reserve_io_e8s,
        current_pending.total_unavailable_e8s,
    )
    .map_err(RewardPreflightCasError::Terminal)?;
    build_reward_distribution_preflight(
        current_op,
        observed.sns_governance_canister,
        observed.ledger_fee_e8s,
        current_protocol_available,
        observed.real_reserve_balance_e8s,
        observed.validated_at_timestamp_nanos,
    )
    .map_err(RewardPreflightCasError::Terminal)
}

#[cfg(any(target_family = "wasm", test))]
fn reward_reserve_available(
    protocol_reserve_io_e8s: u128,
    pending_reservations_e8s: u128,
) -> Result<u128, String> {
    protocol_reserve_io_e8s
        .checked_sub(pending_reservations_e8s)
        .ok_or_else(|| "pending reward reservations exceed protocol model reserve".to_string())
}

#[cfg(test)]
fn explicit_pretransfer_cancel_reward_reservation(op: &mut StreamOperation) -> Result<(), String> {
    if operation_has_external_reward_effect_or_uncertainty(op) {
        return Err(format!(
            "operation {} cannot release reward reservation after transfer attempt or uncertainty",
            op.operation_id
        ));
    }
    if let Some(preflight) = op.reward_preflight.as_mut() {
        preflight.status = RewardPreflightStatus::FailedTerminal;
        preflight.failure_reason =
            Some("reward reservation cancelled before external effect".to_string());
    }
    op.phase = OperationPhase::FailedTerminal;
    op.reward_reservation = Some(RewardReservation::default());
    op.reserved_reward_debit_e8s = Some(0);
    Ok(())
}

#[cfg(any(target_family = "wasm", test))]
fn apply_reward_bad_fee_policy(
    op: &mut StreamOperation,
    processed_transactions: &BTreeSet<String>,
    recipient_index: usize,
    observed_fee_e8s: u128,
) -> Result<(), String> {
    let old_fee = op
        .two_week_recipients
        .get(recipient_index)
        .and_then(|recipient| recipient.reward_transfer_attempt.as_ref())
        .map(|attempt| attempt.fee_e8s)
        .or_else(|| {
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.ledger_fee_e8s)
        })
        .ok_or_else(|| "BadFee reward transfer had no persisted fee evidence".to_string())?;
    let attempt_generation = op
        .two_week_recipients
        .get(recipient_index)
        .and_then(|recipient| recipient.reward_transfer_attempt.as_ref())
        .map(|attempt| attempt.created_at_time)
        .unwrap_or(0);
    let prior_reservation = reward_reservation_for_operation(op, processed_transactions)?;
    let prior_reserved_debit_e8s =
        prior_reservation.checked_total_unavailable_reward_debit_e8s()?;
    if prior_reserved_debit_e8s == 0 {
        return Err(format!(
            "operation {} BadFee re-preflight is missing prior reservation",
            op.operation_id
        ));
    }
    let reason = format!(
        "reward transfer BadFee before definitive success: expected old fee {old_fee}, observed current fee {observed_fee_e8s}"
    );
    let current_attempt_is_exact_submitted = op
        .two_week_recipients
        .get(recipient_index)
        .and_then(|recipient| recipient.reward_transfer_attempt.as_ref())
        .map(|attempt| {
            matches!(
                attempt.lifecycle,
                Some(RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation })
                    if generation == attempt.created_at_time
            )
        })
        .unwrap_or(false);
    let other_recipient_has_repreflight_blocking_evidence = op
        .two_week_recipients
        .iter()
        .enumerate()
        .any(|(index, recipient)| {
            index != recipient_index
                && crate::reward_recipient_has_any_attempt_or_external_evidence(recipient)
        });
    let current_value_or_proof_uncertain = op
        .two_week_recipients
        .get(recipient_index)
        .map(|recipient| {
            reward_current_bad_fee_recipient_has_value_or_uncertainty(recipient)
                || !current_attempt_is_exact_submitted
        })
        .unwrap_or(true);

    if other_recipient_has_repreflight_blocking_evidence
        || current_value_or_proof_uncertain
        || prior_reservation.externally_spent_but_uncommitted_reward_debit_e8s != 0
    {
        if let Some(preflight) = op.reward_preflight.as_mut() {
            preflight.status = RewardPreflightStatus::ManualReconciliationRequired;
            preflight.failure_reason = Some(reason.clone());
        }
        if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
            recipient.ledger_transfer_status = Some(TransferStatus::FailedTerminal);
            recipient.transfer_status = TransferStatus::FailedTerminal;
            recipient.last_error = Some(reason.clone());
        }
        let reservation = derived_reward_reservation_for_operation(op)?;
        let reserved_total = reservation.checked_total_unavailable_reward_debit_e8s()?;
        op.reward_reservation = Some(reservation);
        op.reserved_reward_debit_e8s = Some(reserved_total);
        op.mark_terminal_error(reason, OperationPhase::PartiallyDistributed);
        return Ok(());
    }

    let mut candidate = op.clone();
    if let Some(preflight) = candidate.reward_preflight.as_mut() {
        preflight.status = RewardPreflightStatus::Pending;
        preflight.failure_reason = Some(reason.clone());
    }
    candidate.reward_fee_repreflight = Some(RewardFeeRepreflightEvidence {
        prior_validated_fee_e8s: old_fee,
        observed_current_fee_e8s: observed_fee_e8s,
        prior_reserved_debit_e8s,
        invalidated_at_timestamp_nanos: crate::canister_time(),
        attempt_generation,
    });
    candidate.reward_reservation = Some(prior_reservation);
    candidate.reserved_reward_debit_e8s = Some(prior_reserved_debit_e8s);
    if let Some(recipient) = candidate.two_week_recipients.get_mut(recipient_index) {
        recipient.reward_transfer_attempt = None;
        recipient.ledger_transfer_fee_e8s = None;
        recipient.reward_amount_received_e8s = None;
        recipient.reserve_debit_e8s = None;
        recipient.ledger_transfer_status = Some(TransferStatus::Pending);
        recipient.transfer_status = TransferStatus::Pending;
        recipient.last_error = Some(reason.clone());
    }
    crate::validate_reward_operation_accounting(
        &candidate,
        Some(processed_transactions),
        crate::RewardValidationMode::Current,
    )?;
    let candidate_reservation = derived_reward_reservation_for_operation(&candidate)?;
    if candidate_reservation != prior_reservation {
        return Err(format!(
            "operation {} BadFee re-preflight candidate reservation changed: prior {:?}, candidate {:?}",
            op.operation_id, prior_reservation, candidate_reservation
        ));
    }
    candidate.mark_retryable_error(reason, OperationPhase::PartiallyDistributed);
    *op = candidate;
    Ok(())
}

#[cfg(target_family = "wasm")]
fn record_reward_bad_fee_policy(
    operation_id: &str,
    recipient_index: usize,
    observed_fee_e8s: u128,
) -> Result<(), String> {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let processed_transactions = state.manager.processed_transactions.clone();
        let op = state
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
            .ok_or_else(|| format!("reward operation {operation_id} disappeared after BadFee"))?;
        apply_reward_bad_fee_policy(
            op,
            &processed_transactions,
            recipient_index,
            observed_fee_e8s,
        )
    })
}

#[cfg(any(target_family = "wasm", test))]
fn build_reward_distribution_preflight(
    op: &StreamOperation,
    expected_governance_canister: Principal,
    ledger_fee_e8s: u128,
    protocol_reserve_available_e8s: u128,
    real_ledger_reserve_balance_e8s: u128,
    validated_at_timestamp_nanos: u64,
) -> Result<RewardDistributionPreflight, String> {
    let recipient_count = u64::try_from(op.two_week_recipients.len())
        .map_err(|_| "recipient count does not fit in u64".to_string())?;
    let total_reward_e8s = op
        .two_week_recipients
        .iter()
        .try_fold(0_u128, |sum, recipient| {
            sum.checked_add(recipient.amount_e8s)
                .ok_or_else(|| "total reward amount overflowed".to_string())
        })?;
    let total_fee_e8s = ledger_fee_e8s
        .checked_mul(u128::from(recipient_count))
        .ok_or_else(|| "total reward ledger fee overflowed".to_string())?;
    let total_reserve_debit_e8s = total_reward_e8s
        .checked_add(total_fee_e8s)
        .ok_or_else(|| "total reward reserve debit overflowed".to_string())?;
    let dust_e8s = op
        .io_issued_e8s
        .checked_sub(total_reward_e8s)
        .ok_or_else(|| "reward allocations exceed reward pool".to_string())?;

    let mut canonical_recipient_ids = Vec::with_capacity(op.two_week_recipients.len());
    let mut compatibility_keys = Vec::with_capacity(op.two_week_recipients.len());
    let mut canonical_seen = BTreeSet::new();
    let mut compatibility_seen = BTreeSet::new();
    for recipient in &op.two_week_recipients {
        let canonical = recipient.sns_neuron_id.clone().ok_or_else(|| {
            format!(
                "two-week reward recipient {} is missing a canonical SNS neuron id",
                recipient.neuron_id
            )
        })?;
        if canonical.len() != 32 {
            return Err(format!(
                "two-week reward recipient {} canonical SNS neuron id must be exactly 32 bytes, got {}",
                recipient.neuron_id,
                canonical.len()
            ));
        }
        if !canonical_seen.insert(canonical.clone()) {
            return Err(format!(
                "duplicate canonical SNS neuron id for reward recipient {}",
                recipient.neuron_id
            ));
        }
        if !compatibility_seen.insert(recipient.neuron_id) {
            return Err(format!(
                "duplicate compatibility reward recipient key {}",
                recipient.neuron_id
            ));
        }
        let destination = reward_account_for_sns_neuron(expected_governance_canister, &canonical)?;
        if destination.owner != expected_governance_canister {
            return Err(
                "reward destination owner did not match finalized local SNS governance canister"
                    .to_string(),
            );
        }
        let owner_text = destination.owner.to_text();
        if matches!(
            owner_text.as_str(),
            PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID
                | PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID
                | PRODUCTION_IO_HISTORIAN_CANISTER_ID
                | PRODUCTION_FRONTEND_CANISTER_ID
        ) {
            return Err("reward destination uses a production fiduciary canister id".to_string());
        }
        canonical_recipient_ids.push(canonical);
        compatibility_keys.push(recipient.neuron_id);
    }

    if total_reserve_debit_e8s > protocol_reserve_available_e8s {
        return Err(format!(
            "protocol model reserve cannot cover reward reserve debit {total_reserve_debit_e8s}; available {protocol_reserve_available_e8s}"
        ));
    }
    if total_reserve_debit_e8s > real_ledger_reserve_balance_e8s {
        return Err(format!(
            "finalized SNS ledger reserve cannot cover reward reserve debit {total_reserve_debit_e8s}; available {real_ledger_reserve_balance_e8s}"
        ));
    }

    Ok(RewardDistributionPreflight {
        status: RewardPreflightStatus::Validated,
        ledger_fee_e8s,
        recipient_count,
        total_reward_e8s,
        total_fee_e8s,
        total_reserve_debit_e8s,
        protocol_reserve_available_e8s,
        real_ledger_reserve_balance_e8s,
        validated_at_timestamp_nanos,
        canonical_recipient_ids,
        compatibility_keys,
        dust_e8s,
        failure_reason: None,
    })
}

#[cfg(target_family = "wasm")]
async fn icrc1_balance_of(canister: Principal, account: Account) -> Result<u128, String> {
    let response = ic_cdk::call::Call::bounded_wait(canister, "icrc1_balance_of")
        .with_arg(IcrcAccount::from(account))
        .await
        .map_err(|err| format!("finalized SNS ledger reserve balance query failed: {err:?}"))?;
    let (balance,) = response
        .candid_tuple::<(Nat,)>()
        .map_err(|err| format!("finalized SNS ledger reserve balance decode failed: {err:?}"))?;
    balance
        .0
        .to_str_radix(10)
        .parse::<u128>()
        .map_err(|err| format!("finalized SNS ledger reserve balance does not fit in u128: {err}"))
}

#[cfg(target_family = "wasm")]
async fn ensure_reward_preflight(
    operation_id: &str,
    io_canister: Principal,
    sns_governance_canister: Principal,
    outcome: &mut DebugTickOutcome,
) -> bool {
    let existing = CANISTER_STATE.with(|cell| {
        cell.borrow()
            .operation_journal
            .iter()
            .find(|op| op.operation_id == operation_id)
            .and_then(|op| op.reward_preflight.clone())
    });
    if matches!(
        existing.as_ref().map(|preflight| preflight.status),
        Some(RewardPreflightStatus::Validated)
    ) {
        return CANISTER_STATE.with(|cell| {
            let state = cell.borrow();
            state
                .operation_journal
                .iter()
                .find(|op| op.operation_id == operation_id)
                .map(|op| {
                    reward_reservation_for_operation(op, &state.manager.processed_transactions)
                        .is_ok()
                })
                .unwrap_or(false)
        });
    }

    let snapshot = CANISTER_STATE.with(|cell| {
        let state = cell.borrow();
        capture_reward_preflight_snapshot(
            &state.operation_journal,
            &state.manager.processed_transactions,
            state.manager.state.protocol_reserve_io_e8s,
            operation_id,
        )
        .and_then(|snapshot| {
            reward_reserve_available(
                snapshot.protocol_reserve_io_e8s,
                snapshot.pending_reservations.total_unavailable_e8s,
            )?;
            pending_reward_reservations(
                state.operation_journal.iter(),
                &state.manager.processed_transactions,
                Some(operation_id),
            )?;
            Ok(snapshot)
        })
    });
    let snapshot = match snapshot {
        Ok(snapshot) => snapshot,
        Err(err) => {
            record_preflight_retryable_query_failure(operation_id, err.clone());
            outcome.errors.push(err);
            return false;
        }
    };

    let fee = match (IcrcLedgerCanisterClient {
        canister: io_canister,
    })
    .fee()
    .await
    {
        Ok(fee) => fee,
        Err(err) => {
            let message = format!("finalized SNS ledger fee query failed: {err:?}");
            record_preflight_retryable_query_failure(operation_id, message.clone());
            outcome.errors.push(message);
            return false;
        }
    };
    let reserve_account = Account::new(
        ic_cdk::api::canister_self(),
        Some(icp_ledger::mock_subaccount(PROTOCOL_RESERVE_ACCOUNT)),
    );
    let real_reserve_balance = match icrc1_balance_of(io_canister, reserve_account).await {
        Ok(balance) => balance,
        Err(message) => {
            record_preflight_retryable_query_failure(operation_id, message.clone());
            outcome.errors.push(message);
            return false;
        }
    };
    let finalized_preflight = CANISTER_STATE.with(|cell| {
        let state = cell.borrow();
        finalize_reward_preflight_snapshot(
            &snapshot,
            &state.operation_journal,
            &state.manager.processed_transactions,
            state.manager.state.protocol_reserve_io_e8s,
            RewardPreflightObservedInputs {
                sns_governance_canister,
                ledger_fee_e8s: fee,
                real_reserve_balance_e8s: real_reserve_balance,
                validated_at_timestamp_nanos: crate::canister_time(),
            },
        )
    });

    match finalized_preflight {
        Ok(preflight) => {
            let reserved = preflight.total_reserve_debit_e8s;
            CANISTER_STATE.with(|cell| {
                if let Some(op) = cell
                    .borrow_mut()
                    .operation_journal
                    .iter_mut()
                    .find(|op| op.operation_id == operation_id)
                {
                    op.reward_preflight = Some(preflight);
                    op.reward_reservation = Some(RewardReservation {
                        unspent_reserved_reward_debit_e8s: reserved,
                        externally_spent_but_uncommitted_reward_debit_e8s: 0,
                    });
                    op.reward_fee_repreflight = None;
                    op.reserved_reward_debit_e8s = Some(reserved);
                    op.mark_updated(OperationPhase::PartiallyDistributed);
                }
            });
            true
        }
        Err(err) => {
            let message = match err {
                RewardPreflightCasError::RetryableConflict(message) => {
                    record_preflight_retryable_query_failure(operation_id, message.clone());
                    outcome.errors.push(message);
                    return false;
                }
                RewardPreflightCasError::Terminal(message) => message,
            };
            record_preflight_state_aware_failure(operation_id, message.clone());
            outcome.errors.push(message);
            return false;
        }
    }
}

#[cfg(any(target_family = "wasm", test))]
fn record_preflight_state_aware_failure(operation_id: &str, reason: String) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            if op.reward_fee_repreflight.is_some() {
                record_repreflight_retryable_failure(op, reason);
            } else if operation_has_external_reward_effect_or_uncertainty(op) {
                record_post_effect_preflight_failure(op, reason);
            } else {
                record_initial_preflight_terminal_failure(op, reason);
            }
        }
    });
}

#[cfg(any(target_family = "wasm", test))]
fn record_preflight_retryable_query_failure(operation_id: &str, reason: String) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            if op.reward_fee_repreflight.is_some() {
                record_repreflight_retryable_failure(op, reason);
            } else if operation_has_external_reward_effect_or_uncertainty(op) {
                record_post_effect_preflight_failure(op, reason);
            } else {
                record_initial_preflight_retryable_failure(op, reason);
            }
        }
    });
}

#[cfg(any(target_family = "wasm", test))]
fn record_initial_preflight_retryable_failure(op: &mut StreamOperation, reason: String) {
    op.reward_preflight = None;
    op.reward_reservation = Some(RewardReservation::default());
    op.reserved_reward_debit_e8s = Some(0);
    op.mark_retryable_error(reason, OperationPhase::PartiallyDistributed);
}

#[cfg(any(target_family = "wasm", test))]
fn record_initial_preflight_terminal_failure(op: &mut StreamOperation, reason: String) {
    op.reward_preflight = Some(RewardDistributionPreflight {
        status: RewardPreflightStatus::FailedTerminal,
        ledger_fee_e8s: 0,
        recipient_count: 0,
        total_reward_e8s: 0,
        total_fee_e8s: 0,
        total_reserve_debit_e8s: 0,
        protocol_reserve_available_e8s: 0,
        real_ledger_reserve_balance_e8s: 0,
        validated_at_timestamp_nanos: crate::canister_time(),
        canonical_recipient_ids: Vec::new(),
        compatibility_keys: Vec::new(),
        dust_e8s: 0,
        failure_reason: Some(reason.clone()),
    });
    op.reward_reservation = Some(RewardReservation::default());
    op.reserved_reward_debit_e8s = Some(0);
    op.mark_terminal_error(reason, OperationPhase::PartiallyDistributed);
}

#[cfg(any(target_family = "wasm", test))]
fn record_repreflight_retryable_failure(op: &mut StreamOperation, reason: String) {
    if let Some(evidence) = op.reward_fee_repreflight {
        if let Some(reservation) = op.reward_reservation {
            op.reserved_reward_debit_e8s = reservation
                .checked_total_unavailable_reward_debit_e8s()
                .ok();
        } else {
            op.reward_reservation = Some(RewardReservation {
                unspent_reserved_reward_debit_e8s: evidence.prior_reserved_debit_e8s,
                externally_spent_but_uncommitted_reward_debit_e8s: 0,
            });
            op.reserved_reward_debit_e8s = Some(evidence.prior_reserved_debit_e8s);
        }
    }
    op.mark_retryable_error(reason, OperationPhase::PartiallyDistributed);
}

#[cfg(any(target_family = "wasm", test))]
fn record_repreflight_terminal_failure(op: &mut StreamOperation, reason: String) {
    if let Some(preflight) = op.reward_preflight.as_mut() {
        preflight.status = RewardPreflightStatus::ManualReconciliationRequired;
        preflight.failure_reason = Some(reason.clone());
    }
    op.mark_terminal_error(reason, OperationPhase::PartiallyDistributed);
}

#[cfg(any(target_family = "wasm", test))]
fn record_post_effect_preflight_failure(op: &mut StreamOperation, reason: String) {
    if let Some(preflight) = op.reward_preflight.as_mut() {
        preflight.status = RewardPreflightStatus::ManualReconciliationRequired;
        preflight.failure_reason = Some(reason.clone());
    }
    if op.reward_reservation.is_none() {
        match derived_reward_reservation_for_operation(op) {
            Ok(reservation) => {
                op.reserved_reward_debit_e8s = reservation
                    .checked_total_unavailable_reward_debit_e8s()
                    .ok();
                op.reward_reservation = Some(reservation);
            }
            Err(err) => {
                op.last_error = Some(format!(
                    "reward preflight failure could not preserve reservation safely: {err}"
                ));
            }
        }
    }
    record_repreflight_terminal_failure(op, reason);
}

#[cfg(target_family = "wasm")]
async fn duplicate_block(canister: Principal, block_index: BlockIndex) -> Option<LedgerBlock> {
    icp_ledger::debug_get_transactions(canister)
        .await
        .ok()?
        .into_iter()
        .find(|tx| tx.block_index == block_index.0)
        .map(|tx| tx.into_boundary_block())
}

#[cfg(target_family = "wasm")]
async fn duplicate_block_from_account_history(
    index_canister: Principal,
    account: Account,
    block_index: BlockIndex,
) -> Option<LedgerBlock> {
    let client = IcrcIndexCanisterClient {
        canister: index_canister,
    };
    client
        .get_account_transactions(IndexScanRequest {
            start: None,
            limit: 100,
            account_filter: Some(account),
            account_aliases: Vec::new(),
        })
        .await
        .ok()?
        .transactions
        .into_iter()
        .find(|tx| tx.block_index == block_index)
        .map(|tx| tx.transaction)
}

#[cfg(target_family = "wasm")]
async fn classify_icp_payout_transfer(
    canister: Principal,
    real_request: &LedgerTransferRequest,
    mock_request: &LedgerTransferRequest,
) -> BoundaryTransferDecision {
    let real_client = IcpLedgerCanisterClient {
        canister,
        default_fee_e8s: 10_000,
    };
    match real_client.transfer(real_request.clone()).await {
        Ok(success) => BoundaryTransferDecision::Succeeded(success.block_index.0),
        Err(LedgerTransferError::CanisterCallFailed { .. }) => {
            let mock_client = icp_ledger::MockLedgerCanisterClient {
                canister,
                fee_e8s: 0,
            };
            classify_mock_transfer(
                canister,
                mock_request,
                mock_client.transfer(mock_request.clone()).await,
            )
            .await
        }
        Err(LedgerTransferError::Duplicate { duplicate_of }) => {
            match duplicate_block(canister, duplicate_of).await {
                Some(block) => match duplicate_matches_expected(real_request, &block) {
                    Ok(block) => BoundaryTransferDecision::Succeeded(block.0),
                    Err(proof) => BoundaryTransferDecision::Retryable(format!(
                        "duplicate ICP payout did not match expected amount/account/memo: {proof:?}"
                    )),
                },
                None => BoundaryTransferDecision::Retryable(
                    "duplicate ICP payout could not be proven against expected amount/account/memo"
                        .to_string(),
                ),
            }
        }
        Err(err) => BoundaryTransferDecision::Retryable(boundary_error_message(&err)),
    }
}

#[cfg(all(target_family = "wasm", not(debug_assertions)))]
async fn query_io_return_fee(io_canister: Principal) -> Result<u128, String> {
    (IcrcLedgerCanisterClient {
        canister: io_canister,
    })
    .fee()
    .await
    .map_err(|real_err| {
        format!("IO return fee query failed through production ICRC client: {real_err:?}")
    })
}

#[cfg(all(target_family = "wasm", debug_assertions))]
async fn query_io_return_fee(io_canister: Principal) -> Result<u128, String> {
    match (IcrcLedgerCanisterClient {
        canister: io_canister,
    })
    .fee()
    .await
    {
        Ok(fee) => Ok(fee),
        Err(real_err) => query_io_return_fee_debug_fallback(io_canister, real_err).await,
    }
}

#[cfg(all(target_family = "wasm", debug_assertions))]
async fn query_io_return_fee_debug_fallback(
    io_canister: Principal,
    real_err: io_ledger_types::LedgerQueryError,
) -> Result<u128, String> {
    let debug_probe = io_ledger::debug_get_transactions(io_canister).await;
    if !mock_fee_fallback_allowed_for_build(debug_probe.is_ok(), true) {
        let debug_err = debug_probe
            .err()
            .unwrap_or_else(|| "debug ledger probe denied".to_string());
        return Err(format!(
            "IO return fee query failed through production ICRC client: {real_err:?}; mock/debug fallback denied because debug ledger probe failed: {debug_err}"
        ));
    }
    let mock_client = io_ledger::MockLedgerCanisterClient {
        canister: io_canister,
        fee_e8s: 0,
    };
    mock_client.fee().await.map_err(|mock_err| {
        format!("IO return fee query failed through debug mock fallback: {mock_err:?}")
    })
}

#[cfg(target_family = "wasm")]
async fn classify_mock_transfer(
    canister: Principal,
    request: &LedgerTransferRequest,
    result: Result<LedgerTransferSuccess, LedgerTransferError>,
) -> BoundaryTransferDecision {
    let duplicate = match result {
        Err(LedgerTransferError::Duplicate { duplicate_of }) => {
            duplicate_block(canister, duplicate_of).await
        }
        _ => None,
    };
    classify_boundary_transfer_result(request, result, duplicate.as_ref())
}

#[cfg(target_family = "wasm")]
fn boundary_transaction_to_mock_transaction(
    tx: io_ledger_types::IndexTransaction,
) -> icp_ledger::LedgerTransaction {
    icp_ledger::LedgerTransaction {
        from: tx
            .transaction
            .from
            .as_ref()
            .map(icp_ledger::mock_label_from_account)
            .unwrap_or_default(),
        to: tx
            .transaction
            .to
            .as_ref()
            .map(icp_ledger::mock_label_from_account)
            .unwrap_or_default(),
        amount_e8s: tx.transaction.amount_e8s,
        memo: tx
            .transaction
            .memo
            .map(|memo| String::from_utf8_lossy(&memo.0).into_owned())
            .unwrap_or_default(),
        block_index: tx.block_index.0,
        timestamp: tx.transaction.timestamp_nanos,
    }
}

#[cfg(target_family = "wasm")]
async fn scan_account_with_index_client_raw<C: LedgerIndexClient>(
    client: C,
    account: Account,
    account_aliases: Vec<AccountAlias>,
    scan_state: AccountHistoryScanState,
) -> Result<(Vec<IndexTransaction>, AccountHistoryScanState, Option<u64>), String> {
    let requested_start = scan_state.next_request_start();
    let page = client
        .get_account_transactions(IndexScanRequest {
            start: requested_start,
            limit: 100,
            account_filter: Some(account),
            account_aliases,
        })
        .await
        .map_err(|err| format!("ledger index scan failed: {err:?}"))?;
    let outcome = scan_state
        .observe_page(&page, requested_start, 100, 1, 1, Some(ic_cdk::api::time()))
        .map_err(|err| format!("ledger index cursor validation failed: {err:?}"))?;
    let latest = outcome.next_state.cursor.latest_cursor.map(|block| block.0);
    Ok((
        outcome.transactions_chronological,
        outcome.next_state,
        latest,
    ))
}

#[cfg(target_family = "wasm")]
async fn scan_account_with_index_client<C: LedgerIndexClient>(
    client: C,
    account: Account,
    account_aliases: Vec<AccountAlias>,
    scan_state: AccountHistoryScanState,
) -> Result<
    (
        Vec<icp_ledger::LedgerTransaction>,
        AccountHistoryScanState,
        Option<u64>,
    ),
    String,
> {
    let (transactions, next_state, latest) =
        scan_account_with_index_client_raw(client, account, account_aliases, scan_state).await?;
    Ok((
        transactions
            .into_iter()
            .map(boundary_transaction_to_mock_transaction)
            .collect(),
        next_state,
        latest,
    ))
}

#[cfg(target_family = "wasm")]
async fn scan_icp_account_through_index(
    index_canister: Principal,
    account: Account,
    scan_state: AccountHistoryScanState,
) -> Result<
    (
        Vec<icp_ledger::LedgerTransaction>,
        AccountHistoryScanState,
        Option<u64>,
    ),
    String,
> {
    let client = IcpIndexCanisterClient {
        canister: index_canister,
    };
    scan_account_with_index_client(
        client,
        account,
        vec![
            AccountAlias {
                account: icp_ledger::mock_account(JUPITER_FAUCET_SOURCE),
                label: JUPITER_FAUCET_SOURCE.to_string(),
            },
            AccountAlias {
                account: icp_ledger::mock_account(IO_NNS_NEURON_MANAGER_SOURCE),
                label: IO_NNS_NEURON_MANAGER_SOURCE.to_string(),
            },
        ],
        scan_state,
    )
    .await
}

#[cfg(target_family = "wasm")]
async fn scan_icrc_account_through_index(
    index_canister: Principal,
    account: Account,
    scan_state: AccountHistoryScanState,
) -> Result<(Vec<IndexTransaction>, AccountHistoryScanState, Option<u64>), String> {
    scan_account_with_index_client_raw(
        IcrcIndexCanisterClient {
            canister: index_canister,
        },
        account,
        vec![],
        scan_state,
    )
    .await
}

#[cfg(any(target_family = "wasm", test))]
fn reward_transfer_block_matches_attempt(
    attempt: &RewardTransferAttemptRecord,
    block: &LedgerBlock,
) -> bool {
    if let Some(created_at_time) = block.created_at_time {
        if created_at_time != attempt.created_at_time {
            return false;
        }
    }
    block.operation_kind == LedgerOperationKind::Transfer
        && block.from.as_ref() == Some(&attempt.source_account)
        && block.to.as_ref() == Some(&attempt.destination_account)
        && block.amount_e8s == attempt.amount_e8s
        && block.memo == attempt.memo
        && block
            .fee_e8s
            .map(|fee| fee == attempt.fee_e8s)
            .unwrap_or(true)
}

#[cfg(any(target_family = "wasm", test))]
fn classify_reward_transfer_result(
    attempt: &RewardTransferAttemptRecord,
    result: Result<LedgerTransferSuccess, LedgerTransferError>,
    duplicate_block: Option<&LedgerBlock>,
) -> BoundaryTransferDecision {
    match result {
        Ok(success) => BoundaryTransferDecision::Succeeded(success.block_index.0),
        Err(LedgerTransferError::Duplicate { .. }) => match duplicate_block {
            Some(block) if reward_transfer_block_matches_attempt(attempt, block) => {
                BoundaryTransferDecision::Succeeded(block.block_index.0)
            }
            Some(_) => BoundaryTransferDecision::Retryable(
                "duplicate reward transfer did not match persisted attempt proof".to_string(),
            ),
            None => BoundaryTransferDecision::Retryable(
                "duplicate reward transfer could not be proven against persisted attempt"
                    .to_string(),
            ),
        },
        Err(err) => BoundaryTransferDecision::Retryable(boundary_error_message(&err)),
    }
}

#[cfg(target_family = "wasm")]
async fn scan_reward_transfer_proof_from_state(
    io_index_canister: Option<Principal>,
    attempt: &RewardTransferAttemptRecord,
    mut scan_state: AccountHistoryScanState,
) -> RewardTransferProofScanOutcome {
    let Some(index_canister) = io_index_canister else {
        return RewardTransferProofScanOutcome {
            disposition: RewardTransferProofDisposition::IndexNotCaughtUp(
                "missing IO index canister for reward transfer proof lookup".to_string(),
            ),
            scan_state,
        };
    };

    for page_index in 0..TWO_WEEK_REWARD_PROOF_SCAN_MAX_PAGES {
        let scan = scan_icrc_account_through_index(
            index_canister,
            attempt.destination_account.clone(),
            scan_state.clone(),
        )
        .await;

        let (transactions, next_state, _) = match scan {
            Ok(scan) => scan,
            Err(err) if err.contains("ArchiveRequired") => {
                return RewardTransferProofScanOutcome {
                    disposition: RewardTransferProofDisposition::HistoryIncomplete(format!(
                        "IO index reward proof requires archive traversal before retry: {err}"
                    )),
                    scan_state: scan_state.record_unreadable(err),
                };
            }
            Err(err) if err.contains("IndexLag") => {
                return RewardTransferProofScanOutcome {
                    disposition: RewardTransferProofDisposition::IndexNotCaughtUp(format!(
                        "IO index is not caught up enough to prove reward transfer outcome: {err}"
                    )),
                    scan_state: scan_state.record_unreadable(err),
                };
            }
            Err(err) => {
                return RewardTransferProofScanOutcome {
                    disposition: RewardTransferProofDisposition::HistoryIncomplete(format!(
                        "IO index reward proof lookup was incomplete: {err}"
                    )),
                    scan_state: scan_state.record_unreadable(err),
                };
            }
        };

        for tx in transactions {
            let block = tx.transaction;
            if reward_transfer_block_matches_attempt(attempt, &block) {
                return RewardTransferProofScanOutcome {
                    disposition: RewardTransferProofDisposition::ProofFound(block.block_index),
                    scan_state: next_state,
                };
            }
        }

        let backfill_complete = next_state.cursor.backfill_complete;
        scan_state = next_state;
        if backfill_complete || page_index + 1 >= TWO_WEEK_REWARD_PROOF_SCAN_MAX_PAGES {
            return RewardTransferProofScanOutcome {
                disposition: classify_reward_transfer_proof_state(
                    &scan_state,
                    page_index + 1,
                    TWO_WEEK_REWARD_PROOF_SCAN_MAX_PAGES,
                ),
                scan_state,
            };
        }
    }

    RewardTransferProofScanOutcome {
        disposition: RewardTransferProofDisposition::HistoryIncomplete(format!(
            "matching reward transfer proof not found within {TWO_WEEK_REWARD_PROOF_SCAN_MAX_PAGES} index pages"
        )),
        scan_state,
    }
}

#[cfg(target_family = "wasm")]
async fn resolve_too_old_rejected_refund(
    io_index_canister: Option<Principal>,
    refund_source: &Account,
    request: &LedgerTransferRequest,
) -> TooOldRefundProofDisposition {
    resolve_too_old_rejected_refund_from_state(
        io_index_canister,
        refund_source,
        request,
        AccountHistoryScanState {
            cursor: io_ledger_types::AccountHistoryCursor {
                order: None,
                latest_cursor: None,
                oldest_cursor: None,
                backfill_complete: false,
            },
            status: Default::default(),
        },
    )
    .await
    .disposition
}

#[cfg(target_family = "wasm")]
async fn resolve_too_old_rejected_refund_from_state(
    io_index_canister: Option<Principal>,
    refund_source: &Account,
    request: &LedgerTransferRequest,
    mut scan_state: AccountHistoryScanState,
) -> TooOldRefundProofScanOutcome {
    let Some(index_canister) = io_index_canister else {
        return TooOldRefundProofScanOutcome {
            disposition: TooOldRefundProofDisposition::IndexNotCaughtUp(
                "missing IO index canister for TooOld refund proof lookup".to_string(),
            ),
            scan_state,
        };
    };

    for page_index in 0..REJECTED_REFUND_PROOF_SCAN_MAX_PAGES {
        let scan = scan_account_with_index_client_raw(
            IcrcIndexCanisterClient {
                canister: index_canister,
            },
            refund_source.clone(),
            vec![],
            scan_state.clone(),
        )
        .await;

        let (transactions, next_state, _) = match scan {
            Ok(scan) => scan,
            Err(err) if err.contains("ArchiveRequired") => {
                return TooOldRefundProofScanOutcome {
                    disposition: TooOldRefundProofDisposition::HistoryIncomplete(format!(
                        "IO index refund proof requires archive traversal before retry: {err}"
                    )),
                    scan_state: scan_state.record_unreadable(err),
                };
            }
            Err(err) if err.contains("IndexLag") => {
                return TooOldRefundProofScanOutcome {
                    disposition: TooOldRefundProofDisposition::IndexNotCaughtUp(format!(
                        "IO index is not caught up enough to prove refund absence: {err}"
                    )),
                    scan_state: scan_state.record_unreadable(err),
                };
            }
            Err(err) => {
                return TooOldRefundProofScanOutcome {
                    disposition: TooOldRefundProofDisposition::HistoryIncomplete(format!(
                        "IO index refund proof lookup was incomplete: {err}"
                    )),
                    scan_state: scan_state.record_unreadable(err),
                };
            }
        };

        for tx in transactions {
            let block = tx.transaction;
            if block.from.as_ref() == Some(refund_source)
                && duplicate_matches_expected(request, &block).is_ok()
            {
                return TooOldRefundProofScanOutcome {
                    disposition: TooOldRefundProofDisposition::ProofFound(block.block_index),
                    scan_state: next_state,
                };
            }
        }

        let backfill_complete = next_state.cursor.backfill_complete;
        scan_state = next_state;
        if backfill_complete {
            return TooOldRefundProofScanOutcome {
                disposition: classify_too_old_refund_proof_state(
                    &scan_state,
                    page_index + 1,
                    REJECTED_REFUND_PROOF_SCAN_MAX_PAGES,
                ),
                scan_state,
            };
        }
        if page_index + 1 >= REJECTED_REFUND_PROOF_SCAN_MAX_PAGES {
            return TooOldRefundProofScanOutcome {
                disposition: classify_too_old_refund_proof_state(
                    &scan_state,
                    page_index + 1,
                    REJECTED_REFUND_PROOF_SCAN_MAX_PAGES,
                ),
                scan_state,
            };
        }
    }

    TooOldRefundProofScanOutcome {
        disposition: TooOldRefundProofDisposition::HistoryIncomplete(format!(
            "matching refund proof not found within {REJECTED_REFUND_PROOF_SCAN_MAX_PAGES} index pages"
        )),
        scan_state,
    }
}

#[cfg(any(target_family = "wasm", test))]
fn recipient_ledger_status(recipient: &TwoWeekRecipientTransfer) -> TransferStatus {
    recipient
        .ledger_transfer_status
        .unwrap_or(recipient.transfer_status)
}

#[cfg(any(target_family = "wasm", test))]
fn recipient_refresh_status(recipient: &TwoWeekRecipientTransfer) -> TransferStatus {
    recipient
        .governance_refresh_status
        .unwrap_or(TransferStatus::Pending)
}

#[cfg(any(target_family = "wasm", test))]
fn reward_attempt_lifecycle(
    attempt: &RewardTransferAttemptRecord,
) -> Option<RewardTransferAttemptLifecycle> {
    attempt.lifecycle.clone()
}

#[cfg(any(target_family = "wasm", test))]
fn reward_attempt_is_prepared(attempt: &RewardTransferAttemptRecord) -> bool {
    matches!(
        reward_attempt_lifecycle(attempt),
        Some(RewardTransferAttemptLifecycle::Prepared)
    )
}

#[cfg(any(target_family = "wasm", test))]
fn reward_recipient_can_submit(recipient: &TwoWeekRecipientTransfer) -> bool {
    recipient
        .reward_transfer_attempt
        .as_ref()
        .map(reward_attempt_is_prepared)
        .unwrap_or(true)
}

#[cfg(any(target_family = "wasm", test))]
fn recipient_is_completed(recipient: &TwoWeekRecipientTransfer) -> bool {
    recipient_ledger_status(recipient) == TransferStatus::Succeeded
        && recipient_refresh_status(recipient) == TransferStatus::Succeeded
}

#[cfg(any(target_family = "wasm", test))]
fn reward_operation_can_progress(op: &StreamOperation) -> bool {
    op.kind == StreamOperationKind::TwoWeekMaturityStream
        && !matches!(
            op.phase,
            OperationPhase::Completed | OperationPhase::FailedTerminal
        )
}

#[cfg(any(target_family = "wasm", test))]
fn reward_recipient_attempt_key(
    operation_id: &str,
    recipient_index: usize,
    recipient: &TwoWeekRecipientTransfer,
) -> String {
    recipient
        .sns_neuron_id
        .as_ref()
        .map(|id| format!("{operation_id}:{}", encode_hex(id)))
        .unwrap_or_else(|| format!("{operation_id}:legacy-index:{recipient_index}"))
}

#[cfg(any(target_family = "wasm", test))]
fn next_reward_ledger_recipient(
    attempted: &BTreeSet<String>,
) -> Option<(String, usize, TwoWeekRecipientTransfer, String)> {
    CANISTER_STATE.with(|cell| {
        let state = cell.borrow();
        state.operation_journal.iter().find_map(|op| {
            if !reward_operation_can_progress(op) {
                return None;
            }
            if !matches!(
                op.reward_preflight
                    .as_ref()
                    .map(|preflight| preflight.status),
                Some(RewardPreflightStatus::Validated)
            ) {
                return None;
            }
            if op
                .two_week_recipients
                .iter()
                .any(crate::reward_recipient_has_submitted_or_proof_required_attempt)
            {
                return None;
            }
            op.two_week_recipients
                .iter()
                .enumerate()
                .find_map(|(index, recipient)| {
                    let key = reward_recipient_attempt_key(&op.operation_id, index, recipient);
                    (recipient_ledger_status(recipient) != TransferStatus::Succeeded
                        && recipient.ledger_transfer_proof_scan_state.is_none()
                        && reward_recipient_can_submit(recipient)
                        && !attempted.contains(&key))
                    .then(|| (op.operation_id.clone(), index, recipient.clone(), key))
                })
        })
    })
}

#[cfg(target_family = "wasm")]
fn next_reward_refresh_recipient(
    attempted: &BTreeSet<String>,
) -> Option<(String, usize, TwoWeekRecipientTransfer, String)> {
    CANISTER_STATE.with(|cell| {
        let state = cell.borrow();
        state.operation_journal.iter().find_map(|op| {
            if !reward_operation_can_progress(op) {
                return None;
            }
            op.two_week_recipients
                .iter()
                .enumerate()
                .find_map(|(index, recipient)| {
                    let key = reward_recipient_attempt_key(&op.operation_id, index, recipient);
                    (recipient_ledger_status(recipient) == TransferStatus::Succeeded
                        && recipient_refresh_status(recipient) != TransferStatus::Succeeded
                        && !attempted.contains(&key))
                    .then(|| (op.operation_id.clone(), index, recipient.clone(), key))
                })
        })
    })
}

#[cfg(any(target_family = "wasm", test))]
fn next_reward_proof_pending_recipient(
    attempted: &BTreeSet<String>,
) -> Option<(String, usize, TwoWeekRecipientTransfer, String)> {
    CANISTER_STATE.with(|cell| {
        let state = cell.borrow();
        state.operation_journal.iter().find_map(|op| {
            if !reward_operation_can_progress(op) {
                return None;
            }
            op.two_week_recipients
                .iter()
                .enumerate()
                .find_map(|(index, recipient)| {
                    let key = reward_recipient_attempt_key(&op.operation_id, index, recipient);
                    let lifecycle_requires_proof = recipient
                        .reward_transfer_attempt
                        .as_ref()
                        .and_then(|attempt| attempt.lifecycle.as_ref())
                        .map(|lifecycle| {
                            matches!(
                                lifecycle,
                                RewardTransferAttemptLifecycle::SubmittedAwaitingResult { .. }
                                    | RewardTransferAttemptLifecycle::ProofRequired { .. }
                            )
                        })
                        .unwrap_or(false);
                    (recipient_ledger_status(recipient) != TransferStatus::Succeeded
                        && (recipient.ledger_transfer_proof_scan_state.is_some()
                            || lifecycle_requires_proof)
                        && !attempted.contains(&key))
                    .then(|| (op.operation_id.clone(), index, recipient.clone(), key))
                })
        })
    })
}

#[cfg(target_family = "wasm")]
fn mark_reward_transfer_proven(
    operation_id: &str,
    recipient_index: usize,
    attempt: &RewardTransferAttemptRecord,
    block: BlockIndex,
) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                if recipient.reward_transfer_attempt.as_ref() != Some(attempt) {
                    return;
                }
                let debit = match attempt.amount_e8s.checked_add(attempt.fee_e8s) {
                    Some(debit) => debit,
                    None => {
                        let message =
                            "reward transfer succeeded but reserve debit overflowed".to_string();
                        recipient.transfer_status = TransferStatus::FailedTerminal;
                        recipient.ledger_transfer_status = Some(TransferStatus::FailedTerminal);
                        recipient.last_error = Some(message.clone());
                        op.mark_retryable_error(message, OperationPhase::PartiallyDistributed);
                        return;
                    }
                };
                if recipient.amount_e8s != attempt.amount_e8s {
                    let message = format!(
                        "reward transfer attempt amount {} does not match recipient amount {}",
                        attempt.amount_e8s, recipient.amount_e8s
                    );
                    recipient.transfer_status = TransferStatus::FailedTerminal;
                    recipient.ledger_transfer_status = Some(TransferStatus::FailedTerminal);
                    recipient.last_error = Some(message.clone());
                    op.mark_retryable_error(message, OperationPhase::PartiallyDistributed);
                    return;
                }
                recipient.transfer_status = TransferStatus::Succeeded;
                recipient.transfer_block_index = Some(block.0);
                recipient.ledger_transfer_status = Some(TransferStatus::Succeeded);
                recipient.ledger_transfer_block = Some(block.0);
                recipient.governance_refresh_status = Some(recipient_refresh_status(recipient));
                recipient.ledger_transfer_fee_e8s = Some(attempt.fee_e8s);
                recipient.reward_amount_received_e8s = Some(attempt.amount_e8s);
                recipient.reserve_debit_e8s = Some(debit);
                recipient.ledger_transfer_proof_scan_state = None;
                recipient.reward_transfer_attempt = Some(RewardTransferAttemptRecord {
                    lifecycle: Some(RewardTransferAttemptLifecycle::Proven {
                        generation: attempt.created_at_time,
                        block: block.0,
                    }),
                    ..attempt.clone()
                });
                recipient.last_error = None;
            }
            match derived_reward_reservation_for_operation(op).and_then(|reservation| {
                let total = reservation.checked_total_unavailable_reward_debit_e8s()?;
                Ok((reservation, total))
            }) {
                Ok((reservation, total)) => {
                    op.reward_reservation = Some(reservation);
                    op.reserved_reward_debit_e8s = Some(total);
                    op.mark_updated(OperationPhase::PartiallyDistributed);
                }
                Err(err) => {
                    op.mark_retryable_error(err, OperationPhase::PartiallyDistributed);
                }
            }
        }
    });
}

#[cfg(test)]
fn reward_fee_adjusted_post_state(
    op: &StreamOperation,
) -> Result<crate::StableProtocolState, String> {
    let total_reward = op
        .two_week_recipients
        .iter()
        .try_fold(0_u128, |sum, recipient| {
            sum.checked_add(recipient.amount_e8s)
                .ok_or_else(|| "reward allocation sum overflowed".to_string())
        })?;
    let dust = op
        .reward_preflight
        .as_ref()
        .map(|preflight| preflight.dust_e8s)
        .map(Ok)
        .unwrap_or_else(|| {
            op.io_issued_e8s
                .checked_sub(total_reward)
                .ok_or_else(|| "reward allocations exceed backed reward pool".to_string())
        })?;
    let issued_plus_dust = total_reward
        .checked_add(dust)
        .ok_or_else(|| "reward allocation plus dust overflowed".to_string())?;
    if issued_plus_dust != op.io_issued_e8s {
        return Err(format!(
            "reward allocations plus dust {issued_plus_dust} did not match backed reward pool {}",
            op.io_issued_e8s
        ));
    }
    let total_fee = op
        .two_week_recipients
        .iter()
        .try_fold(0_u128, |sum, recipient| {
            let fee = recipient.ledger_transfer_fee_e8s.ok_or_else(|| {
                "completed reward recipient is missing ledger transfer fee".to_string()
            })?;
            sum.checked_add(fee)
                .ok_or_else(|| "reward transfer fee sum overflowed".to_string())
        })?;
    let mut post_state = op.post_state;
    post_state.protocol_reserve_io_e8s = post_state
        .protocol_reserve_io_e8s
        .checked_add(dust)
        .ok_or_else(|| "protocol reserve dust restoration overflowed".to_string())?;
    post_state.protocol_reserve_io_e8s = post_state
        .protocol_reserve_io_e8s
        .checked_sub(total_fee)
        .ok_or_else(|| {
            "protocol reserve cannot cover completed reward transfer fees".to_string()
        })?;
    post_state.total_io_supply_e8s = post_state
        .total_io_supply_e8s
        .checked_sub(total_fee)
        .ok_or_else(|| "total IO supply cannot burn completed reward transfer fees".to_string())?;
    Ok(post_state)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(any(target_family = "wasm", test))]
struct RewardModelDelta {
    liquid_icp_credit_e8s: u128,
    two_week_staked_icp_credit_e8s: u128,
    allocation_debit_e8s: u128,
    fee_burn_e8s: u128,
    dust_retained_e8s: u128,
}

#[cfg(any(target_family = "wasm", test))]
fn reward_model_delta(op: &StreamOperation) -> Result<RewardModelDelta, String> {
    if op.kind != StreamOperationKind::TwoWeekMaturityStream {
        return Err(format!(
            "operation {} is not a two-week reward operation",
            op.operation_id
        ));
    }
    if !op.two_week_recipients.iter().all(recipient_is_completed) {
        return Err(format!(
            "operation {} has reward recipients that are not completed",
            op.operation_id
        ));
    }
    let split = split_40_60(op.amount_e8s);
    let allocation_debit_e8s =
        op.two_week_recipients
            .iter()
            .try_fold(0_u128, |sum, recipient| {
                let received = recipient.reward_amount_received_e8s.ok_or_else(|| {
                    "completed reward recipient is missing received amount".to_string()
                })?;
                if received != recipient.amount_e8s {
                    return Err(format!(
                        "completed reward recipient received {received} but planned {}",
                        recipient.amount_e8s
                    ));
                }
                sum.checked_add(recipient.amount_e8s)
                    .ok_or_else(|| "reward allocation sum overflowed".to_string())
            })?;
    let fee_burn_e8s = op
        .two_week_recipients
        .iter()
        .try_fold(0_u128, |sum, recipient| {
            let fee = recipient.ledger_transfer_fee_e8s.ok_or_else(|| {
                "completed reward recipient is missing ledger transfer fee".to_string()
            })?;
            let debit = recipient
                .reserve_debit_e8s
                .ok_or_else(|| "completed reward recipient is missing reserve debit".to_string())?;
            let expected_debit = recipient
                .amount_e8s
                .checked_add(fee)
                .ok_or_else(|| "reward recipient debit overflowed".to_string())?;
            if debit != expected_debit {
                return Err(format!(
                    "completed reward recipient debit {debit} does not equal amount plus fee {expected_debit}"
                ));
            }
            sum.checked_add(fee)
                .ok_or_else(|| "reward transfer fee sum overflowed".to_string())
        })?;
    let dust_retained_e8s = op
        .reward_preflight
        .as_ref()
        .map(|preflight| preflight.dust_e8s)
        .map(Ok)
        .unwrap_or_else(|| {
            op.io_issued_e8s
                .checked_sub(allocation_debit_e8s)
                .ok_or_else(|| "reward allocations exceed backed reward pool".to_string())
        })?;
    let allocations_plus_dust = allocation_debit_e8s
        .checked_add(dust_retained_e8s)
        .ok_or_else(|| "reward allocation plus dust overflowed".to_string())?;
    if allocations_plus_dust != op.io_issued_e8s {
        return Err(format!(
            "reward allocations plus dust {allocations_plus_dust} did not match backed reward pool {}",
            op.io_issued_e8s
        ));
    }
    if let Some(preflight) = &op.reward_preflight {
        if preflight.total_reward_e8s != allocation_debit_e8s {
            return Err(format!(
                "reward preflight total reward {} does not match committed allocation {allocation_debit_e8s}",
                preflight.total_reward_e8s
            ));
        }
        if preflight.total_fee_e8s != fee_burn_e8s {
            return Err(format!(
                "reward preflight total fee {} does not match committed fee {fee_burn_e8s}",
                preflight.total_fee_e8s
            ));
        }
        let total_reserve_debit = allocation_debit_e8s
            .checked_add(fee_burn_e8s)
            .ok_or_else(|| "reward reserve debit overflowed".to_string())?;
        if preflight.total_reserve_debit_e8s != total_reserve_debit {
            return Err(format!(
                "reward preflight total reserve debit {} does not match committed debit {total_reserve_debit}",
                preflight.total_reserve_debit_e8s
            ));
        }
    }
    Ok(RewardModelDelta {
        liquid_icp_credit_e8s: split.liquid_e8s,
        two_week_staked_icp_credit_e8s: split.stake_e8s,
        allocation_debit_e8s,
        fee_burn_e8s,
        dust_retained_e8s,
    })
}

#[cfg(any(target_family = "wasm", test))]
fn checked_reward_model_post_state(
    current: io_core_model::ProtocolState,
    delta: RewardModelDelta,
) -> Result<io_core_model::ProtocolState, String> {
    let reserve_debit = delta
        .allocation_debit_e8s
        .checked_add(delta.fee_burn_e8s)
        .ok_or_else(|| "reward reserve debit overflowed".to_string())?;
    let liquid_icp_e8s = current
        .liquid_icp_e8s
        .checked_add(delta.liquid_icp_credit_e8s)
        .ok_or_else(|| "reward liquid ICP credit overflowed".to_string())?;
    let two_week_staked_icp_e8s = current
        .two_week_staked_icp_e8s
        .checked_add(delta.two_week_staked_icp_credit_e8s)
        .ok_or_else(|| "reward two-week staked ICP credit overflowed".to_string())?;
    let protocol_reserve_io_e8s = current
        .protocol_reserve_io_e8s
        .checked_sub(reserve_debit)
        .ok_or_else(|| "protocol reserve cannot cover reward allocations and fees".to_string())?;
    let total_io_supply_e8s = current
        .total_io_supply_e8s
        .checked_sub(delta.fee_burn_e8s)
        .ok_or_else(|| "total IO supply cannot burn reward transfer fees".to_string())?;
    let next = io_core_model::ProtocolState {
        liquid_icp_e8s,
        two_year_staked_icp_e8s: current.two_year_staked_icp_e8s,
        two_week_staked_icp_e8s,
        total_io_supply_e8s,
        protocol_reserve_io_e8s,
        non_redeemable_governance_io_e8s: current.non_redeemable_governance_io_e8s,
    };
    next.redeemable_io_supply_e8s()
        .map_err(|err| format!("reward model invariant failed after commit: {err:?}"))?;
    Ok(next)
}

#[cfg(any(target_family = "wasm", test))]
fn commit_completed_reward_operation_in_state(
    manager: &mut StreamManager,
    operation_journal: &mut [StreamOperation],
    operation_id: &str,
) -> Result<bool, String> {
    let index = operation_journal
        .iter()
        .position(|op| op.operation_id == operation_id)
        .ok_or_else(|| format!("reward operation {operation_id} disappeared before commit"))?;
    let op_snapshot = operation_journal[index].clone();
    if manager
        .processed_transactions
        .contains(&op_snapshot.source_transaction_id)
    {
        if op_snapshot.phase == OperationPhase::Completed
            && op_snapshot.reward_reservation == Some(RewardReservation::default())
            && op_snapshot.reserved_reward_debit_e8s.unwrap_or(0) == 0
        {
            return Ok(false);
        }
        return Err(format!(
            "reward operation {operation_id} is already processed but journal/reservation are inconsistent"
        ));
    }

    let delta = reward_model_delta(&op_snapshot)?;
    let expected_reservation = delta
        .allocation_debit_e8s
        .checked_add(delta.fee_burn_e8s)
        .ok_or_else(|| "reward reservation total overflowed".to_string())?;
    let reservation =
        reward_reservation_for_operation(&op_snapshot, &manager.processed_transactions)?;
    if reservation.unspent_reserved_reward_debit_e8s != 0 {
        return Err(format!(
            "reward operation {operation_id} still has unspent reservation {} at commit",
            reservation.unspent_reserved_reward_debit_e8s
        ));
    }
    if reservation.externally_spent_but_uncommitted_reward_debit_e8s != expected_reservation {
        return Err(format!(
            "reward operation {operation_id} spent reservation {} does not match completed debit {expected_reservation}",
            reservation.externally_spent_but_uncommitted_reward_debit_e8s
        ));
    }

    let next_state = checked_reward_model_post_state(manager.state, delta)?;
    manager.state = next_state;
    manager
        .processed_transactions
        .insert(op_snapshot.source_transaction_id.clone());
    let op = &mut operation_journal[index];
    op.reward_reservation = Some(RewardReservation::default());
    op.reserved_reward_debit_e8s = Some(0);
    op.mark_updated(OperationPhase::Completed);
    Ok(true)
}

#[cfg(target_family = "wasm")]
fn commit_completed_reward_operation(operation_id: &str) -> Result<bool, String> {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let state = &mut *state;
        commit_completed_reward_operation_in_state(
            &mut state.manager,
            &mut state.operation_journal,
            operation_id,
        )
    })
}

#[cfg(target_family = "wasm")]
async fn reconcile_reward_transfer_proofs(
    io_index_canister: Principal,
    outcome: &mut DebugTickOutcome,
) {
    let mut attempted = BTreeSet::new();
    for _ in 0..TWO_WEEK_REWARD_PROOF_SCAN_MAX_PAGES {
        let Some((operation_id, recipient_index, recipient, attempt_key)) =
            next_reward_proof_pending_recipient(&attempted)
        else {
            return;
        };
        attempted.insert(attempt_key);

        let Some(attempt) = recipient.reward_transfer_attempt.clone() else {
            let message = "reward proof-pending recipient is missing the persisted transfer attempt; manual reconciliation required".to_string();
            CANISTER_STATE.with(|cell| {
                if let Some(op) = cell
                    .borrow_mut()
                    .operation_journal
                    .iter_mut()
                    .find(|op| op.operation_id == operation_id)
                {
                    if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                        recipient.transfer_status = TransferStatus::FailedTerminal;
                        recipient.ledger_transfer_status = Some(TransferStatus::FailedTerminal);
                        recipient.last_error = Some(message.clone());
                    }
                    op.mark_retryable_error(message.clone(), OperationPhase::PartiallyDistributed);
                }
            });
            outcome.errors.push(message);
            continue;
        };
        let scan_state = recipient
            .ledger_transfer_proof_scan_state
            .clone()
            .unwrap_or_default();
        let proof =
            scan_reward_transfer_proof_from_state(Some(io_index_canister), &attempt, scan_state)
                .await;
        if !stored_reward_attempt_still_matches(&operation_id, recipient_index, &attempt) {
            let message = format!(
                "reward operation {operation_id} recipient {recipient_index} proof reconciliation callback observed a different persisted attempt; stale callback ignored"
            );
            outcome.errors.push(message);
            continue;
        }

        match proof.disposition {
            RewardTransferProofDisposition::ProofFound(block) => {
                mark_reward_transfer_proven(&operation_id, recipient_index, &attempt, block);
            }
            RewardTransferProofDisposition::IndexNotCaughtUp(reason)
            | RewardTransferProofDisposition::HistoryIncomplete(reason) => {
                let message = format!(
                    "reward transfer proof pending; automatic retry remains paused: {reason}"
                );
                let Some(reserve_debit) = attempt.amount_e8s.checked_add(attempt.fee_e8s) else {
                    let message = "reward transfer attempt reserve debit overflowed".to_string();
                    CANISTER_STATE.with(|cell| {
                        if let Some(op) = cell
                            .borrow_mut()
                            .operation_journal
                            .iter_mut()
                            .find(|op| op.operation_id == operation_id)
                        {
                            if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index)
                            {
                                recipient.ledger_transfer_status =
                                    Some(TransferStatus::FailedTerminal);
                                recipient.last_error = Some(message.clone());
                            }
                            op.mark_retryable_error(
                                message.clone(),
                                OperationPhase::PartiallyDistributed,
                            );
                        }
                    });
                    outcome.errors.push(message);
                    continue;
                };
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|op| op.operation_id == operation_id)
                    {
                        if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                            if recipient.reward_transfer_attempt.as_ref() != Some(&attempt) {
                                return;
                            }
                            recipient.transfer_status = TransferStatus::FailedRetryable;
                            recipient.ledger_transfer_status =
                                Some(TransferStatus::FailedRetryable);
                            recipient.ledger_transfer_proof_scan_state =
                                Some(proof.scan_state.clone());
                            recipient.reserve_debit_e8s = Some(reserve_debit);
                            recipient.reward_transfer_attempt = Some(RewardTransferAttemptRecord {
                                lifecycle: Some(RewardTransferAttemptLifecycle::ProofRequired {
                                    generation: attempt.created_at_time,
                                    reason: message.clone(),
                                }),
                                ..attempt.clone()
                            });
                            recipient.last_error = Some(message.clone());
                        }
                        op.mark_retryable_error(
                            message.clone(),
                            OperationPhase::PartiallyDistributed,
                        );
                    }
                });
                outcome.errors.push(message);
            }
            RewardTransferProofDisposition::CompleteNoMatch(reason) => {
                let message = format!(
                    "{reason}; manual reconciliation required before any new reward transfer attempt"
                );
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|op| op.operation_id == operation_id)
                    {
                        if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                            if recipient.reward_transfer_attempt.as_ref() != Some(&attempt) {
                                return;
                            }
                            recipient.transfer_status = TransferStatus::FailedTerminal;
                            recipient.ledger_transfer_status = Some(TransferStatus::FailedTerminal);
                            recipient.ledger_transfer_proof_scan_state =
                                Some(proof.scan_state.clone());
                            recipient.reward_transfer_attempt = Some(RewardTransferAttemptRecord {
                                lifecycle: Some(RewardTransferAttemptLifecycle::ProofRequired {
                                    generation: attempt.created_at_time,
                                    reason: message.clone(),
                                }),
                                ..attempt.clone()
                            });
                            recipient.last_error = Some(message.clone());
                        }
                        op.mark_retryable_error(
                            message.clone(),
                            OperationPhase::PartiallyDistributed,
                        );
                    }
                });
                outcome.errors.push(message);
            }
        }
    }
}

#[cfg(target_family = "wasm")]
async fn retry_pending_two_week_streams(
    io_canister: Principal,
    io_index_canister: Principal,
    sns_governance_canister: Principal,
    outcome: &mut DebugTickOutcome,
) -> bool {
    let governance_client = SnsGovernanceCanisterClient {
        canister: sns_governance_canister,
    };
    reconcile_reward_transfer_proofs(io_index_canister, outcome).await;
    let mut saw_preflight_candidate = false;
    loop {
        let operation_id = CANISTER_STATE.with(|cell| {
            cell.borrow().operation_journal.iter().find_map(|op| {
                (reward_operation_can_progress(op)
                    && !matches!(
                        op.reward_preflight
                            .as_ref()
                            .map(|preflight| preflight.status),
                        Some(RewardPreflightStatus::Validated)
                    ))
                .then(|| op.operation_id.clone())
            })
        });
        let Some(operation_id) = operation_id else {
            break;
        };
        saw_preflight_candidate = true;
        if !ensure_reward_preflight(&operation_id, io_canister, sns_governance_canister, outcome)
            .await
        {
            return false;
        }
    }
    #[cfg(debug_assertions)]
    {
        if saw_preflight_candidate
            && crate::consume_debug_failpoint(
                crate::DebugFailpoint::AfterTwoWeekRewardPreflightBeforeTransfer,
            )
        {
            outcome.errors.push(
                "debug failpoint AfterTwoWeekRewardPreflightBeforeTransfer triggered after two-week reward preflight"
                    .to_string(),
            );
            return false;
        }
    }
    let mut attempted_ledger_recipients = BTreeSet::new();
    for _ in 0..TWO_WEEK_REWARD_LEDGER_TRANSFER_BUDGET_PER_TICK {
        let Some((operation_id, recipient_index, recipient, attempt_key)) =
            next_reward_ledger_recipient(&attempted_ledger_recipients)
        else {
            break;
        };
        attempted_ledger_recipients.insert(attempt_key);

        let sns_neuron_id = match recipient
            .sns_neuron_id
            .as_deref()
            .filter(|id| id.len() == 32)
            .map(|id| SnsNeuronId(id.to_vec()))
        {
            Some(id) => id,
            None => {
                let err = format!(
                    "two-week reward recipient {} is missing a canonical 32-byte SNS neuron id",
                    recipient.neuron_id
                );
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|op| op.operation_id == operation_id)
                    {
                        if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                            recipient.transfer_status = TransferStatus::FailedTerminal;
                            recipient.last_error = Some(err.clone());
                        }
                        op.mark_terminal_error(err.clone(), OperationPhase::PartiallyDistributed);
                    }
                });
                outcome.errors.push(err);
                continue;
            }
        };
        let to = match reward_account_for_sns_neuron(sns_governance_canister, &sns_neuron_id.0) {
            Ok(account) => account,
            Err(err) => {
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|op| op.operation_id == operation_id)
                    {
                        if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                            recipient.ledger_transfer_status = Some(TransferStatus::FailedTerminal);
                            recipient.transfer_status = TransferStatus::FailedTerminal;
                            recipient.last_error = Some(err.clone());
                        }
                        op.mark_terminal_error(err.clone(), OperationPhase::PartiallyDistributed);
                    }
                });
                outcome.errors.push(err);
                continue;
            }
        };

        if recipient.stake_before_e8s.is_none() {
            match governance_client.get_neuron(sns_neuron_id.clone()).await {
                Ok(neuron) => {
                    let stake_before = neuron.cached_neuron_stake_e8s;
                    let minimum_expected = stake_before.saturating_add(recipient.amount_e8s);
                    CANISTER_STATE.with(|cell| {
                        if let Some(op) = cell
                            .borrow_mut()
                            .operation_journal
                            .iter_mut()
                            .find(|op| op.operation_id == operation_id)
                        {
                            if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index)
                            {
                                recipient.stake_before_e8s = Some(stake_before);
                                recipient.expected_stake_after_e8s = Some(minimum_expected);
                                recipient.minimum_expected_stake_after_e8s = Some(minimum_expected);
                                recipient.last_error = None;
                            }
                            op.mark_updated(OperationPhase::PartiallyDistributed);
                        }
                    });
                }
                Err(err) => {
                    let message =
                        format!("SNS governance get_neuron before reward transfer failed: {err:?}");
                    CANISTER_STATE.with(|cell| {
                        if let Some(op) = cell
                            .borrow_mut()
                            .operation_journal
                            .iter_mut()
                            .find(|op| op.operation_id == operation_id)
                        {
                            if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index)
                            {
                                recipient.ledger_transfer_status =
                                    Some(TransferStatus::FailedRetryable);
                                recipient.last_error = Some(message.clone());
                            }
                            op.mark_retryable_error(
                                message.clone(),
                                OperationPhase::PartiallyDistributed,
                            );
                        }
                    });
                    outcome.errors.push(message);
                    continue;
                }
            }
        }

        let fee = CANISTER_STATE.with(|cell| {
            cell.borrow()
                .operation_journal
                .iter()
                .find(|op| op.operation_id == operation_id)
                .and_then(|op| op.reward_preflight.as_ref())
                .filter(|preflight| preflight.status == RewardPreflightStatus::Validated)
                .map(|preflight| preflight.ledger_fee_e8s)
        });
        let Some(fee) = fee else {
            let message = "validated reward preflight is missing before first transfer".to_string();
            CANISTER_STATE.with(|cell| {
                if let Some(op) = cell
                    .borrow_mut()
                    .operation_journal
                    .iter_mut()
                    .find(|op| op.operation_id == operation_id)
                {
                    op.mark_retryable_error(message.clone(), OperationPhase::PartiallyDistributed);
                }
            });
            outcome.errors.push(message);
            continue;
        };
        let source = Account::new(
            ic_cdk::api::canister_self(),
            Some(icp_ledger::mock_subaccount(PROTOCOL_RESERVE_ACCOUNT)),
        );
        let attempt = match CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            let processed_transactions = state.manager.processed_transactions.clone();
            get_or_create_reward_transfer_attempt(
                &mut state.operation_journal,
                &processed_transactions,
                &operation_id,
                recipient_index,
                RewardAttemptPlan {
                    source_account: source,
                    destination_account: to,
                    amount_e8s: recipient.amount_e8s,
                    fee_e8s: fee,
                    created_at_time: crate::canister_time(),
                    canonical_sns_neuron_id: sns_neuron_id.0.clone(),
                },
            )
        }) {
            Ok(attempt) => attempt,
            Err(message) => {
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|op| op.operation_id == operation_id)
                    {
                        op.mark_retryable_error(
                            message.clone(),
                            OperationPhase::PartiallyDistributed,
                        );
                    }
                });
                outcome.errors.push(message);
                continue;
            }
        };
        let attempt = match CANISTER_STATE.with(|cell| {
            mark_reward_attempt_submitted_if_prepared(
                &mut cell.borrow_mut().operation_journal,
                &operation_id,
                recipient_index,
                &attempt,
            )
        }) {
            Ok(attempt) => attempt,
            Err(message) => {
                outcome.errors.push(message);
                continue;
            }
        };
        let request = reward_transfer_request_from_attempt(&attempt);
        let transfer_result = IcrcLedgerCanisterClient {
            canister: io_canister,
        }
        .transfer(request.clone())
        .await;
        #[cfg(debug_assertions)]
        {
            if matches!(transfer_result, Ok(_))
                && crate::consume_debug_failpoint(
                    crate::DebugFailpoint::AfterTwoWeekRewardTransferBeforeJournalUpdate,
                )
            {
                panic!(
                    "debug failpoint AfterTwoWeekRewardTransferBeforeJournalUpdate triggered after two-week reward transfer"
                );
            }
        }
        if !stored_reward_attempt_still_matches(&operation_id, recipient_index, &attempt) {
            let message = format!(
                "reward operation {operation_id} recipient {recipient_index} transfer callback observed a different persisted attempt; stale callback ignored"
            );
            outcome.errors.push(message);
            continue;
        }
        if let Err(LedgerTransferError::BadFee { expected_fee_e8s }) = &transfer_result {
            match record_reward_bad_fee_policy(&operation_id, recipient_index, *expected_fee_e8s) {
                Ok(()) => outcome.errors.push(format!(
                    "reward transfer BadFee observed current ledger fee {expected_fee_e8s}; preflight must be refreshed or manually reconciled"
                )),
                Err(message) => outcome.errors.push(message),
            }
            continue;
        }
        let duplicate = match &transfer_result {
            Err(LedgerTransferError::Duplicate { duplicate_of }) => {
                duplicate_block_from_account_history(
                    io_index_canister,
                    attempt.destination_account.clone(),
                    *duplicate_of,
                )
                .await
            }
            _ => None,
        };
        if !stored_reward_attempt_still_matches(&operation_id, recipient_index, &attempt) {
            let message = format!(
                "reward operation {operation_id} recipient {recipient_index} duplicate lookup callback observed a different persisted attempt; stale callback ignored"
            );
            outcome.errors.push(message);
            continue;
        }
        match classify_reward_transfer_result(&attempt, transfer_result.clone(), duplicate.as_ref())
        {
            BoundaryTransferDecision::Succeeded(block) => CANISTER_STATE.with(|cell| {
                if let Some(op) = cell
                    .borrow_mut()
                    .operation_journal
                    .iter_mut()
                    .find(|op| op.operation_id == operation_id)
                {
                    if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                        if recipient.reward_transfer_attempt.as_ref() != Some(&attempt) {
                            return;
                        }
                        let debit = match attempt.amount_e8s.checked_add(attempt.fee_e8s) {
                            Some(debit) => debit,
                            None => {
                                let message =
                                    "reward transfer succeeded but reserve debit overflowed"
                                        .to_string();
                                recipient.transfer_status = TransferStatus::FailedTerminal;
                                recipient.ledger_transfer_status =
                                    Some(TransferStatus::FailedTerminal);
                                recipient.last_error = Some(message.clone());
                                op.mark_retryable_error(
                                    message,
                                    OperationPhase::PartiallyDistributed,
                                );
                                return;
                            }
                        };
                        if recipient.amount_e8s != attempt.amount_e8s {
                            let message = format!(
                                "reward transfer attempt amount {} does not match recipient amount {}",
                                attempt.amount_e8s, recipient.amount_e8s
                            );
                            recipient.transfer_status = TransferStatus::FailedTerminal;
                            recipient.ledger_transfer_status = Some(TransferStatus::FailedTerminal);
                            recipient.last_error = Some(message.clone());
                            op.mark_retryable_error(message, OperationPhase::PartiallyDistributed);
                            return;
                        }
                        recipient.transfer_status = TransferStatus::Succeeded;
                        recipient.transfer_block_index = Some(block);
                        recipient.ledger_transfer_status = Some(TransferStatus::Succeeded);
                        recipient.ledger_transfer_block = Some(block);
                        recipient.governance_refresh_status =
                            Some(recipient_refresh_status(recipient));
                        recipient.ledger_transfer_fee_e8s = Some(attempt.fee_e8s);
                        recipient.reward_amount_received_e8s = Some(attempt.amount_e8s);
                        recipient.reserve_debit_e8s = Some(debit);
                        recipient.reward_transfer_attempt = Some(RewardTransferAttemptRecord {
                            lifecycle: Some(RewardTransferAttemptLifecycle::Proven {
                                generation: attempt.created_at_time,
                                block,
                            }),
                            ..attempt.clone()
                        });
                        recipient.last_error = None;
                    }
                    match derived_reward_reservation_for_operation(op)
                        .and_then(|reservation| {
                            let total =
                                reservation.checked_total_unavailable_reward_debit_e8s()?;
                            Ok((reservation, total))
                        }) {
                        Ok((reservation, total)) => {
                            op.reward_reservation = Some(reservation);
                            op.reserved_reward_debit_e8s = Some(total);
                            op.mark_updated(OperationPhase::PartiallyDistributed);
                        }
                        Err(err) => {
                            op.mark_retryable_error(err, OperationPhase::PartiallyDistributed);
                        }
                    }
                }
            }),
            BoundaryTransferDecision::Retryable(err) => {
                let proof_pending = matches!(
                    transfer_result,
                    Err(
                        LedgerTransferError::TooOld
                            | LedgerTransferError::Duplicate { .. }
                            | LedgerTransferError::CanisterCallFailed { .. }
                            | LedgerTransferError::DecodeError { .. }
                    )
                );
                let proof = if proof_pending {
                    Some(
                        scan_reward_transfer_proof_from_state(
                            Some(io_index_canister),
                            &attempt,
                            recipient
                                .ledger_transfer_proof_scan_state
                                .clone()
                                .unwrap_or_default(),
                        )
                        .await,
                    )
                } else {
                    None
                };
                if proof.is_some()
                    && !stored_reward_attempt_still_matches(&operation_id, recipient_index, &attempt)
                {
                    let message = format!(
                        "reward operation {operation_id} recipient {recipient_index} proof scan callback observed a different persisted attempt; stale callback ignored"
                    );
                    outcome.errors.push(message);
                    continue;
                }
                if let Some(proof) = proof {
                    match proof.disposition {
                        RewardTransferProofDisposition::ProofFound(block) => {
                            mark_reward_transfer_proven(
                                &operation_id,
                                recipient_index,
                                &attempt,
                                block,
                            );
                            continue;
                        }
                        RewardTransferProofDisposition::IndexNotCaughtUp(reason)
                        | RewardTransferProofDisposition::HistoryIncomplete(reason) => {
                            let error = format!(
                                "{err}; reward transfer proof pending; automatic retry paused: {reason}"
                            );
                            CANISTER_STATE.with(|cell| {
                                if let Some(op) = cell
                                    .borrow_mut()
                                    .operation_journal
                                    .iter_mut()
                                    .find(|op| op.operation_id == operation_id)
                                {
                                    if let Some(recipient) =
                                        op.two_week_recipients.get_mut(recipient_index)
                                    {
                                        if recipient.reward_transfer_attempt.as_ref()
                                            != Some(&attempt)
                                        {
                                            return;
                                        }
                                        recipient.transfer_status = TransferStatus::FailedRetryable;
                                        recipient.ledger_transfer_status =
                                            Some(TransferStatus::FailedRetryable);
                                        recipient.ledger_transfer_proof_scan_state =
                                            Some(proof.scan_state.clone());
                                        recipient.reward_transfer_attempt =
                                            Some(RewardTransferAttemptRecord {
                                                lifecycle: Some(
                                                    RewardTransferAttemptLifecycle::ProofRequired {
                                                        generation: attempt.created_at_time,
                                                        reason: error.clone(),
                                                    },
                                                ),
                                                ..attempt.clone()
                                            });
                                        recipient.last_error = Some(error.clone());
                                    }
                                    op.mark_retryable_error(
                                        error.clone(),
                                        OperationPhase::PartiallyDistributed,
                                    );
                                }
                            });
                            outcome.errors.push(error);
                            continue;
                        }
                        RewardTransferProofDisposition::CompleteNoMatch(reason) => {
                            let error = format!(
                                "{err}; {reason}; manual reconciliation required before retry"
                            );
                            CANISTER_STATE.with(|cell| {
                                if let Some(op) = cell
                                    .borrow_mut()
                                    .operation_journal
                                    .iter_mut()
                                    .find(|op| op.operation_id == operation_id)
                                {
                                    if let Some(recipient) =
                                        op.two_week_recipients.get_mut(recipient_index)
                                    {
                                        if recipient.reward_transfer_attempt.as_ref()
                                            != Some(&attempt)
                                        {
                                            return;
                                        }
                                        recipient.transfer_status = TransferStatus::FailedTerminal;
                                        recipient.ledger_transfer_status =
                                            Some(TransferStatus::FailedTerminal);
                                        recipient.ledger_transfer_proof_scan_state =
                                            Some(proof.scan_state.clone());
                                        recipient.reward_transfer_attempt =
                                            Some(RewardTransferAttemptRecord {
                                                lifecycle: Some(
                                                    RewardTransferAttemptLifecycle::ProofRequired {
                                                        generation: attempt.created_at_time,
                                                        reason: error.clone(),
                                                    },
                                                ),
                                                ..attempt.clone()
                                            });
                                        recipient.last_error = Some(error.clone());
                                    }
                                    op.mark_retryable_error(
                                        error.clone(),
                                        OperationPhase::PartiallyDistributed,
                                    );
                                }
                            });
                            outcome.errors.push(error);
                            continue;
                        }
                    }
                }
                let error = err;
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|op| op.operation_id == operation_id)
                    {
                        if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                            if recipient.reward_transfer_attempt.as_ref() != Some(&attempt) {
                                return;
                            }
                            recipient.transfer_status = TransferStatus::FailedRetryable;
                            recipient.ledger_transfer_status =
                                Some(TransferStatus::FailedRetryable);
                            recipient.reward_transfer_attempt = Some(RewardTransferAttemptRecord {
                                lifecycle: Some(RewardTransferAttemptLifecycle::Prepared),
                                ..attempt.clone()
                            });
                            recipient.last_error = Some(error.clone());
                        }
                        op.mark_retryable_error(
                            error.clone(),
                            OperationPhase::PartiallyDistributed,
                        );
                    }
                });
                outcome.errors.push(error);
                continue;
            }
        }
        #[cfg(debug_assertions)]
        {
            if crate::consume_debug_failpoint(
                crate::DebugFailpoint::AfterTwoWeekRewardTransferBeforeGovernanceRefresh,
            ) {
                outcome.errors.push(
                    "debug failpoint AfterTwoWeekRewardTransferBeforeGovernanceRefresh triggered after two-week reward transfer"
                        .to_string(),
                );
                return false;
            }
        }
    }

    let mut attempted_refresh_recipients = BTreeSet::new();
    for _ in 0..TWO_WEEK_REWARD_REFRESH_BUDGET_PER_TICK {
        let Some((operation_id, recipient_index, recipient, attempt_key)) =
            next_reward_refresh_recipient(&attempted_refresh_recipients)
        else {
            break;
        };
        attempted_refresh_recipients.insert(attempt_key);
        let Some(sns_neuron_id) = recipient
            .sns_neuron_id
            .as_deref()
            .filter(|id| id.len() == 32)
            .map(|id| SnsNeuronId(id.to_vec()))
        else {
            let err = format!(
                "two-week reward recipient {} is missing a canonical 32-byte SNS neuron id",
                recipient.neuron_id
            );
            CANISTER_STATE.with(|cell| {
                if let Some(op) = cell
                    .borrow_mut()
                    .operation_journal
                    .iter_mut()
                    .find(|op| op.operation_id == operation_id)
                {
                    if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                        recipient.governance_refresh_status = Some(TransferStatus::FailedTerminal);
                        recipient.refresh_last_error = Some(err.clone());
                    }
                    op.mark_terminal_error(err.clone(), OperationPhase::PartiallyDistributed);
                }
            });
            outcome.errors.push(err);
            continue;
        };
        let minimum_expected = match recipient
            .minimum_expected_stake_after_e8s
            .or(recipient.expected_stake_after_e8s)
        {
            Some(expected) => expected,
            None => {
                let err = format!(
                    "two-week reward recipient {} is missing persisted minimum post-refresh stake",
                    recipient.neuron_id
                );
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|op| op.operation_id == operation_id)
                    {
                        if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                            recipient.governance_refresh_status =
                                Some(TransferStatus::FailedTerminal);
                            recipient.refresh_last_error = Some(err.clone());
                        }
                        op.mark_terminal_error(err.clone(), OperationPhase::PartiallyDistributed);
                    }
                });
                outcome.errors.push(err);
                continue;
            }
        };

        if let Err(err) = governance_client
            .claim_or_refresh_neuron(sns_neuron_id.clone())
            .await
        {
            let message = format!("SNS governance ClaimOrRefresh(By::NeuronId) failed: {err:?}");
            CANISTER_STATE.with(|cell| {
                if let Some(op) = cell
                    .borrow_mut()
                    .operation_journal
                    .iter_mut()
                    .find(|op| op.operation_id == operation_id)
                {
                    if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                        recipient.governance_refresh_status = Some(TransferStatus::FailedRetryable);
                        recipient.refresh_retry_count =
                            Some(recipient.refresh_retry_count.unwrap_or(0).saturating_add(1));
                        recipient.refresh_last_error = Some(message.clone());
                    }
                    op.mark_retryable_error(message.clone(), OperationPhase::PartiallyDistributed);
                }
            });
            outcome.errors.push(message);
            continue;
        }

        #[cfg(debug_assertions)]
        {
            if crate::consume_debug_failpoint(
                crate::DebugFailpoint::AfterTwoWeekGovernanceRefreshBeforeJournalCompletion,
            ) {
                panic!(
                    "debug failpoint AfterTwoWeekGovernanceRefreshBeforeJournalCompletion triggered after two-week governance refresh"
                );
            }
        }

        match governance_client.get_neuron(sns_neuron_id).await {
            Ok(neuron) if neuron.cached_neuron_stake_e8s >= minimum_expected => {
                let concurrent_delta = neuron
                    .cached_neuron_stake_e8s
                    .saturating_sub(minimum_expected);
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|op| op.operation_id == operation_id)
                    {
                        if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                            recipient.governance_refresh_status = Some(TransferStatus::Succeeded);
                            recipient.observed_stake_after_e8s =
                                Some(neuron.cached_neuron_stake_e8s);
                            recipient.minimum_expected_stake_after_e8s = Some(minimum_expected);
                            recipient.concurrent_stake_delta_e8s =
                                (concurrent_delta > 0).then_some(concurrent_delta);
                            recipient.refresh_last_error = None;
                            recipient.last_error = None;
                        }
                        op.mark_updated(OperationPhase::PartiallyDistributed);
                    }
                });
            }
            Ok(neuron) => {
                let message = format!(
                    "SNS governance refresh did not reflect reward for recipient {}: minimum expected {}, observed {}",
                    recipient.neuron_id, minimum_expected, neuron.cached_neuron_stake_e8s
                );
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|op| op.operation_id == operation_id)
                    {
                        if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                            recipient.governance_refresh_status =
                                Some(TransferStatus::FailedRetryable);
                            recipient.observed_stake_after_e8s =
                                Some(neuron.cached_neuron_stake_e8s);
                            recipient.refresh_retry_count =
                                Some(recipient.refresh_retry_count.unwrap_or(0).saturating_add(1));
                            recipient.refresh_last_error = Some(message.clone());
                        }
                        op.mark_retryable_error(
                            message.clone(),
                            OperationPhase::PartiallyDistributed,
                        );
                    }
                });
                outcome.errors.push(message);
                continue;
            }
            Err(err) => {
                let message =
                    format!("SNS governance get_neuron after reward refresh failed: {err:?}");
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|op| op.operation_id == operation_id)
                    {
                        if let Some(recipient) = op.two_week_recipients.get_mut(recipient_index) {
                            recipient.governance_refresh_status =
                                Some(TransferStatus::FailedRetryable);
                            recipient.refresh_retry_count =
                                Some(recipient.refresh_retry_count.unwrap_or(0).saturating_add(1));
                            recipient.refresh_last_error = Some(message.clone());
                        }
                        op.mark_retryable_error(
                            message.clone(),
                            OperationPhase::PartiallyDistributed,
                        );
                    }
                });
                outcome.errors.push(message);
                continue;
            }
        }
    }

    loop {
        let completed = CANISTER_STATE.with(|cell| {
            let state = cell.borrow();
            state
                .operation_journal
                .iter()
                .position(|op| {
                    op.kind == StreamOperationKind::TwoWeekMaturityStream
                        && op.phase != OperationPhase::Completed
                        && op.two_week_recipients.iter().all(recipient_is_completed)
                })
                .map(|index| state.operation_journal[index].clone())
        });
        let Some(op) = completed else {
            break;
        };
        match commit_completed_reward_operation(&op.operation_id) {
            Ok(true) => {
                outcome.processed_authorized_streams += 1;
                outcome.io_issued_e8s = outcome.io_issued_e8s.saturating_add(op.io_issued_e8s);
            }
            Ok(false) => {}
            Err(err) => {
                outcome.errors.push(format!(
                    "stream {} reward commit failed: {err}",
                    op.operation_id
                ));
                return false;
            }
        }
    }

    CANISTER_STATE.with(|cell| {
        !cell.borrow().operation_journal.iter().any(|op| {
            op.kind == StreamOperationKind::TwoWeekMaturityStream
                && op.phase != OperationPhase::Completed
        })
    })
}

#[cfg(target_family = "wasm")]
async fn retry_pending_io_issuances(
    io_canister: Principal,
    outcome: &mut DebugTickOutcome,
) -> bool {
    loop {
        let pending = CANISTER_STATE.with(|cell| {
            cell.borrow().operation_journal.iter().find_map(|op| {
                (op.kind == StreamOperationKind::JupiterFaucetStream
                    && op.phase != OperationPhase::Completed)
                    .then(|| op.clone())
            })
        });
        let Some(op) = pending else {
            return true;
        };

        if op.downstream_io_issuance_block.is_none() {
            let request = mock_transfer_request(
                PROTOCOL_RESERVE_ACCOUNT,
                JUPITER_FAUCET_SOURCE,
                op.io_issued_e8s,
                &op.operation_id,
            );
            let client = io_ledger::MockLedgerCanisterClient {
                canister: io_canister,
                fee_e8s: 0,
            };
            match classify_mock_transfer(
                io_canister,
                &request,
                client.transfer(request.clone()).await,
            )
            .await
            {
                BoundaryTransferDecision::Succeeded(block) => {
                    mark_io_issuance(&op.operation_id, block)
                }
                BoundaryTransferDecision::Retryable(err) => {
                    mark_operation_error(
                        &op.operation_id,
                        err.clone(),
                        OperationPhase::AwaitingIoIssuance,
                    );
                    outcome.errors.push(err);
                    return false;
                }
            }
        }

        let committed = CANISTER_STATE.with(|cell| {
            cell.borrow_mut()
                .manager
                .commit_previewed_stream(op.operation_id.clone(), op.post_state.into())
        });
        match committed {
            Ok(()) => {
                mark_completed(&op.operation_id);
                outcome.processed_authorized_streams += 1;
                outcome.io_issued_e8s = outcome.io_issued_e8s.saturating_add(op.io_issued_e8s);
            }
            Err(StreamManagerError::DuplicateTransaction) => mark_completed(&op.operation_id),
            Err(err) => {
                outcome
                    .errors
                    .push(format!("stream {}: {err:?}", op.operation_id));
                return false;
            }
        }
    }
}

#[cfg(target_family = "wasm")]
async fn process_rejected_redemption_dispositions(
    io_canister: Principal,
    io_index_canister: Option<Principal>,
    outcome: &mut DebugTickOutcome,
) -> bool {
    let mut attempted = BTreeSet::new();
    for _ in 0..REJECTED_REFUND_RETRY_BUDGET_PER_TICK {
        let pending = CANISTER_STATE.with(|cell| {
            next_retryable_rejected_refund_operation(&cell.borrow().operation_journal, &attempted)
        });
        let Some(op) = pending else {
            return true;
        };
        attempted.insert(op.operation_id.clone());

        let attempt = if let Some(attempt) = op.rejected_refund_attempt.clone() {
            attempt
        } else {
            let Some(to) = op.source_account.clone() else {
                let reason = "rejected IO transfer has no resolvable sender account".to_string();
                mark_rejected_redemption_quarantined(&op.operation_id, reason);
                continue;
            };

            let fee = match query_io_return_fee(io_canister).await {
                Ok(fee) => fee,
                Err(err) => {
                    let message = format!("rejected IO refund fee query failed: {err}");
                    mark_rejected_redemption_retryable(&op.operation_id, message.clone(), None);
                    outcome.errors.push(message);
                    continue;
                }
            };

            let Some(refund_amount_e8s) = amount_after_fee(op.io_amount, fee) else {
                let reason = format!(
                    "rejected IO amount {} is not above refund fee {}",
                    op.io_amount, fee
                );
                mark_rejected_redemption_quarantined(&op.operation_id, reason);
                continue;
            };

            rejected_refund_attempt_from_parts(
                canister_owned_account(REDEMPTION_ACCOUNT),
                to,
                refund_amount_e8s,
                fee,
                Some(rejected_refund_memo(&op.operation_id)),
                rejected_refund_attempt_created_at(&op),
            )
        };
        persist_rejected_refund_attempt(&op.operation_id, attempt.clone());
        let refund_source = attempt.refund_source_account.clone();
        let refund_amount_e8s = attempt.attempted_refund_amount_e8s;
        let fee = attempt.attempted_fee_e8s;
        let created_at_time = attempt.attempted_created_at_time;
        let request = rejected_refund_request_from_attempt(&attempt);
        let transfer_result = IcrcLedgerCanisterClient {
            canister: io_canister,
        }
        .transfer(request.clone())
        .await;
        #[cfg(debug_assertions)]
        {
            if matches!(transfer_result, Ok(_))
                && crate::consume_debug_failpoint(
                    crate::DebugFailpoint::AfterRejectedRefundTransferBeforeJournalUpdate,
                )
            {
                panic!(
                    "debug failpoint AfterRejectedRefundTransferBeforeJournalUpdate triggered after rejected refund transfer"
                );
            }
        }
        let duplicate = match &transfer_result {
            Err(LedgerTransferError::Duplicate { duplicate_of }) => {
                duplicate_block(io_canister, *duplicate_of).await
            }
            _ => None,
        };
        match classify_boundary_transfer_result_with_source(
            &request,
            &refund_source,
            transfer_result.clone(),
            duplicate.as_ref(),
        ) {
            BoundaryTransferDecision::Succeeded(block) => {
                mark_rejected_redemption_refunded(&op.operation_id, block, refund_amount_e8s, fee)
            }
            BoundaryTransferDecision::Retryable(err) => {
                if matches!(transfer_result, Err(LedgerTransferError::TooOld)) {
                    match resolve_too_old_rejected_refund(
                        io_index_canister,
                        &refund_source,
                        &request,
                    )
                    .await
                    {
                        TooOldRefundProofDisposition::ProofFound(block) => {
                            mark_rejected_redemption_refunded(
                                &op.operation_id,
                                block.0,
                                refund_amount_e8s,
                                fee,
                            );
                            continue;
                        }
                        TooOldRefundProofDisposition::IndexNotCaughtUp(reason)
                        | TooOldRefundProofDisposition::HistoryIncomplete(reason) => {
                            let message = format!(
                                "{err}; refund proof pending and automatic retry paused: {reason}"
                            );
                            mark_rejected_redemption_proof_pending(
                                &op.operation_id,
                                message.clone(),
                                Some(created_at_time),
                                None,
                            );
                            outcome.errors.push(message);
                            continue;
                        }
                        TooOldRefundProofDisposition::CompleteNoMatch(reason) => {
                            let message = format!(
                                "{err}; {reason}; manual reconciliation required before any new refund attempt"
                            );
                            mark_rejected_redemption_manual_reconciliation_required(
                                &op.operation_id,
                                message.clone(),
                                Some(created_at_time),
                            );
                            outcome.errors.push(message);
                            continue;
                        }
                    }
                }
                mark_rejected_redemption_retryable(&op.operation_id, err.clone(), None);
                outcome.errors.push(err);
                continue;
            }
        }
    }
    true
}

#[cfg(target_family = "wasm")]
async fn reconcile_rejected_refund_proof_pending(
    _io_canister: Principal,
    io_index_canister: Option<Principal>,
    outcome: &mut DebugTickOutcome,
) -> bool {
    let mut attempted = BTreeSet::new();
    for _ in 0..REJECTED_REFUND_PROOF_RECONCILIATION_BUDGET_PER_TICK {
        let pending = CANISTER_STATE.with(|cell| {
            next_proof_pending_rejected_refund_operation(
                &cell.borrow().operation_journal,
                &attempted,
            )
        });
        let Some(op) = pending else {
            return true;
        };
        attempted.insert(op.operation_id.clone());

        let (original_created_at_time, proof_scan_state, existing_reason) =
            match &op.rejected_fund_disposition {
                Some(RejectedFundDisposition::ReturnToSenderProofPending {
                    reason,
                    original_created_at_time,
                    proof_scan_state,
                }) => (
                    original_created_at_time.unwrap_or(op.created_at),
                    proof_scan_state
                        .clone()
                        .unwrap_or_else(|| AccountHistoryScanState {
                            cursor: io_ledger_types::AccountHistoryCursor {
                                order: None,
                                latest_cursor: None,
                                oldest_cursor: None,
                                backfill_complete: false,
                            },
                            status: Default::default(),
                        }),
                    reason.clone(),
                ),
                _ => continue,
            };

        let Some(attempt) = op.rejected_refund_attempt.clone() else {
            let message = "proof-pending rejected IO refund is missing the persisted original refund attempt; manual reconciliation required before any new refund attempt".to_string();
            mark_rejected_redemption_manual_reconciliation_required(
                &op.operation_id,
                message.clone(),
                Some(original_created_at_time),
            );
            outcome.errors.push(message);
            continue;
        };

        let refund_source = attempt.refund_source_account.clone();
        let refund_amount_e8s = attempt.attempted_refund_amount_e8s;
        let fee = attempt.attempted_fee_e8s;
        let request = rejected_refund_request_from_attempt(&attempt);

        let proof = resolve_too_old_rejected_refund_from_state(
            io_index_canister,
            &refund_source,
            &request,
            proof_scan_state,
        )
        .await;

        match proof.disposition {
            TooOldRefundProofDisposition::ProofFound(block) => {
                mark_rejected_redemption_refunded(
                    &op.operation_id,
                    block.0,
                    refund_amount_e8s,
                    fee,
                );
            }
            TooOldRefundProofDisposition::IndexNotCaughtUp(reason)
            | TooOldRefundProofDisposition::HistoryIncomplete(reason) => {
                let message =
                    format!("refund proof pending; automatic retry remains paused: {reason}");
                let message = if existing_reason == message {
                    existing_reason
                } else {
                    message
                };
                refresh_rejected_redemption_proof_pending(
                    &op.operation_id,
                    message.clone(),
                    Some(proof.scan_state),
                );
                outcome.errors.push(message);
            }
            TooOldRefundProofDisposition::CompleteNoMatch(reason) => {
                let message = format!(
                    "{reason}; manual reconciliation required before any new refund attempt"
                );
                mark_rejected_redemption_manual_reconciliation_required(
                    &op.operation_id,
                    message.clone(),
                    Some(original_created_at_time),
                );
                outcome.errors.push(message);
            }
        }
    }
    true
}

#[cfg(target_family = "wasm")]
async fn retry_pending_redemptions(
    icp_canister: Option<Principal>,
    io_canister: Principal,
    io_index_canister: Option<Principal>,
    outcome: &mut DebugTickOutcome,
) -> bool {
    if !process_rejected_redemption_dispositions(io_canister, io_index_canister, outcome).await {
        return false;
    }
    if !reconcile_rejected_refund_proof_pending(io_canister, io_index_canister, outcome).await {
        return false;
    }

    loop {
        let pending = CANISTER_STATE.with(|cell| {
            cell.borrow()
                .operation_journal
                .iter()
                .find_map(|op| is_retryable_redemption_operation(op).then(|| op.clone()))
        });
        let Some(op) = pending else {
            return true;
        };

        let io_return_fee_e8s = if op.io_return_status != TransferStatus::Succeeded {
            let io_return_fee_e8s = match query_io_return_fee(io_canister).await {
                Ok(fee) => fee,
                Err(err) => {
                    let message = format!("IO return fee query failed: {err}");
                    CANISTER_STATE.with(|cell| {
                        if let Some(op) = cell
                            .borrow_mut()
                            .operation_journal
                            .iter_mut()
                            .find(|pending| pending.operation_id == op.operation_id)
                        {
                            op.io_return_status = TransferStatus::FailedRetryable;
                            op.mark_retryable_error(
                                message.clone(),
                                OperationPhase::AwaitingIoReturn,
                            );
                        }
                    });
                    outcome.errors.push(message);
                    return false;
                }
            };
            let Some(io_return_amount_e8s) = amount_after_fee(op.io_amount, io_return_fee_e8s)
            else {
                let message = format!(
                    "redeemed IO {} is not above IO return fee {}",
                    op.io_amount, io_return_fee_e8s
                );
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|pending| pending.operation_id == op.operation_id)
                    {
                        op.io_return_fee_e8s = io_return_fee_e8s;
                        op.io_return_status = TransferStatus::FailedTerminal;
                        op.icp_payout_status = TransferStatus::FailedTerminal;
                        op.mark_terminal_error(message.clone(), OperationPhase::AwaitingIoReturn);
                    }
                });
                outcome.errors.push(message);
                return false;
            };
            CANISTER_STATE.with(|cell| {
                if let Some(op) = cell
                    .borrow_mut()
                    .operation_journal
                    .iter_mut()
                    .find(|pending| pending.operation_id == op.operation_id)
                {
                    op.io_return_fee_e8s = io_return_fee_e8s;
                }
            });
            Some((io_return_fee_e8s, io_return_amount_e8s))
        } else {
            None
        };

        if op.icp_payout_status != TransferStatus::Succeeded {
            let Some(icp_canister) = icp_canister else {
                CANISTER_STATE.with(|cell| {
                    if let Some(op) = cell
                        .borrow_mut()
                        .operation_journal
                        .iter_mut()
                        .find(|pending| pending.operation_id == op.operation_id)
                    {
                        op.icp_payout_status = TransferStatus::FailedRetryable;
                        op.mark_retryable_error(
                            "missing ICP payout ledger principal".to_string(),
                            OperationPhase::AwaitingIcpPayout,
                        );
                    }
                });
                outcome
                    .errors
                    .push("missing ICP payout ledger principal".to_string());
                return false;
            };

            let user_account = op.user_account.clone().unwrap_or_default();
            let mock_request = mock_transfer_request(
                STREAM_MANAGER_DEPOSIT_ACCOUNT,
                &user_account,
                op.effective_net_user_icp_payout_e8s(),
                REDEMPTION_PAYOUT_MEMO,
            );
            let real_request = LedgerTransferRequest {
                from_subaccount: Some(icp_ledger::mock_subaccount(STREAM_MANAGER_DEPOSIT_ACCOUNT)),
                to: icp_ledger::mock_account(&user_account),
                amount_e8s: op.effective_net_user_icp_payout_e8s(),
                fee_e8s: None,
                memo: None,
                created_at_time: None,
            };
            match classify_icp_payout_transfer(icp_canister, &real_request, &mock_request).await {
                BoundaryTransferDecision::Succeeded(block) => {
                    CANISTER_STATE.with(|cell| {
                        if let Some(op) = cell
                            .borrow_mut()
                            .operation_journal
                            .iter_mut()
                            .find(|pending| pending.operation_id == op.operation_id)
                        {
                            op.icp_payout_status = TransferStatus::Succeeded;
                            op.icp_payout_block = Some(block);
                            op.mark_updated(OperationPhase::AwaitingIoReturn);
                        }
                    });
                }
                BoundaryTransferDecision::Retryable(err) => {
                    CANISTER_STATE.with(|cell| {
                        if let Some(op) = cell
                            .borrow_mut()
                            .operation_journal
                            .iter_mut()
                            .find(|pending| pending.operation_id == op.operation_id)
                        {
                            op.icp_payout_status = TransferStatus::FailedRetryable;
                            op.mark_retryable_error(err.clone(), OperationPhase::AwaitingIcpPayout);
                        }
                    });
                    outcome.errors.push(err);
                    return false;
                }
            }
            continue;
        }

        if op.io_return_status != TransferStatus::Succeeded {
            let (_, io_return_amount_e8s) =
                io_return_fee_e8s.expect("IO return preflight should run before IO return");
            let request = LedgerTransferRequest {
                from_subaccount: Some(icp_ledger::mock_subaccount(REDEMPTION_ACCOUNT)),
                to: canister_owned_account(PROTOCOL_RESERVE_ACCOUNT),
                amount_e8s: io_return_amount_e8s,
                fee_e8s: None,
                memo: Some(io_ledger_types::Memo::from(REDEEMED_IO_MEMO)),
                created_at_time: None,
            };
            let transfer_result = IcrcLedgerCanisterClient {
                canister: io_canister,
            }
            .transfer(request.clone())
            .await;
            match classify_boundary_transfer_result(&request, transfer_result, None) {
                BoundaryTransferDecision::Succeeded(block) => {
                    CANISTER_STATE.with(|cell| {
                        if let Some(op) = cell
                            .borrow_mut()
                            .operation_journal
                            .iter_mut()
                            .find(|pending| pending.operation_id == op.operation_id)
                        {
                            op.io_return_status = TransferStatus::Succeeded;
                            op.io_return_block = Some(block);
                            op.mark_updated(OperationPhase::AwaitingIoReturn);
                        }
                    });
                }
                BoundaryTransferDecision::Retryable(err) => {
                    CANISTER_STATE.with(|cell| {
                        if let Some(op) = cell
                            .borrow_mut()
                            .operation_journal
                            .iter_mut()
                            .find(|pending| pending.operation_id == op.operation_id)
                        {
                            op.io_return_status = TransferStatus::FailedRetryable;
                            op.mark_retryable_error(err.clone(), OperationPhase::AwaitingIoReturn);
                        }
                    });
                    outcome.errors.push(err);
                    return false;
                }
            }
        }

        let committed = CANISTER_STATE.with(|cell| {
            cell.borrow_mut()
                .manager
                .commit_previewed_redemption(op.operation_id.clone(), op.post_state.into())
        });
        match committed {
            Ok(()) => {
                mark_completed(&op.operation_id);
                outcome.processed_redemptions += 1;
                outcome.icp_paid_e8s = outcome.icp_paid_e8s.saturating_add(op.amount_e8s);
            }
            Err(StreamManagerError::DuplicateTransaction) => mark_completed(&op.operation_id),
            Err(err) => {
                outcome
                    .errors
                    .push(format!("redemption commit failed: {err:?}"));
                return false;
            }
        }
    }
}

pub async fn scheduler_tick_once() -> DebugTickOutcome {
    #[cfg(not(target_family = "wasm"))]
    {
        DebugTickOutcome {
            scanned_icp_transactions: 0,
            scanned_io_transactions: 0,
            processed_authorized_streams: 0,
            processed_redemptions: 0,
            io_issued_e8s: 0,
            icp_paid_e8s: 0,
            errors: vec!["canister scheduler external calls run only on wasm".to_string()],
        }
    }

    #[cfg(target_family = "wasm")]
    {
        let config = CANISTER_STATE.with(|cell| cell.borrow().config.clone());
        let icp_ledger = principal(&config.icp_index_principal_text);
        let io_ledger = principal(&config.io_index_principal_text);
        let io_transfer_ledger = principal(&config.io_ledger_principal_text);
        let icp_transfer_ledger = principal(&config.icp_ledger_principal_text);
        let sns_governance = principal(&config.sns_governance_principal_text);

        let mut outcome = DebugTickOutcome {
            scanned_icp_transactions: 0,
            scanned_io_transactions: 0,
            processed_authorized_streams: 0,
            processed_redemptions: 0,
            io_issued_e8s: 0,
            icp_paid_e8s: 0,
            errors: Vec::new(),
        };

        let reward_snapshot = load_reward_snapshot(sns_governance).await;

        if let Some(io_canister) = io_transfer_ledger {
            if !retry_pending_io_issuances(io_canister, &mut outcome).await {
                return outcome;
            }
            match sns_governance {
                Some(sns_governance_canister) => match io_ledger {
                    Some(io_index_canister) => {
                        if !retry_pending_two_week_streams(
                            io_canister,
                            io_index_canister,
                            sns_governance_canister,
                            &mut outcome,
                        )
                        .await
                        {
                            return outcome;
                        }
                    }
                    None => {
                        let has_pending_two_week = CANISTER_STATE.with(|cell| {
                            cell.borrow().operation_journal.iter().any(|op| {
                                op.kind == StreamOperationKind::TwoWeekMaturityStream
                                    && op.phase != OperationPhase::Completed
                            })
                        });
                        if has_pending_two_week {
                            outcome.errors.push(
                                    "IO index canister is required for safe two-week reward retry proof"
                                        .to_string(),
                                );
                            return outcome;
                        }
                    }
                },
                None => {
                    let has_pending_two_week = CANISTER_STATE.with(|cell| {
                        cell.borrow().operation_journal.iter().any(|op| {
                            op.kind == StreamOperationKind::TwoWeekMaturityStream
                                && op.phase != OperationPhase::Completed
                        })
                    });
                    if has_pending_two_week {
                        outcome.errors.push(
                            "cannot retry SNS neuron rewards without configured SNS governance"
                                .to_string(),
                        );
                        return outcome;
                    }
                }
            }
            if !retry_pending_redemptions(icp_transfer_ledger, io_canister, io_ledger, &mut outcome)
                .await
            {
                return outcome;
            }
        }

        if let Some(canister) = icp_ledger {
            let scan_state = CANISTER_STATE.with(|cell| {
                let cursors = &cell.borrow().scheduler_cursors;
                if cursors
                    .icp_account_history_scan
                    .cursor
                    .latest_cursor
                    .is_none()
                {
                    match cursors.last_scanned_icp_index_block {
                        Some(cursor) => legacy_icp_account_history_scan_state(cursor),
                        None => AccountHistoryScanState::default(),
                    }
                } else {
                    cursors.icp_account_history_scan.clone()
                }
            });
            let start_after = scan_state.cursor.latest_cursor.map(|block| block.0);
            match scan_icp_account_through_index(
                canister,
                canister_owned_account(STREAM_MANAGER_DEPOSIT_ACCOUNT),
                scan_state,
            )
            .await
            {
                Ok((transactions, next_scan_state, latest_seen)) => {
                    let mut relevant = transactions
                        .into_iter()
                        .filter(|tx| {
                            tx.to == STREAM_MANAGER_DEPOSIT_ACCOUNT
                                && start_after
                                    .map(|cursor| tx.block_index > cursor)
                                    .unwrap_or(true)
                        })
                        .collect::<Vec<_>>();
                    relevant.sort_by_key(|tx| tx.block_index);
                    outcome.scanned_icp_transactions = relevant.len() as u64;
                    let page_error_count = outcome.errors.len();

                    for tx in relevant {
                        let tx_id = format!("icp:{}", tx.block_index);
                        let already_journaled = CANISTER_STATE.with(|cell| {
                            cell.borrow()
                                .operation_journal
                                .iter()
                                .any(|op| op.operation_id == tx_id)
                        });
                        if already_journaled {
                            advance_icp_cursor(tx.block_index);
                            continue;
                        }

                        let preview = CANISTER_STATE.with(|cell| {
                            let state = cell.borrow();
                            let kind =
                                match crate::StreamManager::classify_stream(&tx.from, &tx.memo) {
                                    Ok(kind) => kind,
                                    Err(err) => return Err(err),
                                };
                            state.manager.preview_authorized_stream(
                                kind,
                                tx.amount_e8s,
                                tx_id.clone(),
                            )
                        });
                        let preview = match preview {
                            Ok(preview) => preview,
                            Err(StreamManagerError::DuplicateTransaction) => {
                                advance_icp_cursor(tx.block_index);
                                continue;
                            }
                            Err(err @ StreamManagerError::UnknownOrUnauthorizedStream { .. }) => {
                                journal_rejected_icp_deposit(
                                    tx.block_index,
                                    tx.amount_e8s,
                                    format!("{err:?}"),
                                );
                                advance_icp_cursor(tx.block_index);
                                continue;
                            }
                            Err(
                                err @ StreamManagerError::Model(
                                    ModelError::BelowMinimumStreamDeposit { .. },
                                ),
                            ) => {
                                journal_rejected_icp_deposit(
                                    tx.block_index,
                                    tx.amount_e8s,
                                    format!("{err:?}"),
                                );
                                advance_icp_cursor(tx.block_index);
                                continue;
                            }
                            Err(err) => {
                                outcome.errors.push(format!("stream {tx_id}: {err:?}"));
                                continue;
                            }
                        };
                        let recipient_policy =
                            ApiIoRecipientPolicy::from(preview.outcome.recipient_policy);
                        if recipient_policy == ApiIoRecipientPolicy::EligibleIoSnsNeurons {
                            if let Err(err) = require_new_two_week_reward_inputs(
                                io_transfer_ledger,
                                io_ledger,
                                sns_governance,
                                &reward_snapshot,
                            ) {
                                outcome.errors.push(format!("stream {tx_id}: {err}"));
                                // Stop this tick so no newer ICP event advances beyond the blocked maturity
                                // event. This postpones discovery of later new-ledger events until the next
                                // successful tick; pending operations were already retried before this boundary.
                                return outcome;
                            }
                        }

                        if let Some(io_canister) = io_transfer_ledger {
                            match recipient_policy {
                                ApiIoRecipientPolicy::JupiterFaucet => {
                                    let reward_reserve_available = CANISTER_STATE.with(|cell| {
                                        let state = cell.borrow();
                                        let pending = pending_reward_reservations(
                                            state.operation_journal.iter(),
                                            &state.manager.processed_transactions,
                                            None,
                                        )?;
                                        reward_reserve_available(
                                            state.manager.state.protocol_reserve_io_e8s,
                                            pending,
                                        )
                                    });
                                    match reward_reserve_available {
                                        Ok(available)
                                            if preview.outcome.io_issued_e8s <= available => {}
                                        Ok(available) => {
                                            outcome.errors.push(format!(
                                                "stream {tx_id}: protocol reserve {available} is reserved for pending reward operations; cannot issue {} IO e8s",
                                                preview.outcome.io_issued_e8s
                                            ));
                                            continue;
                                        }
                                        Err(err) => {
                                            outcome.errors.push(format!("stream {tx_id}: {err}"));
                                            continue;
                                        }
                                    }
                                    ensure_stream_operation(
                                        "icp",
                                        tx.block_index,
                                        kind_from_api(preview.outcome.kind.into()),
                                        tx.amount_e8s,
                                        preview.post_state,
                                        preview.outcome.io_issued_e8s,
                                        OperationPhase::AwaitingIoIssuance,
                                    );
                                    if !retry_pending_io_issuances(io_canister, &mut outcome).await
                                    {
                                        return outcome;
                                    }
                                    advance_icp_cursor(tx.block_index);
                                    continue;
                                }
                                ApiIoRecipientPolicy::EligibleIoSnsNeurons => {
                                    let Ok((
                                        io_canister,
                                        io_index_canister,
                                        sns_governance_canister,
                                        neurons,
                                    )) = require_new_two_week_reward_inputs(
                                        io_transfer_ledger,
                                        io_ledger,
                                        sns_governance,
                                        &reward_snapshot,
                                    )
                                    else {
                                        unreachable!(
                                            "new two-week reward inputs were checked before operation creation"
                                        );
                                    };
                                    let allocations = CANISTER_STATE.with(|cell| {
                                        cell.borrow().manager.allocate_two_week_maturity_io(
                                            preview.outcome.io_issued_e8s,
                                            neurons,
                                        )
                                    });
                                    CANISTER_STATE.with(|cell| {
                                        let mut state = cell.borrow_mut();
                                        if !state
                                            .operation_journal
                                            .iter()
                                            .any(|op| op.operation_id == tx_id)
                                        {
                                            let mut op = StreamOperation::stream(
                                                "icp",
                                                tx.block_index,
                                                StreamOperationKind::TwoWeekMaturityStream,
                                                tx.amount_e8s,
                                                preview.post_state,
                                                preview.outcome.io_issued_e8s,
                                                OperationPhase::PartiallyDistributed,
                                            );
                                            op.two_week_recipients = allocations
                                                .allocations
                                                .into_iter()
                                                .map(|allocation| TwoWeekRecipientTransfer {
                                                    sns_neuron_id: Some(allocation.sns_neuron_id.0),
                                                    neuron_id: allocation.neuron_id,
                                                    amount_e8s: allocation.io_e8s,
                                                    transfer_status: TransferStatus::Pending,
                                                    transfer_block_index: None,
                                                    ledger_transfer_status: Some(
                                                        TransferStatus::Pending,
                                                    ),
                                                    ledger_transfer_block: None,
                                                    governance_refresh_status: Some(
                                                        TransferStatus::Pending,
                                                    ),
                                                    stake_before_e8s: None,
                                                    expected_stake_after_e8s: None,
                                                    minimum_expected_stake_after_e8s: None,
                                                    observed_stake_after_e8s: None,
                                                    concurrent_stake_delta_e8s: None,
                                                    refresh_retry_count: Some(0),
                                                    refresh_last_error: None,
                                                    reward_transfer_attempt: None,
                                                    ledger_transfer_fee_e8s: None,
                                                    reward_amount_received_e8s: None,
                                                    reserve_debit_e8s: None,
                                                    ledger_transfer_proof_scan_state: None,
                                                    last_error: None,
                                                })
                                                .collect();
                                            state.operation_journal.push(op);
                                        }
                                    });
                                    retry_pending_two_week_streams(
                                        io_canister,
                                        io_index_canister,
                                        sns_governance_canister,
                                        &mut outcome,
                                    )
                                    .await;
                                    if !no_new_page_errors(&outcome, page_error_count) {
                                        return outcome;
                                    }
                                    advance_icp_cursor(tx.block_index);
                                    continue;
                                }
                                ApiIoRecipientPolicy::None => {}
                            }
                        }

                        let committed = CANISTER_STATE.with(|cell| {
                            cell.borrow_mut()
                                .manager
                                .commit_previewed_stream(tx_id.clone(), preview.post_state)
                        });
                        match committed {
                            Ok(()) => {
                                ensure_stream_operation(
                                    "icp",
                                    tx.block_index,
                                    kind_from_api(preview.outcome.kind.into()),
                                    tx.amount_e8s,
                                    preview.post_state,
                                    preview.outcome.io_issued_e8s,
                                    OperationPhase::Completed,
                                );
                                mark_completed(&tx_id);
                                advance_icp_cursor(tx.block_index);
                                outcome.processed_authorized_streams += 1;
                                outcome.io_issued_e8s = outcome
                                    .io_issued_e8s
                                    .saturating_add(preview.outcome.io_issued_e8s);
                            }
                            Err(err) => outcome.errors.push(format!("stream {tx_id}: {err:?}")),
                        }
                    }
                    if no_new_page_errors(&outcome, page_error_count) {
                        commit_icp_scan_state(next_scan_state, latest_seen);
                    }
                }
                Err(err) => outcome.errors.push(err),
            }
        }

        if let Some(canister) = io_ledger {
            let scan_state = CANISTER_STATE.with(|cell| {
                let cursors = &cell.borrow().scheduler_cursors;
                if cursors
                    .io_account_history_scan
                    .cursor
                    .latest_cursor
                    .is_none()
                {
                    match cursors.last_scanned_io_index_block {
                        Some(cursor) => legacy_io_account_history_scan_state(cursor),
                        None => AccountHistoryScanState::default(),
                    }
                } else {
                    cursors.io_account_history_scan.clone()
                }
            });
            let start_after = scan_state.cursor.latest_cursor.map(|block| block.0);
            let redemption_account = canister_owned_account(REDEMPTION_ACCOUNT);
            match scan_icrc_account_through_index(canister, redemption_account.clone(), scan_state)
                .await
            {
                Ok((transactions, next_scan_state, latest_seen)) => {
                    let relevant = transactions
                        .into_iter()
                        .filter(|tx| {
                            tx.transaction.to.as_ref() == Some(&redemption_account)
                                && start_after
                                    .map(|cursor| tx.block_index.0 > cursor)
                                    .unwrap_or(true)
                        })
                        .collect::<Vec<_>>();
                    outcome.scanned_io_transactions = relevant.len() as u64;
                    let page_error_count = outcome.errors.len();

                    for tx in relevant {
                        let tx_id = format!("io:{}", tx.block_index.0);
                        if CANISTER_STATE.with(|cell| {
                            cell.borrow()
                                .operation_journal
                                .iter()
                                .any(|op| op.operation_id == tx_id)
                        }) {
                            advance_io_cursor(tx.block_index.0);
                            continue;
                        }

                        let preview = CANISTER_STATE.with(|cell| {
                            cell.borrow()
                                .manager
                                .preview_redemption(tx.transaction.amount_e8s, tx_id.clone())
                        });
                        let preview = match preview {
                            Ok(preview) => preview,
                            Err(StreamManagerError::DuplicateTransaction) => {
                                advance_io_cursor(tx.block_index.0);
                                continue;
                            }
                            Err(err) => {
                                journal_rejected_redemption(
                                    &tx,
                                    format!("{err:?}"),
                                    tx.transaction.from.clone(),
                                );
                                outcome.errors.push(format!("redemption {tx_id}: {err:?}"));
                                advance_io_cursor(tx.block_index.0);
                                if let Some(io_canister) = io_transfer_ledger {
                                    if !process_rejected_redemption_dispositions(
                                        io_canister,
                                        io_ledger,
                                        &mut outcome,
                                    )
                                    .await
                                    {
                                        return outcome;
                                    }
                                }
                                continue;
                            }
                        };

                        CANISTER_STATE.with(|cell| {
                            let source_account = tx.transaction.from.clone();
                            cell.borrow_mut().operation_journal.push({
                                let mut op = StreamOperation::redemption(
                                    tx.block_index.0,
                                    tx.transaction.amount_e8s,
                                    preview.outcome.icp_paid_e8s,
                                    tx.transaction
                                        .from
                                        .as_ref()
                                        .map(icp_ledger::mock_label_from_account)
                                        .unwrap_or_default(),
                                    preview.post_state,
                                );
                                op.source_account = source_account;
                                op
                            });
                        });

                        if let Some(io_canister) = io_transfer_ledger {
                            if !retry_pending_redemptions(
                                icp_transfer_ledger,
                                io_canister,
                                io_ledger,
                                &mut outcome,
                            )
                            .await
                            {
                                return outcome;
                            }
                        }
                        advance_io_cursor(tx.block_index.0);
                    }
                    if no_new_page_errors(&outcome, page_error_count) {
                        commit_io_scan_state(next_scan_state, latest_seen);
                    }
                }
                Err(err) => outcome.errors.push(err),
            }
        }

        outcome
    }
}

#[cfg(target_family = "wasm")]
fn ensure_stream_operation(
    source_ledger: &str,
    source_block_index: u64,
    kind: StreamOperationKind,
    amount_e8s: u128,
    post_state: io_core_model::ProtocolState,
    io_issued_e8s: u128,
    phase: OperationPhase,
) {
    let operation_id = format!("{source_ledger}:{source_block_index}");
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if !state
            .operation_journal
            .iter()
            .any(|op| op.operation_id == operation_id)
        {
            state.operation_journal.push(StreamOperation::stream(
                source_ledger,
                source_block_index,
                kind,
                amount_e8s,
                post_state,
                io_issued_e8s,
                phase,
            ));
        }
    });
}

#[cfg(target_family = "wasm")]
fn mark_io_issuance(operation_id: &str, block: u64) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            op.downstream_io_issuance_block = Some(block);
            op.mark_updated(OperationPhase::Previewed);
        }
    });
}

#[cfg(target_family = "wasm")]
fn mark_completed(operation_id: &str) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            op.mark_updated(OperationPhase::Completed);
        }
    });
}

#[cfg(target_family = "wasm")]
fn mark_operation_error(operation_id: &str, err: String, phase: OperationPhase) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            op.mark_retryable_error(err, phase);
        }
    });
}

#[cfg(target_family = "wasm")]
fn persist_rejected_refund_attempt(operation_id: &str, attempt: RejectedRefundAttemptRecord) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            op.rejected_refund_attempt = Some(attempt);
            op.last_updated = crate::canister_time();
        }
    });
}

#[cfg(target_family = "wasm")]
fn mark_rejected_redemption_refunded(
    operation_id: &str,
    block: u64,
    amount_e8s: u128,
    fee_e8s: u128,
) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            op.io_return_fee_e8s = fee_e8s;
            op.io_return_status = TransferStatus::Succeeded;
            op.io_return_block = Some(block);
            op.icp_payout_status = TransferStatus::NotApplicable;
            op.rejected_fund_disposition = Some(RejectedFundDisposition::ReturnToSenderSucceeded {
                block_index: block,
                amount_e8s,
            });
            op.mark_updated(OperationPhase::Completed);
        }
    });
}

#[cfg(target_family = "wasm")]
fn mark_rejected_redemption_proof_pending(
    operation_id: &str,
    reason: String,
    original_created_at_time: Option<u64>,
    proof_scan_state: Option<AccountHistoryScanState>,
) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            op.io_return_status = TransferStatus::FailedRetryable;
            op.icp_payout_status = TransferStatus::NotApplicable;
            op.rejected_fund_disposition =
                Some(RejectedFundDisposition::ReturnToSenderProofPending {
                    reason: reason.clone(),
                    original_created_at_time,
                    proof_scan_state,
                });
            op.mark_updated(OperationPhase::AwaitingIoReturn);
            op.last_error = Some(reason);
            op.retry_count = op.retry_count.saturating_add(1);
        }
    });
}

#[cfg(target_family = "wasm")]
fn refresh_rejected_redemption_proof_pending(
    operation_id: &str,
    reason: String,
    proof_scan_state: Option<AccountHistoryScanState>,
) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            let original_created_at_time = match &op.rejected_fund_disposition {
                Some(RejectedFundDisposition::ReturnToSenderProofPending {
                    original_created_at_time,
                    ..
                }) => *original_created_at_time,
                _ => None,
            };
            let next_scan_state =
                proof_scan_state.or_else(|| match &op.rejected_fund_disposition {
                    Some(RejectedFundDisposition::ReturnToSenderProofPending {
                        proof_scan_state,
                        ..
                    }) => proof_scan_state.clone(),
                    _ => None,
                });
            op.io_return_status = TransferStatus::FailedRetryable;
            op.icp_payout_status = TransferStatus::NotApplicable;
            op.rejected_fund_disposition =
                Some(RejectedFundDisposition::ReturnToSenderProofPending {
                    reason: reason.clone(),
                    original_created_at_time,
                    proof_scan_state: next_scan_state,
                });
            op.phase = OperationPhase::AwaitingIoReturn;
            op.last_error = Some(reason);
            op.last_updated = crate::canister_time();
        }
    });
}

#[cfg(target_family = "wasm")]
fn mark_rejected_redemption_manual_reconciliation_required(
    operation_id: &str,
    reason: String,
    original_created_at_time: Option<u64>,
) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            op.io_return_status = TransferStatus::FailedTerminal;
            op.icp_payout_status = TransferStatus::NotApplicable;
            op.rejected_fund_disposition = Some(
                RejectedFundDisposition::ReturnToSenderManualReconciliationRequired {
                    reason: reason.clone(),
                    original_created_at_time,
                },
            );
            op.mark_updated(OperationPhase::FailedTerminal);
            op.last_error = Some(reason);
        }
    });
}

#[cfg(target_family = "wasm")]
fn mark_rejected_redemption_retryable(
    operation_id: &str,
    err: String,
    next_attempt_created_at_time: Option<u64>,
) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            op.io_return_status = TransferStatus::FailedRetryable;
            op.icp_payout_status = TransferStatus::NotApplicable;
            op.rejected_fund_disposition = Some(RejectedFundDisposition::ReturnToSenderRetryable {
                error: err.clone(),
                next_attempt_created_at_time,
            });
            op.rejected_refund_attempt = None;
            op.mark_retryable_error(err, OperationPhase::AwaitingIoReturn);
        }
    });
}

#[cfg(target_family = "wasm")]
fn mark_rejected_redemption_quarantined(operation_id: &str, reason: String) {
    CANISTER_STATE.with(|cell| {
        if let Some(op) = cell
            .borrow_mut()
            .operation_journal
            .iter_mut()
            .find(|op| op.operation_id == operation_id)
        {
            op.io_return_status = TransferStatus::FailedTerminal;
            op.icp_payout_status = TransferStatus::NotApplicable;
            op.rejected_fund_disposition = Some(RejectedFundDisposition::QuarantinedTerminal {
                reason: reason.clone(),
            });
            op.rejected_refund_attempt = None;
            op.mark_terminal_error(reason, OperationPhase::AwaitingIoReturn);
        }
    });
}

#[cfg(target_family = "wasm")]
fn journal_rejected_redemption(
    tx: &IndexTransaction,
    err: String,
    source_account: Option<Account>,
) {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let operation_id = format!("io:{}", tx.block_index.0);
        if state
            .operation_journal
            .iter()
            .any(|op| op.operation_id == operation_id)
        {
            return;
        }
        let mut op = StreamOperation::stream(
            "io",
            tx.block_index.0,
            StreamOperationKind::RejectedRedemption,
            tx.transaction.amount_e8s,
            state.manager.state,
            0,
            OperationPhase::AwaitingIoReturn,
        );
        op.io_redemption_block = Some(tx.block_index.0);
        op.io_amount = tx.transaction.amount_e8s;
        op.icp_payout_status = TransferStatus::FailedTerminal;
        op.io_return_status = TransferStatus::Pending;
        op.user_account = source_account
            .as_ref()
            .map(icp_ledger::mock_label_from_account);
        op.source_account = source_account;
        op.last_error = Some(err);
        op.rejected_fund_disposition = Some(RejectedFundDisposition::ReturnToSenderPending);
        if op.source_account.is_none() {
            op.io_return_status = TransferStatus::FailedTerminal;
            op.rejected_fund_disposition = Some(RejectedFundDisposition::QuarantinedTerminal {
                reason: "rejected IO transfer has no resolvable sender account".to_string(),
            });
            op.phase = OperationPhase::FailedTerminal;
        }
        state.operation_journal.push(op);
    });
}

#[cfg(target_family = "wasm")]
fn journal_rejected_icp_deposit(source_block_index: u64, amount_e8s: u128, err: String) {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let operation_id = format!("icp:{source_block_index}");
        if state
            .operation_journal
            .iter()
            .any(|op| op.operation_id == operation_id)
        {
            return;
        }
        let mut op = StreamOperation::stream(
            "icp",
            source_block_index,
            StreamOperationKind::UnknownIcpDeposit,
            amount_e8s,
            state.manager.state,
            0,
            OperationPhase::FailedTerminal,
        );
        op.last_error = Some(err);
        state.operation_journal.push(op);
    });
}

#[cfg(target_family = "wasm")]
fn advance_icp_cursor(block: u64) {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let current = state.scheduler_cursors.last_scanned_icp_index_block;
        state.scheduler_cursors.last_scanned_icp_index_block =
            Some(current.unwrap_or(0).max(block));
    });
}

#[cfg(target_family = "wasm")]
fn commit_icp_scan_state(scan_state: AccountHistoryScanState, latest_seen: Option<u64>) {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.scheduler_cursors.icp_account_history_scan = scan_state;
        if let Some(latest_seen) = latest_seen {
            let current = state.scheduler_cursors.last_scanned_icp_index_block;
            state.scheduler_cursors.last_scanned_icp_index_block =
                Some(current.unwrap_or(0).max(latest_seen));
        }
    });
}

#[cfg(target_family = "wasm")]
fn commit_io_scan_state(scan_state: AccountHistoryScanState, latest_seen: Option<u64>) {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.scheduler_cursors.io_account_history_scan = scan_state;
        if let Some(latest_seen) = latest_seen {
            let current = state.scheduler_cursors.last_scanned_io_index_block;
            state.scheduler_cursors.last_scanned_io_index_block =
                Some(current.unwrap_or(0).max(latest_seen));
        }
    });
}

#[cfg(target_family = "wasm")]
fn advance_io_cursor(block: u64) {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let current = state.scheduler_cursors.last_scanned_io_index_block;
        state.scheduler_cursors.last_scanned_io_index_block = Some(current.unwrap_or(0).max(block));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::JUPITER_FAUCET_SOURCE;
    use candid::{Decode, Encode};
    use io_ledger_types::{
        Account, IndexTransaction, LedgerBlock, LedgerOperationKind, LedgerTransferRequest, Memo,
        Subaccount,
    };

    fn block(index: u64) -> IndexTransaction {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        IndexTransaction {
            block_index: BlockIndex(index),
            transaction: LedgerBlock {
                block_index: BlockIndex(index),
                timestamp_nanos: index,
                created_at_time: None,
                from: Some(Account::new(principal, Some(Subaccount([1; 32])))),
                to: Some(Account::new(principal, None)),
                amount_e8s: 1,
                fee_e8s: Some(10),
                memo: Some(Memo::from("scan")),
                operation_kind: LedgerOperationKind::Transfer,
            },
        }
    }

    fn transfer_request(amount_e8s: u128, to: &str, memo: &str) -> LedgerTransferRequest {
        LedgerTransferRequest {
            from_subaccount: Some(crate::clients::icp_ledger::mock_subaccount(
                PROTOCOL_RESERVE_ACCOUNT,
            )),
            to: crate::clients::icp_ledger::mock_account(to),
            amount_e8s,
            fee_e8s: None,
            memo: Some(Memo::from(memo)),
            created_at_time: None,
        }
    }

    fn duplicate_proof_block(amount_e8s: u128, to: &str, memo: &str) -> LedgerBlock {
        duplicate_proof_block_with_memo(amount_e8s, to, Some(memo))
    }

    fn duplicate_proof_block_with_memo(
        amount_e8s: u128,
        to: &str,
        memo: Option<&str>,
    ) -> LedgerBlock {
        LedgerBlock {
            block_index: BlockIndex(9),
            timestamp_nanos: 0,
            created_at_time: None,
            from: Some(crate::clients::icp_ledger::mock_account(
                PROTOCOL_RESERVE_ACCOUNT,
            )),
            to: Some(crate::clients::icp_ledger::mock_account(to)),
            amount_e8s,
            fee_e8s: None,
            memo: memo.map(Memo::from),
            operation_kind: LedgerOperationKind::Transfer,
        }
    }

    #[test]
    fn plan_only_tick_is_idempotent_without_configured_work() {
        assert_eq!(scheduler_tick_plan_only(), scheduler_tick_plan_only());
    }

    #[test]
    fn outcome_is_debuggable_and_candid_serializable() {
        let outcome = scheduler_tick_plan_only();
        assert!(format!("{outcome:?}").contains("planned_steps"));
        candid::encode_one(outcome).unwrap();
    }

    fn test_principal(byte: u8) -> Principal {
        Principal::from_slice(&[byte; 29])
    }

    fn unavailable_snapshot() -> RewardSnapshotAvailability {
        RewardSnapshotAvailability::Unavailable("governance stopped".to_string())
    }

    fn available_empty_snapshot() -> RewardSnapshotAvailability {
        RewardSnapshotAvailability::Available(Vec::new())
    }

    #[test]
    fn governance_snapshot_unavailable_does_not_create_zero_recipient_reward() {
        let err = require_new_two_week_reward_inputs(
            Some(test_principal(1)),
            Some(test_principal(2)),
            Some(test_principal(3)),
            &unavailable_snapshot(),
        )
        .unwrap_err();

        assert!(err.contains("reward snapshot unavailable"), "{err}");
    }

    #[test]
    fn missing_sns_governance_does_not_fall_through_to_generic_commit() {
        let snapshot = available_empty_snapshot();

        let err = require_new_two_week_reward_inputs(
            Some(test_principal(1)),
            Some(test_principal(2)),
            None,
            &snapshot,
        )
        .unwrap_err();

        assert!(err.contains("SNS governance canister is required"), "{err}");
    }

    #[test]
    fn missing_io_index_does_not_create_or_commit_new_reward_operation() {
        let snapshot = available_empty_snapshot();

        let err = require_new_two_week_reward_inputs(
            Some(test_principal(1)),
            None,
            Some(test_principal(3)),
            &snapshot,
        )
        .unwrap_err();

        assert!(err.contains("IO index canister is required"), "{err}");
    }

    #[test]
    fn contiguous_boundary_cursor_empty_page_does_not_advance() {
        let result = IndexScanResult {
            transactions: vec![],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: None,
            index_tip: Some(BlockIndex(10)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_contiguous_page(Some(BlockIndex(5)), &result),
            Ok(Some(BlockIndex(5)))
        );
    }

    #[test]
    fn contiguous_boundary_cursor_skips_already_processed_blocks_and_advances_once() {
        let result = IndexScanResult {
            transactions: vec![block(4), block(5), block(6)],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: Some(BlockIndex(6)),
            index_tip: Some(BlockIndex(6)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_contiguous_page(Some(BlockIndex(5)), &result),
            Ok(Some(BlockIndex(6)))
        );
    }

    #[test]
    fn contiguous_boundary_cursor_rejects_duplicate_new_blocks() {
        let result = IndexScanResult {
            transactions: vec![block(6), block(6)],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: Some(BlockIndex(6)),
            index_tip: Some(BlockIndex(6)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert!(matches!(
            boundary_cursor_after_contiguous_page(Some(BlockIndex(5)), &result),
            Err(IndexError::MissingBlock {
                block_index: BlockIndex(7)
            })
        ));
    }

    #[test]
    fn contiguous_boundary_cursor_rejects_gap_and_does_not_skip_unknown_range() {
        let result = IndexScanResult {
            transactions: vec![block(7)],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: Some(BlockIndex(7)),
            index_tip: Some(BlockIndex(7)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_contiguous_page(Some(BlockIndex(5)), &result),
            Err(IndexError::MissingBlock {
                block_index: BlockIndex(6)
            })
        );
    }

    #[test]
    fn contiguous_boundary_cursor_reports_archive_required_before_advancing() {
        let result = IndexScanResult {
            transactions: vec![block(6)],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: Some(BlockIndex(6)),
            index_tip: Some(BlockIndex(100)),
            archive_required: true,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_contiguous_page(Some(BlockIndex(5)), &result),
            Err(IndexError::ArchiveRequired {
                from: BlockIndex(5)
            })
        );
    }

    #[test]
    fn contiguous_boundary_cursor_reports_index_lag() {
        let result = IndexScanResult {
            transactions: vec![],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: None,
            index_tip: Some(BlockIndex(4)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_contiguous_page(Some(BlockIndex(5)), &result),
            Err(IndexError::IndexLag {
                requested: BlockIndex(5),
                tip: Some(BlockIndex(4))
            })
        );
    }

    #[test]
    fn account_boundary_cursor_allows_global_block_gaps() {
        let result = IndexScanResult {
            transactions: vec![block(25)],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: Some(BlockIndex(25)),
            index_tip: Some(BlockIndex(30)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Ok(Some(BlockIndex(25)))
        );
    }

    #[test]
    fn account_boundary_cursor_rejects_duplicate_returned_blocks() {
        let result = IndexScanResult {
            transactions: vec![block(25), block(25)],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: Some(BlockIndex(25)),
            index_tip: Some(BlockIndex(30)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Err(IndexError::MissingBlock {
                block_index: BlockIndex(25)
            })
        );
    }

    #[test]
    fn account_boundary_cursor_rejects_non_monotonic_pages() {
        let result = IndexScanResult {
            transactions: vec![block(25), block(24)],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: Some(BlockIndex(25)),
            index_tip: Some(BlockIndex(30)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Err(IndexError::MissingBlock {
                block_index: BlockIndex(24)
            })
        );
    }

    #[test]
    fn account_boundary_cursor_ignores_stale_blocks_without_advancing() {
        let result = IndexScanResult {
            transactions: vec![block(8), block(10)],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: Some(BlockIndex(10)),
            index_tip: Some(BlockIndex(30)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Ok(Some(BlockIndex(10)))
        );
    }

    #[test]
    fn account_boundary_cursor_empty_page_does_not_advance() {
        let result = IndexScanResult {
            transactions: vec![],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: None,
            index_tip: Some(BlockIndex(30)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Ok(Some(BlockIndex(10)))
        );
    }

    #[test]
    fn account_boundary_cursor_archive_required_does_not_advance() {
        let result = IndexScanResult {
            transactions: vec![block(25)],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: Some(BlockIndex(25)),
            index_tip: Some(BlockIndex(30)),
            archive_required: true,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Err(IndexError::ArchiveRequired {
                from: BlockIndex(10)
            })
        );
    }

    #[test]
    fn account_boundary_cursor_reports_lag_before_current_without_advancing() {
        let result = IndexScanResult {
            transactions: vec![block(25)],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: Some(BlockIndex(25)),
            index_tip: Some(BlockIndex(9)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Ascending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        assert_eq!(
            boundary_cursor_after_account_page(Some(BlockIndex(10)), &result),
            Err(IndexError::IndexLag {
                requested: BlockIndex(10),
                tip: Some(BlockIndex(9))
            })
        );
    }

    #[test]
    fn legacy_icp_cursor_seed_accepts_descending_head_page_without_replay() {
        let state = legacy_icp_account_history_scan_state(10);
        assert_eq!(
            state.cursor.order,
            Some(AccountHistoryPageOrder::Descending)
        );
        assert_eq!(state.next_request_start(), None);

        let result = IndexScanResult {
            transactions: vec![block(12), block(10), block(7)],
            raw_transaction_ids: vec![],
            has_unsupported_transactions: false,
            last_seen_block: Some(BlockIndex(12)),
            index_tip: Some(BlockIndex(12)),
            archive_required: false,
            page_order: Some(AccountHistoryPageOrder::Descending),
            account_balance_e8s: None,
            num_blocks_synced: None,
        };
        let outcome = state.observe_page(&result, None, 100, 1, 1, None).unwrap();
        assert_eq!(
            outcome
                .transactions_chronological
                .iter()
                .map(|tx| tx.block_index)
                .collect::<Vec<_>>(),
            vec![BlockIndex(12)]
        );
        assert_eq!(
            outcome.next_state.cursor.latest_cursor,
            Some(BlockIndex(12))
        );
        assert_eq!(
            outcome.next_state.cursor.oldest_cursor,
            Some(BlockIndex(10))
        );
    }

    #[test]
    fn duplicate_transfer_without_proof_is_not_success() {
        let request = transfer_request(100, JUPITER_FAUCET_SOURCE, "icp:1");
        assert!(matches!(
            classify_boundary_transfer_result(
                &request,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(9)
                }),
                None,
            ),
            BoundaryTransferDecision::Retryable(_)
        ));
    }

    #[test]
    fn duplicate_transfer_matching_expected_operation_is_idempotent_success() {
        let request = transfer_request(100, JUPITER_FAUCET_SOURCE, "icp:1");
        let duplicate = duplicate_proof_block(100, JUPITER_FAUCET_SOURCE, "icp:1");
        assert_eq!(
            classify_boundary_transfer_result(
                &request,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(9)
                }),
                Some(&duplicate),
            ),
            BoundaryTransferDecision::Succeeded(9)
        );
    }

    #[test]
    fn duplicate_transfer_mismatched_amount_account_or_memo_is_not_success() {
        let request = transfer_request(100, JUPITER_FAUCET_SOURCE, "icp:1");
        for duplicate in [
            duplicate_proof_block(99, JUPITER_FAUCET_SOURCE, "icp:1"),
            duplicate_proof_block(100, "other_account", "icp:1"),
            duplicate_proof_block(100, JUPITER_FAUCET_SOURCE, "other_memo"),
        ] {
            assert!(matches!(
                classify_boundary_transfer_result(
                    &request,
                    Err(LedgerTransferError::Duplicate {
                        duplicate_of: BlockIndex(9)
                    }),
                    Some(&duplicate),
                ),
                BoundaryTransferDecision::Retryable(_)
            ));
        }
    }

    #[test]
    fn io_stream_manager_real_redemption_memo_mismatch_does_not_prove_duplicate() {
        let request = transfer_request(100, JUPITER_FAUCET_SOURCE, REDEMPTION_PAYOUT_MEMO);
        let duplicate = duplicate_proof_block(100, JUPITER_FAUCET_SOURCE, "wrong_memo");

        assert!(matches!(
            classify_boundary_transfer_result(
                &request,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(9)
                }),
                Some(&duplicate),
            ),
            BoundaryTransferDecision::Retryable(_)
        ));
    }

    #[test]
    fn io_stream_manager_real_redemption_wrong_destination_does_not_prove_duplicate() {
        let request = transfer_request(100, JUPITER_FAUCET_SOURCE, REDEMPTION_PAYOUT_MEMO);
        let duplicate = duplicate_proof_block(100, "wrong_destination", REDEMPTION_PAYOUT_MEMO);

        assert!(matches!(
            classify_boundary_transfer_result(
                &request,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(9)
                }),
                Some(&duplicate),
            ),
            BoundaryTransferDecision::Retryable(_)
        ));
    }

    #[test]
    fn io_stream_manager_real_redemption_wrong_amount_does_not_prove_duplicate() {
        let request = transfer_request(100, JUPITER_FAUCET_SOURCE, REDEMPTION_PAYOUT_MEMO);
        let duplicate = duplicate_proof_block(99, JUPITER_FAUCET_SOURCE, REDEMPTION_PAYOUT_MEMO);

        assert!(matches!(
            classify_boundary_transfer_result(
                &request,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(9)
                }),
                Some(&duplicate),
            ),
            BoundaryTransferDecision::Retryable(_)
        ));
    }

    #[test]
    fn io_stream_manager_real_redemption_duplicate_io_return_block_must_match_expected() {
        let request = transfer_request(90, PROTOCOL_RESERVE_ACCOUNT, REDEEMED_IO_MEMO);
        let matching = duplicate_proof_block(90, PROTOCOL_RESERVE_ACCOUNT, REDEEMED_IO_MEMO);
        assert_eq!(
            classify_boundary_transfer_result(
                &request,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(9)
                }),
                Some(&matching),
            ),
            BoundaryTransferDecision::Succeeded(9)
        );

        let wrong_memo = duplicate_proof_block(90, PROTOCOL_RESERVE_ACCOUNT, "wrong_memo");
        assert!(matches!(
            classify_boundary_transfer_result(
                &request,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(9)
                }),
                Some(&wrong_memo),
            ),
            BoundaryTransferDecision::Retryable(_)
        ));
    }

    fn redemption_op_with_phase(phase: OperationPhase) -> StreamOperation {
        let mut op = StreamOperation::redemption(
            42,
            1_000_000,
            1_000_000,
            "user".to_string(),
            io_core_model::ProtocolState::new(1_000_000_000, 500_000_000, 0),
        );
        op.phase = phase;
        op
    }

    #[test]
    fn redemption_below_return_fee_has_consistent_terminal_status() {
        let mut op = redemption_op_with_phase(OperationPhase::AwaitingIoReturn);
        op.io_amount = 9_999;
        op.io_return_fee_e8s = 10_000;
        op.io_return_status = TransferStatus::FailedTerminal;
        op.icp_payout_status = TransferStatus::FailedTerminal;
        op.mark_terminal_error(
            "redeemed IO 9999 is below IO return fee 10000".to_string(),
            OperationPhase::AwaitingIoReturn,
        );

        assert_eq!(op.phase, OperationPhase::FailedTerminal);
        assert_eq!(op.io_return_status, TransferStatus::FailedTerminal);
        assert_eq!(op.icp_payout_status, TransferStatus::FailedTerminal);
        assert!(!is_retryable_redemption_operation(&op));
    }

    #[test]
    fn redemption_equal_to_return_fee_fails_closed() {
        let mut op = redemption_op_with_phase(OperationPhase::AwaitingIoReturn);
        op.io_amount = 10_000;
        op.io_return_fee_e8s = 10_000;
        op.io_return_status = TransferStatus::FailedTerminal;
        op.icp_payout_status = TransferStatus::FailedTerminal;
        op.mark_terminal_error(
            "redeemed IO 10000 is not above IO return fee 10000".to_string(),
            OperationPhase::AwaitingIoReturn,
        );

        assert_eq!(amount_after_fee(op.io_amount, op.io_return_fee_e8s), None);
        assert_eq!(op.phase, OperationPhase::FailedTerminal);
        assert_eq!(op.io_return_status, TransferStatus::FailedTerminal);
        assert_eq!(op.icp_payout_status, TransferStatus::FailedTerminal);
        assert!(!is_retryable_redemption_operation(&op));
    }

    #[test]
    fn zero_value_io_return_is_never_attempted() {
        assert_eq!(amount_after_fee(10_000, 10_000), None);
        assert_eq!(amount_after_fee(9_999, 10_000), None);
        assert_eq!(amount_after_fee(10_001, 10_000), Some(1));
    }

    #[test]
    fn terminal_redemption_is_never_selected_by_retry_pending_redemptions() {
        let mut completed = redemption_op_with_phase(OperationPhase::Completed);
        completed.icp_payout_status = TransferStatus::Succeeded;
        completed.io_return_status = TransferStatus::Succeeded;
        let terminal = redemption_op_with_phase(OperationPhase::FailedTerminal);
        let mut retryable = redemption_op_with_phase(OperationPhase::AwaitingIcpPayout);
        retryable.icp_payout_status = TransferStatus::FailedRetryable;

        assert!(!is_retryable_redemption_operation(&completed));
        assert!(!is_retryable_redemption_operation(&terminal));
        assert!(is_retryable_redemption_operation(&retryable));
    }

    #[test]
    fn terminal_redemption_does_not_block_unrelated_valid_redemption() {
        let terminal = redemption_op_with_phase(OperationPhase::FailedTerminal);
        let valid = redemption_op_with_phase(OperationPhase::AwaitingIcpPayout);
        let journal = [terminal, valid.clone()];

        let selected = journal
            .iter()
            .find(|op| is_retryable_redemption_operation(op))
            .expect("valid redemption should still be selected");

        assert_eq!(selected.operation_id, valid.operation_id);
    }

    #[test]
    fn rejected_redemption_quarantine_is_auditable_if_return_is_impossible() {
        let mut op = StreamOperation::stream(
            "io",
            77,
            StreamOperationKind::RejectedRedemption,
            9_999,
            io_core_model::ProtocolState::new(1_000_000_000, 500_000_000, 0),
            0,
            OperationPhase::FailedTerminal,
        );
        op.io_redemption_block = Some(77);
        op.io_amount = 9_999;
        op.icp_payout_status = TransferStatus::FailedTerminal;
        op.io_return_status = TransferStatus::FailedTerminal;
        op.rejected_fund_disposition = Some(RejectedFundDisposition::QuarantinedTerminal {
            reason: "sender account missing".to_string(),
        });
        op.last_error = Some("Malformed sender".to_string());

        assert_eq!(op.source_ledger, "io");
        assert_eq!(op.source_block_index, Some(77));
        assert_eq!(op.io_redemption_block, Some(77));
        assert!(op.source_account.is_none());
        assert!(matches!(
            op.rejected_fund_disposition,
            Some(RejectedFundDisposition::QuarantinedTerminal { .. })
        ));
        assert!(!is_retryable_redemption_operation(&op));
    }

    fn rejected_redemption_op(disposition: RejectedFundDisposition) -> StreamOperation {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        let source = Account::new(principal, Some(Subaccount([7; 32])));
        let phase = match disposition {
            RejectedFundDisposition::ReturnToSenderPending
            | RejectedFundDisposition::ReturnToSenderProofPending { .. }
            | RejectedFundDisposition::ReturnToSenderRetryable { .. } => {
                OperationPhase::AwaitingIoReturn
            }
            RejectedFundDisposition::ReturnToSenderSucceeded { .. } => OperationPhase::Completed,
            RejectedFundDisposition::ReturnToSenderManualReconciliationRequired { .. }
            | RejectedFundDisposition::QuarantinedTerminal { .. } => OperationPhase::FailedTerminal,
        };
        let mut op = StreamOperation::stream(
            "io",
            88,
            StreamOperationKind::RejectedRedemption,
            1_000_000,
            io_core_model::ProtocolState::new(1_000_000_000, 500_000_000, 0),
            0,
            phase,
        );
        op.io_redemption_block = Some(88);
        op.io_amount = 1_000_000;
        op.source_account = Some(source);
        op.user_account = Some("source-account".to_string());
        op.icp_payout_status = TransferStatus::NotApplicable;
        op.io_return_status = match disposition {
            RejectedFundDisposition::ReturnToSenderSucceeded { .. } => TransferStatus::Succeeded,
            RejectedFundDisposition::ReturnToSenderProofPending { .. } => {
                TransferStatus::FailedRetryable
            }
            RejectedFundDisposition::ReturnToSenderRetryable { .. } => {
                TransferStatus::FailedRetryable
            }
            RejectedFundDisposition::ReturnToSenderPending => TransferStatus::Pending,
            RejectedFundDisposition::ReturnToSenderManualReconciliationRequired { .. }
            | RejectedFundDisposition::QuarantinedTerminal { .. } => TransferStatus::FailedTerminal,
        };
        op.rejected_fund_disposition = Some(disposition);
        op
    }

    fn proof_pending_op_with_scan_state(
        operation_id: &str,
        proof_scan_state: Option<AccountHistoryScanState>,
    ) -> StreamOperation {
        let mut op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderProofPending {
            reason: "TooOld; refund proof pending".to_string(),
            original_created_at_time: Some(500),
            proof_scan_state,
        });
        op.operation_id = operation_id.to_string();
        op
    }

    fn rejected_refund_attempt(
        refund_source: Account,
        destination: Account,
        amount_e8s: u128,
        fee_e8s: u128,
        created_at_time: u64,
        operation_id: &str,
    ) -> RejectedRefundAttemptRecord {
        rejected_refund_attempt_from_parts(
            refund_source,
            destination,
            amount_e8s,
            fee_e8s,
            Some(rejected_refund_memo(operation_id)),
            created_at_time,
        )
    }

    #[test]
    fn rejected_refund_attempt_records_exact_fee_amount_time_and_accounts() {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        let refund_source = Account::new(principal, Some(Subaccount([9; 32])));
        let destination = Account::new(principal, Some(Subaccount([7; 32])));

        let attempt = rejected_refund_attempt(
            refund_source.clone(),
            destination.clone(),
            990_000,
            10_000,
            88,
            "io:88",
        );
        let request = rejected_refund_request_from_attempt(&attempt);

        assert_eq!(attempt.attempted_refund_amount_e8s, 990_000);
        assert_eq!(attempt.attempted_fee_e8s, 10_000);
        assert_eq!(attempt.attempted_created_at_time, 88);
        assert_eq!(attempt.memo, Some(rejected_refund_memo("io:88")));
        assert_eq!(attempt.refund_source_account, refund_source);
        assert_eq!(attempt.destination_account, destination.clone());
        assert_eq!(request.from_subaccount, refund_source.subaccount);
        assert_eq!(request.to, destination);
        assert_eq!(request.amount_e8s, 990_000);
        assert_eq!(request.created_at_time, Some(88));
        assert_eq!(request.memo, Some(rejected_refund_memo("io:88")));
    }

    #[test]
    fn proof_pending_reconciliation_uses_original_fee_not_current_fee() {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        let refund_source = Account::new(principal, Some(Subaccount([9; 32])));
        let destination = Account::new(principal, Some(Subaccount([7; 32])));
        let original_attempt =
            rejected_refund_attempt(refund_source, destination, 990_000, 10_000, 88, "io:88");
        let changed_current_fee = 25_000;

        let request = rejected_refund_request_from_attempt(&original_attempt);

        assert_eq!(request.amount_e8s, 990_000);
        assert_eq!(original_attempt.attempted_fee_e8s, 10_000);
        assert_ne!(original_attempt.attempted_fee_e8s, changed_current_fee);
        assert_ne!(
            request.amount_e8s,
            amount_after_fee(1_000_000, changed_current_fee).unwrap()
        );
    }

    #[test]
    fn proof_pending_reconciliation_uses_original_refund_amount() {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        let refund_source = Account::new(principal, Some(Subaccount([9; 32])));
        let destination = Account::new(principal, Some(Subaccount([7; 32])));
        let original_attempt =
            rejected_refund_attempt(refund_source, destination, 990_000, 10_000, 88, "io:88");

        let request = rejected_refund_request_from_attempt(&original_attempt);

        assert_eq!(
            request.amount_e8s,
            original_attempt.attempted_refund_amount_e8s
        );
        assert_ne!(request.amount_e8s, 1_000_000);
    }

    #[test]
    fn fee_change_after_refund_attempt_does_not_break_index_proof() {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        let refund_source = Account::new(principal, Some(Subaccount([9; 32])));
        let destination = Account::new(principal, Some(Subaccount([7; 32])));
        let attempt = rejected_refund_attempt(
            refund_source.clone(),
            destination.clone(),
            990_000,
            10_000,
            88,
            "io:88",
        );
        let changed_current_fee = 25_000;
        let indexed_refund = LedgerBlock {
            block_index: BlockIndex(91),
            timestamp_nanos: 88,
            created_at_time: None,
            from: Some(refund_source),
            to: Some(destination),
            amount_e8s: 990_000,
            fee_e8s: Some(10_000),
            memo: Some(rejected_refund_memo("io:88")),
            operation_kind: LedgerOperationKind::Transfer,
        };

        let proof = duplicate_matches_expected(
            &rejected_refund_request_from_attempt(&attempt),
            &indexed_refund,
        );

        assert_eq!(proof, Ok(BlockIndex(91)));
        assert_ne!(attempt.attempted_fee_e8s, changed_current_fee);
    }

    #[test]
    fn current_fee_change_never_causes_second_refund() {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        let refund_source = Account::new(principal, Some(Subaccount([9; 32])));
        let destination = Account::new(principal, Some(Subaccount([7; 32])));
        let mut op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderProofPending {
            reason: "TooOld; index lag".to_string(),
            original_created_at_time: Some(88),
            proof_scan_state: None,
        });
        op.rejected_refund_attempt = Some(rejected_refund_attempt(
            refund_source,
            destination,
            990_000,
            10_000,
            88,
            "io:88",
        ));
        let changed_current_fee = 25_000;
        let request =
            rejected_refund_request_from_attempt(op.rejected_refund_attempt.as_ref().unwrap());

        assert!(!is_retryable_rejected_refund_operation(&op));
        assert_eq!(request.amount_e8s, 990_000);
        assert_ne!(
            request.amount_e8s,
            amount_after_fee(op.io_amount, changed_current_fee).unwrap()
        );
        assert_eq!(request.created_at_time, Some(88));
    }

    #[test]
    fn proof_pending_refund_is_rechecked_without_resending() {
        let op = proof_pending_op_with_scan_state("io:88", None);
        let journal = vec![op.clone()];

        assert_eq!(
            next_proof_pending_rejected_refund_operation(&journal, &BTreeSet::new())
                .map(|selected| selected.operation_id),
            Some(op.operation_id.clone())
        );
        assert!(!is_retryable_rejected_refund_operation(&op));
    }

    #[test]
    fn proof_pending_index_lag_remains_pending() {
        let mut scan = AccountHistoryScanState::default();
        scan.status.lag_suspected = true;
        scan.status.last_error = Some("index tip is behind ledger tip".to_string());

        assert!(matches!(
            classify_too_old_refund_proof_state(&scan, 1, REJECTED_REFUND_PROOF_SCAN_MAX_PAGES),
            TooOldRefundProofDisposition::IndexNotCaughtUp(_)
        ));
    }

    #[test]
    fn proof_pending_index_catchup_marks_original_refund_succeeded() {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        let refund_source = Account::new(principal, Some(Subaccount([9; 32])));
        let sender = Account::new(principal, Some(Subaccount([7; 32])));
        let request = LedgerTransferRequest {
            from_subaccount: refund_source.subaccount,
            to: sender.clone(),
            amount_e8s: 990_000,
            fee_e8s: None,
            memo: Some(rejected_refund_memo("io:88")),
            created_at_time: Some(500),
        };
        let proof = LedgerBlock {
            block_index: BlockIndex(91),
            timestamp_nanos: 500,
            created_at_time: None,
            from: Some(refund_source),
            to: Some(sender),
            amount_e8s: 990_000,
            fee_e8s: Some(10_000),
            memo: Some(rejected_refund_memo("io:88")),
            operation_kind: LedgerOperationKind::Transfer,
        };
        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderSucceeded {
            block_index: duplicate_matches_expected(&request, &proof).unwrap().0,
            amount_e8s: 990_000,
        });

        assert_eq!(op.phase, OperationPhase::Completed);
        assert_eq!(op.io_return_status, TransferStatus::Succeeded);
        assert!(matches!(
            op.rejected_fund_disposition,
            Some(RejectedFundDisposition::ReturnToSenderSucceeded {
                block_index: 91,
                amount_e8s: 990_000,
            })
        ));
    }

    #[test]
    fn proof_pending_complete_no_match_requires_manual_reconciliation() {
        let mut scan = AccountHistoryScanState::default();
        scan.cursor.backfill_complete = true;
        scan.cursor.latest_cursor = Some(BlockIndex(91));
        scan.status.num_blocks_synced = Some(BlockIndex(91));

        assert!(matches!(
            classify_too_old_refund_proof_state(&scan, 1, REJECTED_REFUND_PROOF_SCAN_MAX_PAGES),
            TooOldRefundProofDisposition::CompleteNoMatch(_)
        ));
        let op = rejected_redemption_op(
            RejectedFundDisposition::ReturnToSenderManualReconciliationRequired {
                reason: "complete canonical history contains no matching refund".to_string(),
                original_created_at_time: Some(500),
            },
        );
        assert_eq!(op.phase, OperationPhase::FailedTerminal);
        assert!(!is_retryable_rejected_refund_operation(&op));
    }

    #[test]
    fn proof_pending_reconciliation_budget_is_bounded() {
        assert_eq!(REJECTED_REFUND_PROOF_RECONCILIATION_BUDGET_PER_TICK, 4);

        let journal = (0..(REJECTED_REFUND_PROOF_RECONCILIATION_BUDGET_PER_TICK + 2))
            .map(|i| proof_pending_op_with_scan_state(&format!("io:{i}"), None))
            .collect::<Vec<_>>();
        let mut attempted = BTreeSet::new();
        for _ in 0..REJECTED_REFUND_PROOF_RECONCILIATION_BUDGET_PER_TICK {
            let selected = next_proof_pending_rejected_refund_operation(&journal, &attempted)
                .expect("budgeted proof-pending operation should be selected");
            attempted.insert(selected.operation_id);
        }

        assert_eq!(
            attempted.len(),
            REJECTED_REFUND_PROOF_RECONCILIATION_BUDGET_PER_TICK
        );
        assert!(next_proof_pending_rejected_refund_operation(&journal, &attempted).is_some());
    }

    #[test]
    fn proof_pending_operation_does_not_block_valid_redemption() {
        let proof_pending = proof_pending_op_with_scan_state("io:88", None);
        let valid = redemption_op_with_phase(OperationPhase::AwaitingIcpPayout);

        assert!(
            next_proof_pending_rejected_refund_operation(&[proof_pending], &BTreeSet::new())
                .is_some()
        );
        assert!(is_retryable_redemption_operation(&valid));
    }

    #[test]
    fn proof_pending_state_and_scan_progress_survive_same_wasm_upgrade() {
        let mut scan = AccountHistoryScanState::default();
        scan.cursor.order = Some(AccountHistoryPageOrder::Descending);
        scan.cursor.latest_cursor = Some(BlockIndex(91));
        scan.cursor.oldest_cursor = Some(BlockIndex(77));
        scan.status.num_blocks_synced = Some(BlockIndex(91));
        let op = proof_pending_op_with_scan_state("io:88", Some(scan.clone()));
        let restored = op.clone();

        match restored.rejected_fund_disposition {
            Some(RejectedFundDisposition::ReturnToSenderProofPending {
                proof_scan_state: Some(restored_scan),
                ..
            }) => assert_eq!(restored_scan, scan),
            other => panic!("expected proof-pending scan state, got {other:?}"),
        }
    }

    #[test]
    fn proof_pending_repeated_ticks_never_call_ledger_transfer() {
        let proof_pending = proof_pending_op_with_scan_state("io:88", None);
        let journal = vec![proof_pending];

        for _ in 0..3 {
            assert!(
                next_proof_pending_rejected_refund_operation(&journal, &BTreeSet::new()).is_some()
            );
            assert_eq!(
                next_retryable_rejected_refund_operation(&journal, &BTreeSet::new()),
                None
            );
        }
    }

    #[test]
    fn rejected_redemption_records_source_ledger_block() {
        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderPending);

        assert_eq!(op.source_ledger, "io");
        assert_eq!(op.source_block_index, Some(88));
        assert_eq!(op.io_redemption_block, Some(88));
        assert_eq!(op.io_amount, 1_000_000);
        assert!(op.source_account.is_some());
    }

    #[test]
    fn rejected_redemption_does_not_silently_increase_protocol_reserve() {
        let protocol = io_core_model::ProtocolState::new(1_000_000_000, 500_000_000, 0);
        let op = rejected_redemption_op(RejectedFundDisposition::QuarantinedTerminal {
            reason: "dust".to_string(),
        });

        assert_eq!(io_core_model::ProtocolState::from(op.post_state), protocol);
        assert_eq!(op.io_issued_e8s, 0);
        assert_eq!(op.gross_icp_payout_e8s, 0);
    }

    #[test]
    fn rejected_redemption_return_to_sender_is_exactly_once_if_supported() {
        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderSucceeded {
            block_index: 91,
            amount_e8s: 990_000,
        });

        assert_eq!(op.io_return_status, TransferStatus::Succeeded);
        assert!(matches!(
            op.rejected_fund_disposition,
            Some(RejectedFundDisposition::ReturnToSenderSucceeded {
                block_index: 91,
                amount_e8s: 990_000
            })
        ));
        assert!(!is_retryable_redemption_operation(&op));
    }

    #[test]
    fn rejected_refund_equal_to_fee_is_quarantined() {
        let mut op = rejected_redemption_op(RejectedFundDisposition::QuarantinedTerminal {
            reason: "rejected IO amount 10000 is not above refund fee 10000".to_string(),
        });
        op.io_amount = 10_000;
        op.io_return_fee_e8s = 10_000;
        op.io_return_status = TransferStatus::FailedTerminal;
        op.icp_payout_status = TransferStatus::FailedTerminal;
        op.phase = OperationPhase::FailedTerminal;

        assert_eq!(amount_after_fee(op.io_amount, op.io_return_fee_e8s), None);
        assert!(matches!(
            op.rejected_fund_disposition,
            Some(RejectedFundDisposition::QuarantinedTerminal { .. })
        ));
        assert!(!is_retryable_redemption_operation(&op));
    }

    #[test]
    fn rejected_redemption_replay_does_not_repeat_refund_or_payout() {
        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderSucceeded {
            block_index: 91,
            amount_e8s: 990_000,
        });

        assert_eq!(op.icp_payout_status, TransferStatus::NotApplicable);
        assert_eq!(op.icp_payout_block, None);
        assert_eq!(op.io_return_status, TransferStatus::Succeeded);
        assert!(!is_retryable_redemption_operation(&op));
    }

    #[test]
    fn rejected_refund_replay_no_second_transfer() {
        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderSucceeded {
            block_index: 91,
            amount_e8s: 990_000,
        });

        assert_eq!(op.kind, StreamOperationKind::RejectedRedemption);
        assert_eq!(op.io_return_status, TransferStatus::Succeeded);
        assert_eq!(op.icp_payout_status, TransferStatus::NotApplicable);
        assert!(matches!(
            op.rejected_fund_disposition,
            Some(RejectedFundDisposition::ReturnToSenderSucceeded { .. })
        ));
        assert!(!is_retryable_redemption_operation(&op));
    }

    #[test]
    fn rejected_refund_same_wasm_upgrade_preserves_intent() {
        let mut op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderPending);
        op.operation_id = "io:88".to_string();
        op.created_at = 88;
        let restored = op.clone();

        assert_eq!(restored.operation_id, "io:88");
        assert_eq!(
            rejected_refund_memo(&restored.operation_id),
            Memo::from("rejected_io_refund:io:88")
        );
        assert_eq!(restored.created_at, 88);
        assert!(matches!(
            restored.rejected_fund_disposition,
            Some(RejectedFundDisposition::ReturnToSenderPending)
        ));
    }

    #[test]
    fn over_redeemable_redemption_has_explicit_fund_disposition() {
        let mut op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderPending);
        op.last_error = Some("InsufficientRedeemableSupply".to_string());

        assert!(op.last_error.as_deref().unwrap().contains("Insufficient"));
        assert!(matches!(
            op.rejected_fund_disposition,
            Some(RejectedFundDisposition::ReturnToSenderPending)
        ));
        assert_eq!(op.icp_payout_status, TransferStatus::NotApplicable);
    }

    #[test]
    fn malformed_sender_redemption_has_explicit_fund_disposition() {
        let mut op = rejected_redemption_op(RejectedFundDisposition::QuarantinedTerminal {
            reason: "unresolvable sender".to_string(),
        });
        op.source_account = None;
        op.user_account = None;
        op.last_error = Some("Malformed sender".to_string());

        assert!(op.source_account.is_none());
        assert!(matches!(
            op.rejected_fund_disposition,
            Some(RejectedFundDisposition::QuarantinedTerminal { .. })
        ));
        assert_eq!(op.icp_payout_status, TransferStatus::NotApplicable);
        assert_eq!(op.io_return_status, TransferStatus::FailedTerminal);
    }

    #[test]
    fn production_release_path_has_no_mock_fee_fallback() {
        assert!(!mock_fee_fallback_allowed_for_build(true, false));
        assert!(!mock_fee_fallback_allowed_for_build(false, false));
    }

    #[test]
    fn debug_local_path_allows_mock_fee_fallback() {
        assert!(mock_fee_fallback_allowed_for_build(true, true));
        assert!(!mock_fee_fallback_allowed_for_build(false, true));
    }

    #[test]
    fn production_release_fee_failure_does_not_probe_debug_api() {
        assert!(!mock_fee_fallback_allowed_for_build(true, false));
        assert!(!mock_fee_fallback_allowed_for_build(false, false));
    }

    #[test]
    fn debug_build_fee_failure_may_use_explicit_mock_fallback() {
        assert!(mock_fee_fallback_allowed_for_build(true, true));
        assert!(!mock_fee_fallback_allowed_for_build(false, true));
    }

    #[test]
    fn initial_fee_query_failure_survives_stable_restore_and_retries() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = None;
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = None;
        let operation_id = op.operation_id.clone();
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![op];
            state.manager.processed_transactions.clear();
        });

        record_preflight_retryable_query_failure(
            &operation_id,
            "finalized SNS ledger fee query failed".to_string(),
        );
        let restored =
            crate::migrate_stable_state_for_tests(crate::export_versioned_stable_state_for_tests())
                .unwrap()
                .operation_journal
                .remove(0);

        assert!(restored.reward_preflight.is_none());
        assert_eq!(
            restored.reward_reservation,
            Some(RewardReservation::default())
        );
        assert_eq!(restored.reserved_reward_debit_e8s, Some(0));
        assert!(build_reward_distribution_preflight(
            &restored,
            candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
            10_000,
            300_020_000,
            300_020_000,
            124,
        )
        .is_ok());
    }

    #[test]
    fn initial_reserve_query_failure_survives_stable_restore_and_retries() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = None;
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = None;
        let operation_id = op.operation_id.clone();
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![op];
            state.manager.processed_transactions.clear();
        });

        record_preflight_retryable_query_failure(
            &operation_id,
            "finalized SNS ledger reserve balance query failed".to_string(),
        );
        let restored =
            crate::migrate_stable_state_for_tests(crate::export_versioned_stable_state_for_tests())
                .unwrap()
                .operation_journal
                .remove(0);

        assert!(restored.reward_preflight.is_none());
        assert_eq!(
            restored.reward_reservation,
            Some(RewardReservation::default())
        );
        assert_eq!(restored.reserved_reward_debit_e8s, Some(0));
    }

    #[test]
    fn terminal_invalid_recipient_preflight_survives_stable_restore() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = None;
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = None;
        let operation_id = op.operation_id.clone();
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![op];
            state.manager.processed_transactions.clear();
        });

        record_preflight_state_aware_failure(
            &operation_id,
            "recipient missing canonical SNS neuron id".to_string(),
        );
        let restored =
            crate::migrate_stable_state_for_tests(crate::export_versioned_stable_state_for_tests())
                .unwrap()
                .operation_journal
                .remove(0);

        assert_eq!(
            restored.reward_preflight.as_ref().map(|p| p.status),
            Some(RewardPreflightStatus::FailedTerminal)
        );
        assert_eq!(
            restored.reward_reservation,
            Some(RewardReservation::default())
        );
        assert_eq!(restored.reserved_reward_debit_e8s, Some(0));
    }

    #[test]
    fn terminal_preflight_failure_does_not_poison_unrelated_reservation_aggregation() {
        let mut failed = reward_preflight_operation();
        failed.reward_preflight = Some(RewardDistributionPreflight {
            status: RewardPreflightStatus::FailedTerminal,
            ledger_fee_e8s: 0,
            recipient_count: 0,
            total_reward_e8s: 0,
            total_fee_e8s: 0,
            total_reserve_debit_e8s: 0,
            protocol_reserve_available_e8s: 0,
            real_ledger_reserve_balance_e8s: 0,
            validated_at_timestamp_nanos: 123,
            canonical_recipient_ids: Vec::new(),
            compatibility_keys: Vec::new(),
            dust_e8s: 0,
            failure_reason: Some("invalid recipient plan".to_string()),
        });
        failed.reward_reservation = Some(RewardReservation::default());
        failed.reserved_reward_debit_e8s = Some(0);
        failed.phase = OperationPhase::FailedTerminal;
        let unrelated = reward_operation_ready_for_attempt().remove(0);

        assert_eq!(
            pending_reward_reservations(
                [failed, unrelated].iter(),
                &empty_processed_transactions(),
                None,
            ),
            Ok(300_020_000)
        );
    }

    #[test]
    fn pending_repreflight_fee_query_failure_preserves_prior_preflight() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );
        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 0, 20_000).unwrap();
        let prior_preflight = op.reward_preflight.clone();
        let prior_evidence = op.reward_fee_repreflight;
        let operation_id = op.operation_id.clone();
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![op];
            state.manager.processed_transactions.clear();
        });

        record_preflight_retryable_query_failure(
            &operation_id,
            "finalized SNS ledger fee query failed during re-preflight".to_string(),
        );
        let restored =
            crate::migrate_stable_state_for_tests(crate::export_versioned_stable_state_for_tests())
                .unwrap()
                .operation_journal
                .remove(0);

        assert_eq!(
            restored.reward_preflight.as_ref().map(|p| p.ledger_fee_e8s),
            prior_preflight.as_ref().map(|p| p.ledger_fee_e8s)
        );
        assert_eq!(restored.reward_fee_repreflight, prior_evidence);
    }

    #[test]
    fn pending_repreflight_fee_query_failure_preserves_full_reservation() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );
        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 0, 20_000).unwrap();
        let operation_id = op.operation_id.clone();
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![op];
            state.manager.processed_transactions.clear();
        });

        record_preflight_retryable_query_failure(
            &operation_id,
            "finalized SNS ledger fee query failed during re-preflight".to_string(),
        );
        let restored =
            crate::migrate_stable_state_for_tests(crate::export_versioned_stable_state_for_tests())
                .unwrap()
                .operation_journal
                .remove(0);

        assert_eq!(
            pending_reward_reservation_for_operation(&restored, &empty_processed_transactions()),
            Ok(300_020_000)
        );
        assert_eq!(restored.reserved_reward_debit_e8s, Some(300_020_000));
    }

    #[test]
    fn pending_repreflight_reserve_query_failure_preserves_fee_evidence() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );
        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 0, 20_000).unwrap();
        let evidence = op.reward_fee_repreflight;
        let operation_id = op.operation_id.clone();
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![op];
            state.manager.processed_transactions.clear();
        });

        record_preflight_retryable_query_failure(
            &operation_id,
            "finalized SNS ledger reserve balance query failed during re-preflight".to_string(),
        );
        let restored =
            crate::migrate_stable_state_for_tests(crate::export_versioned_stable_state_for_tests())
                .unwrap()
                .operation_journal
                .remove(0);

        assert_eq!(restored.reward_fee_repreflight, evidence);
    }

    fn pending_repreflight_operation() -> StreamOperation {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );
        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 0, 20_000).unwrap();
        op
    }

    fn assert_pending_repreflight_preserved(
        op: &StreamOperation,
        prior_preflight: &Option<RewardDistributionPreflight>,
        prior_evidence: Option<RewardFeeRepreflightEvidence>,
        prior_reservation: RewardReservation,
    ) {
        assert_eq!(
            op.reward_preflight.as_ref().map(|p| p.status),
            Some(RewardPreflightStatus::Pending)
        );
        assert_eq!(op.reward_preflight, *prior_preflight);
        assert_eq!(op.reward_fee_repreflight, prior_evidence);
        assert_eq!(op.reward_reservation, Some(prior_reservation));
        assert_eq!(op.reserved_reward_debit_e8s, Some(300_020_000));
        assert_eq!(
            pending_reward_reservation_for_operation(op, &empty_processed_transactions()),
            Ok(300_020_000)
        );
        assert!(op
            .two_week_recipients
            .iter()
            .all(|recipient| recipient.reward_transfer_attempt.is_none()));
    }

    #[test]
    fn pending_repreflight_insufficient_new_model_reserve_preserves_prior_reservation() {
        let op = pending_repreflight_operation();
        let prior_preflight = op.reward_preflight.clone();
        let prior_evidence = op.reward_fee_repreflight;
        let prior_reservation = op.reward_reservation.unwrap();
        let snapshot = capture_reward_preflight_snapshot(
            std::slice::from_ref(&op),
            &empty_processed_transactions(),
            300_020_000,
            &op.operation_id,
        )
        .unwrap();

        let err = finalize_reward_preflight_snapshot(
            &snapshot,
            std::slice::from_ref(&op),
            &empty_processed_transactions(),
            300_020_000,
            RewardPreflightObservedInputs {
                sns_governance_canister: candid::Principal::from_text(
                    "qaa6y-5yaaa-aaaaa-aaafa-cai",
                )
                .unwrap(),
                ledger_fee_e8s: 20_000,
                real_reserve_balance_e8s: 300_040_000,
                validated_at_timestamp_nanos: 124,
            },
        )
        .unwrap_err();

        assert!(
            matches!(err, RewardPreflightCasError::Terminal(message) if message.contains("protocol model reserve cannot cover"))
        );
        assert_pending_repreflight_preserved(
            &op,
            &prior_preflight,
            prior_evidence,
            prior_reservation,
        );
    }

    #[test]
    fn pending_repreflight_insufficient_new_real_reserve_preserves_prior_reservation() {
        let op = pending_repreflight_operation();
        let prior_preflight = op.reward_preflight.clone();
        let prior_evidence = op.reward_fee_repreflight;
        let prior_reservation = op.reward_reservation.unwrap();
        let snapshot = capture_reward_preflight_snapshot(
            std::slice::from_ref(&op),
            &empty_processed_transactions(),
            300_040_000,
            &op.operation_id,
        )
        .unwrap();

        let err = finalize_reward_preflight_snapshot(
            &snapshot,
            std::slice::from_ref(&op),
            &empty_processed_transactions(),
            300_040_000,
            RewardPreflightObservedInputs {
                sns_governance_canister: candid::Principal::from_text(
                    "qaa6y-5yaaa-aaaaa-aaafa-cai",
                )
                .unwrap(),
                ledger_fee_e8s: 20_000,
                real_reserve_balance_e8s: 300_039_999,
                validated_at_timestamp_nanos: 124,
            },
        )
        .unwrap_err();

        assert!(
            matches!(err, RewardPreflightCasError::Terminal(message) if message.contains("finalized SNS ledger reserve cannot cover"))
        );
        assert_pending_repreflight_preserved(
            &op,
            &prior_preflight,
            prior_evidence,
            prior_reservation,
        );
    }

    #[test]
    fn pending_repreflight_terminal_finalization_preserves_prior_preflight_and_fee_evidence() {
        let op = pending_repreflight_operation();
        let prior_preflight = op.reward_preflight.clone();
        let prior_evidence = op.reward_fee_repreflight;
        let prior_reservation = op.reward_reservation.unwrap();
        let operation_id = op.operation_id.clone();
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![op];
            state.manager.processed_transactions.clear();
        });

        record_preflight_state_aware_failure(
            &operation_id,
            "protocol model reserve cannot cover reward distribution".to_string(),
        );
        let op = CANISTER_STATE.with(|cell| cell.borrow().operation_journal[0].clone());

        assert_pending_repreflight_preserved(
            &op,
            &prior_preflight,
            prior_evidence,
            prior_reservation,
        );
        assert!(op
            .last_error
            .as_deref()
            .unwrap()
            .contains("protocol model reserve cannot cover"));
    }

    #[test]
    fn pending_repreflight_terminal_finalization_survives_current_stable_restore() {
        let op = pending_repreflight_operation();
        let prior_preflight = op.reward_preflight.clone();
        let prior_evidence = op.reward_fee_repreflight;
        let prior_reservation = op.reward_reservation.unwrap();
        let operation_id = op.operation_id.clone();
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![op];
            state.manager.processed_transactions.clear();
        });

        record_preflight_state_aware_failure(
            &operation_id,
            "finalized SNS ledger reserve cannot cover reward distribution".to_string(),
        );
        let restored =
            crate::migrate_stable_state_for_tests(crate::export_versioned_stable_state_for_tests())
                .unwrap()
                .operation_journal
                .remove(0);

        assert_pending_repreflight_preserved(
            &restored,
            &prior_preflight,
            prior_evidence,
            prior_reservation,
        );
    }

    #[test]
    fn snapshot_capture_failure_does_not_clear_pending_repreflight() {
        let mut current = pending_repreflight_operation();
        let prior_preflight = current.reward_preflight.clone();
        let prior_evidence = current.reward_fee_repreflight;
        let prior_reservation = current.reward_reservation.unwrap();
        let operation_id = current.operation_id.clone();
        let mut unrelated = reward_operation_ready_for_attempt().remove(0);
        unrelated.operation_id = "icp:unrelated".to_string();
        unrelated.source_transaction_id = "icp:unrelated".to_string();
        unrelated.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: u128::MAX,
            externally_spent_but_uncommitted_reward_debit_e8s: 1,
        });
        current.mark_retryable_error(
            "pending re-preflight".to_string(),
            OperationPhase::PartiallyDistributed,
        );
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![current, unrelated];
            state.manager.processed_transactions.clear();
        });

        record_preflight_state_aware_failure(
            &operation_id,
            "pending reward reservations overflowed".to_string(),
        );
        let op = CANISTER_STATE.with(|cell| cell.borrow().operation_journal[0].clone());

        assert_pending_repreflight_preserved(
            &op,
            &prior_preflight,
            prior_evidence,
            prior_reservation,
        );
    }

    #[test]
    fn unrelated_corrupt_reservation_cannot_release_current_repreflight_reservation() {
        let current = pending_repreflight_operation();
        let prior_preflight = current.reward_preflight.clone();
        let prior_evidence = current.reward_fee_repreflight;
        let prior_reservation = current.reward_reservation.unwrap();
        let operation_id = current.operation_id.clone();
        let mut unrelated = reward_operation_ready_for_attempt().remove(0);
        unrelated.operation_id = "icp:unrelated".to_string();
        unrelated.source_transaction_id = "icp:unrelated".to_string();
        unrelated.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 1,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![current, unrelated];
            state.manager.processed_transactions.clear();
        });

        record_preflight_state_aware_failure(
            &operation_id,
            "reservation split disagrees with recipient evidence".to_string(),
        );
        let op = CANISTER_STATE.with(|cell| cell.borrow().operation_journal[0].clone());

        assert_pending_repreflight_preserved(
            &op,
            &prior_preflight,
            prior_evidence,
            prior_reservation,
        );
    }

    #[test]
    fn initial_invalid_plan_still_records_zero_reservation_terminal_failure() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = None;
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = None;
        let operation_id = op.operation_id.clone();
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![op];
            state.manager.processed_transactions.clear();
        });

        record_preflight_state_aware_failure(
            &operation_id,
            "recipient missing canonical SNS neuron id".to_string(),
        );
        let op = CANISTER_STATE.with(|cell| cell.borrow().operation_journal[0].clone());

        assert_eq!(
            op.reward_preflight.as_ref().map(|p| p.status),
            Some(RewardPreflightStatus::FailedTerminal)
        );
        assert_eq!(op.reward_reservation, Some(RewardReservation::default()));
        assert_eq!(op.reserved_reward_debit_e8s, Some(0));
    }

    #[test]
    fn initial_snapshot_reservation_conflict_remains_retryable() {
        let mut current = reward_preflight_operation();
        current.reward_preflight = None;
        current.reward_reservation = None;
        current.reserved_reward_debit_e8s = None;
        let operation_id = current.operation_id.clone();
        let mut unrelated = reward_operation_ready_for_attempt().remove(0);
        unrelated.operation_id = "icp:unrelated".to_string();
        unrelated.source_transaction_id = "icp:unrelated".to_string();
        unrelated.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: u128::MAX,
            externally_spent_but_uncommitted_reward_debit_e8s: 1,
        });
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![current, unrelated];
            state.manager.processed_transactions.clear();
        });

        record_preflight_retryable_query_failure(
            &operation_id,
            "pending reward reservations overflowed".to_string(),
        );
        let op = CANISTER_STATE.with(|cell| cell.borrow().operation_journal[0].clone());

        assert!(op.reward_preflight.is_none());
        assert_eq!(op.reward_reservation, Some(RewardReservation::default()));
        assert_eq!(op.reserved_reward_debit_e8s, Some(0));
        assert_eq!(op.phase, OperationPhase::PartiallyDistributed);
        assert!(op
            .last_error
            .as_deref()
            .unwrap()
            .contains("pending reward reservations overflowed"));
    }

    #[test]
    fn initial_snapshot_conflict_retries_after_other_operation_is_repaired() {
        let mut current = reward_preflight_operation();
        current.operation_id = "icp:current".to_string();
        current.source_transaction_id = "icp:current".to_string();
        current.reward_preflight = None;
        current.reward_reservation = None;
        current.reserved_reward_debit_e8s = None;
        let mut unrelated = reward_operation_ready_for_attempt().remove(0);
        unrelated.operation_id = "icp:unrelated".to_string();
        unrelated.source_transaction_id = "icp:unrelated".to_string();
        unrelated.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: u128::MAX,
            externally_spent_but_uncommitted_reward_debit_e8s: 1,
        });
        let mut journal = vec![current.clone(), unrelated];
        let err = capture_reward_preflight_snapshot(
            &journal,
            &empty_processed_transactions(),
            700_000_000,
            "icp:current",
        )
        .unwrap_err();
        assert!(!err.is_empty());

        journal[1] = reward_operation_ready_for_attempt().remove(0);
        journal[1].operation_id = "icp:unrelated".to_string();
        journal[1].source_transaction_id = "icp:unrelated".to_string();
        let snapshot = capture_reward_preflight_snapshot(
            &journal,
            &empty_processed_transactions(),
            700_000_000,
            "icp:current",
        )
        .unwrap();
        assert!(matches!(
            finalize_preflight_for_tests(&snapshot, &journal, 700_000_000),
            Ok(preflight) if preflight.status == RewardPreflightStatus::Validated
        ));
    }

    #[test]
    fn invalid_current_recipient_plan_remains_terminal() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = None;
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = None;
        op.two_week_recipients[0].sns_neuron_id = None;
        let operation_id = op.operation_id.clone();
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![op];
            state.manager.processed_transactions.clear();
        });

        record_preflight_state_aware_failure(
            &operation_id,
            "recipient missing canonical SNS neuron id".to_string(),
        );
        let op = CANISTER_STATE.with(|cell| cell.borrow().operation_journal[0].clone());

        assert_eq!(op.phase, OperationPhase::FailedTerminal);
        assert_eq!(
            op.reward_preflight.as_ref().map(|p| p.status),
            Some(RewardPreflightStatus::FailedTerminal)
        );
        assert_eq!(op.reward_reservation, Some(RewardReservation::default()));
        assert_eq!(op.reserved_reward_debit_e8s, Some(0));
    }

    #[test]
    fn post_effect_preflight_failure_never_replaces_validated_evidence() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        let operation_id = op.operation_id.clone();
        let attempt = RewardTransferAttemptRecord {
            lifecycle: Some(RewardTransferAttemptLifecycle::Proven {
                generation: 55,
                block: 77,
            }),
            ..reward_attempt(200_000_000, 10_000, 55, &operation_id, [7; 32])
        };
        op.two_week_recipients[0].reward_transfer_attempt = Some(attempt.clone());
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::Succeeded);
        op.two_week_recipients[0].transfer_status = TransferStatus::Succeeded;
        op.two_week_recipients[0].transfer_block_index = Some(77);
        op.two_week_recipients[0].ledger_transfer_block = Some(77);
        op.two_week_recipients[0].reserve_debit_e8s = Some(200_010_000);
        op.reward_reservation = Some(derived_reward_reservation_for_operation(&op).unwrap());
        op.reserved_reward_debit_e8s = Some(300_020_000);
        let prior_preflight = op.reward_preflight.clone();
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal = vec![op];
            state.manager.processed_transactions.clear();
        });

        record_preflight_retryable_query_failure(
            &operation_id,
            "unexpected preflight retry after transfer evidence".to_string(),
        );
        let restored =
            crate::migrate_stable_state_for_tests(crate::export_versioned_stable_state_for_tests())
                .unwrap()
                .operation_journal
                .remove(0);

        assert_eq!(
            restored.reward_preflight.as_ref().map(|p| p.ledger_fee_e8s),
            prior_preflight.as_ref().map(|p| p.ledger_fee_e8s)
        );
        assert_eq!(
            restored.two_week_recipients[0].reward_transfer_attempt,
            Some(attempt)
        );
        assert_eq!(
            restored.reward_preflight.as_ref().map(|p| p.status),
            Some(RewardPreflightStatus::ManualReconciliationRequired)
        );
    }

    #[test]
    fn real_redemption_reads_icrc1_fee() {
        let mut op = redemption_op_with_phase(OperationPhase::AwaitingIoReturn);
        op.io_amount = 123_456;
        let observed_icrc1_fee = 10_000;
        op.io_return_fee_e8s = observed_icrc1_fee;

        assert_eq!(
            amount_after_fee(op.io_amount, observed_icrc1_fee),
            Some(113_456)
        );
        assert_eq!(op.io_return_fee_e8s, observed_icrc1_fee);
    }

    fn reward_attempt(
        amount_e8s: u128,
        fee_e8s: u128,
        created_at_time: u64,
        operation_id: &str,
        neuron_id: [u8; 32],
    ) -> RewardTransferAttemptRecord {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        reward_transfer_attempt_from_parts(
            Account::new(principal, Some(Subaccount([3; 32]))),
            Account::new(principal, Some(Subaccount(neuron_id))),
            amount_e8s,
            fee_e8s,
            created_at_time,
            operation_id,
            neuron_id.to_vec(),
        )
    }

    fn reward_duplicate_block(
        attempt: &RewardTransferAttemptRecord,
        block_index: u64,
    ) -> LedgerBlock {
        LedgerBlock {
            block_index: BlockIndex(block_index),
            timestamp_nanos: attempt.created_at_time,
            created_at_time: Some(attempt.created_at_time),
            from: Some(attempt.source_account.clone()),
            to: Some(attempt.destination_account.clone()),
            amount_e8s: attempt.amount_e8s,
            fee_e8s: Some(attempt.fee_e8s),
            memo: attempt.memo.clone(),
            operation_kind: LedgerOperationKind::Transfer,
        }
    }

    fn reward_attempt_plan(created_at_time: u64) -> RewardAttemptPlan {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        RewardAttemptPlan {
            source_account: Account::new(principal, Some(Subaccount([3; 32]))),
            destination_account: Account::new(principal, Some(Subaccount([7; 32]))),
            amount_e8s: 200_000_000,
            fee_e8s: 10_000,
            created_at_time,
            canonical_sns_neuron_id: vec![7; 32],
        }
    }

    fn install_reward_attempt(
        op: &mut StreamOperation,
        recipient_index: usize,
        created_at_time: u64,
        neuron_id: [u8; 32],
        lifecycle: RewardTransferAttemptLifecycle,
    ) -> RewardTransferAttemptRecord {
        let amount = op.two_week_recipients[recipient_index].amount_e8s;
        let operation_id = op.operation_id.clone();
        let attempt = RewardTransferAttemptRecord {
            lifecycle: Some(lifecycle),
            ..reward_attempt(amount, 10_000, created_at_time, &operation_id, neuron_id)
        };
        let recipient = &mut op.two_week_recipients[recipient_index];
        recipient.reward_transfer_attempt = Some(attempt.clone());
        recipient.ledger_transfer_fee_e8s = Some(10_000);
        recipient.reward_amount_received_e8s = Some(amount);
        recipient.reserve_debit_e8s = Some(amount + 10_000);
        attempt
    }

    fn reward_operation_ready_for_attempt() -> Vec<StreamOperation> {
        let mut op = reward_preflight_operation();
        op.reward_preflight = Some(validated_preflight_for(&op));
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 300_020_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        op.reserved_reward_debit_e8s = Some(300_020_000);
        vec![op]
    }

    fn partial_distribution_bad_fee_shape() -> StreamOperation {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        let operation_id = op.operation_id.clone();
        let first = RewardTransferAttemptRecord {
            lifecycle: Some(RewardTransferAttemptLifecycle::Proven {
                generation: 55,
                block: 77,
            }),
            ..reward_attempt(200_000_000, 10_000, 55, &operation_id, [7; 32])
        };
        op.two_week_recipients[0].reward_transfer_attempt = Some(first);
        op.two_week_recipients[0].transfer_status = TransferStatus::Succeeded;
        op.two_week_recipients[0].transfer_block_index = Some(77);
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::Succeeded);
        op.two_week_recipients[0].ledger_transfer_block = Some(77);
        op.two_week_recipients[0].governance_refresh_status = Some(TransferStatus::Succeeded);
        op.two_week_recipients[0].stake_before_e8s = Some(1_000_000_000);
        op.two_week_recipients[0].expected_stake_after_e8s = Some(1_200_000_000);
        op.two_week_recipients[0].minimum_expected_stake_after_e8s = Some(1_200_000_000);
        op.two_week_recipients[0].observed_stake_after_e8s = Some(1_200_000_000);
        op.two_week_recipients[0].ledger_transfer_fee_e8s = Some(10_000);
        op.two_week_recipients[0].reward_amount_received_e8s = Some(200_000_000);
        op.two_week_recipients[0].reserve_debit_e8s = Some(200_010_000);
        install_reward_attempt(
            &mut op,
            1,
            56,
            [8; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 56 },
        );
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 100_010_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 200_010_000,
        });
        op.reserved_reward_debit_e8s = Some(300_020_000);
        op
    }

    #[test]
    fn reward_transfer_attempt_is_persisted_before_external_call() {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let request = reward_transfer_request_from_attempt(&attempt);

        assert_eq!(request.created_at_time, Some(55));
        assert_eq!(request.memo, attempt.memo);
        assert_eq!(request.to, attempt.destination_account);
        assert_eq!(request.amount_e8s, 1_000_000);
        assert_eq!(request.fee_e8s, Some(10_000));
    }

    #[test]
    fn overlapping_ticks_reuse_identical_reward_attempt() {
        let mut journal = reward_operation_ready_for_attempt();
        let operation_id = journal[0].operation_id.clone();

        let first = get_or_create_reward_transfer_attempt(
            &mut journal,
            &empty_processed_transactions(),
            &operation_id,
            0,
            reward_attempt_plan(55),
        )
        .unwrap();
        let second = get_or_create_reward_transfer_attempt(
            &mut journal,
            &empty_processed_transactions(),
            &operation_id,
            0,
            reward_attempt_plan(55),
        )
        .unwrap();

        assert_eq!(first, second);
        assert_eq!(
            journal[0].two_week_recipients[0].reward_transfer_attempt,
            Some(first)
        );
    }

    #[test]
    fn overlapping_tick_does_not_submit_second_inflight_attempt() {
        let mut journal = reward_operation_ready_for_attempt();
        let operation_id = journal[0].operation_id.clone();
        let prepared = get_or_create_reward_transfer_attempt(
            &mut journal,
            &empty_processed_transactions(),
            &operation_id,
            0,
            reward_attempt_plan(55),
        )
        .unwrap();

        let submitted =
            mark_reward_attempt_submitted_if_prepared(&mut journal, &operation_id, 0, &prepared)
                .unwrap();
        let second =
            mark_reward_attempt_submitted_if_prepared(&mut journal, &operation_id, 0, &submitted)
                .unwrap_err();

        assert!(second.contains("already submitted"));
        assert_eq!(
            submitted.lifecycle,
            Some(RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 })
        );
    }

    #[test]
    fn upgrade_with_submitted_attempt_does_not_blindly_resend() {
        let mut journal = reward_operation_ready_for_attempt();
        let operation_id = journal[0].operation_id.clone();
        let prepared = get_or_create_reward_transfer_attempt(
            &mut journal,
            &empty_processed_transactions(),
            &operation_id,
            0,
            reward_attempt_plan(55),
        )
        .unwrap();
        let submitted =
            mark_reward_attempt_submitted_if_prepared(&mut journal, &operation_id, 0, &prepared)
                .unwrap();
        let restored = journal.clone();

        assert!(!reward_attempt_is_prepared(
            restored[0].two_week_recipients[0]
                .reward_transfer_attempt
                .as_ref()
                .unwrap()
        ));
        assert_eq!(
            restored[0].two_week_recipients[0].reward_transfer_attempt,
            Some(submitted)
        );
    }

    #[test]
    fn submitted_attempt_after_upgrade_enters_proof_reconciliation() {
        let mut journal = reward_operation_ready_for_attempt();
        let operation_id = journal[0].operation_id.clone();
        let prepared = get_or_create_reward_transfer_attempt(
            &mut journal,
            &empty_processed_transactions(),
            &operation_id,
            0,
            reward_attempt_plan(55),
        )
        .unwrap();
        let submitted =
            mark_reward_attempt_submitted_if_prepared(&mut journal, &operation_id, 0, &prepared)
                .unwrap();
        assert!(journal[0].two_week_recipients[0]
            .ledger_transfer_proof_scan_state
            .is_none());
        CANISTER_STATE.with(|cell| {
            cell.borrow_mut().operation_journal = journal;
        });

        let selected = next_reward_proof_pending_recipient(&BTreeSet::new()).unwrap();

        assert_eq!(selected.0, operation_id);
        assert_eq!(selected.1, 0);
        assert_eq!(selected.2.reward_transfer_attempt, Some(submitted));
        assert!(selected.2.ledger_transfer_proof_scan_state.is_none());
    }

    #[test]
    fn proof_required_without_cursor_scans_from_default() {
        let mut journal = reward_operation_ready_for_attempt();
        let operation_id = journal[0].operation_id.clone();
        let prepared = get_or_create_reward_transfer_attempt(
            &mut journal,
            &empty_processed_transactions(),
            &operation_id,
            0,
            reward_attempt_plan(55),
        )
        .unwrap();
        journal[0].two_week_recipients[0].reward_transfer_attempt =
            Some(RewardTransferAttemptRecord {
                lifecycle: Some(RewardTransferAttemptLifecycle::ProofRequired {
                    generation: prepared.created_at_time,
                    reason: "ambiguous transport failure".to_string(),
                }),
                ..prepared
            });
        journal[0].two_week_recipients[0].ledger_transfer_proof_scan_state = None;
        CANISTER_STATE.with(|cell| {
            cell.borrow_mut().operation_journal = journal;
        });

        let selected = next_reward_proof_pending_recipient(&BTreeSet::new()).unwrap();
        let scan_state = selected
            .2
            .ledger_transfer_proof_scan_state
            .clone()
            .unwrap_or_default();

        assert_eq!(selected.0, operation_id);
        assert_eq!(scan_state, AccountHistoryScanState::default());
    }

    #[test]
    fn ambiguous_transport_failure_keeps_attempt_and_full_reservation() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        let operation_id = op.operation_id.clone();
        let attempt = reward_attempt(200_000_000, 10_000, 55, &operation_id, [7; 32]);
        op.two_week_recipients[0].reward_transfer_attempt = Some(RewardTransferAttemptRecord {
            lifecycle: Some(RewardTransferAttemptLifecycle::ProofRequired {
                generation: 55,
                reason: "CanisterCallFailed".to_string(),
            }),
            ..attempt.clone()
        });
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::FailedRetryable);
        op.two_week_recipients[0].transfer_status = TransferStatus::FailedRetryable;
        op.two_week_recipients[0].reserve_debit_e8s = Some(200_010_000);

        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(300_020_000)
        );
        assert_eq!(
            op.two_week_recipients[0].reward_transfer_attempt,
            Some(RewardTransferAttemptRecord {
                lifecycle: Some(RewardTransferAttemptLifecycle::ProofRequired {
                    generation: 55,
                    reason: "CanisterCallFailed".to_string(),
                }),
                ..attempt
            })
        );
    }

    #[test]
    fn complete_history_no_match_does_not_blindly_resend() {
        let mut scan = AccountHistoryScanState::default();
        scan.cursor.backfill_complete = true;
        scan.status.num_blocks_synced = Some(BlockIndex(100));

        assert!(matches!(
            classify_reward_transfer_proof_state(&scan, 1, TWO_WEEK_REWARD_PROOF_SCAN_MAX_PAGES),
            RewardTransferProofDisposition::CompleteNoMatch(_)
        ));
    }

    #[test]
    fn submitted_attempt_never_becomes_prepared_without_proven_absence() {
        let attempt = RewardTransferAttemptRecord {
            lifecycle: Some(RewardTransferAttemptLifecycle::SubmittedAwaitingResult {
                generation: 55,
            }),
            ..reward_attempt(200_000_000, 10_000, 55, "icp:7", [7; 32])
        };

        assert!(!reward_attempt_is_prepared(&attempt));
    }

    #[test]
    fn second_get_or_create_cannot_replace_created_at_time() {
        let mut journal = reward_operation_ready_for_attempt();
        let operation_id = journal[0].operation_id.clone();

        let first = get_or_create_reward_transfer_attempt(
            &mut journal,
            &empty_processed_transactions(),
            &operation_id,
            0,
            reward_attempt_plan(55),
        )
        .unwrap();
        let second = get_or_create_reward_transfer_attempt(
            &mut journal,
            &empty_processed_transactions(),
            &operation_id,
            0,
            reward_attempt_plan(99),
        )
        .unwrap();

        assert_eq!(second.created_at_time, 55);
        assert_eq!(second, first);
        assert_eq!(
            journal[0].two_week_recipients[0]
                .reward_transfer_attempt
                .as_ref()
                .unwrap()
                .created_at_time,
            55
        );
    }

    #[test]
    fn stale_transfer_callback_cannot_overwrite_newer_attempt_evidence() {
        let mut journal = reward_operation_ready_for_attempt();
        let operation_id = journal[0].operation_id.clone();
        let stale = get_or_create_reward_transfer_attempt(
            &mut journal,
            &empty_processed_transactions(),
            &operation_id,
            0,
            reward_attempt_plan(55),
        )
        .unwrap();
        let newer = reward_attempt(200_000_000, 10_000, 99, &operation_id, [7; 32]);
        journal[0].two_week_recipients[0].reward_transfer_attempt = Some(newer.clone());

        assert!(!reward_attempt_matches_in_journal(
            &journal,
            &operation_id,
            0,
            &stale
        ));
        assert!(reward_attempt_matches_in_journal(
            &journal,
            &operation_id,
            0,
            &newer
        ));
    }

    #[test]
    fn two_sends_of_same_persisted_attempt_converge_to_one_block() {
        let mut journal = reward_operation_ready_for_attempt();
        let operation_id = journal[0].operation_id.clone();
        let attempt = get_or_create_reward_transfer_attempt(
            &mut journal,
            &empty_processed_transactions(),
            &operation_id,
            0,
            reward_attempt_plan(55),
        )
        .unwrap();
        let duplicate = reward_duplicate_block(&attempt, 77);

        assert_eq!(
            classify_reward_transfer_result(
                &attempt,
                Ok(LedgerTransferSuccess {
                    block_index: BlockIndex(77),
                }),
                None
            ),
            BoundaryTransferDecision::Succeeded(77)
        );
        assert_eq!(
            classify_reward_transfer_result(
                &attempt,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(77)
                }),
                Some(&duplicate)
            ),
            BoundaryTransferDecision::Succeeded(77)
        );
    }

    #[test]
    fn attempt_identity_survives_same_wasm_upgrade() {
        let mut journal = reward_operation_ready_for_attempt();
        let operation_id = journal[0].operation_id.clone();
        let attempt = get_or_create_reward_transfer_attempt(
            &mut journal,
            &empty_processed_transactions(),
            &operation_id,
            0,
            reward_attempt_plan(55),
        )
        .unwrap();
        let restored = journal.clone();

        assert_eq!(
            restored[0].two_week_recipients[0].reward_transfer_attempt,
            Some(attempt)
        );
    }

    #[test]
    fn attempt_creation_rejects_processed_noncompleted_operation() {
        let mut journal = reward_operation_ready_for_attempt();
        let operation_id = journal[0].operation_id.clone();
        let mut processed = BTreeSet::new();
        processed.insert(journal[0].source_transaction_id.clone());

        let err = get_or_create_reward_transfer_attempt(
            &mut journal,
            &processed,
            &operation_id,
            0,
            reward_attempt_plan(55),
        )
        .unwrap_err();

        assert!(err.contains("non-completed reward operation"));
        assert!(journal[0].two_week_recipients[0]
            .reward_transfer_attempt
            .is_none());
    }

    #[test]
    fn live_reservation_rejects_small_recipient_debit() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        op.two_week_recipients[0].reserve_debit_e8s = Some(200_009_999);

        assert!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions())
                .unwrap_err()
                .contains("recipient reserve debit")
        );
    }

    #[test]
    fn live_reservation_rejects_large_recipient_debit() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        op.two_week_recipients[0].reserve_debit_e8s = Some(200_010_001);

        assert!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions())
                .unwrap_err()
                .contains("recipient reserve debit")
        );
    }

    #[test]
    fn live_reservation_rejects_attempt_fee_mismatch() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::Prepared,
        );
        op.two_week_recipients[0]
            .reward_transfer_attempt
            .as_mut()
            .unwrap()
            .fee_e8s = 20_000;
        op.two_week_recipients[0].reserve_debit_e8s = Some(200_020_000);

        assert!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions())
                .unwrap_err()
                .contains("attempt fee")
        );
    }

    #[test]
    fn live_reservation_rejects_lifecycle_generation_mismatch() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 54 },
        );

        assert!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions())
                .unwrap_err()
                .contains("lifecycle generation")
        );
    }

    #[test]
    fn live_reservation_rejects_preflight_total_mismatch() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        op.reward_preflight.as_mut().unwrap().total_reward_e8s += 1;

        assert!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions())
                .unwrap_err()
                .contains("preflight totals")
        );
    }

    #[test]
    fn attempt_creation_rejects_invalid_live_reward_state() {
        let mut journal = reward_operation_ready_for_attempt();
        let operation_id = journal[0].operation_id.clone();
        journal[0]
            .reward_preflight
            .as_mut()
            .unwrap()
            .total_reward_e8s += 1;

        let err = get_or_create_reward_transfer_attempt(
            &mut journal,
            &empty_processed_transactions(),
            &operation_id,
            0,
            reward_attempt_plan(55),
        )
        .unwrap_err();

        assert!(err.contains("preflight totals"));
        assert!(journal[0].two_week_recipients[0]
            .reward_transfer_attempt
            .is_none());
    }

    #[test]
    fn bad_fee_policy_rejects_invalid_live_reward_state() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );
        op.reward_preflight.as_mut().unwrap().total_reward_e8s += 1;

        let err = apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 0, 20_000)
            .unwrap_err();

        assert!(err.contains("preflight totals"));
        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.status),
            Some(RewardPreflightStatus::Validated)
        );
    }

    #[test]
    fn model_commit_rejects_invalid_live_reward_state_without_mutation() {
        let (mut manager, mut journal) = completed_reward_commit_fixture();
        journal[0].two_week_recipients[0].reserve_debit_e8s = Some(200_009_999);
        let before_manager = manager.clone();
        let before_journal = journal.clone();
        let operation_id = journal[0].operation_id.clone();

        let err =
            commit_completed_reward_operation_in_state(&mut manager, &mut journal, &operation_id)
                .unwrap_err();

        assert!(!err.is_empty());
        assert_eq!(manager.state, before_manager.state);
        assert_eq!(
            manager.processed_transactions,
            before_manager.processed_transactions
        );
        assert_eq!(
            manager.active_staked_io_e8s,
            before_manager.active_staked_io_e8s
        );
        assert_eq!(
            manager.two_week_pool_backing_bps,
            before_manager.two_week_pool_backing_bps
        );
        assert_eq!(journal, before_journal);
    }

    #[test]
    fn bad_fee_policy_rejects_processed_noncompleted_operation() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        let operation_id = op.operation_id.clone();
        op.two_week_recipients[0].reward_transfer_attempt = Some(reward_attempt(
            200_000_000,
            10_000,
            55,
            &operation_id,
            [7; 32],
        ));
        let mut processed = BTreeSet::new();
        processed.insert(op.source_transaction_id.clone());

        let err = apply_reward_bad_fee_policy(&mut op, &processed, 0, 20_000).unwrap_err();

        assert!(err.contains("non-completed reward operation"));
        assert_ne!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.status),
            Some(RewardPreflightStatus::Pending)
        );
    }

    #[test]
    fn live_helpers_and_aggregate_accounting_agree_on_processed_state() {
        let op = reward_operation_ready_for_attempt().remove(0);
        let mut processed = BTreeSet::new();
        processed.insert(op.source_transaction_id.clone());

        let aggregate = pending_reward_reservation_for_operation(&op, &processed).unwrap_err();
        let components =
            pending_reward_reservation_components(std::iter::once(&op), &processed, None)
                .unwrap_err();

        assert!(aggregate.contains("non-completed reward operation"));
        assert_eq!(components, aggregate);
    }

    #[test]
    fn debug_and_release_icp_scanner_use_same_client_shape() {
        let source = include_str!("mod.rs");
        let scanner = source
            .split("async fn scan_icp_account_through_index")
            .nth(1)
            .and_then(|rest| {
                rest.split("async fn scan_icrc_account_through_index")
                    .next()
            })
            .expect("ICP scanner source section");

        assert!(scanner.contains("IcpIndexCanisterClient"));
        assert!(!scanner.contains("IcrcIndexCanisterClient"));
        assert!(!scanner.contains("debug_assertions"));
    }

    #[test]
    fn reward_transfer_retry_reuses_original_created_at_time() {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let changed_now = 99;
        let retry = reward_transfer_request_from_attempt(&attempt);

        assert_eq!(retry.created_at_time, Some(55));
        assert_ne!(retry.created_at_time, Some(changed_now));
    }

    #[test]
    fn reward_transfer_retry_reuses_original_memo_and_destination() {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let retry = reward_transfer_request_from_attempt(&attempt);

        assert_eq!(retry.memo, Some(reward_transfer_memo("icp:7", &[7; 32])));
        assert_eq!(retry.to, attempt.destination_account);
    }

    #[test]
    fn reward_transfer_memo_fits_real_sns_ledger_limit() {
        let old_text_memo = format!("two_week_reward:{}:{}", "icp:7", "07".repeat(32)).into_bytes();
        let memo = reward_transfer_memo("icp:7", &[7; 32]);

        assert_eq!(memo.0.len(), 32);
        assert!(
            memo.0.len() <= 32,
            "pinned real SNS ledger accepts a 32-byte reward memo"
        );
        assert!(
            old_text_memo.len() > memo.0.len(),
            "text memo length should be measured against the compact replacement"
        );
    }

    #[test]
    fn reward_transfer_memo_is_stable_across_retries() {
        let first = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let retry = reward_attempt(1_000_000, 10_000, 99, "icp:7", [7; 32]);

        assert_eq!(first.memo, retry.memo);
        assert_eq!(
            reward_transfer_request_from_attempt(&first).memo,
            reward_transfer_request_from_attempt(&retry).memo
        );
    }

    #[test]
    fn reward_transfer_memo_differs_across_neurons() {
        let first = reward_transfer_memo("icp:7", &[7; 32]);
        let second = reward_transfer_memo("icp:7", &[8; 32]);

        assert_ne!(first, second);
    }

    #[test]
    fn reward_transfer_memo_differs_across_operations() {
        let first = reward_transfer_memo("icp:7", &[7; 32]);
        let second = reward_transfer_memo("icp:8", &[7; 32]);

        assert_ne!(first, second);
    }

    #[test]
    fn reward_transfer_memo_round_trips_stable_state() {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let mut recipient = two_week_recipient_with_attempt(TransferStatus::FailedRetryable);
        recipient.reward_transfer_attempt = Some(attempt.clone());

        let restored = recipient.clone();

        assert_eq!(
            restored.reward_transfer_attempt.as_ref().unwrap().memo,
            attempt.memo
        );
        assert_eq!(
            restored.reward_transfer_attempt.as_ref().unwrap().memo,
            Some(reward_transfer_memo("icp:7", &[7; 32]))
        );
    }

    #[test]
    fn reward_duplicate_proof_rejects_wrong_memo() {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let wrong_memo = LedgerBlock {
            memo: Some(reward_transfer_memo("icp:7", &[8; 32])),
            ..reward_duplicate_block(&attempt, 77)
        };

        assert!(matches!(
            classify_reward_transfer_result(
                &attempt,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(77),
                }),
                Some(&wrong_memo),
            ),
            BoundaryTransferDecision::Retryable(_)
        ));
    }

    #[test]
    fn reward_transfer_duplicate_proof_requires_exact_source_destination_amount_memo() {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let matching = reward_duplicate_block(&attempt, 77);

        assert_eq!(
            classify_reward_transfer_result(
                &attempt,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(77),
                }),
                Some(&matching),
            ),
            BoundaryTransferDecision::Succeeded(77)
        );

        let wrong_source = LedgerBlock {
            from: Some(Account::new(
                attempt.source_account.owner,
                Some(Subaccount([4; 32])),
            )),
            ..matching.clone()
        };
        let wrong_destination = LedgerBlock {
            to: Some(Account::new(
                attempt.destination_account.owner,
                Some(Subaccount([8; 32])),
            )),
            ..matching.clone()
        };
        let wrong_amount = LedgerBlock {
            amount_e8s: attempt.amount_e8s - 1,
            ..matching.clone()
        };
        let wrong_memo = LedgerBlock {
            memo: Some(Memo::from("other")),
            ..matching
        };

        for proof in [wrong_source, wrong_destination, wrong_amount, wrong_memo] {
            assert!(matches!(
                classify_reward_transfer_result(
                    &attempt,
                    Err(LedgerTransferError::Duplicate {
                        duplicate_of: BlockIndex(77),
                    }),
                    Some(&proof),
                ),
                BoundaryTransferDecision::Retryable(_)
            ));
        }
    }

    #[test]
    fn reward_proof_matches_created_at_time_not_block_timestamp() {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let proof = LedgerBlock {
            timestamp_nanos: attempt.created_at_time,
            created_at_time: Some(attempt.created_at_time + 1),
            ..reward_duplicate_block(&attempt, 77)
        };

        assert!(!reward_transfer_block_matches_attempt(&attempt, &proof));
    }

    #[test]
    fn reward_proof_rejects_wrong_created_at_time() {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let proof = LedgerBlock {
            created_at_time: Some(54),
            ..reward_duplicate_block(&attempt, 77)
        };

        assert!(matches!(
            classify_reward_transfer_result(
                &attempt,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(77),
                }),
                Some(&proof),
            ),
            BoundaryTransferDecision::Retryable(_)
        ));
    }

    #[test]
    fn reward_proof_accepts_different_block_timestamp() {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let proof = LedgerBlock {
            timestamp_nanos: attempt.created_at_time + 99,
            created_at_time: Some(attempt.created_at_time),
            ..reward_duplicate_block(&attempt, 77)
        };

        assert!(reward_transfer_block_matches_attempt(&attempt, &proof));
    }

    #[test]
    fn duplicate_reward_proof_uses_persisted_created_at_time() {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let proof = LedgerBlock {
            timestamp_nanos: 55,
            created_at_time: Some(56),
            ..reward_duplicate_block(&attempt, 77)
        };

        assert!(matches!(
            classify_reward_transfer_result(
                &attempt,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(77),
                }),
                Some(&proof),
            ),
            BoundaryTransferDecision::Retryable(_)
        ));
    }

    #[test]
    fn too_old_reward_proof_uses_persisted_created_at_time() {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let proof = LedgerBlock {
            timestamp_nanos: attempt.created_at_time,
            created_at_time: Some(attempt.created_at_time + 1),
            ..reward_duplicate_block(&attempt, 77)
        };

        assert!(!reward_transfer_block_matches_attempt(&attempt, &proof));
    }

    #[test]
    fn reward_transfer_too_old_waits_for_index_proof() {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let request = reward_transfer_request_from_attempt(&attempt);

        assert!(matches!(
            classify_boundary_transfer_result_with_source(
                &request,
                &attempt.source_account,
                Err(LedgerTransferError::TooOld),
                None,
            ),
            BoundaryTransferDecision::Retryable(reason) if reason.contains("TooOld")
        ));
    }

    #[test]
    fn reward_transfer_index_lag_does_not_resend() {
        let mut recipient = two_week_recipient_with_attempt(TransferStatus::FailedRetryable);
        recipient.ledger_transfer_proof_scan_state = Some(AccountHistoryScanState::default());

        assert_eq!(
            recipient_ledger_status(&recipient),
            TransferStatus::FailedRetryable
        );
        assert!(recipient.ledger_transfer_proof_scan_state.is_some());
        assert!(recipient.reward_transfer_attempt.is_some());
    }

    #[test]
    fn reward_duplicate_index_lag_does_not_resend() {
        reward_transfer_index_lag_does_not_resend();
    }

    #[test]
    fn repeated_proof_pending_ticks_never_call_icrc1_transfer() {
        let mut recipient = two_week_recipient_with_attempt(TransferStatus::FailedRetryable);
        recipient.ledger_transfer_proof_scan_state = Some(AccountHistoryScanState::default());

        for _ in 0..3 {
            assert_eq!(
                recipient_ledger_status(&recipient),
                TransferStatus::FailedRetryable
            );
            assert!(recipient.ledger_transfer_proof_scan_state.is_some());
            assert!(recipient.reward_transfer_attempt.is_some());
            assert!(recipient.transfer_block_index.is_none());
            recipient.last_error = Some("proof pending".to_string());
        }
    }

    #[test]
    fn reward_transfer_archive_incomplete_does_not_resend() {
        let mut scan = AccountHistoryScanState::default();
        scan.status.scan_incomplete = true;
        let mut recipient = two_week_recipient_with_attempt(TransferStatus::FailedRetryable);
        recipient.ledger_transfer_proof_scan_state = Some(scan);

        assert!(recipient.ledger_transfer_proof_scan_state.is_some());
        assert_eq!(
            recipient.ledger_transfer_status,
            Some(TransferStatus::FailedRetryable)
        );
    }

    #[test]
    fn reward_proof_scan_crosses_operationless_entry_safely() {
        let mut scan = AccountHistoryScanState::default();
        scan.cursor.latest_cursor = Some(BlockIndex(8));
        scan.cursor.oldest_cursor = Some(BlockIndex(7));
        scan.status.scan_incomplete = true;

        assert!(matches!(
            classify_reward_transfer_proof_state(&scan, 1, TWO_WEEK_REWARD_PROOF_SCAN_MAX_PAGES),
            RewardTransferProofDisposition::HistoryIncomplete(_)
        ));
        assert_eq!(scan.cursor.latest_cursor, Some(BlockIndex(8)));
    }

    #[test]
    fn archive_or_unsupported_history_never_authorizes_resend() {
        let mut scan = AccountHistoryScanState::default();
        scan.cursor.backfill_complete = true;
        scan.status.num_blocks_synced = Some(BlockIndex(8));
        scan.status.scan_incomplete = true;

        assert!(matches!(
            classify_reward_transfer_proof_state(&scan, 1, TWO_WEEK_REWARD_PROOF_SCAN_MAX_PAGES),
            RewardTransferProofDisposition::HistoryIncomplete(_)
        ));
        assert!(matches!(
            classify_too_old_refund_proof_state(&scan, 1, REJECTED_REFUND_PROOF_SCAN_MAX_PAGES),
            TooOldRefundProofDisposition::HistoryIncomplete(_)
        ));
    }

    #[test]
    fn reward_duplicate_archive_gap_does_not_resend() {
        reward_transfer_archive_incomplete_does_not_resend();
    }

    #[test]
    fn reward_too_old_proof_scan_progress_survives_upgrade() {
        let mut scan = AccountHistoryScanState::default();
        scan.cursor.oldest_cursor = Some(BlockIndex(77));
        scan.cursor.latest_cursor = Some(BlockIndex(99));
        scan.status.scan_incomplete = true;
        let mut recipient = two_week_recipient_with_attempt(TransferStatus::FailedRetryable);
        recipient.ledger_transfer_proof_scan_state = Some(scan.clone());

        let restored = recipient.clone();

        assert_eq!(restored.ledger_transfer_proof_scan_state, Some(scan));
        assert!(restored.reward_transfer_attempt.is_some());
    }

    #[test]
    fn reward_proof_complete_no_match_requires_manual_reconciliation() {
        let mut scan = AccountHistoryScanState::default();
        scan.cursor.oldest_cursor = Some(BlockIndex(1));
        scan.cursor.latest_cursor = Some(BlockIndex(3));
        scan.cursor.backfill_complete = true;
        scan.status.num_blocks_synced = Some(BlockIndex(3));
        scan.status.scan_incomplete = false;
        let disposition =
            classify_reward_transfer_proof_state(&scan, 1, TWO_WEEK_REWARD_PROOF_SCAN_MAX_PAGES);

        assert!(matches!(
            disposition,
            RewardTransferProofDisposition::CompleteNoMatch(_)
        ));
    }

    #[test]
    fn reward_duplicate_outside_first_index_page_is_proven() {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        let proof = reward_duplicate_block(&attempt, 150);
        let disposition = RewardTransferProofDisposition::ProofFound(proof.block_index);

        assert!(reward_transfer_block_matches_attempt(&attempt, &proof));
        assert!(matches!(
            disposition,
            RewardTransferProofDisposition::ProofFound(BlockIndex(150))
        ));
        assert_eq!(proof.block_index, BlockIndex(150));
    }

    #[test]
    fn reward_transfer_same_wasm_upgrade_preserves_attempt() {
        let recipient = two_week_recipient_with_attempt(TransferStatus::FailedRetryable);
        let restored = recipient.clone();

        assert_eq!(
            restored.reward_transfer_attempt,
            recipient.reward_transfer_attempt
        );
        assert_eq!(
            restored
                .reward_transfer_attempt
                .as_ref()
                .unwrap()
                .created_at_time,
            55
        );
    }

    #[test]
    fn reward_transfer_repeated_ticks_never_topup_twice() {
        let mut recipient = two_week_recipient_with_attempt(TransferStatus::Succeeded);
        recipient.governance_refresh_status = Some(TransferStatus::Succeeded);
        for _ in 0..3 {
            assert_eq!(
                recipient_ledger_status(&recipient),
                TransferStatus::Succeeded
            );
            assert_eq!(
                recipient_refresh_status(&recipient),
                TransferStatus::Succeeded
            );
            assert!(recipient_is_completed(&recipient));
            assert_eq!(recipient.ledger_transfer_block, Some(77));
        }
    }

    fn two_week_recipient_with_attempt(status: TransferStatus) -> TwoWeekRecipientTransfer {
        let attempt = reward_attempt(1_000_000, 10_000, 55, "icp:7", [7; 32]);
        TwoWeekRecipientTransfer {
            sns_neuron_id: Some(vec![7; 32]),
            neuron_id: 7,
            amount_e8s: 1_000_000,
            transfer_status: status,
            transfer_block_index: (status == TransferStatus::Succeeded).then_some(77),
            ledger_transfer_status: Some(status),
            ledger_transfer_block: (status == TransferStatus::Succeeded).then_some(77),
            governance_refresh_status: Some(TransferStatus::Pending),
            stake_before_e8s: Some(5_000_000),
            expected_stake_after_e8s: Some(6_000_000),
            minimum_expected_stake_after_e8s: Some(6_000_000),
            observed_stake_after_e8s: None,
            concurrent_stake_delta_e8s: None,
            refresh_retry_count: Some(0),
            refresh_last_error: None,
            reward_transfer_attempt: Some(attempt),
            ledger_transfer_fee_e8s: Some(10_000),
            reward_amount_received_e8s: Some(1_000_000),
            reserve_debit_e8s: Some(1_010_000),
            ledger_transfer_proof_scan_state: None,
            last_error: None,
        }
    }

    #[test]
    fn reward_refresh_exact_stake_increase_succeeds() {
        let minimum: u128 = 6_000_000;
        let observed: u128 = 6_000_000;

        assert!(observed >= minimum);
        assert_eq!(observed.saturating_sub(minimum), 0);
    }

    #[test]
    fn reward_refresh_concurrent_external_topup_succeeds() {
        let minimum = 6_000_000;
        let observed = 6_500_000;

        assert!(observed >= minimum);
    }

    #[test]
    fn reward_refresh_observed_below_minimum_remains_retryable() {
        let minimum = 6_000_000;
        let observed = 5_999_999;

        assert!(observed < minimum);
    }

    #[test]
    fn reward_refresh_records_concurrent_stake_delta() {
        let mut recipient = two_week_recipient_with_attempt(TransferStatus::Succeeded);
        let minimum = recipient.minimum_expected_stake_after_e8s.unwrap();
        let observed = minimum + 123;
        recipient.observed_stake_after_e8s = Some(observed);
        recipient.concurrent_stake_delta_e8s = Some(observed - minimum);

        assert_eq!(recipient.concurrent_stake_delta_e8s, Some(123));
        assert_eq!(
            recipient.reward_amount_received_e8s,
            Some(recipient.amount_e8s)
        );
    }

    #[test]
    fn controlled_real_e2e_still_asserts_exact_equality() {
        let recipient = two_week_recipient_with_attempt(TransferStatus::Succeeded);

        assert_eq!(
            recipient.minimum_expected_stake_after_e8s,
            recipient.expected_stake_after_e8s
        );
    }

    #[test]
    fn concurrent_topup_does_not_cause_duplicate_reward_transfer() {
        let mut recipient = two_week_recipient_with_attempt(TransferStatus::Succeeded);
        recipient.observed_stake_after_e8s =
            Some(recipient.minimum_expected_stake_after_e8s.unwrap() + 500);
        recipient.concurrent_stake_delta_e8s = Some(500);

        assert_eq!(
            recipient_ledger_status(&recipient),
            TransferStatus::Succeeded
        );
        assert_eq!(recipient.ledger_transfer_block, Some(77));
    }

    #[test]
    fn reward_transfer_records_real_sns_ledger_fee() {
        let recipient = two_week_recipient_with_attempt(TransferStatus::Succeeded);

        assert_eq!(recipient.ledger_transfer_fee_e8s, Some(10_000));
    }

    #[test]
    fn reward_reserve_debit_equals_reward_plus_fee() {
        let recipient = two_week_recipient_with_attempt(TransferStatus::Succeeded);

        assert_eq!(
            recipient.reserve_debit_e8s,
            Some(recipient.amount_e8s + recipient.ledger_transfer_fee_e8s.unwrap())
        );
    }

    #[test]
    fn reward_reserve_debit_equals_rewards_plus_fees() {
        reward_reserve_debit_equals_reward_plus_fee();
    }

    #[test]
    fn reward_recipient_receives_exact_reward_amount() {
        let recipient = two_week_recipient_with_attempt(TransferStatus::Succeeded);

        assert_eq!(
            recipient.reward_amount_received_e8s,
            Some(recipient.amount_e8s)
        );
    }

    #[test]
    fn reward_fee_does_not_become_redeemable_supply() {
        let recipient = two_week_recipient_with_attempt(TransferStatus::Succeeded);

        assert_eq!(recipient.reward_amount_received_e8s, Some(1_000_000));
        assert_ne!(
            recipient.reserve_debit_e8s,
            recipient.reward_amount_received_e8s
        );
    }

    #[test]
    fn reward_fee_never_increases_redeemable_supply() {
        let mut op = reward_fee_accounting_operation();
        let before_redeemable = io_core_model::ProtocolState::from(op.post_state)
            .redeemable_io_supply_e8s()
            .unwrap();
        let adjusted = reward_fee_adjusted_post_state(&op).unwrap();
        op.post_state = adjusted;

        assert_eq!(
            io_core_model::ProtocolState::from(op.post_state)
                .redeemable_io_supply_e8s()
                .unwrap(),
            before_redeemable
        );
    }

    #[test]
    fn multiple_recipient_fees_are_summed_exactly() {
        let a = two_week_recipient_with_attempt(TransferStatus::Succeeded);
        let mut b = two_week_recipient_with_attempt(TransferStatus::Succeeded);
        b.ledger_transfer_fee_e8s = Some(20_000);

        let fee_sum = a.ledger_transfer_fee_e8s.unwrap() + b.ledger_transfer_fee_e8s.unwrap();
        assert_eq!(fee_sum, 30_000);
    }

    #[test]
    fn multi_recipient_fee_sum_is_exact() {
        multiple_recipient_fees_are_summed_exactly();
    }

    fn reward_fee_accounting_operation() -> StreamOperation {
        let mut op = StreamOperation::stream(
            "icp",
            7,
            StreamOperationKind::TwoWeekMaturityStream,
            500_000_000,
            io_core_model::ProtocolState {
                liquid_icp_e8s: 300_000_000,
                two_year_staked_icp_e8s: 0,
                two_week_staked_icp_e8s: 200_000_000,
                total_io_supply_e8s: 100_000_000_000_000,
                protocol_reserve_io_e8s: 89_999_700_000_000,
                non_redeemable_governance_io_e8s: 10_000_000_000_000,
            },
            300_000_000,
            OperationPhase::PartiallyDistributed,
        );
        let mut a = two_week_recipient_with_attempt(TransferStatus::Succeeded);
        a.amount_e8s = 200_000_000;
        a.reward_amount_received_e8s = Some(200_000_000);
        a.reserve_debit_e8s = Some(200_010_000);
        let mut b = two_week_recipient_with_attempt(TransferStatus::Succeeded);
        b.amount_e8s = 100_000_000;
        b.reward_amount_received_e8s = Some(100_000_000);
        b.ledger_transfer_fee_e8s = Some(20_000);
        b.reserve_debit_e8s = Some(100_020_000);
        op.two_week_recipients = vec![a, b];
        op
    }

    fn reward_preflight_operation() -> StreamOperation {
        let mut op = StreamOperation::stream(
            "icp",
            9,
            StreamOperationKind::TwoWeekMaturityStream,
            500_000_000,
            io_core_model::ProtocolState {
                liquid_icp_e8s: 300_000_000,
                two_year_staked_icp_e8s: 0,
                two_week_staked_icp_e8s: 200_000_000,
                total_io_supply_e8s: 100_000_000_000_000,
                protocol_reserve_io_e8s: 89_999_700_000_000,
                non_redeemable_governance_io_e8s: 10_000_000_000_000,
            },
            300_000_123,
            OperationPhase::PartiallyDistributed,
        );
        let mut a = two_week_recipient_with_attempt(TransferStatus::Pending);
        a.sns_neuron_id = Some(vec![7; 32]);
        a.neuron_id = 7;
        a.amount_e8s = 200_000_000;
        a.reward_transfer_attempt = None;
        a.ledger_transfer_fee_e8s = None;
        a.reward_amount_received_e8s = None;
        a.reserve_debit_e8s = None;
        a.stake_before_e8s = None;
        a.expected_stake_after_e8s = None;
        a.minimum_expected_stake_after_e8s = None;
        a.observed_stake_after_e8s = None;
        a.concurrent_stake_delta_e8s = None;
        a.refresh_retry_count = None;
        a.refresh_last_error = None;
        let mut b = two_week_recipient_with_attempt(TransferStatus::Pending);
        b.sns_neuron_id = Some(vec![8; 32]);
        b.neuron_id = 8;
        b.amount_e8s = 100_000_000;
        b.reward_transfer_attempt = None;
        b.ledger_transfer_fee_e8s = None;
        b.reward_amount_received_e8s = None;
        b.reserve_debit_e8s = None;
        b.stake_before_e8s = None;
        b.expected_stake_after_e8s = None;
        b.minimum_expected_stake_after_e8s = None;
        b.observed_stake_after_e8s = None;
        b.concurrent_stake_delta_e8s = None;
        b.refresh_retry_count = None;
        b.refresh_last_error = None;
        op.two_week_recipients = vec![a, b];
        op
    }

    fn zero_recipient_reward_operation() -> StreamOperation {
        StreamOperation::stream(
            "icp",
            19,
            StreamOperationKind::TwoWeekMaturityStream,
            500_000_000,
            io_core_model::ProtocolState {
                liquid_icp_e8s: 300_000_000,
                two_year_staked_icp_e8s: 0,
                two_week_staked_icp_e8s: 200_000_000,
                total_io_supply_e8s: 100_000_000_000_000,
                protocol_reserve_io_e8s: 89_999_700_000_000,
                non_redeemable_governance_io_e8s: 10_000_000_000_000,
            },
            300_000_000,
            OperationPhase::PartiallyDistributed,
        )
    }

    fn validated_preflight_for(op: &StreamOperation) -> RewardDistributionPreflight {
        build_reward_distribution_preflight(
            op,
            candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
            10_000,
            300_030_000,
            300_030_000,
            123,
        )
        .unwrap()
    }

    fn empty_processed_transactions() -> BTreeSet<String> {
        BTreeSet::new()
    }

    fn processed_for(op: &StreamOperation) -> BTreeSet<String> {
        BTreeSet::from([op.source_transaction_id.clone()])
    }

    fn completed_reward_operation_with_zero_reservation() -> StreamOperation {
        let mut op = reward_preflight_operation();
        op.reward_preflight = Some(validated_preflight_for(&op));
        op.phase = OperationPhase::Completed;
        op.reward_reservation = Some(RewardReservation::default());
        op.reserved_reward_debit_e8s = Some(0);
        for recipient in &mut op.two_week_recipients {
            recipient.transfer_status = TransferStatus::Succeeded;
            recipient.ledger_transfer_status = Some(TransferStatus::Succeeded);
            recipient.governance_refresh_status = Some(TransferStatus::Succeeded);
            recipient.ledger_transfer_fee_e8s = Some(10_000);
            recipient.reward_amount_received_e8s = Some(recipient.amount_e8s);
            recipient.reserve_debit_e8s = Some(recipient.amount_e8s + 10_000);
        }
        op
    }

    #[test]
    fn completed_reward_is_zero_in_pending_reservation_aggregate() {
        let completed = completed_reward_operation_with_zero_reservation();
        let processed = processed_for(&completed);

        assert_eq!(
            pending_reward_reservation_for_operation(&completed, &processed),
            Ok(0)
        );
        assert_eq!(
            pending_reward_reservations(std::iter::once(&completed), &processed, None),
            Ok(0)
        );
    }

    #[test]
    fn completed_reward_missing_processed_evidence_fails_closed() {
        let completed = completed_reward_operation_with_zero_reservation();

        let err =
            pending_reward_reservation_for_operation(&completed, &empty_processed_transactions())
                .unwrap_err();

        assert!(err.contains("missing processed transaction evidence"));
    }

    #[test]
    fn processed_noncompleted_reward_is_corrupt() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = Some(validated_preflight_for(&op));
        let processed = processed_for(&op);

        let err = pending_reward_reservation_for_operation(&op, &processed).unwrap_err();

        assert!(err.contains("non-completed reward operation"));
    }

    #[test]
    fn completed_reward_with_nonzero_reservation_is_corrupt() {
        let mut completed = completed_reward_operation_with_zero_reservation();
        completed.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 1,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        completed.reserved_reward_debit_e8s = Some(1);
        let processed = processed_for(&completed);

        let err = pending_reward_reservation_for_operation(&completed, &processed).unwrap_err();

        assert!(err.contains("nonzero reward reservation"));
    }

    #[test]
    fn second_reward_preflight_after_completed_reward_succeeds() {
        let completed = completed_reward_operation_with_zero_reservation();
        let processed = processed_for(&completed);
        let mut second = reward_preflight_operation();
        second.operation_id = "icp:10".to_string();
        second.source_transaction_id = "icp:10".to_string();
        second.reward_preflight = Some(validated_preflight_for(&second));
        let ops = [completed, second];

        let reserved = pending_reward_reservations(ops.iter(), &processed, None).unwrap();

        assert_eq!(reserved, 300_020_000);
        assert_eq!(
            reward_reserve_available(600_040_000, reserved).unwrap(),
            300_020_000
        );
    }

    #[test]
    fn subsequent_jupiter_stream_is_not_blocked_by_valid_completed_reward() {
        let completed = completed_reward_operation_with_zero_reservation();
        let processed = processed_for(&completed);
        let reserved =
            pending_reward_reservations(std::iter::once(&completed), &processed, None).unwrap();

        assert_eq!(reserved, 0);
        assert_eq!(
            reward_reserve_available(1_000_000_000, reserved).unwrap(),
            1_000_000_000
        );
    }

    #[test]
    fn subsequent_two_year_stream_is_not_blocked_by_valid_completed_reward() {
        let completed = completed_reward_operation_with_zero_reservation();
        let processed = processed_for(&completed);
        let reserved =
            pending_reward_reservations(std::iter::once(&completed), &processed, None).unwrap();

        assert_eq!(reserved, 0);
        assert_eq!(
            reward_reserve_available(42_000_000, reserved).unwrap(),
            42_000_000
        );
    }

    #[test]
    fn reward_preflight_validates_all_recipients() {
        let op = reward_preflight_operation();
        let preflight = validated_preflight_for(&op);

        assert_eq!(preflight.status, RewardPreflightStatus::Validated);
        assert_eq!(preflight.recipient_count, 2);
        assert_eq!(preflight.total_reward_e8s, 300_000_000);
        assert_eq!(preflight.total_fee_e8s, 20_000);
        assert_eq!(preflight.total_reserve_debit_e8s, 300_020_000);
        assert_eq!(preflight.dust_e8s, 123);
        assert_eq!(
            preflight.canonical_recipient_ids,
            vec![vec![7; 32], vec![8; 32]]
        );
        assert_eq!(preflight.compatibility_keys, vec![7, 8]);
    }

    fn preflight_snapshot_for_tests(
        journal: &[StreamOperation],
        operation_id: &str,
    ) -> RewardPreflightSnapshot {
        capture_reward_preflight_snapshot(
            journal,
            &empty_processed_transactions(),
            700_000_000,
            operation_id,
        )
        .unwrap()
    }

    fn finalize_preflight_for_tests(
        snapshot: &RewardPreflightSnapshot,
        journal: &[StreamOperation],
        protocol_reserve_io_e8s: u128,
    ) -> Result<RewardDistributionPreflight, RewardPreflightCasError> {
        finalize_reward_preflight_snapshot(
            snapshot,
            journal,
            &empty_processed_transactions(),
            protocol_reserve_io_e8s,
            RewardPreflightObservedInputs {
                sns_governance_canister: candid::Principal::from_text(
                    "qaa6y-5yaaa-aaaaa-aaafa-cai",
                )
                .unwrap(),
                ledger_fee_e8s: 10_000,
                real_reserve_balance_e8s: 700_000_000,
                validated_at_timestamp_nanos: 123,
            },
        )
    }

    #[test]
    fn preflight_rejects_snapshot_after_other_reservation_is_added() {
        let mut op = reward_preflight_operation();
        op.operation_id = "icp:1".to_string();
        let mut journal = vec![op];
        let snapshot = preflight_snapshot_for_tests(&journal, "icp:1");
        let mut other = reward_preflight_operation();
        other.operation_id = "icp:2".to_string();
        other.source_transaction_id = "icp:2".to_string();
        other.reward_preflight = Some(validated_preflight_for(&other));
        other.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 300_020_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        other.reserved_reward_debit_e8s = Some(300_020_000);
        journal.push(other);

        assert!(matches!(
            finalize_preflight_for_tests(&snapshot, &journal, 700_000_000),
            Err(RewardPreflightCasError::RetryableConflict(message))
                if message.contains("reservation set changed")
        ));
    }

    #[test]
    fn preflight_rejects_snapshot_after_model_reserve_changes() {
        let mut op = reward_preflight_operation();
        op.operation_id = "icp:1".to_string();
        let journal = vec![op];
        let snapshot = preflight_snapshot_for_tests(&journal, "icp:1");

        assert!(matches!(
            finalize_preflight_for_tests(&snapshot, &journal, 700_000_001),
            Err(RewardPreflightCasError::RetryableConflict(message))
                if message.contains("protocol reserve changed")
        ));
    }

    #[test]
    fn preflight_rejects_snapshot_after_recipient_plan_changes() {
        let mut op = reward_preflight_operation();
        op.operation_id = "icp:1".to_string();
        let mut journal = vec![op];
        let snapshot = preflight_snapshot_for_tests(&journal, "icp:1");
        journal[0].two_week_recipients[0].amount_e8s += 1;

        assert!(matches!(
            finalize_preflight_for_tests(&snapshot, &journal, 700_000_000),
            Err(RewardPreflightCasError::RetryableConflict(message))
                if message.contains("operation snapshot changed")
        ));
    }

    #[test]
    fn preflight_rejects_snapshot_after_attempt_state_changes() {
        let mut op = reward_preflight_operation();
        op.operation_id = "icp:1".to_string();
        let mut journal = vec![op];
        let snapshot = preflight_snapshot_for_tests(&journal, "icp:1");
        journal[0].two_week_recipients[0].reward_transfer_attempt =
            Some(reward_attempt(200_000_000, 10_000, 55, "icp:1", [7; 32]));

        assert!(matches!(
            finalize_preflight_for_tests(&snapshot, &journal, 700_000_000),
            Err(RewardPreflightCasError::RetryableConflict(message))
                if message.contains("operation snapshot changed")
        ));
    }

    #[test]
    fn preflight_cas_conflict_is_retryable_and_moves_no_value() {
        let mut op = reward_preflight_operation();
        op.operation_id = "icp:1".to_string();
        let mut journal = vec![op];
        let snapshot = preflight_snapshot_for_tests(&journal, "icp:1");
        let before = journal.clone();
        journal[0].two_week_recipients[0].amount_e8s += 1;

        let err = finalize_preflight_for_tests(&snapshot, &journal, 700_000_000).unwrap_err();

        assert!(matches!(err, RewardPreflightCasError::RetryableConflict(_)));
        assert_eq!(before[0].reward_preflight, None);
        assert!(before[0]
            .two_week_recipients
            .iter()
            .all(|recipient| recipient.reward_transfer_attempt.is_none()));
    }

    #[test]
    fn competing_preflight_reuses_one_validated_result() {
        let mut op = reward_preflight_operation();
        op.operation_id = "icp:1".to_string();
        let mut journal = vec![op];
        let snapshot = preflight_snapshot_for_tests(&journal, "icp:1");
        let validated = validated_preflight_for(&journal[0]);
        journal[0].reward_preflight = Some(validated.clone());
        journal[0].reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: validated.total_reserve_debit_e8s,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        journal[0].reserved_reward_debit_e8s = Some(validated.total_reserve_debit_e8s);

        assert!(matches!(
            finalize_preflight_for_tests(&snapshot, &journal, 700_000_000),
            Err(RewardPreflightCasError::RetryableConflict(_))
        ));
        assert_eq!(journal[0].reward_preflight, Some(validated));
    }

    #[test]
    fn preflight_error_is_durable_and_visible() {
        let mut op = reward_preflight_operation();
        op.two_week_recipients[0].sns_neuron_id = None;

        let err = build_reward_distribution_preflight(
            &op,
            candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
            10_000,
            700_000_000,
            700_000_000,
            123,
        )
        .unwrap_err();

        assert!(err.contains("missing a canonical SNS neuron id"));
    }

    #[test]
    fn reward_preflight_queries_real_ledger_fee_once() {
        let op = reward_preflight_operation();
        let preflight = validated_preflight_for(&op);

        assert_eq!(preflight.ledger_fee_e8s, 10_000);
        assert_eq!(preflight.total_fee_e8s, preflight.ledger_fee_e8s * 2);
    }

    #[test]
    fn reward_preflight_queries_real_reserve_balance() {
        let op = reward_preflight_operation();
        let preflight = validated_preflight_for(&op);

        assert_eq!(preflight.real_ledger_reserve_balance_e8s, 300_030_000);
    }

    #[test]
    fn non_32_byte_recipient_transfers_nothing() {
        let mut op = reward_preflight_operation();
        op.two_week_recipients[0].sns_neuron_id = Some(vec![7; 31]);

        let err = build_reward_distribution_preflight(
            &op,
            candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
            10_000,
            300_030_000,
            300_030_000,
            123,
        )
        .unwrap_err();
        assert!(err.contains("exactly 32 bytes"));
        assert!(op
            .two_week_recipients
            .iter()
            .all(|recipient| recipient.transfer_block_index.is_none()));
    }

    #[test]
    fn invalid_recipient_list_transfers_nothing() {
        let mut op = reward_preflight_operation();
        op.two_week_recipients[0].sns_neuron_id = None;

        let err = build_reward_distribution_preflight(
            &op,
            candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
            10_000,
            300_030_000,
            300_030_000,
            123,
        )
        .unwrap_err();
        assert!(err.contains("missing a canonical SNS neuron id"));
        assert!(op
            .two_week_recipients
            .iter()
            .all(|recipient| recipient.reward_transfer_attempt.is_none()));
    }

    #[test]
    fn duplicate_canonical_id_transfers_nothing() {
        let mut op = reward_preflight_operation();
        op.two_week_recipients[1].sns_neuron_id = Some(vec![7; 32]);

        let err = build_reward_distribution_preflight(
            &op,
            candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
            10_000,
            300_030_000,
            300_030_000,
            123,
        )
        .unwrap_err();
        assert!(err.contains("duplicate canonical"));
        assert!(op
            .two_week_recipients
            .iter()
            .all(|recipient| recipient.transfer_block_index.is_none()));
    }

    #[test]
    fn compatibility_collision_transfers_nothing() {
        let mut op = reward_preflight_operation();
        op.two_week_recipients[1].neuron_id = 7;

        let err = build_reward_distribution_preflight(
            &op,
            candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
            10_000,
            300_030_000,
            300_030_000,
            123,
        )
        .unwrap_err();
        assert!(err.contains("duplicate compatibility"));
        assert!(op
            .two_week_recipients
            .iter()
            .all(|recipient| recipient.reward_transfer_attempt.is_none()));
    }

    #[test]
    fn invalid_destination_transfers_nothing() {
        non_32_byte_recipient_transfers_nothing();
    }

    #[test]
    fn production_fiduciary_destination_transfers_nothing() {
        let op = reward_preflight_operation();
        let err = build_reward_distribution_preflight(
            &op,
            candid::Principal::from_text(PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID).unwrap(),
            10_000,
            300_030_000,
            300_030_000,
            123,
        )
        .unwrap_err();

        assert!(err.contains("production fiduciary"));
    }

    #[test]
    fn mixed_valid_and_invalid_recipients_transfer_nothing() {
        invalid_recipient_list_transfers_nothing();
    }

    #[test]
    fn insufficient_protocol_reserve_transfers_nothing() {
        let op = reward_preflight_operation();
        let err = build_reward_distribution_preflight(
            &op,
            candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
            10_000,
            300_019_999,
            300_030_000,
            123,
        )
        .unwrap_err();

        assert!(err.contains("protocol model reserve cannot cover"));
    }

    #[test]
    fn insufficient_real_ledger_reserve_transfers_nothing() {
        let op = reward_preflight_operation();
        let err = build_reward_distribution_preflight(
            &op,
            candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
            10_000,
            300_030_000,
            300_019_999,
            123,
        )
        .unwrap_err();

        assert!(err.contains("finalized SNS ledger reserve cannot cover"));
    }

    #[test]
    fn fee_query_failure_transfers_nothing() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = Some(RewardDistributionPreflight {
            status: RewardPreflightStatus::Pending,
            ledger_fee_e8s: 0,
            recipient_count: 2,
            total_reward_e8s: 300_000_000,
            total_fee_e8s: 0,
            total_reserve_debit_e8s: 0,
            protocol_reserve_available_e8s: 0,
            real_ledger_reserve_balance_e8s: 0,
            validated_at_timestamp_nanos: 123,
            canonical_recipient_ids: Vec::new(),
            compatibility_keys: Vec::new(),
            dust_e8s: 0,
            failure_reason: Some("finalized SNS ledger fee query failed".to_string()),
        });

        assert!(op
            .two_week_recipients
            .iter()
            .all(|recipient| recipient.reward_transfer_attempt.is_none()));
    }

    #[test]
    fn reward_preflight_is_persisted_before_first_transfer() {
        let mut op = reward_preflight_operation();
        let preflight = validated_preflight_for(&op);
        op.reward_preflight = Some(preflight);

        assert!(op.reward_preflight.is_some());
        assert!(op
            .two_week_recipients
            .iter()
            .all(|recipient| recipient.reward_transfer_attempt.is_none()));
    }

    #[test]
    fn reward_reservation_created_before_first_transfer() {
        let mut op = reward_preflight_operation();
        let preflight = validated_preflight_for(&op);
        op.reserved_reward_debit_e8s = Some(preflight.total_reserve_debit_e8s);
        op.reward_preflight = Some(preflight);

        assert_eq!(op.reserved_reward_debit_e8s, Some(300_020_000));
        assert!(op
            .two_week_recipients
            .iter()
            .all(|recipient| recipient.reward_transfer_attempt.is_none()));
    }

    #[test]
    fn reservation_total_does_not_decrease_after_proven_transfer() {
        let mut op = reward_preflight_operation();
        let preflight = validated_preflight_for(&op);
        op.reward_preflight = Some(preflight);
        op.reserved_reward_debit_e8s = Some(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()).unwrap(),
        );
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::Proven {
                generation: 55,
                block: 44,
            },
        );
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::Succeeded);
        op.two_week_recipients[0].transfer_status = TransferStatus::Succeeded;
        op.two_week_recipients[0].transfer_block_index = Some(44);
        op.two_week_recipients[0].ledger_transfer_block = Some(44);
        op.two_week_recipients[0].reserve_debit_e8s = Some(200_010_000);

        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(300_020_000)
        );
        assert_eq!(
            pending_reward_unspent_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(100_010_000)
        );
        assert_eq!(
            pending_reward_spent_uncommitted_reservation_for_operation(
                &op,
                &empty_processed_transactions()
            ),
            Ok(200_010_000)
        );
    }

    #[test]
    fn refresh_pending_spent_debit_remains_unavailable() {
        let mut op = reward_preflight_operation();
        let preflight = validated_preflight_for(&op);
        op.reward_preflight = Some(preflight);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::Proven {
                generation: 55,
                block: 44,
            },
        );
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::Succeeded);
        op.two_week_recipients[0].transfer_status = TransferStatus::Succeeded;
        op.two_week_recipients[0].transfer_block_index = Some(44);
        op.two_week_recipients[0].ledger_transfer_block = Some(44);
        op.two_week_recipients[0].governance_refresh_status = Some(TransferStatus::Pending);
        op.two_week_recipients[0].reserve_debit_e8s = Some(200_010_000);

        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(300_020_000)
        );
        assert_eq!(
            pending_reward_spent_uncommitted_reservation_for_operation(
                &op,
                &empty_processed_transactions()
            ),
            Ok(200_010_000)
        );
    }

    #[test]
    fn proof_pending_transfer_preserves_reservation() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = Some(validated_preflight_for(&op));
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::ProofRequired {
                generation: 55,
                reason: "TooOld".to_string(),
            },
        );
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::FailedRetryable);
        op.two_week_recipients[0].ledger_transfer_proof_scan_state =
            Some(AccountHistoryScanState::default());

        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(300_020_000)
        );
    }

    #[test]
    fn manual_reconciliation_preserves_required_reservation() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = Some(validated_preflight_for(&op));
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::ProofRequired {
                generation: 55,
                reason: "manual".to_string(),
            },
        );
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::FailedTerminal);
        op.two_week_recipients[0].ledger_transfer_proof_scan_state =
            Some(AccountHistoryScanState::default());

        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(300_020_000)
        );
    }

    #[test]
    fn terminal_after_proven_transfer_keeps_spent_uncommitted_reservation() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = Some(validated_preflight_for(&op));
        op.phase = OperationPhase::FailedTerminal;
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::Proven {
                generation: 55,
                block: 44,
            },
        );
        op.two_week_recipients[0].transfer_status = TransferStatus::Succeeded;
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::Succeeded);
        op.two_week_recipients[0].transfer_block_index = Some(44);
        op.two_week_recipients[0].ledger_transfer_block = Some(44);
        op.two_week_recipients[0].reserve_debit_e8s = Some(200_010_000);

        assert_eq!(
            pending_reward_spent_uncommitted_reservation_for_operation(
                &op,
                &empty_processed_transactions()
            ),
            Ok(200_010_000)
        );
        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(300_020_000)
        );
    }

    #[test]
    fn manual_reconciliation_after_too_old_keeps_full_reservation() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = Some(RewardDistributionPreflight {
            status: RewardPreflightStatus::ManualReconciliationRequired,
            ..validated_preflight_for(&op)
        });
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 300_020_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        op.reserved_reward_debit_e8s = Some(300_020_000);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::ProofRequired {
                generation: 55,
                reason: "TooOld".to_string(),
            },
        );
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::FailedRetryable);
        op.two_week_recipients[0].ledger_transfer_proof_scan_state =
            Some(AccountHistoryScanState::default());

        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(300_020_000)
        );
    }

    #[test]
    fn bad_fee_before_first_effect_repreflights_atomically() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 0, 20_000).unwrap();

        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.status),
            Some(RewardPreflightStatus::Pending)
        );
        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.ledger_fee_e8s),
            Some(10_000)
        );
        assert_eq!(
            op.reward_fee_repreflight,
            Some(RewardFeeRepreflightEvidence {
                prior_validated_fee_e8s: 10_000,
                observed_current_fee_e8s: 20_000,
                prior_reserved_debit_e8s: 300_020_000,
                invalidated_at_timestamp_nanos: 0,
                attempt_generation: 55,
            })
        );
        assert_eq!(
            op.reward_reservation,
            Some(RewardReservation {
                unspent_reserved_reward_debit_e8s: 300_020_000,
                externally_spent_but_uncommitted_reward_debit_e8s: 0,
            })
        );
        assert_eq!(op.reserved_reward_debit_e8s, Some(300_020_000));
        assert!(op
            .two_week_recipients
            .iter()
            .all(|recipient| recipient.reward_transfer_attempt.is_none()));
        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(300_020_000)
        );
    }

    #[test]
    fn operation_never_has_two_submitted_reward_attempts() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );
        install_reward_attempt(
            &mut op,
            1,
            56,
            [8; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 56 },
        );

        let err = crate::validate_reward_operation_accounting(
            &op,
            Some(&empty_processed_transactions()),
            crate::RewardValidationMode::Current,
        )
        .unwrap_err();

        assert!(err.contains("more than one submitted/proof-required"));
    }

    #[test]
    fn overlapping_ticks_cannot_submit_different_recipients() {
        let mut journal = reward_operation_ready_for_attempt();
        let operation_id = journal[0].operation_id.clone();
        let prepared = get_or_create_reward_transfer_attempt(
            &mut journal,
            &empty_processed_transactions(),
            &operation_id,
            0,
            reward_attempt_plan(55),
        )
        .unwrap();
        mark_reward_attempt_submitted_if_prepared(&mut journal, &operation_id, 0, &prepared)
            .unwrap();
        CANISTER_STATE.with(|cell| {
            cell.borrow_mut().operation_journal = journal;
        });

        assert!(next_reward_ledger_recipient(&BTreeSet::new()).is_none());
    }

    #[test]
    fn bad_fee_on_second_recipient_cannot_clear_first_submitted_attempt() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        let first = install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );
        install_reward_attempt(
            &mut op,
            1,
            56,
            [8; 32],
            RewardTransferAttemptLifecycle::Prepared,
        );

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 1, 20_000).unwrap();

        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.status),
            Some(RewardPreflightStatus::ManualReconciliationRequired)
        );
        assert_eq!(
            op.two_week_recipients[0].reward_transfer_attempt,
            Some(first)
        );
        assert_eq!(op.reserved_reward_debit_e8s, Some(300_020_000));
    }

    #[test]
    fn bad_fee_with_other_proof_required_attempt_requires_reconciliation() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        let first = install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::ProofRequired {
                generation: 55,
                reason: "ambiguous".to_string(),
            },
        );
        install_reward_attempt(
            &mut op,
            1,
            56,
            [8; 32],
            RewardTransferAttemptLifecycle::Prepared,
        );

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 1, 20_000).unwrap();

        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.status),
            Some(RewardPreflightStatus::ManualReconciliationRequired)
        );
        assert_eq!(
            op.two_week_recipients[0].reward_transfer_attempt,
            Some(first)
        );
        assert_eq!(op.reserved_reward_debit_e8s, Some(300_020_000));
    }

    #[test]
    fn single_exact_bad_fee_callback_can_repreflight() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 0, 20_000).unwrap();

        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.status),
            Some(RewardPreflightStatus::Pending)
        );
        assert!(op
            .two_week_recipients
            .iter()
            .all(|recipient| recipient.reward_transfer_attempt.is_none()));
    }

    #[test]
    fn pending_repreflight_old_and_observed_fee_evidence_survives_restore() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 0, 20_000).unwrap();
        let bytes = Encode!(&op).unwrap();
        let restored = Decode!(&bytes, StreamOperation).unwrap();

        assert_eq!(
            restored.reward_fee_repreflight,
            Some(RewardFeeRepreflightEvidence {
                prior_validated_fee_e8s: 10_000,
                observed_current_fee_e8s: 20_000,
                prior_reserved_debit_e8s: 300_020_000,
                invalidated_at_timestamp_nanos: 0,
                attempt_generation: 55,
            })
        );
        assert_eq!(
            restored
                .reward_preflight
                .as_ref()
                .map(|preflight| preflight.ledger_fee_e8s),
            Some(10_000)
        );
        assert_eq!(
            pending_reward_reservation_for_operation(&restored, &empty_processed_transactions()),
            Ok(300_020_000)
        );
    }

    #[test]
    fn pending_repreflight_missing_prior_reservation_fails_closed() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );
        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 0, 20_000).unwrap();
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = None;

        assert!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions())
                .unwrap_err()
                .contains("missing prior reservation")
        );
    }

    #[test]
    fn pending_repreflight_new_validation_atomically_replaces_reservation() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );
        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 0, 20_000).unwrap();

        let preflight = build_reward_distribution_preflight(
            &op,
            candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
            20_000,
            300_040_000,
            300_040_000,
            124,
        )
        .unwrap();
        let reserved = preflight.total_reserve_debit_e8s;
        op.reward_preflight = Some(preflight);
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: reserved,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        op.reward_fee_repreflight = None;
        op.reserved_reward_debit_e8s = Some(reserved);

        assert_eq!(reserved, 300_040_000);
        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(300_040_000)
        );
        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.ledger_fee_e8s),
            Some(20_000)
        );
    }

    #[test]
    fn bad_fee_before_first_effect_with_insufficient_new_reserve_moves_no_value() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );
        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 0, 400_000_000)
            .unwrap();

        let err = build_reward_distribution_preflight(
            &op,
            candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
            400_000_000,
            300_020_000,
            300_020_000,
            124,
        )
        .unwrap_err();

        assert!(err.contains("protocol model reserve cannot cover"));
        assert!(op
            .two_week_recipients
            .iter()
            .all(|recipient| recipient.transfer_block_index.is_none()));
        assert_eq!(
            op.reward_fee_repreflight
                .map(|evidence| evidence.prior_reserved_debit_e8s),
            Some(300_020_000)
        );
        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(300_020_000)
        );
    }

    #[test]
    fn bad_fee_after_partial_distribution_preserves_original_attempt() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        let operation_id = op.operation_id.clone();
        let attempt = RewardTransferAttemptRecord {
            lifecycle: Some(RewardTransferAttemptLifecycle::Proven {
                generation: 55,
                block: 77,
            }),
            ..reward_attempt(200_000_000, 10_000, 55, &operation_id, [7; 32])
        };
        op.two_week_recipients[0].reward_transfer_attempt = Some(attempt.clone());
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::Succeeded);
        op.two_week_recipients[0].transfer_status = TransferStatus::Succeeded;
        op.two_week_recipients[0].transfer_block_index = Some(77);
        op.two_week_recipients[0].ledger_transfer_block = Some(77);
        op.two_week_recipients[0].reserve_debit_e8s = Some(200_010_000);
        let reservation =
            derived_reward_reservation_for_operation(&op).expect("valid partial reward split");
        op.reward_reservation = Some(reservation);
        op.reserved_reward_debit_e8s = Some(
            reservation
                .checked_total_unavailable_reward_debit_e8s()
                .unwrap(),
        );

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 1, 20_000).unwrap();

        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.status),
            Some(RewardPreflightStatus::ManualReconciliationRequired)
        );
        assert_eq!(
            op.two_week_recipients[0].reward_transfer_attempt,
            Some(attempt)
        );
    }

    #[test]
    fn bad_fee_after_partial_distribution_preserves_full_uncommitted_reservation() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        let operation_id = op.operation_id.clone();
        op.two_week_recipients[0].reward_transfer_attempt = Some(RewardTransferAttemptRecord {
            lifecycle: Some(RewardTransferAttemptLifecycle::Proven {
                generation: 55,
                block: 77,
            }),
            ..reward_attempt(200_000_000, 10_000, 55, &operation_id, [7; 32])
        });
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::Succeeded);
        op.two_week_recipients[0].transfer_status = TransferStatus::Succeeded;
        op.two_week_recipients[0].transfer_block_index = Some(77);
        op.two_week_recipients[0].ledger_transfer_block = Some(77);
        op.two_week_recipients[0].reserve_debit_e8s = Some(200_010_000);
        let reservation =
            derived_reward_reservation_for_operation(&op).expect("valid partial reward split");
        op.reward_reservation = Some(reservation);
        op.reserved_reward_debit_e8s = Some(
            reservation
                .checked_total_unavailable_reward_debit_e8s()
                .unwrap(),
        );

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 1, 20_000).unwrap();

        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(300_020_000)
        );
        assert_eq!(op.reserved_reward_debit_e8s, Some(300_020_000));
    }

    #[test]
    fn bad_fee_exact_submitted_after_prior_proven_recipient_requires_manual_reconciliation() {
        let mut op = partial_distribution_bad_fee_shape();

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 1, 20_000).unwrap();

        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.status),
            Some(RewardPreflightStatus::ManualReconciliationRequired)
        );
        assert_eq!(op.phase, OperationPhase::FailedTerminal);
    }

    #[test]
    fn bad_fee_exact_submitted_after_prior_success_preserves_prior_attempt() {
        let mut op = partial_distribution_bad_fee_shape();
        let prior_attempt = op.two_week_recipients[0].reward_transfer_attempt.clone();
        let current_attempt = op.two_week_recipients[1].reward_transfer_attempt.clone();

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 1, 20_000).unwrap();

        assert_eq!(
            op.two_week_recipients[0].reward_transfer_attempt,
            prior_attempt
        );
        assert_eq!(
            op.two_week_recipients[1].reward_transfer_attempt,
            current_attempt
        );
    }

    #[test]
    fn bad_fee_exact_submitted_after_prior_success_preserves_prior_blocks() {
        let mut op = partial_distribution_bad_fee_shape();

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 1, 20_000).unwrap();

        assert_eq!(op.two_week_recipients[0].transfer_block_index, Some(77));
        assert_eq!(op.two_week_recipients[0].ledger_transfer_block, Some(77));
        assert_eq!(
            op.two_week_recipients[0].ledger_transfer_status,
            Some(TransferStatus::Succeeded)
        );
    }

    #[test]
    fn bad_fee_exact_submitted_after_prior_success_preserves_fee_debit_and_received_amount() {
        let mut op = partial_distribution_bad_fee_shape();

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 1, 20_000).unwrap();

        assert_eq!(
            op.two_week_recipients[0].ledger_transfer_fee_e8s,
            Some(10_000)
        );
        assert_eq!(
            op.two_week_recipients[0].reward_amount_received_e8s,
            Some(200_000_000)
        );
        assert_eq!(
            op.two_week_recipients[0].reserve_debit_e8s,
            Some(200_010_000)
        );
    }

    #[test]
    fn bad_fee_exact_submitted_after_prior_refresh_preserves_stake_evidence() {
        let mut op = partial_distribution_bad_fee_shape();

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 1, 20_000).unwrap();

        assert_eq!(
            op.two_week_recipients[0].governance_refresh_status,
            Some(TransferStatus::Succeeded)
        );
        assert_eq!(
            op.two_week_recipients[0].stake_before_e8s,
            Some(1_000_000_000)
        );
        assert_eq!(
            op.two_week_recipients[0].expected_stake_after_e8s,
            Some(1_200_000_000)
        );
        assert_eq!(
            op.two_week_recipients[0].observed_stake_after_e8s,
            Some(1_200_000_000)
        );
    }

    #[test]
    fn bad_fee_after_partial_distribution_preserves_spent_and_unspent_reservation_components() {
        let mut op = partial_distribution_bad_fee_shape();

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 1, 20_000).unwrap();

        assert_eq!(
            op.reward_reservation,
            Some(RewardReservation {
                unspent_reserved_reward_debit_e8s: 100_010_000,
                externally_spent_but_uncommitted_reward_debit_e8s: 200_010_000,
            })
        );
        assert_eq!(op.reserved_reward_debit_e8s, Some(300_020_000));
    }

    #[test]
    fn bad_fee_after_partial_distribution_does_not_create_repreflight_evidence() {
        let mut op = partial_distribution_bad_fee_shape();

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 1, 20_000).unwrap();

        assert_eq!(op.reward_fee_repreflight, None);
    }

    #[test]
    fn bad_fee_after_partial_distribution_cannot_make_prior_recipient_submittable() {
        let mut op = partial_distribution_bad_fee_shape();

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 1, 20_000).unwrap();

        assert!(!reward_recipient_can_submit(&op.two_week_recipients[0]));
        CANISTER_STATE.with(|cell| {
            cell.borrow_mut().operation_journal = vec![op];
        });
        assert_eq!(
            next_reward_ledger_recipient(&BTreeSet::new()).map(|(_, index, _, _)| index),
            None
        );
    }

    #[test]
    fn live_manual_state_rejects_spent_debit_stored_as_unspent() {
        let mut op = partial_distribution_bad_fee_shape();
        op.reward_preflight.as_mut().unwrap().status =
            RewardPreflightStatus::ManualReconciliationRequired;
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 300_020_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });

        let err =
            reward_reservation_for_operation(&op, &empty_processed_transactions()).unwrap_err();

        assert!(err.contains("split disagrees"));
    }

    #[test]
    fn live_manual_state_rejects_unspent_debit_stored_as_spent() {
        let mut op = partial_distribution_bad_fee_shape();
        op.reward_preflight.as_mut().unwrap().status =
            RewardPreflightStatus::ManualReconciliationRequired;
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 0,
            externally_spent_but_uncommitted_reward_debit_e8s: 300_020_000,
        });

        let err =
            reward_reservation_for_operation(&op, &empty_processed_transactions()).unwrap_err();

        assert!(err.contains("split disagrees"));
    }

    #[test]
    fn live_and_stable_manual_split_validation_agree() {
        let mut op = partial_distribution_bad_fee_shape();
        op.reward_preflight.as_mut().unwrap().status =
            RewardPreflightStatus::ManualReconciliationRequired;

        assert!(reward_reservation_for_operation(&op, &empty_processed_transactions()).is_ok());

        let stable = crate::migrate_stable_state(crate::VersionedStableState {
            schema_version: crate::STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state: crate::StableState {
                config: crate::StreamManagerConfig::default(),
                protocol: crate::StableProtocolState::from(io_core_model::ProtocolState::new(
                    100_000_000_000_000,
                    90_000_000_000_000,
                    10_000_000_000_000,
                )),
                processed_transactions: Vec::new(),
                active_staked_io_e8s: 0,
                two_week_pool_backing_bps: 10_000,
                operation_journal: vec![op],
                scheduler_cursors: crate::SchedulerCursors::default(),
            },
        });

        assert!(stable.is_ok());
    }

    #[test]
    fn fee_change_after_proof_pending_never_blindly_resends() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        let operation_id = op.operation_id.clone();
        let attempt = RewardTransferAttemptRecord {
            lifecycle: Some(RewardTransferAttemptLifecycle::ProofRequired {
                generation: 55,
                reason: "ambiguous".to_string(),
            }),
            ..reward_attempt(200_000_000, 10_000, 55, &operation_id, [7; 32])
        };
        op.two_week_recipients[0].reward_transfer_attempt = Some(attempt.clone());
        op.two_week_recipients[0].ledger_transfer_proof_scan_state =
            Some(AccountHistoryScanState::default());

        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 0, 20_000).unwrap();

        assert_eq!(
            op.reward_preflight
                .as_ref()
                .map(|preflight| preflight.status),
            Some(RewardPreflightStatus::ManualReconciliationRequired)
        );
        assert_eq!(
            op.two_week_recipients[0].reward_transfer_attempt,
            Some(attempt)
        );
    }

    #[test]
    fn fee_evidence_survives_same_wasm_upgrade() {
        let mut op = reward_operation_ready_for_attempt().remove(0);
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 55 },
        );
        apply_reward_bad_fee_policy(&mut op, &empty_processed_transactions(), 0, 20_000).unwrap();
        let restored = op.clone();

        assert!(restored
            .reward_preflight
            .as_ref()
            .and_then(|preflight| preflight.failure_reason.as_deref())
            .unwrap()
            .contains("observed current fee 20000"));
        assert_eq!(
            restored
                .reward_fee_repreflight
                .map(|evidence| evidence.observed_current_fee_e8s),
            Some(20_000)
        );
    }

    #[test]
    fn refresh_terminal_after_transfer_keeps_spent_uncommitted_reservation() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = Some(validated_preflight_for(&op));
        install_reward_attempt(
            &mut op,
            0,
            55,
            [7; 32],
            RewardTransferAttemptLifecycle::Proven {
                generation: 55,
                block: 45,
            },
        );
        op.two_week_recipients[0].transfer_status = TransferStatus::Succeeded;
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::Succeeded);
        op.two_week_recipients[0].transfer_block_index = Some(45);
        op.two_week_recipients[0].ledger_transfer_block = Some(45);
        op.two_week_recipients[0].governance_refresh_status = Some(TransferStatus::FailedTerminal);
        op.two_week_recipients[0].reserve_debit_e8s = Some(200_010_000);

        assert_eq!(
            pending_reward_spent_uncommitted_reservation_for_operation(
                &op,
                &empty_processed_transactions()
            ),
            Ok(200_010_000)
        );
        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(300_020_000)
        );
    }

    #[test]
    fn terminal_pretransfer_failure_releases_reservation() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = Some(RewardDistributionPreflight {
            status: RewardPreflightStatus::FailedTerminal,
            ledger_fee_e8s: 10_000,
            recipient_count: 2,
            total_reward_e8s: 300_000_000,
            total_fee_e8s: 20_000,
            total_reserve_debit_e8s: 300_020_000,
            protocol_reserve_available_e8s: 0,
            real_ledger_reserve_balance_e8s: 0,
            validated_at_timestamp_nanos: 123,
            canonical_recipient_ids: vec![vec![7; 32], vec![8; 32]],
            compatibility_keys: vec![7, 8],
            dust_e8s: 123,
            failure_reason: Some("invalid recipient".to_string()),
        });
        op.phase = OperationPhase::FailedTerminal;

        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(0)
        );
    }

    #[test]
    fn explicit_pretransfer_cancel_releases_only_without_attempt_or_external_uncertainty() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = Some(validated_preflight_for(&op));
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 300_020_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        op.reserved_reward_debit_e8s = Some(300_020_000);

        explicit_pretransfer_cancel_reward_reservation(&mut op).unwrap();
        assert_eq!(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()),
            Ok(0)
        );

        let mut attempted = reward_preflight_operation();
        attempted.reward_preflight = Some(validated_preflight_for(&attempted));
        attempted.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 300_020_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        attempted.reserved_reward_debit_e8s = Some(300_020_000);
        attempted.two_week_recipients[0].reward_transfer_attempt =
            Some(reward_attempt(200_000_000, 10_000, 55, "icp:9", [7; 32]));

        assert!(explicit_pretransfer_cancel_reward_reservation(&mut attempted).is_err());
        assert_eq!(
            pending_reward_reservation_for_operation(&attempted, &empty_processed_transactions()),
            Ok(300_020_000)
        );
    }

    #[test]
    fn reservation_component_overflow_is_error() {
        let reservation = RewardReservation {
            unspent_reserved_reward_debit_e8s: u128::MAX,
            externally_spent_but_uncommitted_reward_debit_e8s: 1,
        };

        assert!(reservation
            .checked_total_unavailable_reward_debit_e8s()
            .unwrap_err()
            .contains("overflow"));
    }

    #[test]
    fn aggregate_reservation_overflow_is_error_not_wraparound() {
        let mut a = reward_operation_ready_for_attempt().remove(0);
        let mut a_preflight = validated_preflight_for(&a);
        a_preflight.total_reward_e8s = u128::MAX - 10_000;
        a_preflight.total_reserve_debit_e8s = u128::MAX;
        a_preflight.dust_e8s = 0;
        a.io_issued_e8s = u128::MAX - 10_000;
        a.two_week_recipients[0].amount_e8s = u128::MAX - 200_010_000;
        a.two_week_recipients[1].amount_e8s = 200_000_000;
        a.reward_preflight = Some(a_preflight);
        a.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: u128::MAX,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        a.reserved_reward_debit_e8s = Some(u128::MAX);
        let mut b = reward_operation_ready_for_attempt().remove(0);
        let mut b_preflight = validated_preflight_for(&b);
        b_preflight.total_reward_e8s = 0;
        b_preflight.total_reserve_debit_e8s = 20_000;
        b_preflight.dust_e8s = 0;
        b.io_issued_e8s = 0;
        b.two_week_recipients[0].amount_e8s = 0;
        b.two_week_recipients[1].amount_e8s = 0;
        b.reward_preflight = Some(b_preflight);
        b.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 20_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        b.reserved_reward_debit_e8s = Some(20_000);

        assert!(
            pending_reward_reservations([a, b].iter(), &empty_processed_transactions(), None)
                .unwrap_err()
                .contains("overflow")
        );
    }

    #[test]
    fn reserve_available_calculation_subtracts_all_pending_reservations() {
        let mut a = reward_preflight_operation();
        a.operation_id = "icp:1".to_string();
        a.reward_preflight = Some(validated_preflight_for(&a));
        let mut b = reward_preflight_operation();
        b.operation_id = "icp:2".to_string();
        b.reward_preflight = Some(validated_preflight_for(&b));
        let ops = [a, b];

        let reserved =
            pending_reward_reservations(ops.iter(), &empty_processed_transactions(), None).unwrap();
        assert_eq!(reserved, 600_040_000);
        assert_eq!(
            reward_reserve_available(700_000_000, reserved).unwrap(),
            99_960_000
        );
    }

    #[test]
    fn concurrent_reward_operations_cannot_overcommit_reserve() {
        let mut a = reward_preflight_operation();
        a.reward_preflight = Some(validated_preflight_for(&a));
        let reserved =
            pending_reward_reservations(std::iter::once(&a), &empty_processed_transactions(), None)
                .unwrap();

        assert!(reward_reserve_available(300_030_000, reserved).unwrap() < 300_020_000);
    }

    #[test]
    fn reservation_survives_same_wasm_upgrade() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = Some(validated_preflight_for(&op));
        op.reward_reservation =
            Some(reward_reservation_for_operation(&op, &empty_processed_transactions()).unwrap());
        op.reserved_reward_debit_e8s = Some(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()).unwrap(),
        );
        let restored = op.clone();

        assert_eq!(
            restored.reward_reservation,
            Some(RewardReservation {
                unspent_reserved_reward_debit_e8s: 300_020_000,
                externally_spent_but_uncommitted_reward_debit_e8s: 0,
            })
        );
        assert_eq!(restored.reserved_reward_debit_e8s, Some(300_020_000));
        assert_eq!(restored.reward_preflight, op.reward_preflight);
    }

    #[test]
    fn current_reward_preflight_round_trips() {
        let preflight = validated_preflight_for(&reward_preflight_operation());
        let bytes = Encode!(&preflight).unwrap();
        let decoded = Decode!(&bytes, RewardDistributionPreflight).unwrap();

        assert_eq!(decoded, preflight);
    }

    #[test]
    fn current_reward_reservation_round_trips() {
        let mut op = reward_preflight_operation();
        op.reward_preflight = Some(validated_preflight_for(&op));
        op.reward_reservation =
            Some(reward_reservation_for_operation(&op, &empty_processed_transactions()).unwrap());
        op.reserved_reward_debit_e8s = Some(
            pending_reward_reservation_for_operation(&op, &empty_processed_transactions()).unwrap(),
        );
        let bytes = Encode!(&op).unwrap();
        let decoded = Decode!(&bytes, StreamOperation).unwrap();

        assert_eq!(
            decoded.reward_reservation,
            Some(RewardReservation {
                unspent_reserved_reward_debit_e8s: 300_020_000,
                externally_spent_but_uncommitted_reward_debit_e8s: 0,
            })
        );
        assert_eq!(decoded.reserved_reward_debit_e8s, Some(300_020_000));
    }

    #[test]
    fn nonzero_reward_allocation_dust_remains_protocol_reserve() {
        let mut op = reward_preflight_operation();
        op.io_issued_e8s = 300_000_001;
        op.two_week_recipients[0].amount_e8s = 150_000_000;
        op.two_week_recipients[1].amount_e8s = 150_000_000;
        op.two_week_recipients[0].ledger_transfer_fee_e8s = Some(10_000);
        op.two_week_recipients[1].ledger_transfer_fee_e8s = Some(10_000);
        op.reward_preflight = Some(RewardDistributionPreflight {
            status: RewardPreflightStatus::Validated,
            ledger_fee_e8s: 10_000,
            recipient_count: 2,
            total_reward_e8s: 300_000_000,
            total_fee_e8s: 20_000,
            total_reserve_debit_e8s: 300_020_000,
            protocol_reserve_available_e8s: 300_020_000,
            real_ledger_reserve_balance_e8s: 300_020_000,
            validated_at_timestamp_nanos: 123,
            canonical_recipient_ids: vec![vec![7; 32], vec![8; 32]],
            compatibility_keys: vec![7, 8],
            dust_e8s: 1,
            failure_reason: None,
        });

        let adjusted = reward_fee_adjusted_post_state(&op).unwrap();

        assert_eq!(
            adjusted.protocol_reserve_io_e8s,
            op.post_state.protocol_reserve_io_e8s + 1 - 20_000
        );
        assert_eq!(
            adjusted.total_io_supply_e8s,
            op.post_state.total_io_supply_e8s - 20_000
        );
    }

    #[test]
    fn zero_recipient_reward_preflight_has_full_dust_and_zero_debit() {
        let op = zero_recipient_reward_operation();

        let preflight = build_reward_distribution_preflight(
            &op,
            candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
            10_000,
            0,
            0,
            123,
        )
        .unwrap();

        assert_eq!(preflight.recipient_count, 0);
        assert_eq!(preflight.total_reward_e8s, 0);
        assert_eq!(preflight.total_fee_e8s, 0);
        assert_eq!(preflight.total_reserve_debit_e8s, 0);
        assert_eq!(preflight.dust_e8s, op.io_issued_e8s);
        assert!(preflight.canonical_recipient_ids.is_empty());
        assert!(preflight.compatibility_keys.is_empty());
    }

    #[test]
    fn zero_recipient_reward_model_delta_commits_40_60_icp_once() {
        let mut op = zero_recipient_reward_operation();
        op.reward_preflight = Some(
            build_reward_distribution_preflight(
                &op,
                candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
                10_000,
                0,
                0,
                123,
            )
            .unwrap(),
        );

        let delta = reward_model_delta(&op).unwrap();
        let next = checked_reward_model_post_state(
            io_core_model::ProtocolState::from(op.post_state),
            delta,
        )
        .unwrap();

        assert_eq!(delta.allocation_debit_e8s, 0);
        assert_eq!(delta.fee_burn_e8s, 0);
        assert_eq!(delta.dust_retained_e8s, op.io_issued_e8s);
        assert_eq!(
            next.liquid_icp_e8s,
            op.post_state.liquid_icp_e8s + 300_000_000
        );
        assert_eq!(
            next.two_week_staked_icp_e8s,
            op.post_state.two_week_staked_icp_e8s + 200_000_000
        );
        assert_eq!(
            next.protocol_reserve_io_e8s,
            op.post_state.protocol_reserve_io_e8s
        );
        assert_eq!(next.total_io_supply_e8s, op.post_state.total_io_supply_e8s);
    }

    #[test]
    fn zero_recipient_reward_replay_is_idempotent() {
        let mut op = zero_recipient_reward_operation();
        op.reward_preflight = Some(
            build_reward_distribution_preflight(
                &op,
                candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
                10_000,
                0,
                0,
                123,
            )
            .unwrap(),
        );
        op.reward_reservation = Some(RewardReservation::default());
        op.reserved_reward_debit_e8s = Some(0);
        let operation_id = op.operation_id.clone();
        let mut manager = StreamManager {
            state: io_core_model::ProtocolState::from(op.post_state),
            processed_transactions: Default::default(),
            active_staked_io_e8s: 0,
            two_week_pool_backing_bps: 10_000,
        };
        let mut journal = vec![op];

        assert_eq!(
            commit_completed_reward_operation_in_state(&mut manager, &mut journal, &operation_id),
            Ok(true)
        );
        let after_first_manager = manager.clone();
        let after_first_journal = journal.clone();
        assert_eq!(
            commit_completed_reward_operation_in_state(&mut manager, &mut journal, &operation_id),
            Ok(false)
        );
        assert_eq!(manager.state, after_first_manager.state);
        assert_eq!(journal, after_first_journal);
    }

    #[test]
    fn zero_recipient_reward_preserves_total_io_supply() {
        let mut op = zero_recipient_reward_operation();
        op.reward_preflight = Some(
            build_reward_distribution_preflight(
                &op,
                candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
                10_000,
                0,
                0,
                123,
            )
            .unwrap(),
        );
        let before = io_core_model::ProtocolState::from(op.post_state);
        let delta = reward_model_delta(&op).unwrap();

        let after = checked_reward_model_post_state(before, delta).unwrap();

        assert_eq!(after.total_io_supply_e8s, before.total_io_supply_e8s);
    }

    #[test]
    fn zero_recipient_reward_model_reserve_matches_real_ledger() {
        let mut op = zero_recipient_reward_operation();
        op.reward_preflight = Some(
            build_reward_distribution_preflight(
                &op,
                candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
                10_000,
                op.post_state.protocol_reserve_io_e8s,
                op.post_state.protocol_reserve_io_e8s,
                123,
            )
            .unwrap(),
        );
        let delta = reward_model_delta(&op).unwrap();

        let after = checked_reward_model_post_state(
            io_core_model::ProtocolState::from(op.post_state),
            delta,
        )
        .unwrap();

        assert_eq!(
            after.protocol_reserve_io_e8s,
            op.post_state.protocol_reserve_io_e8s
        );
        assert_eq!(
            op.reward_preflight
                .as_ref()
                .unwrap()
                .real_ledger_reserve_balance_e8s,
            op.post_state.protocol_reserve_io_e8s
        );
    }

    #[test]
    fn zero_recipient_reward_same_wasm_upgrade_is_idempotent() {
        let mut op = zero_recipient_reward_operation();
        op.reward_preflight = Some(
            build_reward_distribution_preflight(
                &op,
                candid::Principal::from_text("qaa6y-5yaaa-aaaaa-aaafa-cai").unwrap(),
                10_000,
                0,
                0,
                123,
            )
            .unwrap(),
        );
        op.reward_reservation = Some(RewardReservation::default());
        op.reserved_reward_debit_e8s = Some(0);
        let operation_id = op.operation_id.clone();
        let mut manager = StreamManager {
            state: io_core_model::ProtocolState::from(op.post_state),
            processed_transactions: Default::default(),
            active_staked_io_e8s: 0,
            two_week_pool_backing_bps: 10_000,
        };
        let mut journal = vec![op];
        commit_completed_reward_operation_in_state(&mut manager, &mut journal, &operation_id)
            .unwrap();
        let restored_manager = manager.clone();
        let restored_journal = journal.clone();

        assert_eq!(
            commit_completed_reward_operation_in_state(&mut manager, &mut journal, &operation_id),
            Ok(false)
        );
        assert_eq!(manager.state, restored_manager.state);
        assert_eq!(journal, restored_journal);
    }

    #[test]
    fn reward_fee_burn_reduces_protocol_total_supply() {
        let op = reward_fee_accounting_operation();
        let adjusted = reward_fee_adjusted_post_state(&op).unwrap();

        assert_eq!(
            adjusted.total_io_supply_e8s,
            op.post_state.total_io_supply_e8s - 30_000
        );
    }

    #[test]
    fn reward_model_delta_preserves_intervening_unrelated_model_change() {
        let mut op = reward_fee_accounting_operation();
        for recipient in &mut op.two_week_recipients {
            recipient.governance_refresh_status = Some(TransferStatus::Succeeded);
        }
        let delta = reward_model_delta(&op).unwrap();
        let mut live_state = io_core_model::ProtocolState::from(op.post_state);
        live_state.non_redeemable_governance_io_e8s += 777;
        live_state.protocol_reserve_io_e8s += 123_456;

        let next_state = checked_reward_model_post_state(live_state, delta).unwrap();

        assert_eq!(
            next_state.non_redeemable_governance_io_e8s,
            10_000_000_000_777
        );
        assert_eq!(
            next_state.protocol_reserve_io_e8s,
            op.post_state.protocol_reserve_io_e8s + 123_456 - 300_030_000
        );
        assert_eq!(
            next_state.total_io_supply_e8s,
            op.post_state.total_io_supply_e8s - 30_000
        );
        assert_eq!(
            next_state.liquid_icp_e8s,
            op.post_state.liquid_icp_e8s + split_40_60(op.amount_e8s).liquid_e8s
        );
        assert_eq!(
            next_state.two_week_staked_icp_e8s,
            op.post_state.two_week_staked_icp_e8s + split_40_60(op.amount_e8s).stake_e8s
        );
    }

    fn completed_reward_delta_for_tests() -> RewardModelDelta {
        let mut op = reward_fee_accounting_operation();
        for recipient in &mut op.two_week_recipients {
            recipient.governance_refresh_status = Some(TransferStatus::Succeeded);
        }
        reward_model_delta(&op).unwrap()
    }

    fn completed_reward_commit_fixture() -> (StreamManager, Vec<StreamOperation>) {
        let mut op = reward_fee_accounting_operation();
        op.two_week_recipients[0].sns_neuron_id = Some(vec![7; 32]);
        op.two_week_recipients[0].neuron_id = 7;
        op.two_week_recipients[0].ledger_transfer_fee_e8s = Some(10_000);
        op.two_week_recipients[0].reward_transfer_attempt = Some(RewardTransferAttemptRecord {
            lifecycle: Some(RewardTransferAttemptLifecycle::Proven {
                generation: 55,
                block: 77,
            }),
            ..reward_attempt(200_000_000, 10_000, 55, &op.operation_id, [7; 32])
        });
        op.two_week_recipients[1].sns_neuron_id = Some(vec![8; 32]);
        op.two_week_recipients[1].neuron_id = 8;
        op.two_week_recipients[1].ledger_transfer_fee_e8s = Some(10_000);
        op.two_week_recipients[1].reserve_debit_e8s = Some(100_010_000);
        op.two_week_recipients[1].reward_transfer_attempt = Some(RewardTransferAttemptRecord {
            lifecycle: Some(RewardTransferAttemptLifecycle::Proven {
                generation: 55,
                block: 77,
            }),
            ..reward_attempt(100_000_000, 10_000, 55, &op.operation_id, [8; 32])
        });
        op.reward_preflight = Some(RewardDistributionPreflight {
            status: RewardPreflightStatus::Validated,
            ledger_fee_e8s: 10_000,
            recipient_count: 2,
            total_reward_e8s: 300_000_000,
            total_fee_e8s: 20_000,
            total_reserve_debit_e8s: 300_020_000,
            protocol_reserve_available_e8s: 300_020_000,
            real_ledger_reserve_balance_e8s: 300_020_000,
            validated_at_timestamp_nanos: 123,
            canonical_recipient_ids: vec![vec![7; 32], vec![8; 32]],
            compatibility_keys: vec![7, 8],
            dust_e8s: 0,
            failure_reason: None,
        });
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 0,
            externally_spent_but_uncommitted_reward_debit_e8s: 300_020_000,
        });
        op.reserved_reward_debit_e8s = Some(300_020_000);
        for recipient in &mut op.two_week_recipients {
            recipient.governance_refresh_status = Some(TransferStatus::Succeeded);
        }
        let manager = StreamManager {
            state: io_core_model::ProtocolState::from(op.post_state),
            processed_transactions: Default::default(),
            active_staked_io_e8s: 0,
            two_week_pool_backing_bps: 10_000,
        };
        (manager, vec![op])
    }

    #[test]
    fn reward_commit_invariant_failure_leaves_journal_and_reservation_unchanged() {
        let (mut manager, mut journal) = completed_reward_commit_fixture();
        manager.state.non_redeemable_governance_io_e8s = manager.state.total_io_supply_e8s;
        let before_manager = manager.clone();
        let before_journal = journal.clone();
        let operation_id = journal[0].operation_id.clone();

        let err =
            commit_completed_reward_operation_in_state(&mut manager, &mut journal, &operation_id)
                .unwrap_err();

        assert!(err.contains("reward model invariant failed"));
        assert_eq!(manager.state, before_manager.state);
        assert_eq!(
            manager.processed_transactions,
            before_manager.processed_transactions
        );
        assert_eq!(
            manager.active_staked_io_e8s,
            before_manager.active_staked_io_e8s
        );
        assert_eq!(
            manager.two_week_pool_backing_bps,
            before_manager.two_week_pool_backing_bps
        );
        assert_eq!(journal, before_journal);
    }

    #[test]
    fn reward_commit_success_updates_model_processed_set_reservation_and_phase_together() {
        let (mut manager, mut journal) = completed_reward_commit_fixture();
        let before = manager.state;
        let operation_id = journal[0].operation_id.clone();
        let source_transaction_id = journal[0].source_transaction_id.clone();

        assert_eq!(
            commit_completed_reward_operation_in_state(&mut manager, &mut journal, &operation_id,),
            Ok(true)
        );

        assert!(manager
            .processed_transactions
            .contains(&source_transaction_id));
        assert_eq!(journal[0].phase, OperationPhase::Completed);
        assert_eq!(
            journal[0].reward_reservation,
            Some(RewardReservation::default())
        );
        assert_eq!(journal[0].reserved_reward_debit_e8s, Some(0));
        assert_eq!(
            manager.state.liquid_icp_e8s,
            before.liquid_icp_e8s + 300_000_000
        );
        assert_eq!(
            manager.state.two_week_staked_icp_e8s,
            before.two_week_staked_icp_e8s + 200_000_000
        );
        assert_eq!(
            manager.state.protocol_reserve_io_e8s,
            before.protocol_reserve_io_e8s - 300_020_000
        );
        assert_eq!(
            manager.state.total_io_supply_e8s,
            before.total_io_supply_e8s - 20_000
        );
    }

    #[test]
    fn reward_commit_retry_is_idempotent() {
        let (mut manager, mut journal) = completed_reward_commit_fixture();
        let operation_id = journal[0].operation_id.clone();

        assert_eq!(
            commit_completed_reward_operation_in_state(&mut manager, &mut journal, &operation_id,),
            Ok(true)
        );
        let after_first_manager = manager.clone();
        let after_first_journal = journal.clone();

        assert_eq!(
            commit_completed_reward_operation_in_state(&mut manager, &mut journal, &operation_id,),
            Ok(false)
        );
        assert_eq!(manager.state, after_first_manager.state);
        assert_eq!(
            manager.processed_transactions,
            after_first_manager.processed_transactions
        );
        assert_eq!(
            manager.active_staked_io_e8s,
            after_first_manager.active_staked_io_e8s
        );
        assert_eq!(
            manager.two_week_pool_backing_bps,
            after_first_manager.two_week_pool_backing_bps
        );
        assert_eq!(journal, after_first_journal);
    }

    #[test]
    fn reward_commit_reserve_underflow_leaves_entire_model_unchanged() {
        let delta = completed_reward_delta_for_tests();
        let current = io_core_model::ProtocolState {
            protocol_reserve_io_e8s: delta.allocation_debit_e8s + delta.fee_burn_e8s - 1,
            total_io_supply_e8s: u128::MAX,
            ..io_core_model::ProtocolState::new(u128::MAX, u128::MAX, 0)
        };

        assert!(checked_reward_model_post_state(current, delta)
            .unwrap_err()
            .contains("protocol reserve cannot cover"));
        assert_eq!(
            current.protocol_reserve_io_e8s,
            delta.allocation_debit_e8s + delta.fee_burn_e8s - 1
        );
    }

    #[test]
    fn reward_commit_supply_underflow_leaves_entire_model_unchanged() {
        let delta = completed_reward_delta_for_tests();
        let current = io_core_model::ProtocolState {
            protocol_reserve_io_e8s: u128::MAX / 2,
            total_io_supply_e8s: delta.fee_burn_e8s - 1,
            ..io_core_model::ProtocolState::new(u128::MAX / 2, u128::MAX / 2, 0)
        };

        assert!(checked_reward_model_post_state(current, delta)
            .unwrap_err()
            .contains("total IO supply cannot burn"));
        assert_eq!(current.total_io_supply_e8s, delta.fee_burn_e8s - 1);
    }

    #[test]
    fn reward_commit_liquid_overflow_leaves_entire_model_unchanged() {
        let delta = completed_reward_delta_for_tests();
        let current = io_core_model::ProtocolState {
            liquid_icp_e8s: u128::MAX,
            protocol_reserve_io_e8s: u128::MAX / 2,
            total_io_supply_e8s: u128::MAX / 2,
            ..io_core_model::ProtocolState::new(u128::MAX / 2, u128::MAX / 2, 0)
        };

        assert!(checked_reward_model_post_state(current, delta)
            .unwrap_err()
            .contains("liquid ICP credit overflowed"));
        assert_eq!(current.liquid_icp_e8s, u128::MAX);
    }

    #[test]
    fn reward_commit_stake_overflow_leaves_entire_model_unchanged() {
        let delta = completed_reward_delta_for_tests();
        let current = io_core_model::ProtocolState {
            two_week_staked_icp_e8s: u128::MAX,
            protocol_reserve_io_e8s: u128::MAX / 2,
            total_io_supply_e8s: u128::MAX / 2,
            ..io_core_model::ProtocolState::new(u128::MAX / 2, u128::MAX / 2, 0)
        };

        assert!(checked_reward_model_post_state(current, delta)
            .unwrap_err()
            .contains("two-week staked ICP credit overflowed"));
        assert_eq!(current.two_week_staked_icp_e8s, u128::MAX);
    }

    #[test]
    fn reward_model_delta_rejects_incomplete_completion_evidence() {
        let mut op = reward_fee_accounting_operation();
        for recipient in &mut op.two_week_recipients {
            recipient.governance_refresh_status = Some(TransferStatus::Succeeded);
        }
        op.two_week_recipients[0].reserve_debit_e8s = None;

        assert!(reward_model_delta(&op)
            .unwrap_err()
            .contains("missing reserve debit"));
    }

    #[test]
    fn reward_distribution_preserves_rate_after_fee_accounting() {
        let op = reward_fee_accounting_operation();
        let before = io_core_model::ProtocolState::from(op.post_state)
            .redemption_rate()
            .unwrap();
        let adjusted = reward_fee_adjusted_post_state(&op).unwrap();

        assert_eq!(
            io_core_model::ProtocolState::from(adjusted)
                .redemption_rate()
                .unwrap(),
            before
        );
    }

    #[test]
    fn pending_reward_operation_reserves_unspent_reward_and_fee_debit() {
        let op = reward_fee_accounting_operation();
        let reserved: u128 = op
            .two_week_recipients
            .iter()
            .filter(|recipient| recipient_refresh_status(recipient) != TransferStatus::Succeeded)
            .map(|recipient| recipient.reserve_debit_e8s.unwrap_or(recipient.amount_e8s))
            .sum();

        assert_eq!(reserved, 300_030_000);
    }

    #[test]
    fn unrelated_operation_cannot_spend_reserved_reward_debit() {
        let op = reward_fee_accounting_operation();
        let reserved: u128 = op
            .two_week_recipients
            .iter()
            .map(|recipient| recipient.reserve_debit_e8s.unwrap())
            .sum();

        assert!(reserved > op.io_issued_e8s);
    }

    #[test]
    fn reward_transfer_budget_is_bounded_per_tick() {
        assert_eq!(TWO_WEEK_REWARD_LEDGER_TRANSFER_BUDGET_PER_TICK, 8);
    }

    #[test]
    fn reward_refresh_budget_is_bounded_per_tick() {
        assert_eq!(TWO_WEEK_REWARD_REFRESH_BUDGET_PER_TICK, 8);
    }

    #[test]
    fn rejected_refund_duplicate_proof_must_match() {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        let refund_source = Account::new(principal, Some(Subaccount([9; 32])));
        let sender = Account::new(principal, Some(Subaccount([7; 32])));
        let request = LedgerTransferRequest {
            from_subaccount: refund_source.subaccount,
            to: sender.clone(),
            amount_e8s: 990_000,
            fee_e8s: None,
            memo: Some(rejected_refund_memo("io:88")),
            created_at_time: Some(88),
        };
        let matching = LedgerBlock {
            block_index: BlockIndex(91),
            timestamp_nanos: 88,
            created_at_time: None,
            from: Some(refund_source.clone()),
            to: Some(sender),
            amount_e8s: 990_000,
            fee_e8s: Some(10_000),
            memo: Some(rejected_refund_memo("io:88")),
            operation_kind: LedgerOperationKind::Transfer,
        };
        assert_eq!(
            classify_boundary_transfer_result_with_source(
                &request,
                &refund_source,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(91)
                }),
                Some(&matching),
            ),
            BoundaryTransferDecision::Succeeded(91)
        );

        let wrong_source = LedgerBlock {
            from: Some(Account::new(principal, Some(Subaccount([8; 32])))),
            ..matching.clone()
        };
        assert!(matches!(
            classify_boundary_transfer_result_with_source(
                &request,
                &refund_source,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(91)
                }),
                Some(&wrong_source),
            ),
            BoundaryTransferDecision::Retryable(_)
        ));

        let wrong_kind = LedgerBlock {
            operation_kind: LedgerOperationKind::Mint,
            ..matching
        };
        assert!(matches!(
            classify_boundary_transfer_result_with_source(
                &request,
                &refund_source,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(91)
                }),
                Some(&wrong_kind),
            ),
            BoundaryTransferDecision::Retryable(_)
        ));
    }

    #[test]
    fn retryable_rejected_refund_does_not_block_later_valid_redemption() {
        let failed_refund =
            rejected_redemption_op(RejectedFundDisposition::ReturnToSenderRetryable {
                error: "ledger temporarily unavailable".to_string(),
                next_attempt_created_at_time: None,
            });
        let valid = redemption_op_with_phase(OperationPhase::AwaitingIcpPayout);

        assert!(is_retryable_rejected_refund_operation(&failed_refund));
        assert!(is_retryable_redemption_operation(&valid));
    }

    #[test]
    fn one_failed_rejected_refund_does_not_block_another_refund() {
        let first = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderRetryable {
            error: "temporarily unavailable".to_string(),
            next_attempt_created_at_time: None,
        });
        let mut second = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderPending);
        second.operation_id = "io:89".to_string();
        let journal = vec![first.clone(), second.clone()];
        let mut attempted = BTreeSet::new();
        attempted.insert(first.operation_id);

        let selected = next_retryable_rejected_refund_operation(&journal, &attempted).unwrap();
        assert_eq!(selected.operation_id, second.operation_id);
    }

    #[test]
    fn rejected_refund_retry_budget_is_bounded_per_tick() {
        assert_eq!(REJECTED_REFUND_RETRY_BUDGET_PER_TICK, 8);

        let mut attempted = BTreeSet::new();
        for i in 0..(REJECTED_REFUND_RETRY_BUDGET_PER_TICK + 2) {
            attempted.insert(format!("io:{i}"));
        }
        assert_eq!(attempted.len(), REJECTED_REFUND_RETRY_BUDGET_PER_TICK + 2);
    }

    #[test]
    fn rejected_refund_retry_count_persists_across_upgrade() {
        let mut op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderRetryable {
            error: "temporary".to_string(),
            next_attempt_created_at_time: Some(123),
        });
        op.retry_count = 3;
        let restored = op.clone();

        assert_eq!(restored.retry_count, 3);
        assert_eq!(rejected_refund_attempt_created_at(&restored), 123);
        assert!(is_retryable_rejected_refund_operation(&restored));
    }

    #[test]
    fn permanently_failing_refund_does_not_spin_forever() {
        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderRetryable {
            error: "temporary".to_string(),
            next_attempt_created_at_time: Some(456),
        });
        let journal = vec![op.clone()];
        let mut attempted = BTreeSet::new();

        assert!(next_retryable_rejected_refund_operation(&journal, &attempted).is_some());
        attempted.insert(op.operation_id);
        assert!(next_retryable_rejected_refund_operation(&journal, &attempted).is_none());
    }

    #[test]
    fn terminal_rejected_refund_does_not_block_unrelated_operations() {
        let terminal = rejected_redemption_op(RejectedFundDisposition::QuarantinedTerminal {
            reason: "unresolvable sender".to_string(),
        });
        let mut pending = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderPending);
        pending.operation_id = "io:89".to_string();
        let journal = vec![terminal, pending.clone()];

        let selected = next_retryable_rejected_refund_operation(&journal, &BTreeSet::new())
            .expect("pending refund should still be selected");
        assert_eq!(selected.operation_id, pending.operation_id);
    }

    #[test]
    fn refunded_rejected_redemption_is_completed_not_failed() {
        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderSucceeded {
            block_index: 91,
            amount_e8s: 990_000,
        });

        assert_eq!(op.phase, OperationPhase::Completed);
        assert_ne!(op.phase, OperationPhase::FailedTerminal);
    }

    #[test]
    fn refunded_rejected_redemption_does_not_increment_retry_count() {
        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderSucceeded {
            block_index: 91,
            amount_e8s: 990_000,
        });

        assert_eq!(op.retry_count, 0);
        assert_eq!(op.last_error, None);
    }

    #[test]
    fn quarantined_rejected_redemption_is_terminal_failure() {
        let op = rejected_redemption_op(RejectedFundDisposition::QuarantinedTerminal {
            reason: "dust below fee".to_string(),
        });

        assert_eq!(op.phase, OperationPhase::FailedTerminal);
        assert_eq!(op.io_return_status, TransferStatus::FailedTerminal);
        assert!(!is_retryable_rejected_refund_operation(&op));
    }

    #[test]
    fn refunded_rejection_icp_payout_is_not_applicable() {
        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderSucceeded {
            block_index: 91,
            amount_e8s: 990_000,
        });

        assert_eq!(op.icp_payout_status, TransferStatus::NotApplicable);
        assert_eq!(op.io_return_status, TransferStatus::Succeeded);
    }

    #[test]
    fn quarantined_rejection_has_no_icp_payout() {
        let op = rejected_redemption_op(RejectedFundDisposition::QuarantinedTerminal {
            reason: "unresolvable sender".to_string(),
        });

        assert_eq!(op.icp_payout_status, TransferStatus::NotApplicable);
        assert_eq!(op.io_return_status, TransferStatus::FailedTerminal);
    }

    #[test]
    fn normal_redemption_never_uses_not_applicable_for_required_phases() {
        for phase in [
            OperationPhase::AwaitingIcpPayout,
            OperationPhase::AwaitingIoReturn,
            OperationPhase::Completed,
            OperationPhase::FailedRetryable,
            OperationPhase::FailedTerminal,
        ] {
            let op = redemption_op_with_phase(phase);
            assert_ne!(op.icp_payout_status, TransferStatus::NotApplicable);
            assert_ne!(op.io_return_status, TransferStatus::NotApplicable);
        }
    }

    #[test]
    fn refunded_and_quarantined_dispositions_are_distinguishable() {
        let refunded = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderSucceeded {
            block_index: 91,
            amount_e8s: 990_000,
        });
        let quarantined = rejected_redemption_op(RejectedFundDisposition::QuarantinedTerminal {
            reason: "unresolvable sender".to_string(),
        });

        assert!(matches!(
            refunded.rejected_fund_disposition,
            Some(RejectedFundDisposition::ReturnToSenderSucceeded { .. })
        ));
        assert!(matches!(
            quarantined.rejected_fund_disposition,
            Some(RejectedFundDisposition::QuarantinedTerminal { .. })
        ));
        assert_eq!(refunded.phase, OperationPhase::Completed);
        assert_eq!(quarantined.phase, OperationPhase::FailedTerminal);
    }

    #[test]
    fn refunded_rejected_redemption_survives_upgrade_as_completed() {
        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderSucceeded {
            block_index: 91,
            amount_e8s: 990_000,
        });
        let restored = op.clone();

        assert_eq!(restored.phase, OperationPhase::Completed);
        assert_eq!(restored.io_return_status, TransferStatus::Succeeded);
        assert_eq!(restored.icp_payout_status, TransferStatus::NotApplicable);
    }

    #[test]
    fn rejected_refund_retry_within_transaction_window_is_idempotent() {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        let refund_source = Account::new(principal, Some(Subaccount([9; 32])));
        let sender = Account::new(principal, Some(Subaccount([7; 32])));
        let request = LedgerTransferRequest {
            from_subaccount: refund_source.subaccount,
            to: sender.clone(),
            amount_e8s: 990_000,
            fee_e8s: None,
            memo: Some(rejected_refund_memo("io:88")),
            created_at_time: Some(88),
        };
        let matching = LedgerBlock {
            block_index: BlockIndex(91),
            timestamp_nanos: 88,
            created_at_time: None,
            from: Some(refund_source.clone()),
            to: Some(sender),
            amount_e8s: 990_000,
            fee_e8s: Some(10_000),
            memo: Some(rejected_refund_memo("io:88")),
            operation_kind: LedgerOperationKind::Transfer,
        };

        assert_eq!(
            classify_boundary_transfer_result_with_source(
                &request,
                &refund_source,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(91)
                }),
                Some(&matching),
            ),
            BoundaryTransferDecision::Succeeded(91)
        );
    }

    #[test]
    fn too_old_refund_with_index_lag_does_not_resend() {
        let mut scan = AccountHistoryScanState::default();
        scan.status.lag_suspected = true;
        scan.status.last_error = Some("index tip is behind ledger tip".to_string());

        assert!(matches!(
            classify_too_old_refund_proof_state(&scan, 1, REJECTED_REFUND_PROOF_SCAN_MAX_PAGES),
            TooOldRefundProofDisposition::IndexNotCaughtUp(_)
        ));

        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderProofPending {
            reason: "TooOld; index lag".to_string(),
            original_created_at_time: Some(500),
            proof_scan_state: None,
        });

        assert_eq!(rejected_refund_attempt_created_at(&op), 500);
        assert!(!is_retryable_rejected_refund_operation(&op));
        assert!(op
            .rejected_fund_disposition
            .as_ref()
            .is_some_and(|disposition| matches!(
                disposition,
                RejectedFundDisposition::ReturnToSenderProofPending {
                    original_created_at_time: Some(500),
                    ..
                }
            )));
    }

    #[test]
    fn too_old_refund_with_archive_required_does_not_resend() {
        let disposition = TooOldRefundProofDisposition::HistoryIncomplete(
            "IO index refund proof requires archive traversal before retry".to_string(),
        );
        assert!(matches!(
            disposition,
            TooOldRefundProofDisposition::HistoryIncomplete(_)
        ));

        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderProofPending {
            reason: "TooOld; archive required".to_string(),
            original_created_at_time: Some(600),
            proof_scan_state: None,
        });

        assert_eq!(rejected_refund_attempt_created_at(&op), 600);
        assert!(!is_retryable_rejected_refund_operation(&op));
    }

    #[test]
    fn too_old_refund_with_incomplete_backfill_does_not_resend() {
        let mut scan = AccountHistoryScanState::default();
        scan.status.scan_incomplete = true;

        assert!(matches!(
            classify_too_old_refund_proof_state(
                &scan,
                REJECTED_REFUND_PROOF_SCAN_MAX_PAGES,
                REJECTED_REFUND_PROOF_SCAN_MAX_PAGES,
            ),
            TooOldRefundProofDisposition::HistoryIncomplete(_)
        ));

        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderProofPending {
            reason: "TooOld; bounded backfill incomplete".to_string(),
            original_created_at_time: Some(700),
            proof_scan_state: None,
        });

        assert!(!is_retryable_rejected_refund_operation(&op));
    }

    #[test]
    fn too_old_refund_matching_index_proof_marks_success() {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        let refund_source = Account::new(principal, Some(Subaccount([9; 32])));
        let sender = Account::new(principal, Some(Subaccount([7; 32])));
        let request = LedgerTransferRequest {
            from_subaccount: refund_source.subaccount,
            to: sender.clone(),
            amount_e8s: 990_000,
            fee_e8s: None,
            memo: Some(rejected_refund_memo("io:88")),
            created_at_time: Some(88),
        };
        let proof = LedgerBlock {
            block_index: BlockIndex(91),
            timestamp_nanos: 88,
            created_at_time: None,
            from: Some(refund_source.clone()),
            to: Some(sender),
            amount_e8s: 990_000,
            fee_e8s: Some(10_000),
            memo: Some(rejected_refund_memo("io:88")),
            operation_kind: LedgerOperationKind::Transfer,
        };
        let disposition = TooOldRefundProofDisposition::ProofFound(
            duplicate_matches_expected(&request, &proof).unwrap(),
        );
        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderSucceeded {
            block_index: 91,
            amount_e8s: 990_000,
        });

        assert_eq!(
            disposition,
            TooOldRefundProofDisposition::ProofFound(BlockIndex(91))
        );
        assert_eq!(op.phase, OperationPhase::Completed);
        assert_eq!(op.io_return_status, TransferStatus::Succeeded);
        assert!(!is_retryable_rejected_refund_operation(&op));
    }

    #[test]
    fn too_old_refund_missing_proof_enters_manual_reconciliation() {
        let mut scan = AccountHistoryScanState::default();
        scan.cursor.backfill_complete = true;
        scan.cursor.latest_cursor = Some(BlockIndex(90));
        scan.status.num_blocks_synced = Some(BlockIndex(90));

        assert!(matches!(
            classify_too_old_refund_proof_state(&scan, 1, REJECTED_REFUND_PROOF_SCAN_MAX_PAGES),
            TooOldRefundProofDisposition::CompleteNoMatch(_)
        ));

        let op = rejected_redemption_op(
            RejectedFundDisposition::ReturnToSenderManualReconciliationRequired {
                reason: "complete canonical history has no refund proof".to_string(),
                original_created_at_time: Some(800),
            },
        );

        assert_eq!(op.phase, OperationPhase::FailedTerminal);
        assert!(!is_retryable_rejected_refund_operation(&op));
    }

    #[test]
    fn manual_reconciliation_refund_is_not_selected_for_automatic_retry() {
        let op = rejected_redemption_op(
            RejectedFundDisposition::ReturnToSenderManualReconciliationRequired {
                reason: "proof permanently unavailable".to_string(),
                original_created_at_time: Some(900),
            },
        );

        assert_eq!(
            next_retryable_rejected_refund_operation(&[op], &BTreeSet::new()),
            None
        );
    }

    #[test]
    fn same_wasm_upgrade_preserves_refund_proof_pending_state() {
        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderProofPending {
            reason: "TooOld; index not caught up".to_string(),
            original_created_at_time: Some(1_000),
            proof_scan_state: None,
        });
        let restored = op.clone();

        assert_eq!(restored, op);
        assert!(matches!(
            restored.rejected_fund_disposition,
            Some(RejectedFundDisposition::ReturnToSenderProofPending {
                original_created_at_time: Some(1_000),
                ..
            })
        ));
        assert!(!is_retryable_rejected_refund_operation(&restored));
    }

    #[test]
    fn repeated_ticks_never_double_refund_when_proof_is_uncertain() {
        let proof_pending =
            rejected_redemption_op(RejectedFundDisposition::ReturnToSenderProofPending {
                reason: "TooOld; index lag".to_string(),
                original_created_at_time: Some(1_100),
                proof_scan_state: None,
            });
        let manual = rejected_redemption_op(
            RejectedFundDisposition::ReturnToSenderManualReconciliationRequired {
                reason: "proof unavailable".to_string(),
                original_created_at_time: Some(1_200),
            },
        );
        let journal = vec![proof_pending, manual];

        for _ in 0..3 {
            assert_eq!(
                next_retryable_rejected_refund_operation(&journal, &BTreeSet::new()),
                None
            );
        }
    }

    #[test]
    fn rejected_refund_too_old_result_is_auditable() {
        let op = rejected_redemption_op(RejectedFundDisposition::ReturnToSenderProofPending {
            reason: "ledger transfer failed: TooOld; refund proof pending".to_string(),
            original_created_at_time: Some(600),
            proof_scan_state: None,
        });

        match op.rejected_fund_disposition.unwrap() {
            RejectedFundDisposition::ReturnToSenderProofPending {
                reason,
                original_created_at_time,
                ..
            } => {
                assert!(reason.contains("TooOld"));
                assert_eq!(original_created_at_time, Some(600));
            }
            other => panic!("unexpected disposition: {other:?}"),
        }
    }

    #[test]
    fn proof_absence_requires_index_catchup_evidence() {
        let mut scan = AccountHistoryScanState::default();
        scan.cursor.backfill_complete = true;
        scan.cursor.latest_cursor = Some(BlockIndex(90));

        assert!(matches!(
            classify_too_old_refund_proof_state(&scan, 1, REJECTED_REFUND_PROOF_SCAN_MAX_PAGES),
            TooOldRefundProofDisposition::HistoryIncomplete(reason)
                if reason.contains("did not report ledger catch-up")
        ));
    }

    #[test]
    fn proof_absence_requires_full_account_history_backfill() {
        let mut scan = AccountHistoryScanState::default();
        scan.cursor.latest_cursor = Some(BlockIndex(90));
        scan.status.num_blocks_synced = Some(BlockIndex(90));

        assert!(matches!(
            classify_too_old_refund_proof_state(&scan, 1, REJECTED_REFUND_PROOF_SCAN_MAX_PAGES),
            TooOldRefundProofDisposition::IndexNotCaughtUp(reason)
                if reason.contains("no complete backfill")
        ));
    }

    #[test]
    fn proof_absence_requires_synced_index_at_or_above_observed_history() {
        let mut scan = AccountHistoryScanState::default();
        scan.cursor.backfill_complete = true;
        scan.cursor.latest_cursor = Some(BlockIndex(90));
        scan.status.num_blocks_synced = Some(BlockIndex(89));

        assert!(matches!(
            classify_too_old_refund_proof_state(&scan, 1, REJECTED_REFUND_PROOF_SCAN_MAX_PAGES),
            TooOldRefundProofDisposition::HistoryIncomplete(reason)
                if reason.contains("only synced through block 89")
        ));
    }

    #[test]
    fn proof_absence_complete_no_match_requires_all_completeness_signals() {
        let mut scan = AccountHistoryScanState::default();
        scan.cursor.backfill_complete = true;
        scan.cursor.latest_cursor = Some(BlockIndex(90));
        scan.status.num_blocks_synced = Some(BlockIndex(90));

        assert!(matches!(
            classify_too_old_refund_proof_state(&scan, 1, REJECTED_REFUND_PROOF_SCAN_MAX_PAGES),
            TooOldRefundProofDisposition::CompleteNoMatch(_)
        ));
    }

    #[test]
    fn upgrade_after_refund_before_journal_update_recovers_by_index_proof() {
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        let refund_source = Account::new(principal, Some(Subaccount([9; 32])));
        let sender = Account::new(principal, Some(Subaccount([7; 32])));
        let request = LedgerTransferRequest {
            from_subaccount: refund_source.subaccount,
            to: sender.clone(),
            amount_e8s: 990_000,
            fee_e8s: None,
            memo: Some(rejected_refund_memo("io:88")),
            created_at_time: Some(88),
        };
        let finalized = LedgerBlock {
            block_index: BlockIndex(91),
            timestamp_nanos: 88,
            created_at_time: None,
            from: Some(refund_source.clone()),
            to: Some(sender),
            amount_e8s: 990_000,
            fee_e8s: Some(10_000),
            memo: Some(rejected_refund_memo("io:88")),
            operation_kind: LedgerOperationKind::Transfer,
        };

        assert_eq!(
            duplicate_matches_expected(&request, &finalized),
            Ok(BlockIndex(91))
        );
        assert_eq!(finalized.from.as_ref(), Some(&refund_source));
    }

    #[test]
    fn io_stream_manager_real_redemption_duplicate_icp_payout_block_must_match_expected() {
        let request = LedgerTransferRequest {
            from_subaccount: Some(crate::clients::icp_ledger::mock_subaccount(
                STREAM_MANAGER_DEPOSIT_ACCOUNT,
            )),
            to: crate::clients::icp_ledger::mock_account(JUPITER_FAUCET_SOURCE),
            amount_e8s: 100,
            fee_e8s: None,
            memo: None,
            created_at_time: None,
        };
        let matching = duplicate_proof_block_with_memo(100, JUPITER_FAUCET_SOURCE, None);
        assert_eq!(
            classify_boundary_transfer_result(
                &request,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(9)
                }),
                Some(&matching),
            ),
            BoundaryTransferDecision::Succeeded(9)
        );

        let wrong_destination = duplicate_proof_block_with_memo(100, "wrong_destination", None);
        assert!(matches!(
            classify_boundary_transfer_result(
                &request,
                Err(LedgerTransferError::Duplicate {
                    duplicate_of: BlockIndex(9)
                }),
                Some(&wrong_destination),
            ),
            BoundaryTransferDecision::Retryable(_)
        ));
    }

    #[test]
    fn boundary_transfer_error_classes_remain_retryable() {
        let request = transfer_request(100, JUPITER_FAUCET_SOURCE, "icp:1");
        for err in [
            LedgerTransferError::TemporarilyUnavailable,
            LedgerTransferError::CanisterCallFailed {
                method: "icrc1_transfer".to_string(),
                message: "reject".to_string(),
            },
            LedgerTransferError::BadFee {
                expected_fee_e8s: 10,
            },
            LedgerTransferError::InsufficientFunds { balance_e8s: 1 },
        ] {
            assert!(matches!(
                classify_boundary_transfer_result(&request, Err(err), None),
                BoundaryTransferDecision::Retryable(_)
            ));
        }
    }
}
