use candid::{CandidType, Nat, Principal};
use ic_cdk::call::Call;
pub use io_receipt_types::ClaimBackingReceiptProgress;
use serde::Deserialize;
use std::cell::Cell;

use crate::{
    canonical, receipt,
    redemption::{self, RedemptionOperation, RedemptionStage},
    state::{
        self, Account, DispatchEpoch, Lifecycle, OperationSequence, RedemptionScanCursor,
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
    Completed,
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
    pub redemption_scanner_error: Option<String>,
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

pub fn process_redemptions() -> Result<(), ApiError> {
    require_ready(&state::read())?;
    crate::redemption_timer::wake_soon();
    Ok(())
}

pub(crate) async fn run_redemption_worker(
    now: u64,
    wake: crate::redemption_timer::WakeKind,
) -> Result<RedemptionProgress, ApiError> {
    let _guard = RedemptionWorkGuard::acquire()?;
    let snapshot = state::read();
    require_ready(&snapshot)?;
    if matches!(
        snapshot.active_operation,
        Some(StreamOperation::Redemption(_))
    ) {
        return resume_redemption(now).await;
    }
    if snapshot.active_operation.is_some()
        || snapshot.reward_checkpoint.reward_work_due
        || snapshot.stake_observation_due
        || snapshot.structural_reconciliation_due
    {
        return Ok(RedemptionProgress::Pending);
    }
    let page_limit = redemption::MAX_PENDING_CANDIDATES
        .saturating_sub(snapshot.pending_redemption_blocks.len())
        .min(redemption::MAX_INDEX_TRANSACTIONS_PER_PAGE);
    let should_discover = page_limit > 0
        && match wake {
            crate::redemption_timer::WakeKind::NormalPoll => true,
            crate::redemption_timer::WakeKind::NearTerm => {
                snapshot.redemption_scan_cursor.resume_before.is_some()
                    || snapshot.pending_redemption_blocks.is_empty()
            }
        };
    if should_discover {
        let had_candidate = !snapshot.pending_redemption_blocks.is_empty();
        match discover_redemptions(page_limit).await {
            Ok(()) => {}
            Err(ApiError::Pending(_)) if had_candidate => {}
            Err(error) => return Err(error),
        }
    }
    let Some(block) = state::read().pending_redemption_blocks.first().copied() else {
        return Ok(RedemptionProgress::Idle);
    };
    activate_candidate(block, now).await
}

#[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
#[derive(CandidType)]
struct IndexAccountTransactionsArgs {
    account: Account,
    start: Option<Nat>,
    max_results: Nat,
}

#[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
#[derive(CandidType, Deserialize)]
struct IndexTransferHint {
    to: Account,
    amount: Nat,
}

#[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
#[derive(CandidType, Deserialize)]
struct IndexTransactionBodyHint {
    transfer: Option<IndexTransferHint>,
}

#[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
#[derive(CandidType, Deserialize)]
struct IndexTransactionHint {
    id: Nat,
    transaction: IndexTransactionBodyHint,
}

#[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
#[derive(CandidType, Deserialize)]
struct IndexAccountTransactionsPage {
    transactions: Vec<IndexTransactionHint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScannedIndexEntry {
    id: u64,
    candidate: bool,
}

#[cfg(target_family = "wasm")]
async fn read_index_page(
    index: Principal,
    start: Option<u64>,
    staging: &Account,
    minimum: u128,
    page_limit: usize,
) -> Result<Vec<ScannedIndexEntry>, String> {
    let response = Call::bounded_wait(index, "get_account_transactions")
        .with_arg(IndexAccountTransactionsArgs {
            account: redemption_staging_account(),
            start: start.map(Nat::from),
            max_results: Nat::from(page_limit),
        })
        .await
        .map_err(|error| format!("redemption index call failed: {error:?}"))?;
    let (page,) = response
        .candid_tuple::<(Result<IndexAccountTransactionsPage, String>,)>()
        .map_err(|error| format!("redemption index response decode failed: {error:?}"))?;
    page.map_err(|error| format!("redemption index rejected account history: {error}"))?
        .transactions
        .into_iter()
        .map(|transaction| {
            let id = transaction
                .id
                .0
                .try_into()
                .map_err(|_| "redemption index block does not fit u64".to_string())?;
            let candidate = transaction.transaction.transfer.is_some_and(|transfer| {
                transfer.to.effective_eq(staging).unwrap_or(false)
                    && transfer.amount >= Nat::from(minimum)
            });
            Ok(ScannedIndexEntry { id, candidate })
        })
        .collect()
}

#[cfg(not(target_family = "wasm"))]
async fn read_index_page(
    _index: Principal,
    _start: Option<u64>,
    _staging: &Account,
    _minimum: u128,
    _page_limit: usize,
) -> Result<Vec<ScannedIndexEntry>, String> {
    Err("redemption index discovery is only available in canister Wasm".into())
}

fn advance_scan(
    current: &RedemptionScanCursor,
    entries: &[ScannedIndexEntry],
    requested_page_limit: usize,
) -> Result<(RedemptionScanCursor, Vec<u64>), String> {
    let ids = entries.iter().map(|entry| entry.id).collect::<Vec<_>>();
    if requested_page_limit == 0
        || requested_page_limit > redemption::MAX_INDEX_TRANSACTIONS_PER_PAGE
        || ids.len() > requested_page_limit
    {
        return Err("redemption index returned an oversized page".into());
    }
    if ids.windows(2).any(|pair| pair[0] <= pair[1]) {
        return Err("redemption index page is not strictly newest-first".into());
    }
    if let (Some(start), Some(first)) = (current.resume_before, ids.first()) {
        if *first >= start {
            return Err("redemption index did not honor its exclusive cursor".into());
        }
    }
    let committed = current.committed_head;
    let captured = if current.resume_before.is_none() {
        match (ids.first().copied(), committed) {
            (Some(newest), Some(old)) => Some(newest.max(old)),
            (Some(newest), None) => Some(newest),
            (None, old) => old,
        }
    } else {
        current.captured_head
    };
    let mut candidates = entries
        .iter()
        .filter(|entry| entry.candidate && committed.is_none_or(|head| entry.id > head))
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    let reached_committed = committed.is_some_and(|head| ids.iter().any(|id| *id <= head));
    let short_page = ids.len() < requested_page_limit;
    if current.resume_before.is_some() && committed.is_some() && short_page && !reached_committed {
        return Err("redemption index ended before the committed watermark".into());
    }
    let mut next = current.clone();
    next.last_error = None;
    if reached_committed || short_page {
        next.committed_head = captured.or(committed);
        next.captured_head = None;
        next.resume_before = None;
    } else {
        let oldest = ids.last().copied().ok_or("full redemption page is empty")?;
        next.captured_head = captured;
        next.resume_before = Some(oldest);
    }
    Ok((next, candidates))
}

async fn discover_redemptions(page_limit: usize) -> Result<(), ApiError> {
    let expected = state::read();
    if expected.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    let start = expected.redemption_scan_cursor.resume_before;
    let staging = redemption_staging_account();
    let entries = match read_index_page(
        expected.config.io_index,
        start,
        &staging,
        expected.config.minimum_redemption_io_e8s,
        page_limit,
    )
    .await
    {
        Ok(entries) => entries,
        Err(error) => {
            record_scanner_error(&expected, &error);
            return Err(ApiError::Pending(error));
        }
    };
    let (cursor, candidates) = advance_scan(&expected.redemption_scan_cursor, &entries, page_limit)
        .map_err(|error| {
            record_scanner_error(&expected, &error);
            ApiError::Pending("redemption scanner failed closed".into())
        })?;
    let mut latest = state::read();
    if latest != expected {
        return Err(ApiError::Busy);
    }
    let continue_scan = candidates.is_empty() && cursor.resume_before.is_some();
    enqueue_candidates(&mut latest.pending_redemption_blocks, candidates)
        .map_err(ApiError::Pending)?;
    latest.redemption_scan_cursor = cursor;
    state::write(latest);
    if continue_scan {
        crate::redemption_timer::install_near_term();
    }
    Ok(())
}

fn enqueue_candidates(queue: &mut Vec<u64>, candidates: Vec<u64>) -> Result<(), String> {
    let additional = candidates
        .iter()
        .enumerate()
        .filter(|(index, block)| !queue.contains(block) && !candidates[..*index].contains(block))
        .count();
    if queue.len() + additional > redemption::MAX_PENDING_CANDIDATES {
        return Err(
            "redemption queue must drain before every hinted candidate can be represented".into(),
        );
    }
    for block in candidates {
        if !queue.contains(&block) {
            queue.push(block);
        }
    }
    Ok(())
}

fn record_scanner_error(expected: &StreamStateV1, message: &str) {
    let mut latest = state::read();
    if &latest == expected {
        latest.redemption_scan_cursor.last_error =
            Some(message.chars().take(128).collect::<String>());
        state::write(latest);
    }
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

fn remove_candidate(expected: &StreamStateV1, block: u64) -> Result<(), ApiError> {
    let mut latest = state::read();
    if &latest != expected {
        return Err(ApiError::Busy);
    }
    if let Some(position) = latest
        .pending_redemption_blocks
        .iter()
        .position(|candidate| *candidate == block)
    {
        latest.pending_redemption_blocks.remove(position);
    }
    state::write(latest);
    install_after_candidate();
    Ok(())
}

fn defer_candidate(expected: &StreamStateV1, block: u64) -> Result<(), ApiError> {
    let mut latest = state::read();
    if &latest != expected {
        return Err(ApiError::Busy);
    }
    if latest.pending_redemption_blocks.first().copied() != Some(block) {
        return Err(ApiError::Invalid(
            "deferred redemption candidate is not the service head".into(),
        ));
    }
    latest.pending_redemption_blocks.rotate_left(1);
    state::write(latest);
    crate::redemption_timer::install_normal();
    Ok(())
}

fn install_after_candidate() {
    let current = state::read();
    if current.pending_redemption_blocks.is_empty()
        && current.redemption_scan_cursor.resume_before.is_none()
    {
        crate::redemption_timer::install_normal();
    } else {
        crate::redemption_timer::install_near_term();
    }
}

async fn activate_candidate(block: u64, now: u64) -> Result<RedemptionProgress, ApiError> {
    let before = state::read();
    if before.active_operation.is_some() || !before.pending_redemption_blocks.contains(&block) {
        return Err(ApiError::Busy);
    }
    let exact = canonical::exact_icrc_transfer(before.config.io_ledger, u128::from(block))
        .await
        .map_err(ApiError::Ledger)?;
    if !exact
        .to
        .effective_eq(&redemption_staging_account())
        .map_err(ApiError::Invalid)?
        || !candidate_source_allowed(&before.config, &exact.from)?
        || exact.amount_e8s < before.config.minimum_redemption_io_e8s
    {
        remove_candidate(&before, block)?;
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
        defer_candidate(&before, block)?;
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
    let mut payout = TransferAttempt::prepared(OwnTransferIntent::Icrc1 {
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
    payout.state = submitted_state(DispatchEpoch(1), now, now);
    let operation = RedemptionOperation {
        sequence,
        source_io_block: u128::from(block),
        source_account: exact.from,
        staged_io_amount_e8s: exact.amount_e8s,
        gross_icp_e8s: quote.gross_icp,
        net_icp_e8s: quote.net_icp,
        icp_fee_e8s: snapshot.icp_fee_e8s,
        io_sweep_fee_e8s: snapshot.io_fee_e8s,
        stage: RedemptionStage::Payout(payout),
    };
    operation
        .validate(&latest.config)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(redemption_stream(operation.clone()));
    state::write(latest);
    submit_payout(operation).await
}

fn redemption_stream(operation: RedemptionOperation) -> StreamOperation {
    StreamOperation::Redemption(Box::new(RedemptionStreamOperation::Active(Box::new(
        operation,
    ))))
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
            if matches!(active.as_ref(), RedemptionStreamOperation::Active(value) if value.as_ref() == expected)
    ) {
        return Err(ApiError::Busy);
    }
    operation
        .validate(&latest.config)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(redemption_stream(operation));
    state::write(latest);
    Ok(())
}

fn submitted_state(epoch: DispatchEpoch, first: u64, last: u64) -> TransferState {
    TransferState::Submitted {
        epoch,
        first_submitted_at: first,
        last_submitted_at: last,
    }
}

fn retry_operation(
    mut operation: RedemptionOperation,
    now: u64,
) -> Result<RedemptionOperation, ApiError> {
    let expected = operation.clone();
    let retry = state::read().config.retry_delay_nanos;
    let attempt = match &mut operation.stage {
        RedemptionStage::Payout(attempt) | RedemptionStage::Sweep { attempt, .. } => attempt,
    };
    let (epoch, first, last) = match attempt.state {
        TransferState::Submitted {
            epoch,
            first_submitted_at,
            last_submitted_at,
        } => (epoch, first_submitted_at, last_submitted_at),
        TransferState::Stuck { ref reason } => return Err(ApiError::Stuck(reason.clone())),
        _ => {
            return Err(ApiError::Invalid(
                "redemption transfer state is not retryable".into(),
            ))
        }
    };
    if now.saturating_sub(last) < retry {
        return Err(ApiError::Busy);
    }
    let next = epoch
        .0
        .checked_add(1)
        .map(DispatchEpoch)
        .ok_or_else(|| ApiError::Invalid("transfer dispatch epoch overflow".into()))?;
    attempt.state = submitted_state(next, first, now);
    replace_redemption(&expected, operation.clone())?;
    Ok(operation)
}

async fn resume_redemption(now: u64) -> Result<RedemptionProgress, ApiError> {
    let operation = active_redemption()?;
    if operation.is_stuck() {
        return Ok(RedemptionProgress::Stuck(
            "exact transfer proof or reviewed recovery is required".into(),
        ));
    }
    let operation = retry_operation(operation, now)?;
    match operation.stage {
        RedemptionStage::Payout(_) => submit_payout(operation).await,
        RedemptionStage::Sweep { .. } => submit_sweep(operation).await,
    }
}

fn matching_payout(
    operation: &RedemptionOperation,
    sequence: OperationSequence,
    intent: &OwnTransferIntent,
) -> bool {
    operation.sequence == sequence
        && matches!(&operation.stage, RedemptionStage::Payout(attempt) if &attempt.intent == intent)
}

fn matching_sweep(
    operation: &RedemptionOperation,
    sequence: OperationSequence,
    intent: &OwnTransferIntent,
) -> bool {
    operation.sequence == sequence
        && matches!(&operation.stage, RedemptionStage::Sweep { attempt, .. } if &attempt.intent == intent)
}

fn make_sweep(
    mut operation: RedemptionOperation,
    payout_block: u128,
    now: u64,
) -> Result<RedemptionOperation, ApiError> {
    let config = state::read().config;
    let amount = operation
        .staged_io_amount_e8s
        .checked_sub(operation.io_sweep_fee_e8s)
        .filter(|amount| *amount > 0)
        .ok_or_else(|| {
            ApiError::Invalid("staged amount does not cover reserve sweep fee".into())
        })?;
    let mut attempt = TransferAttempt::prepared(OwnTransferIntent::Icrc1 {
        ledger: config.io_ledger,
        from_subaccount: io_accounts::REDEMPTION_STAGING_SUBACCOUNT,
        to: config.io_reserve,
        amount,
        fee: operation.io_sweep_fee_e8s,
        memo: crate::transfer::deterministic_memo(
            b"io-redemption-sweep-v1",
            Principal::from_slice(&operation.source_io_block.to_be_bytes()),
            operation.sequence.0,
        ),
        created_at_time: now,
    })
    .map_err(ApiError::Invalid)?;
    attempt.state = submitted_state(DispatchEpoch(1), now, now);
    operation.stage = RedemptionStage::Sweep {
        payout_block,
        attempt,
    };
    Ok(operation)
}

async fn submit_payout(operation: RedemptionOperation) -> Result<RedemptionProgress, ApiError> {
    let (intent, epoch) = match &operation.stage {
        RedemptionStage::Payout(attempt) => (
            attempt.intent.clone(),
            match attempt.state {
                TransferState::Submitted { epoch, .. } => epoch,
                _ => return Err(ApiError::Invalid("payout is not submitted".into())),
            },
        ),
        _ => {
            return Err(ApiError::Invalid(
                "redemption is not in payout stage".into(),
            ))
        }
    };
    let sequence = operation.sequence;
    match submit(&intent).await {
        Ok(result) => match classify_result(result).map_err(ApiError::Ledger)? {
            ClassifiedResult::Succeeded(block) => {
                let current = active_redemption()?;
                if !matching_payout(&current, sequence, &intent) {
                    return Err(ApiError::Busy);
                }
                let sweep = make_sweep(current.clone(), block, ic_cdk::api::time())?;
                replace_redemption(&current, sweep.clone())?;
                submit_sweep(sweep).await
            }
            ClassifiedResult::NoEffect(_reason) if epoch.0 == 1 => {
                clear_first_payout(&operation)?;
                Ok(RedemptionProgress::Pending)
            }
            ClassifiedResult::NoEffect(reason) => stuck(&operation, reason),
            ClassifiedResult::Ambiguous(reason) => Err(ApiError::Pending(reason)),
        },
        Err(reason) => Err(ApiError::Pending(reason)),
    }
}

fn clear_first_payout(expected: &RedemptionOperation) -> Result<(), ApiError> {
    let mut latest = state::read();
    if latest.active_operation.as_ref() != Some(&redemption_stream(expected.clone())) {
        return Err(ApiError::Pending("stale payout rejection ignored".into()));
    }
    let block = u64::try_from(expected.source_io_block)
        .map_err(|_| ApiError::Invalid("source IO block exceeds u64".into()))?;
    if latest.pending_redemption_blocks.first().copied() != Some(block) {
        return Err(ApiError::Invalid(
            "rejected redemption candidate is not the service head".into(),
        ));
    }
    latest.pending_redemption_blocks.rotate_left(1);
    latest.active_operation = None;
    state::write(latest);
    crate::redemption_timer::install_normal();
    Ok(())
}

async fn submit_sweep(operation: RedemptionOperation) -> Result<RedemptionProgress, ApiError> {
    let intent = match &operation.stage {
        RedemptionStage::Sweep { attempt, .. } => attempt.intent.clone(),
        _ => return Err(ApiError::Invalid("redemption is not in sweep stage".into())),
    };
    let sequence = operation.sequence;
    match submit(&intent).await {
        Ok(result) => match classify_result(result).map_err(ApiError::Ledger)? {
            ClassifiedResult::Succeeded(_) => {
                let current = active_redemption()?;
                if !matching_sweep(&current, sequence, &intent) {
                    return Err(ApiError::Busy);
                }
                complete_redemption(&current)?;
                Ok(RedemptionProgress::Completed)
            }
            ClassifiedResult::NoEffect(reason) => stuck(&operation, reason),
            ClassifiedResult::Ambiguous(reason) => Err(ApiError::Pending(reason)),
        },
        Err(reason) => Err(ApiError::Pending(reason)),
    }
}

fn stuck(expected: &RedemptionOperation, reason: String) -> Result<RedemptionProgress, ApiError> {
    let mut operation = expected.clone();
    let attempt = match &mut operation.stage {
        RedemptionStage::Payout(attempt) | RedemptionStage::Sweep { attempt, .. } => attempt,
    };
    attempt.state = TransferState::Stuck {
        reason: reason.clone(),
    };
    replace_redemption(expected, operation.clone())?;
    pause_if_redemption(&operation);
    Err(ApiError::Stuck(reason))
}

fn complete_redemption(expected: &RedemptionOperation) -> Result<(), ApiError> {
    let mut latest = state::read();
    if latest.active_operation.as_ref() != Some(&redemption_stream(expected.clone())) {
        return Err(ApiError::Busy);
    }
    let block = u64::try_from(expected.source_io_block)
        .map_err(|_| ApiError::Invalid("source IO block exceeds u64".into()))?;
    let position = latest
        .pending_redemption_blocks
        .iter()
        .position(|candidate| *candidate == block)
        .ok_or_else(|| ApiError::Invalid("completed redemption candidate is missing".into()))?;
    latest.pending_redemption_blocks.remove(position);
    latest.active_operation = None;
    state::write(latest);
    install_after_candidate();
    Ok(())
}

pub(crate) fn pause() {
    let mut current = state::read();
    current.lifecycle = Lifecycle::Paused;
    state::write(current);
    crate::redemption_timer::cancel();
    crate::reward_timer::install_retry();
}

fn pause_if_redemption(expected: &RedemptionOperation) {
    let mut latest = state::read();
    if latest.active_operation.as_ref() == Some(&redemption_stream(expected.clone())) {
        latest.lifecycle = Lifecycle::Paused;
        state::write(latest);
        crate::redemption_timer::cancel();
        crate::reward_timer::install_retry();
    }
}

pub async fn resume_stream(now: u64) -> Result<StreamProgress, ApiError> {
    let snapshot = state::read();
    match snapshot.active_operation {
        Some(StreamOperation::Redemption(_)) => {
            let _guard = RedemptionWorkGuard::acquire()?;
            resume_redemption(now).await.map(StreamProgress::Redemption)
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
    match state::read().active_operation {
        Some(StreamOperation::ClaimReceipt(_)) => {
            receipt::prove_recipient(block_index).await?;
            return Ok(());
        }
        Some(StreamOperation::PoolTopUp(_)) => {
            return crate::pool_reconciliation::prove_transfer(block_index).await;
        }
        Some(StreamOperation::Redemption(_)) => {}
        None => return Err(ApiError::Invalid("no active transfer".into())),
    }
    let _guard = RedemptionWorkGuard::acquire()?;
    let operation = active_redemption()?;
    if !operation.is_stuck() {
        return Err(ApiError::Invalid(
            "only a Stuck transfer accepts proof".into(),
        ));
    }
    match operation.stage {
        RedemptionStage::Payout(_) => prove_redemption_payout(operation, block_index).await,
        RedemptionStage::Sweep { .. } => prove_redemption_sweep(operation, block_index).await,
    }
}

async fn prove_redemption_payout(
    operation: RedemptionOperation,
    block_index: u128,
) -> Result<(), ApiError> {
    let intent = operation.active_attempt().intent.clone();
    let exact = canonical::exact_icp_transfer(intent.ledger(), block_index)
        .await
        .map_err(ApiError::Ledger)?;
    if active_redemption()? != operation {
        return Err(ApiError::Busy);
    }
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
    let sweep = make_sweep(operation.clone(), block_index, ic_cdk::api::time())?;
    replace_redemption(&operation, sweep.clone())?;
    submit_sweep(sweep).await.map(|_| ())
}

async fn prove_redemption_sweep(
    operation: RedemptionOperation,
    block_index: u128,
) -> Result<(), ApiError> {
    let intent = operation.active_attempt().intent.clone();
    let exact = canonical::exact_icrc_transfer(intent.ledger(), block_index)
        .await
        .map_err(ApiError::Ledger)?;
    if active_redemption()? != operation {
        return Err(ApiError::Busy);
    }
    validate_icrc_proof(&intent, &exact)?;
    complete_redemption(&operation)
}

fn validate_icrc_proof(
    intent: &OwnTransferIntent,
    exact: &io_ledger_boundary::ExactIcrcTransfer,
) -> Result<(), ApiError> {
    let OwnTransferIntent::Icrc1 {
        from_subaccount,
        to,
        amount,
        fee,
        memo,
        created_at_time,
        ..
    } = intent;
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(ids: impl IntoIterator<Item = u64>) -> Vec<ScannedIndexEntry> {
        ids.into_iter()
            .map(|id| ScannedIndexEntry {
                id,
                candidate: true,
            })
            .collect()
    }

    fn scan(
        cursor: &RedemptionScanCursor,
        entries: &[ScannedIndexEntry],
    ) -> Result<(RedemptionScanCursor, Vec<u64>), String> {
        advance_scan(cursor, entries, redemption::MAX_INDEX_TRANSACTIONS_PER_PAGE)
    }

    #[test]
    fn singleton_head_then_later_deposit_advances_without_direction_inference() {
        let (first, ids) = scan(&RedemptionScanCursor::default(), &entries([8])).unwrap();
        assert_eq!(ids, vec![8]);
        assert_eq!(first.committed_head, Some(8));
        let (second, ids) = scan(&first, &entries([10, 9, 8])).unwrap();
        assert_eq!(ids, vec![9, 10]);
        assert_eq!(second.committed_head, Some(10));
    }

    #[test]
    fn burst_keeps_committed_watermark_until_every_page_is_represented() {
        let cursor = RedemptionScanCursor {
            committed_head: Some(100),
            ..Default::default()
        };
        let head = (110..=141).rev().collect::<Vec<_>>();
        let (middle, ids) = scan(&cursor, &entries(head)).unwrap();
        assert_eq!(ids, (110..=141).collect::<Vec<_>>());
        assert_eq!(middle.committed_head, Some(100));
        assert_eq!(middle.captured_head, Some(141));
        assert_eq!(middle.resume_before, Some(110));
        let (complete, ids) = scan(&middle, &entries((100..=109).rev())).unwrap();
        assert_eq!(ids, (101..=109).collect::<Vec<_>>());
        assert_eq!(complete.committed_head, Some(141));
        assert_eq!(complete.resume_before, None);
    }

    #[test]
    fn multi_page_restart_and_new_head_arrivals_cannot_skip_captured_interval() {
        let original = RedemptionScanCursor {
            committed_head: Some(100),
            ..Default::default()
        };
        let (after_head, first) = scan(&original, &entries((170..=201).rev())).unwrap();
        assert_eq!(first, (170..=201).collect::<Vec<_>>());
        assert_eq!(after_head.committed_head, Some(100));
        assert_eq!(after_head.captured_head, Some(201));
        assert_eq!(after_head.resume_before, Some(170));

        let restored = after_head.clone();
        let (after_middle, second) = scan(&restored, &entries((138..=169).rev())).unwrap();
        assert_eq!(second, (138..=169).collect::<Vec<_>>());
        assert_eq!(after_middle.committed_head, Some(100));
        assert_eq!(after_middle.captured_head, Some(201));
        assert_eq!(after_middle.resume_before, Some(138));

        let (after_third, third) = scan(&after_middle, &entries((106..=137).rev())).unwrap();
        assert_eq!(third, (106..=137).collect::<Vec<_>>());
        assert_eq!(after_third.committed_head, Some(100));
        assert_eq!(after_third.resume_before, Some(106));
        let (complete, fourth) = scan(&after_third, &entries((100..=105).rev())).unwrap();
        assert_eq!(fourth, (101..=105).collect::<Vec<_>>());
        assert_eq!(complete.committed_head, Some(201));
        assert_eq!(complete.captured_head, None);
        assert_eq!(complete.resume_before, None);

        let (next, arrivals) = scan(&complete, &entries([203, 202, 201])).unwrap();
        assert_eq!(arrivals, vec![202, 203]);
        assert_eq!(next.committed_head, Some(203));
    }

    #[test]
    fn scanner_rejects_duplicate_direction_and_exclusive_cursor_violations() {
        assert!(scan(&RedemptionScanCursor::default(), &entries([3, 3])).is_err());
        assert!(scan(&RedemptionScanCursor::default(), &entries([2, 3])).is_err());
        let cursor = RedemptionScanCursor {
            committed_head: Some(1),
            captured_head: Some(10),
            resume_before: Some(8),
            last_error: None,
        };
        assert!(scan(&cursor, &entries([8, 7])).is_err());
    }

    #[test]
    fn capacity_limited_full_page_preserves_captured_interval() {
        let cursor = RedemptionScanCursor {
            committed_head: Some(10),
            ..Default::default()
        };
        let (next, candidates) = advance_scan(&cursor, &entries([15, 14, 13]), 3).unwrap();
        assert_eq!(candidates, vec![13, 14, 15]);
        assert_eq!(next.committed_head, Some(10));
        assert_eq!(next.captured_head, Some(15));
        assert_eq!(next.resume_before, Some(13));

        let (complete, candidates) = advance_scan(&next, &entries([12, 11, 10]), 3).unwrap();
        assert_eq!(candidates, vec![11, 12]);
        assert_eq!(complete.committed_head, Some(15));
        assert_eq!(complete.captured_head, None);
        assert_eq!(complete.resume_before, None);
    }

    #[test]
    fn candidate_queue_appends_unique_discovery_order_without_resorting_service_order() {
        let mut queue = vec![9, 3];
        enqueue_candidates(&mut queue, vec![7, 3, 5, 7]).unwrap();
        assert_eq!(queue, vec![9, 3, 7, 5]);
    }

    #[test]
    fn minimal_transfer_hint_decoder_accepts_the_launch_index_record_width() {
        #[derive(CandidType)]
        struct FullTransfer {
            from: Account,
            to: Account,
            amount: Nat,
            fee: Option<Nat>,
            memo: Option<Vec<u8>>,
            created_at_time: Option<u64>,
        }

        #[derive(CandidType)]
        struct FullTransactionBody {
            kind: String,
            timestamp: u64,
            transfer: Option<FullTransfer>,
        }

        #[derive(CandidType)]
        struct FullTransaction {
            id: Nat,
            transaction: FullTransactionBody,
        }
        #[derive(CandidType)]
        struct FullPage {
            balance: Nat,
            transactions: Vec<FullTransaction>,
            oldest_tx_id: Option<Nat>,
        }
        let bytes = candid::encode_args((Ok::<_, String>(FullPage {
            balance: Nat::from(99_u8),
            transactions: vec![FullTransaction {
                id: Nat::from(44_u8),
                transaction: FullTransactionBody {
                    kind: "transfer".into(),
                    timestamp: 1,
                    transfer: Some(FullTransfer {
                        from: Account {
                            owner: Principal::anonymous(),
                            subaccount: None,
                        },
                        to: Account {
                            owner: Principal::management_canister(),
                            subaccount: Some(vec![1; 32]),
                        },
                        amount: Nat::from(20_000_u64),
                        fee: Some(Nat::from(10_000_u64)),
                        memo: None,
                        created_at_time: Some(1),
                    }),
                },
            }],
            oldest_tx_id: Some(Nat::from(1_u8)),
        }),))
        .unwrap();
        let (decoded,): (Result<IndexAccountTransactionsPage, String>,) =
            candid::decode_args(&bytes).unwrap();
        let transaction = &decoded.unwrap().transactions[0];
        assert_eq!(transaction.id, Nat::from(44_u8));
        assert_eq!(
            transaction
                .transaction
                .transfer
                .as_ref()
                .map(|transfer| transfer.amount.clone()),
            Some(Nat::from(20_000_u64))
        );
    }

    #[test]
    fn scanner_advances_every_id_but_returns_only_transfer_hints() {
        let mut page = (1..=32)
            .rev()
            .map(|id| ScannedIndexEntry {
                id,
                candidate: id == 32,
            })
            .collect::<Vec<_>>();
        let (cursor, candidates) = scan(&RedemptionScanCursor::default(), &page).unwrap();
        assert_eq!(candidates, vec![32]);
        assert_eq!(cursor.captured_head, Some(32));
        assert_eq!(cursor.resume_before, Some(1));

        page = vec![ScannedIndexEntry {
            id: 0,
            candidate: false,
        }];
        let (cursor, candidates) = scan(&cursor, &page).unwrap();
        assert!(candidates.is_empty());
        assert_eq!(cursor.committed_head, Some(32));
        assert_eq!(cursor.resume_before, None);
    }

    #[test]
    fn queue_pressure_rejects_the_whole_filtered_candidate_set_until_it_fits() {
        let mut queue = (0..redemption::MAX_PENDING_CANDIDATES as u64).collect::<Vec<_>>();
        let original = queue.clone();
        assert!(enqueue_candidates(&mut queue, vec![64, 65]).is_err());
        assert_eq!(
            queue, original,
            "queue pressure cannot partially acknowledge candidates"
        );

        queue.drain(..2);
        enqueue_candidates(&mut queue, vec![64, 65]).unwrap();
        assert_eq!(queue.len(), redemption::MAX_PENDING_CANDIDATES);
        assert_eq!(queue.last(), Some(&65));
    }

    #[test]
    fn heap_guard_coalesces_overlapping_timer_resume_and_proof_work() {
        let guard = RedemptionWorkGuard::acquire().unwrap();
        assert!(matches!(
            RedemptionWorkGuard::acquire(),
            Err(ApiError::Busy)
        ));
        drop(guard);
        assert!(RedemptionWorkGuard::acquire().is_ok());
    }
}
