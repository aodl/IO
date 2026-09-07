use candid::{CandidType, Nat, Principal};
use ic_cdk::call::Call;
pub use io_receipt_types::ClaimBackingReceiptProgress;
use serde::Deserialize;
use std::cell::Cell;

use crate::{
    canonical, receipt,
    redemption::{self, RedemptionOperation, RedemptionPhase},
    state::{
        self, Account, DispatchEpoch, Lifecycle, OperationSequence, RedemptionResult,
        RedemptionStreamOperation, StreamOperation, StreamStateV1,
    },
    transfer::{
        classify_result, ClassifiedResult, IcrcTransferArg, OwnTransferIntent, TransferAttempt,
        TransferResult, TransferState,
    },
};

thread_local! {
    static REDEMPTION_WORK_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

struct RedemptionWorkGuard;

impl RedemptionWorkGuard {
    fn acquire() -> Result<Self, ApiError> {
        REDEMPTION_WORK_ACTIVE.with(|active| {
            if active.replace(true) {
                Err(ApiError::Busy)
            } else {
                Ok(Self)
            }
        })
    }
}

impl Drop for RedemptionWorkGuard {
    fn drop(&mut self) {
        REDEMPTION_WORK_ACTIVE.with(|active| active.set(false));
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ApiError {
    Anonymous,
    Unauthorized,
    Paused,
    Busy,
    Invalid(String),
    Ledger(String),
    Pending(String),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RedemptionProgress {
    Idle,
    Pending,
    RateLimited { retry_at_nanos: u64 },
    Completed(RedemptionResult),
    Stuck(String),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum StreamProgress {
    Redemption(RedemptionProgress),
    ClaimReceipt(ClaimBackingReceiptProgress),
    BackingReconciliation,
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct Status {
    pub lifecycle: Lifecycle,
    pub operation_kind: Option<String>,
    pub operation_phase: Option<String>,
    pub next_operation_sequence: u64,
    pub redemption_staging_account: Account,
    pub pending_redemption_candidates: u64,
    pub redemption_scan_status: io_ledger_types::AccountHistoryScanStatus,
    pub paid_unswept_redemption_io_e8s: Option<u128>,
    pub last_completed_redemption: Option<RedemptionResult>,
    pub latest_entitlement_batch_generation: u64,
    pub latest_processed_reward_event: Option<crate::state::RewardEventId>,
    pub latest_reward_event_classification: Option<crate::state::RewardEventClassification>,
    pub accumulated_entitlements: Vec<crate::state::FrozenEntitlement>,
    pub accumulated_eligible_credit: u128,
    pub accumulated_policy_credit: u128,
    pub processed_reward_event_count: u64,
    pub missed_reward_event_count: u64,
    pub reward_work_due: bool,
    pub reward_processing_paused: bool,
    pub governance_parameters_fresh: bool,
    pub pending_entitlement_batch_eligible_credit: Option<u128>,
    pub pending_entitlement_batch_policy_credit: Option<u128>,
    pub latest_reconciliation_checkpoint: Option<crate::state::ReconciliationCheckpoint>,
    pub prepared_exit_generation: Option<u64>,
    pub prepared_exit_member_count: u32,
    pub committed_exit_member_count: u32,
}

pub(crate) fn require_ready(state: &StreamStateV1) -> Result<(), ApiError> {
    match state.lifecycle {
        Lifecycle::Ready => Ok(()),
        Lifecycle::Paused => Err(ApiError::Paused),
    }
}

pub(crate) async fn submit(intent: &OwnTransferIntent) -> Result<TransferResult, String> {
    match intent {
        OwnTransferIntent::Icrc1 {
            ledger,
            from_subaccount,
            to,
            amount,
            fee,
            memo,
            created_at_time,
        } => Call::bounded_wait(*ledger, "icrc1_transfer")
            .with_arg(IcrcTransferArg {
                from_subaccount: (*from_subaccount != [0; 32]).then(|| from_subaccount.to_vec()),
                to: to.clone(),
                amount: Nat::from(*amount),
                fee: Some(Nat::from(*fee)),
                memo: Some(memo.clone()),
                created_at_time: Some(*created_at_time),
            })
            .await
            .map_err(|error| format!("icrc1_transfer call failed: {error:?}"))?
            .candid()
            .map_err(|error| format!("icrc1_transfer decode failed: {error:?}")),
    }
}

pub fn redemption_staging_account() -> Account {
    io_accounts::redemption_staging(ic_cdk::api::canister_self())
}

pub async fn process_redemptions(now: u64) -> Result<RedemptionProgress, ApiError> {
    let mut current = state::read();
    require_ready(&current)?;
    if let Some(retry_at) =
        manual_redemption_retry_at(current.last_manual_redemption_work_started_at_nanos, now)?
    {
        return Ok(RedemptionProgress::RateLimited {
            retry_at_nanos: retry_at,
        });
    }
    current.last_manual_redemption_work_started_at_nanos = now;
    state::write(current);
    run_redemption_worker(now, false).await
}

fn manual_redemption_retry_at(last_started_at: u64, now: u64) -> Result<Option<u64>, ApiError> {
    if last_started_at == 0 {
        return Ok(None);
    }
    let retry_at = last_started_at
        .checked_add(redemption::MANUAL_WORK_COOLDOWN_NANOS)
        .ok_or_else(|| ApiError::Invalid("manual redemption deadline overflow".into()))?;
    Ok((now < retry_at).then_some(retry_at))
}

pub(crate) async fn run_scheduled_redemption_work(
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    run_redemption_worker(now, true).await
}

async fn run_redemption_worker(now: u64, scheduled: bool) -> Result<RedemptionProgress, ApiError> {
    let _guard = RedemptionWorkGuard::acquire()?;
    let mut snapshot = state::read();
    require_ready(&snapshot)?;
    if snapshot.active_operation.is_some() {
        return resume(now).await;
    }
    if snapshot.reward_checkpoint.reward_work_due
        || snapshot.stake_observation_due
        || snapshot.structural_reconciliation_due
    {
        return Ok(RedemptionProgress::Pending);
    }
    // A scheduled invocation is one bounded poll attempt even when it retries an
    // already-queued illiquid candidate. Persist the cadence gate before the
    // first external call so a pending candidate cannot create a hot timer loop.
    if scheduled {
        let due_at = snapshot
            .last_redemption_poll_started_at_nanos
            .saturating_add(
                snapshot
                    .config
                    .redemption_poll_interval_seconds
                    .saturating_mul(1_000_000_000),
            );
        if snapshot.last_redemption_poll_started_at_nanos != 0 && now < due_at {
            return Ok(RedemptionProgress::Idle);
        }
        snapshot.last_redemption_poll_started_at_nanos = now;
        state::write(snapshot.clone());
    }
    if state::oldest_redemption_candidate().is_none() {
        discover_redemptions(now).await?;
    }
    let Some(block) = state::oldest_redemption_candidate() else {
        return Ok(RedemptionProgress::Idle);
    };
    activate_candidate(block, now).await
}

fn ledger_index_account(account: &Account) -> Result<io_ledger_types::Account, String> {
    let canonical = account.canonical()?;
    Ok(io_ledger_types::Account::new(
        canonical.owner,
        (canonical.subaccount != [0; 32])
            .then_some(io_ledger_types::Subaccount(canonical.subaccount)),
    ))
}

fn stream_account(account: &io_ledger_types::Account) -> Account {
    Account {
        owner: account.owner,
        subaccount: account.subaccount.map(|value| value.0.to_vec()),
    }
}

#[cfg(target_family = "wasm")]
async fn read_index_page(
    index: Principal,
    request: io_ledger_types::IndexScanRequest,
) -> Result<io_ledger_types::IndexScanResult, String> {
    use io_ledger_types::LedgerIndexClient;
    io_ledger_types::IcrcIndexCanisterClient { canister: index }
        .get_account_transactions(request)
        .await
        .map_err(|error| format!("redemption index discovery failed: {error:?}"))
}

#[cfg(not(target_family = "wasm"))]
async fn read_index_page(
    _index: Principal,
    _request: io_ledger_types::IndexScanRequest,
) -> Result<io_ledger_types::IndexScanResult, String> {
    Err("redemption index discovery is only available in canister Wasm".into())
}

async fn discover_redemptions(now: u64) -> Result<(), ApiError> {
    let mut expected = state::read();
    if expected.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    let available =
        redemption::MAX_PENDING_CANDIDATES.saturating_sub(state::redemption_candidate_count());
    if available < redemption::MAX_INDEX_TRANSACTIONS_PER_PAGE {
        return Err(ApiError::Pending(
            "redemption candidate queue must drain before another bounded page".into(),
        ));
    }
    expected.last_redemption_poll_started_at_nanos = now;
    state::write(expected.clone());
    let requested_start = expected.redemption_scan_state.next_request_start();
    let page = read_index_page(
        expected.config.io_index,
        io_ledger_types::IndexScanRequest {
            start: requested_start,
            limit: redemption::MAX_INDEX_TRANSACTIONS_PER_PAGE,
            account_filter: Some(
                ledger_index_account(&redemption_staging_account()).map_err(ApiError::Invalid)?,
            ),
            account_aliases: Vec::new(),
        },
    )
    .await;
    let page = match page {
        Ok(page) => page,
        Err(error) => {
            let mut latest = state::read();
            if latest == expected {
                latest.redemption_scan_state = latest
                    .redemption_scan_state
                    .record_unreadable(error.chars().take(512).collect::<String>());
                state::write(latest);
            }
            return Err(ApiError::Pending(error));
        }
    };
    if page.raw_transaction_ids.len() > redemption::MAX_INDEX_TRANSACTIONS_PER_PAGE as usize
        || page.transactions.len() > redemption::MAX_INDEX_TRANSACTIONS_PER_PAGE as usize
    {
        record_scanner_fault(
            &expected,
            "redemption index returned an unsupported oversized page",
        );
        return Err(ApiError::Pending(
            "redemption discovery page was not safe to advance".into(),
        ));
    }
    let outcome = expected
        .redemption_scan_state
        .observe_page(
            &page,
            requested_start,
            redemption::MAX_INDEX_TRANSACTIONS_PER_PAGE,
            1,
            redemption::MAX_INDEX_PAGES_PER_RUN,
            Some(now),
        )
        .map_err(|fault| {
            record_scanner_fault(&expected, &format!("redemption scan invariant: {fault:?}"));
            ApiError::Pending("redemption scanner failed closed".into())
        })?;
    let staging = ledger_index_account(&redemption_staging_account()).map_err(ApiError::Invalid)?;
    let mut candidates = Vec::new();
    for item in &outcome.transactions_chronological {
        let tx = &item.transaction;
        if tx.operation_kind != io_ledger_types::LedgerOperationKind::Transfer
            || tx.amount_e8s < expected.config.minimum_redemption_io_e8s
            || tx.to.as_ref() != Some(&staging)
            || tx.from.is_none()
        {
            continue;
        }
        if !candidate_source_allowed(
            &expected.config,
            &stream_account(tx.from.as_ref().expect("checked")),
        )? {
            continue;
        }
        candidates.push(item.block_index.0);
    }
    if candidates.len() as u64 > available {
        return Err(ApiError::Pending(
            "redemption queue cannot atomically represent the scanned page".into(),
        ));
    }
    if state::read() != expected {
        return Err(ApiError::Busy);
    }
    for block in candidates {
        if !state::redemption_candidate_contains(block) {
            state::insert_redemption_candidate(block).map_err(ApiError::Invalid)?;
        }
    }
    let mut latest = state::read();
    if latest != expected {
        return Err(ApiError::Busy);
    }
    latest.redemption_scan_state = outcome.next_state;
    state::write(latest);
    Ok(())
}

fn record_scanner_fault(expected: &StreamStateV1, message: &str) {
    let mut latest = state::read();
    if &latest != expected {
        return;
    }
    latest.redemption_scan_state.status.invariant_broken_count = latest
        .redemption_scan_state
        .status
        .invariant_broken_count
        .saturating_add(1);
    latest.redemption_scan_state.status.last_error = Some(message.chars().take(512).collect());
    latest.redemption_scan_state.status.safe_to_continue = false;
    state::write(latest);
}

fn candidate_source_allowed(
    config: &state::StreamConfig,
    source: &Account,
) -> Result<bool, ApiError> {
    source.validate().map_err(ApiError::Invalid)?;
    if source.owner == Principal::anonymous() || source.owner == Principal::management_canister() {
        return Ok(false);
    }
    if source
        .effective_eq(&config.io_reserve)
        .map_err(ApiError::Invalid)?
    {
        return Ok(false);
    }
    for excluded in &config.nonredeemable_governance_io_accounts {
        if source.effective_eq(excluded).map_err(ApiError::Invalid)? {
            return Ok(false);
        }
    }
    Ok(true)
}

async fn activate_candidate(block: u64, now: u64) -> Result<RedemptionProgress, ApiError> {
    let before = state::read();
    if before.active_operation.is_some() || !state::redemption_candidate_contains(block) {
        return Err(ApiError::Busy);
    }
    let exact = canonical::exact_icrc_transfer(before.config.io_ledger, u128::from(block))
        .await
        .map_err(ApiError::Ledger)?;
    let staging = redemption_staging_account();
    if !exact.to.effective_eq(&staging).map_err(ApiError::Invalid)?
        || !candidate_source_allowed(&before.config, &exact.from)?
        || exact.amount_e8s == 0
    {
        state::remove_redemption_candidate(block);
        return Ok(RedemptionProgress::Pending);
    }
    if exact.amount_e8s < before.config.minimum_redemption_io_e8s {
        state::remove_redemption_candidate(block);
        return Ok(RedemptionProgress::Pending);
    }
    let snapshot = canonical::claim_snapshot(&before.config)
        .await
        .map_err(ApiError::Ledger)?;
    if snapshot.io_fee_e8s != before.config.expected_io_fee_e8s
        || snapshot.icp_fee_e8s != before.config.expected_icp_fee_e8s
    {
        return Err(ApiError::Pending(
            "staged redemption awaits reviewed canonical fee configuration".into(),
        ));
    }
    let quote =
        redemption::quote_for_amount(exact.amount_e8s, &snapshot).map_err(ApiError::Invalid)?;
    if snapshot.liquid_icp_e8s < quote.gross_icp {
        return Ok(RedemptionProgress::Pending);
    }
    let mut latest = state::read();
    if latest != before || latest.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    let sequence = latest.next_operation_sequence;
    latest.next_operation_sequence.0 = sequence
        .0
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("operation sequence overflow".into()))?;
    let payout = TransferAttempt::prepared(OwnTransferIntent::Icrc1 {
        ledger: latest.config.icp_ledger,
        from_subaccount: latest
            .config
            .liquid_icp
            .canonical()
            .map_err(ApiError::Invalid)?
            .subaccount,
        to: exact.from.clone(),
        amount: quote.net_icp,
        fee: snapshot.icp_fee_e8s,
        memo: crate::transfer::deterministic_memo(
            b"io-redemption-pay-v2",
            Principal::from_slice(&u128::from(block).to_be_bytes()),
            sequence.0,
        ),
        created_at_time: now,
    })
    .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(StreamOperation::Redemption(Box::new(
        RedemptionStreamOperation::Active(Box::new(RedemptionOperation {
            sequence,
            source_io_block: u128::from(block),
            source_account: exact.from,
            staged_io_amount_e8s: exact.amount_e8s,
            gross_icp_e8s: quote.gross_icp,
            net_icp_e8s: quote.net_icp,
            icp_fee_e8s: snapshot.icp_fee_e8s,
            io_sweep_fee_e8s: snapshot.io_fee_e8s,
            icp_payout: payout,
            reserve_sweep: None,
            last_external_call_started_at_nanos: 0,
            phase: RedemptionPhase::PayoutPrepared,
        })),
    )));
    state::write(latest);
    drive_redemption(sequence, now).await
}

fn active_redemption() -> Result<RedemptionOperation, ApiError> {
    match state::read().active_operation {
        Some(StreamOperation::Redemption(operation)) => match *operation {
            RedemptionStreamOperation::Active(operation) => Ok(*operation),
        },
        _ => Err(ApiError::Invalid("no active redemption".into())),
    }
}

fn replace_redemption(
    expected: &RedemptionOperation,
    operation: RedemptionOperation,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    if !matches!(
        &latest.active_operation,
        Some(StreamOperation::Redemption(active))
            if matches!(active.as_ref(), RedemptionStreamOperation::Active(value)
                if value.as_ref() == expected)
    ) {
        return Err(ApiError::Busy);
    }
    latest.active_operation = Some(StreamOperation::Redemption(Box::new(
        RedemptionStreamOperation::Active(Box::new(operation)),
    )));
    state::write(latest);
    Ok(())
}

pub async fn resume(now: u64) -> Result<RedemptionProgress, ApiError> {
    let operation = active_redemption()?;
    if operation.phase == RedemptionPhase::Stuck {
        return Ok(RedemptionProgress::Stuck(
            "exact transfer proof or reviewed recovery is required".into(),
        ));
    }
    drive_redemption(operation.sequence, now).await
}

async fn drive_redemption(
    sequence: OperationSequence,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    let operation = active_redemption()?;
    if operation.sequence != sequence {
        return Err(ApiError::Busy);
    }
    match operation.phase {
        RedemptionPhase::PayoutPrepared | RedemptionPhase::PayoutSubmitted => {
            dispatch_payout(operation, now).await
        }
        RedemptionPhase::PayoutSucceeded => prepare_sweep(operation, now).await,
        RedemptionPhase::SweepPrepared | RedemptionPhase::SweepSubmitted => {
            dispatch_sweep(operation, now).await
        }
        RedemptionPhase::Stuck => Ok(RedemptionProgress::Stuck(
            "exact transfer proof or reviewed recovery is required".into(),
        )),
    }
}

fn next_dispatch_epoch(
    attempt: &TransferAttempt,
    now: u64,
    retry_delay: u64,
) -> Result<DispatchEpoch, ApiError> {
    match attempt.state {
        TransferState::Prepared => Ok(DispatchEpoch(1)),
        TransferState::Submitted {
            epoch,
            last_submitted_at,
            ..
        } => {
            if now.saturating_sub(last_submitted_at) < retry_delay {
                return Err(ApiError::Busy);
            }
            epoch
                .0
                .checked_add(1)
                .map(DispatchEpoch)
                .ok_or_else(|| ApiError::Invalid("transfer dispatch epoch overflow".into()))
        }
        TransferState::Succeeded { .. } => Err(ApiError::Busy),
        TransferState::Stuck { ref reason } => Err(ApiError::Stuck(reason.clone())),
    }
}

async fn dispatch_payout(
    mut operation: RedemptionOperation,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    let expected = operation.clone();
    let retry = state::read().config.retry_delay_nanos;
    let epoch = next_dispatch_epoch(&operation.icp_payout, now, retry)?;
    let first = match operation.icp_payout.state {
        TransferState::Submitted {
            first_submitted_at, ..
        } => first_submitted_at,
        _ => now,
    };
    operation.icp_payout.state = TransferState::Submitted {
        epoch,
        first_submitted_at: first,
        last_submitted_at: now,
    };
    operation.phase = RedemptionPhase::PayoutSubmitted;
    let intent = operation.icp_payout.intent.clone();
    let sequence = operation.sequence;
    replace_redemption(&expected, operation.clone())?;
    let submitted = operation;
    let response = submit(&intent).await;
    match response {
        Ok(result) => match classify_result(result).map_err(ApiError::Ledger)? {
            ClassifiedResult::Succeeded(block) => {
                let mut latest = active_redemption()?;
                if latest.sequence != sequence || latest.icp_payout.intent != intent {
                    return Err(ApiError::Busy);
                }
                if let TransferState::Succeeded { block: accepted } = latest.icp_payout.state {
                    if accepted != block {
                        return Err(ApiError::Invalid(
                            "conflicting success blocks for immutable payout".into(),
                        ));
                    }
                    return Ok(RedemptionProgress::Pending);
                }
                if !matches!(
                    latest.phase,
                    RedemptionPhase::PayoutSubmitted | RedemptionPhase::Stuck
                ) {
                    return Ok(RedemptionProgress::Pending);
                }
                let current = latest.clone();
                latest.icp_payout.state = TransferState::Succeeded { block };
                latest.phase = RedemptionPhase::PayoutSucceeded;
                latest.last_external_call_started_at_nanos = 0;
                replace_redemption(&current, latest.clone())?;
                prepare_sweep(latest, ic_cdk::api::time()).await
            }
            ClassifiedResult::NoEffect(reason) => {
                let latest = active_redemption()?;
                if latest != submitted {
                    return Err(ApiError::Pending(
                        "stale payout rejection ignored after newer progress".into(),
                    ));
                }
                let current = latest.clone();
                stuck(&current, latest, true, reason)
            }
            ClassifiedResult::Ambiguous(reason) => Err(ApiError::Pending(reason)),
        },
        Err(reason) => Err(ApiError::Pending(reason)),
    }
}

async fn prepare_sweep(
    mut operation: RedemptionOperation,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    if operation.phase != RedemptionPhase::PayoutSucceeded || operation.reserve_sweep.is_some() {
        return Err(ApiError::Busy);
    }
    let expected = operation.clone();
    let config = state::read().config;
    if operation.last_external_call_started_at_nanos != 0
        && now.saturating_sub(operation.last_external_call_started_at_nanos)
            < config.retry_delay_nanos
    {
        return Err(ApiError::Busy);
    }
    operation.last_external_call_started_at_nanos = now;
    replace_redemption(&expected, operation.clone())?;
    let admitted = operation.clone();
    let fee_result = canonical::fee(config.io_ledger).await;
    if active_redemption()? != admitted {
        return Err(ApiError::Busy);
    }
    let current_fee = fee_result.map_err(ApiError::Ledger)?;
    if current_fee != config.expected_io_fee_e8s || current_fee != operation.io_sweep_fee_e8s {
        pause_if_redemption(&admitted);
        return Err(ApiError::Pending(
            "paid redemption awaits reviewed IO sweep fee configuration".into(),
        ));
    }
    let amount = operation
        .staged_io_amount_e8s
        .checked_sub(current_fee)
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            ApiError::Invalid("staged amount does not cover reserve sweep fee".into())
        })?;
    operation.reserve_sweep = Some(
        TransferAttempt::prepared(OwnTransferIntent::Icrc1 {
            ledger: config.io_ledger,
            from_subaccount: io_accounts::REDEMPTION_STAGING_SUBACCOUNT,
            to: config.io_reserve,
            amount,
            fee: current_fee,
            memo: crate::transfer::deterministic_memo(
                b"io-redemption-sweep-v1",
                Principal::from_slice(&operation.source_io_block.to_be_bytes()),
                operation.sequence.0,
            ),
            created_at_time: now,
        })
        .map_err(ApiError::Invalid)?,
    );
    operation.phase = RedemptionPhase::SweepPrepared;
    replace_redemption(&admitted, operation.clone())?;
    dispatch_sweep(operation, now).await
}

async fn dispatch_sweep(
    mut operation: RedemptionOperation,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    let expected = operation.clone();
    let retry = state::read().config.retry_delay_nanos;
    let attempt = operation
        .reserve_sweep
        .as_mut()
        .ok_or_else(|| ApiError::Invalid("reserve sweep intent is missing".into()))?;
    let epoch = next_dispatch_epoch(attempt, now, retry)?;
    let first = match attempt.state {
        TransferState::Submitted {
            first_submitted_at, ..
        } => first_submitted_at,
        _ => now,
    };
    attempt.state = TransferState::Submitted {
        epoch,
        first_submitted_at: first,
        last_submitted_at: now,
    };
    let intent = attempt.intent.clone();
    let sequence = operation.sequence;
    operation.phase = RedemptionPhase::SweepSubmitted;
    replace_redemption(&expected, operation.clone())?;
    let submitted = operation;
    let response = submit(&intent).await;
    match response {
        Ok(result) => match classify_result(result).map_err(ApiError::Ledger)? {
            ClassifiedResult::Succeeded(block) => {
                let mut latest = match active_redemption() {
                    Ok(value) => value,
                    Err(_) => {
                        return completed_redemption(submitted.source_io_block, None, Some(block))
                            .map(RedemptionProgress::Completed)
                            .ok_or(ApiError::Busy)
                    }
                };
                if latest.sequence != sequence
                    || latest.reserve_sweep.as_ref().map(|value| &value.intent) != Some(&intent)
                {
                    return Err(ApiError::Busy);
                }
                let current = latest.clone();
                let target = latest.reserve_sweep.as_mut().expect("checked");
                if let TransferState::Succeeded { block: accepted } = target.state {
                    if accepted != block {
                        return Err(ApiError::Invalid(
                            "conflicting success blocks for immutable reserve sweep".into(),
                        ));
                    }
                }
                target.state = TransferState::Succeeded { block };
                complete_redemption(&current, latest, ic_cdk::api::time())
            }
            ClassifiedResult::NoEffect(reason) => {
                let mut latest = active_redemption()?;
                if latest != submitted {
                    return Err(ApiError::Pending(
                        "stale reserve-sweep rejection ignored after newer progress".into(),
                    ));
                }
                let current = latest.clone();
                let target = latest.reserve_sweep.as_mut().expect("checked");
                target.state = TransferState::Stuck {
                    reason: reason.clone(),
                };
                stuck(&current, latest, false, reason)
            }
            ClassifiedResult::Ambiguous(reason) => Err(ApiError::Pending(reason)),
        },
        Err(reason) => Err(ApiError::Pending(reason)),
    }
}

fn stuck(
    expected: &RedemptionOperation,
    mut operation: RedemptionOperation,
    payout: bool,
    reason: String,
) -> Result<RedemptionProgress, ApiError> {
    if payout {
        operation.icp_payout.state = TransferState::Stuck {
            reason: reason.clone(),
        };
    }
    operation.phase = RedemptionPhase::Stuck;
    operation.last_external_call_started_at_nanos = 0;
    replace_redemption(expected, operation.clone())?;
    pause_if_redemption(&operation);
    Err(ApiError::Stuck(reason))
}

fn complete_redemption(
    expected: &RedemptionOperation,
    operation: RedemptionOperation,
    now: u64,
) -> Result<RedemptionProgress, ApiError> {
    let result = RedemptionResult {
        source_io_block: operation.source_io_block,
        source_account: operation.source_account.clone(),
        gross_icp_e8s: operation.gross_icp_e8s,
        net_icp_e8s: operation.net_icp_e8s,
        icp_fee_e8s: operation.icp_fee_e8s,
        icp_payout_block: operation
            .icp_payout
            .succeeded_block()
            .map_err(ApiError::Invalid)?,
        io_sweep_fee_e8s: operation.io_sweep_fee_e8s,
        reserve_sweep_block: operation
            .reserve_sweep
            .as_ref()
            .ok_or_else(|| ApiError::Invalid("reserve sweep is missing".into()))?
            .succeeded_block()
            .map_err(ApiError::Invalid)?,
        completed_at_nanos: now,
    };
    let mut latest = state::read();
    if !matches!(
        &latest.active_operation,
        Some(StreamOperation::Redemption(active))
            if matches!(active.as_ref(), RedemptionStreamOperation::Active(value)
                if value.as_ref() == expected)
    ) {
        return Err(ApiError::Busy);
    }
    // Remove the discovery marker first. If execution is interrupted between
    // these synchronous writes, the still-active exact operation is recoverable;
    // the inverse order could expose a queued block with no active operation.
    state::remove_redemption_candidate(
        u64::try_from(operation.source_io_block)
            .map_err(|_| ApiError::Invalid("source IO block exceeds u64".into()))?,
    );
    latest.last_completed_redemption = Some(result.clone());
    latest.active_operation = None;
    state::write(latest);
    crate::reward_timer::install_for_ready_state();
    Ok(RedemptionProgress::Completed(result))
}

pub(crate) fn pause() {
    let mut state = state::read();
    state.lifecycle = Lifecycle::Paused;
    state::write(state);
}

fn pause_if_redemption(expected: &RedemptionOperation) {
    let mut latest = state::read();
    if matches!(
        &latest.active_operation,
        Some(StreamOperation::Redemption(active))
            if matches!(active.as_ref(), RedemptionStreamOperation::Active(value)
                if value.as_ref() == expected)
    ) {
        latest.lifecycle = Lifecycle::Paused;
        state::write(latest);
    }
}

fn completed_redemption(
    source_io_block: u128,
    payout_block: Option<u128>,
    sweep_block: Option<u128>,
) -> Option<RedemptionResult> {
    state::read().last_completed_redemption.filter(|result| {
        result.source_io_block == source_io_block
            && payout_block.is_none_or(|block| result.icp_payout_block == block)
            && sweep_block.is_none_or(|block| result.reserve_sweep_block == block)
    })
}

fn admit_redemption_external_call(
    mut operation: RedemptionOperation,
    now: u64,
) -> Result<RedemptionOperation, ApiError> {
    let expected = operation.clone();
    let retry = state::read().config.retry_delay_nanos;
    if operation.last_external_call_started_at_nanos != 0
        && now.saturating_sub(operation.last_external_call_started_at_nanos) < retry
    {
        return Err(ApiError::Busy);
    }
    operation.last_external_call_started_at_nanos = now;
    replace_redemption(&expected, operation.clone())?;
    Ok(operation)
}

pub async fn resume_stream(now: u64) -> Result<StreamProgress, ApiError> {
    match state::read().active_operation {
        Some(StreamOperation::Redemption(_)) => {
            let _guard = RedemptionWorkGuard::acquire()?;
            resume(now).await.map(StreamProgress::Redemption)
        }
        Some(StreamOperation::ClaimReceipt(_)) => {
            receipt::resume(now).await.map(StreamProgress::ClaimReceipt)
        }
        Some(StreamOperation::PoolTopUp(_)) => {
            crate::pool_reconciliation::resume(now).await?;
            Ok(StreamProgress::BackingReconciliation)
        }
        None => Ok(StreamProgress::Idle),
    }
}

pub async fn prove_active_transfer(block_index: u128) -> Result<(), ApiError> {
    if matches!(
        state::read().active_operation,
        Some(StreamOperation::ClaimReceipt(_))
    ) {
        receipt::prove_recipient(block_index).await?;
        return Ok(());
    }
    if matches!(
        state::read().active_operation,
        Some(StreamOperation::PoolTopUp(_))
    ) {
        return crate::pool_reconciliation::prove_transfer(block_index).await;
    }
    let _guard = RedemptionWorkGuard::acquire()?;
    let snapshot = state::read();
    if snapshot.active_operation.is_none()
        && snapshot.last_completed_redemption.is_some_and(|completed| {
            completed.icp_payout_block == block_index
                || completed.reserve_sweep_block == block_index
        })
    {
        return Ok(());
    }
    let operation = active_redemption()?;
    if operation.phase != RedemptionPhase::Stuck {
        return Err(ApiError::Invalid(
            "only a Stuck transfer accepts proof".into(),
        ));
    }
    if matches!(operation.icp_payout.state, TransferState::Stuck { .. }) {
        prove_redemption_payout(
            admit_redemption_external_call(operation, ic_cdk::api::time())?,
            block_index,
        )
        .await
    } else if matches!(
        operation.reserve_sweep.as_ref().map(|value| &value.state),
        Some(TransferState::Stuck { .. })
    ) {
        prove_redemption_sweep(
            admit_redemption_external_call(operation, ic_cdk::api::time())?,
            block_index,
        )
        .await
    } else {
        Err(ApiError::Invalid(
            "stuck redemption has no provable transfer".into(),
        ))
    }
}

async fn prove_redemption_payout(
    mut operation: RedemptionOperation,
    block_index: u128,
) -> Result<(), ApiError> {
    let intent = operation.icp_payout.intent.clone();
    let exact_result = canonical::exact_icp_transfer(intent.ledger(), block_index).await;
    let current = match active_redemption() {
        Ok(value) => value,
        Err(_)
            if completed_redemption(operation.source_io_block, Some(block_index), None)
                .is_some() =>
        {
            return Ok(())
        }
        Err(error) => return Err(error),
    };
    if current != operation {
        return Err(ApiError::Busy);
    }
    let exact = exact_result.map_err(ApiError::Ledger)?;
    let OwnTransferIntent::Icrc1 {
        from_subaccount,
        to,
        amount,
        fee,
        memo,
        created_at_time,
        ..
    } = &intent;
    let source = Account {
        owner: ic_cdk::api::canister_self(),
        subaccount: (*from_subaccount != [0; 32]).then(|| from_subaccount.to_vec()),
    };
    if exact.from != canonical::icp_account_identifier(&source).map_err(ApiError::Invalid)?
        || exact.to != canonical::icp_account_identifier(to).map_err(ApiError::Invalid)?
        || exact.amount_e8s != *amount
        || exact.fee_e8s != *fee
        || exact.native_memo_u64 != 0
        || exact.icrc1_memo.as_deref() != Some(memo.as_slice())
        || exact.created_at_time != *created_at_time
        || exact.spender.is_some()
    {
        return Err(ApiError::Invalid(
            "exact block does not match stuck payout".into(),
        ));
    }
    operation.icp_payout.state = TransferState::Succeeded { block: block_index };
    operation.phase = RedemptionPhase::PayoutSucceeded;
    operation.last_external_call_started_at_nanos = 0;
    replace_redemption(&current, operation)?;
    Ok(())
}

async fn prove_redemption_sweep(
    mut operation: RedemptionOperation,
    block_index: u128,
) -> Result<(), ApiError> {
    let attempt = operation
        .reserve_sweep
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("reserve sweep is missing".into()))?;
    let intent = attempt.intent.clone();
    let exact_result = canonical::exact_icrc_transfer(intent.ledger(), block_index).await;
    let current = match active_redemption() {
        Ok(value) => value,
        Err(_)
            if completed_redemption(operation.source_io_block, None, Some(block_index))
                .is_some() =>
        {
            return Ok(())
        }
        Err(error) => return Err(error),
    };
    if current != operation {
        return Err(ApiError::Busy);
    }
    let exact = exact_result.map_err(ApiError::Ledger)?;
    let OwnTransferIntent::Icrc1 {
        from_subaccount,
        to,
        amount,
        fee,
        memo,
        created_at_time,
        ..
    } = &intent;
    let source = Account {
        owner: ic_cdk::api::canister_self(),
        subaccount: (*from_subaccount != [0; 32]).then(|| from_subaccount.to_vec()),
    };
    if !exact
        .from
        .effective_eq(&source)
        .map_err(ApiError::Invalid)?
        || !exact.to.effective_eq(to).map_err(ApiError::Invalid)?
        || exact.amount_e8s != *amount
        || exact.fee_e8s != Some(*fee)
        || exact.memo.as_deref() != Some(memo.as_slice())
        || exact.created_at_time != Some(*created_at_time)
        || exact.spender.is_some()
    {
        return Err(ApiError::Invalid(
            "exact block does not match stuck reserve sweep".into(),
        ));
    }
    operation.reserve_sweep.as_mut().expect("checked").state =
        TransferState::Succeeded { block: block_index };
    operation.last_external_call_started_at_nanos = 0;
    complete_redemption(&current, operation, ic_cdk::api::time()).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_redemption() -> RedemptionOperation {
        RedemptionOperation {
            sequence: OperationSequence(1),
            source_io_block: 9,
            source_account: Account {
                owner: Principal::from_slice(&[9]),
                subaccount: None,
            },
            staged_io_amount_e8s: 100,
            gross_icp_e8s: 100,
            net_icp_e8s: 90,
            icp_fee_e8s: 10,
            io_sweep_fee_e8s: 10,
            icp_payout: TransferAttempt {
                intent: OwnTransferIntent::Icrc1 {
                    ledger: Principal::from_slice(&[1]),
                    from_subaccount: [1; 32],
                    to: Account {
                        owner: Principal::from_slice(&[9]),
                        subaccount: None,
                    },
                    amount: 90,
                    fee: 10,
                    memo: vec![],
                    created_at_time: 1,
                },
                state: TransferState::Succeeded { block: 7 },
            },
            reserve_sweep: None,
            last_external_call_started_at_nanos: 0,
            phase: RedemptionPhase::PayoutSucceeded,
        }
    }

    #[test]
    fn manual_cooldown_is_global_not_caller_keyed() {
        let first_started_at = 100;
        for caller_number in 0..100 {
            let retry_at =
                manual_redemption_retry_at(first_started_at, first_started_at + caller_number)
                    .unwrap();
            assert_eq!(
                retry_at,
                Some(first_started_at + redemption::MANUAL_WORK_COOLDOWN_NANOS)
            );
        }
        assert_eq!(
            manual_redemption_retry_at(
                first_started_at,
                first_started_at + redemption::MANUAL_WORK_COOLDOWN_NANOS
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn scanner_bounds_are_smaller_than_the_durable_queue() {
        const {
            assert!(redemption::MAX_INDEX_PAGES_PER_RUN == 1);
            assert!(
                redemption::MAX_INDEX_TRANSACTIONS_PER_PAGE < redemption::MAX_PENDING_CANDIDATES
            );
        }
    }

    #[test]
    fn timer_and_manual_worker_share_one_in_flight_gate() {
        let guard = RedemptionWorkGuard::acquire().unwrap();
        assert!(matches!(
            RedemptionWorkGuard::acquire(),
            Err(ApiError::Busy)
        ));
        drop(guard);
        assert!(RedemptionWorkGuard::acquire().is_ok());
    }

    #[test]
    fn stale_redemption_snapshot_cannot_overwrite_newer_durable_progress() {
        let (canister, stream) = crate::state::tests::valid_state();
        let original = test_redemption();
        state::initialize(stream, canister).unwrap();
        let mut stream = state::read();
        stream.active_operation = Some(StreamOperation::Redemption(Box::new(
            RedemptionStreamOperation::Active(Box::new(original.clone())),
        )));
        state::write(stream);

        let mut newer = original.clone();
        newer.last_external_call_started_at_nanos = 100;
        replace_redemption(&original, newer.clone()).unwrap();

        let mut stale_replacement = original.clone();
        stale_replacement.last_external_call_started_at_nanos = 200;
        assert_eq!(
            replace_redemption(&original, stale_replacement),
            Err(ApiError::Busy)
        );
        assert_eq!(active_redemption().unwrap(), newer);
    }

    #[test]
    fn durable_external_call_admission_bounds_sequential_public_recovery() {
        let (canister, mut stream) = crate::state::tests::valid_state();
        let operation = test_redemption();
        stream.config.retry_delay_nanos = 1_000_000_000;
        stream.config.ledger_deduplication_window_nanos = 2_000_000_000;
        state::initialize(stream, canister).unwrap();
        let mut stream = state::read();
        stream.active_operation = Some(StreamOperation::Redemption(Box::new(
            RedemptionStreamOperation::Active(Box::new(operation.clone())),
        )));
        state::write(stream);

        let admitted = admit_redemption_external_call(operation, 20_900_000_000).unwrap();
        assert_eq!(admitted.last_external_call_started_at_nanos, 20_900_000_000);
        assert_eq!(
            admit_redemption_external_call(admitted.clone(), 21_899_999_999),
            Err(ApiError::Busy)
        );
        assert!(admit_redemption_external_call(admitted, 21_900_000_000).is_ok());
    }

    #[test]
    fn dispatch_admission_keeps_the_exact_fractional_retry_boundary() {
        let mut attempt = test_redemption().icp_payout;
        attempt.state = TransferState::Submitted {
            epoch: DispatchEpoch(1),
            first_submitted_at: 20_900_000_000,
            last_submitted_at: 20_900_000_000,
        };
        assert_eq!(
            next_dispatch_epoch(&attempt, 21_899_999_999, 1_000_000_000),
            Err(ApiError::Busy)
        );
        assert_eq!(
            next_dispatch_epoch(&attempt, 21_900_000_000, 1_000_000_000),
            Ok(DispatchEpoch(2))
        );
        assert_eq!(
            next_dispatch_epoch(&attempt, 21_900_000_001, 1_000_000_000),
            Ok(DispatchEpoch(2))
        );
    }
}
