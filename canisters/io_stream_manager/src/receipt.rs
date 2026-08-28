use candid::{CandidType, Principal};
use io_receipt_types::{
    ClaimBackingReceiptKind, ClaimBackingReceiptPermit, ClaimBackingReceiptProgress,
    ClaimBackingReceiptResult, PrepareClaimBackingReceiptArgs, ProveClaimBackingReceiptArgs,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    api::ApiError,
    canonical,
    state::{self, Account, DispatchEpoch, Lifecycle, StreamOperation},
    transfer::{
        classify_result, ClassifiedResult, OwnTransferIntent, TransferAttempt, TransferState,
    },
};

#[cfg(debug_assertions)]
thread_local! {
    static MALFORMED_PREPARE_AFTER_PERSIST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(debug_assertions)]
pub fn debug_fail_malformed_prepare_after_persist(enabled: bool) {
    MALFORMED_PREPARE_AFTER_PERSIST.with(|flag| flag.set(enabled));
}

#[cfg(debug_assertions)]
fn take_malformed_prepare_after_persist() -> bool {
    MALFORMED_PREPARE_AFTER_PERSIST.with(|flag| flag.replace(false))
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct FrozenClaimEconomics {
    pub pre_claim_backing_e8s: u128,
    pub pre_claim_supply_e8s: u128,
    pub liquid_credit_e8s: u128,
    pub io_fee_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct FrozenRecipient {
    pub sns_neuron_id: Option<Vec<u8>>,
    pub destination: Account,
    pub io_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ClaimBackingReceipt {
    pub request: PrepareClaimBackingReceiptArgs,
    pub permit: ClaimBackingReceiptPermit,
    pub economics: FrozenClaimEconomics,
    pub liquid_block: Option<u128>,
    pub recipients: Vec<FrozenRecipient>,
    pub recipient_cursor: u32,
    pub current_recipient: Option<TransferAttempt>,
    pub jupiter_recipient_block: Option<u128>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CompletedClaimBackingReceipt {
    pub request: PrepareClaimBackingReceiptArgs,
    pub stream_operation_sequence: u64,
    pub result: ClaimBackingReceiptResult,
}

impl ClaimBackingReceipt {
    pub fn validate(&self, config: &state::StreamConfig) -> Result<(), String> {
        validate_request(&self.request, config)?;
        if self.permit.stream_operation_sequence == 0
            || self.permit.destination != config.liquid_icp
            || self.permit.amount_e8s != self.request.net_liquid_credit_e8s
            || self.permit.memo
                != io_nns_types::receipt::receipt_memo(self.request.nns_operation_sequence)
            || self.economics.liquid_credit_e8s != self.request.net_liquid_credit_e8s
            || self.economics.io_fee_e8s != config.expected_io_fee_e8s
        {
            return Err("claim receipt identity or economics is malformed".into());
        }
        let cursor =
            usize::try_from(self.recipient_cursor).map_err(|_| "recipient cursor overflow")?;
        if cursor > self.recipients.len()
            || self.liquid_block.is_none()
                && (cursor != 0
                    || self.current_recipient.is_some()
                    || self.jupiter_recipient_block.is_some())
        {
            return Err("claim receipt cursor precedes its liquid proof".into());
        }
        let mut previous_id: Option<&[u8]> = None;
        for recipient in &self.recipients {
            recipient.destination.validate()?;
            if recipient.io_e8s == 0 {
                return Err("claim receipt contains a zero recipient".into());
            }
            if let Some(id) = &recipient.sns_neuron_id {
                if id.len() != 32 || previous_id.is_some_and(|previous| previous >= id.as_slice()) {
                    return Err("claim receipt recipients are malformed or unsorted".into());
                }
                previous_id = Some(id);
            }
        }
        if let Some(transfer) = &self.current_recipient {
            transfer.validate()?;
            let recipient = self
                .recipients
                .get(cursor)
                .ok_or("current recipient is past the cursor")?;
            validate_recipient_transfer(transfer, recipient, self, config)?;
        }
        match &self.request.kind {
            ClaimBackingReceiptKind::Jupiter => {
                if self.recipients.len() != 1
                    || self.recipients[0].sns_neuron_id.is_some()
                    || !self.recipients[0]
                        .destination
                        .effective_eq(&config.jupiter_io_account)?
                {
                    return Err("Jupiter receipt recipient is not canonical".into());
                }
            }
            ClaimBackingReceiptKind::TwoWeek { .. } => {}
        }
        if matches!(self.request.kind, ClaimBackingReceiptKind::TwoWeek { .. })
            && self
                .recipients
                .iter()
                .any(|recipient| recipient.sns_neuron_id.is_none())
        {
            return Err("TwoWeek recipient lacks a neuron identity".into());
        }
        Ok(())
    }
}

impl CompletedClaimBackingReceipt {
    pub fn validate(&self) -> Result<(), String> {
        if self.stream_operation_sequence == 0
            || self.request.nns_operation_sequence == 0
            || self.result.nns_operation_sequence != self.request.nns_operation_sequence
            || self.result.kind != self.request.kind
            || self.result.liquid_credit_e8s == 0
            || self.result.completed_at_nanos == 0
        {
            return Err("completed claim receipt is malformed".into());
        }
        Ok(())
    }

    fn replay_permit(&self, liquid_account: &Account) -> ClaimBackingReceiptPermit {
        ClaimBackingReceiptPermit {
            stream_operation_sequence: self.stream_operation_sequence,
            destination: liquid_account.clone(),
            amount_e8s: self.result.liquid_credit_e8s,
            memo: io_nns_types::receipt::receipt_memo(self.request.nns_operation_sequence),
        }
    }
}

pub async fn prepare(
    caller: Principal,
    request: PrepareClaimBackingReceiptArgs,
) -> Result<ClaimBackingReceiptPermit, ApiError> {
    let initial = state::read();
    if caller != initial.config.nns_manager {
        return Err(ApiError::Unauthorized);
    }
    if initial.lifecycle != Lifecycle::Ready {
        return Err(ApiError::Paused);
    }
    validate_request(&request, &initial.config).map_err(ApiError::Invalid)?;
    if let Some(StreamOperation::ClaimReceipt(active)) = &initial.active_operation {
        return if active.request == request {
            Ok(active.permit.clone())
        } else if active.request.nns_operation_sequence == request.nns_operation_sequence {
            Err(ApiError::Invalid(
                "claim receipt replay conflicts with its request".into(),
            ))
        } else {
            Err(ApiError::Busy)
        };
    }
    if initial.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    if let Some(completed) = &initial.last_completed_claim_receipt {
        if completed.request.nns_operation_sequence == request.nns_operation_sequence {
            return if completed.request == request {
                Ok(completed.replay_permit(&initial.config.liquid_icp))
            } else {
                Err(ApiError::Invalid(
                    "completed claim receipt replay conflicts".into(),
                ))
            };
        }
    }
    let snapshot = canonical::claim_snapshot(&initial.config)
        .await
        .map_err(ApiError::Ledger)?;
    let pre_backing = receipt_pre_backing(&request, snapshot.total_claim_backing_e8s)
        .map_err(ApiError::Invalid)?;
    let (recipients, pending_generation) =
        plan_recipients(&initial, &request, &snapshot, pre_backing)?;
    let sequence = initial.next_operation_sequence.0;
    let permit = ClaimBackingReceiptPermit {
        stream_operation_sequence: sequence,
        destination: initial.config.liquid_icp.clone(),
        amount_e8s: request.net_liquid_credit_e8s,
        memo: io_nns_types::receipt::receipt_memo(request.nns_operation_sequence),
    };
    let operation = ClaimBackingReceipt {
        request,
        permit: permit.clone(),
        economics: FrozenClaimEconomics {
            pre_claim_backing_e8s: pre_backing,
            pre_claim_supply_e8s: snapshot.claim_supply_e8s,
            liquid_credit_e8s: permit.amount_e8s,
            io_fee_e8s: snapshot.io_fee_e8s,
        },
        liquid_block: None,
        recipients,
        recipient_cursor: 0,
        current_recipient: None,
        jupiter_recipient_block: None,
    };
    operation
        .validate(&initial.config)
        .map_err(ApiError::Invalid)?;
    let mut latest = state::read();
    if latest != initial || latest.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    if let Some(generation) = pending_generation {
        if latest
            .pending_entitlement_batch
            .as_ref()
            .map(|batch| batch.generation)
            != Some(generation)
        {
            return Err(ApiError::Busy);
        }
        latest.pending_entitlement_batch = None;
    }
    latest.next_operation_sequence.0 = sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("Stream operation sequence exhausted".into()))?;
    latest.active_operation = Some(StreamOperation::ClaimReceipt(Box::new(operation)));
    state::write(latest);
    #[cfg(debug_assertions)]
    if take_malformed_prepare_after_persist() {
        return Err(ApiError::Pending(
            "controlled malformed prepare response after permit persistence".into(),
        ));
    }
    Ok(permit)
}

fn plan_recipients(
    state: &state::StreamStateV1,
    request: &PrepareClaimBackingReceiptArgs,
    snapshot: &crate::redemption::ClaimSnapshot,
    pre_backing: u128,
) -> Result<(Vec<FrozenRecipient>, Option<u64>), ApiError> {
    let reserve = snapshot
        .reserve_io_e8s
        .checked_sub(snapshot.io_fee_e8s)
        .ok_or_else(|| ApiError::Invalid("IO reserve cannot pay a recipient fee".into()))?;
    match &request.kind {
        ClaimBackingReceiptKind::Jupiter => {
            let amount = io_core_model::backed_io(
                request.net_liquid_credit_e8s,
                pre_backing,
                snapshot.claim_supply_e8s,
            )
            .map_err(|error| ApiError::Invalid(format!("Jupiter backing failed: {error:?}")))?;
            if amount > reserve {
                return Err(ApiError::Invalid(
                    "IO reserve cannot cover Jupiter settlement".into(),
                ));
            }
            Ok((
                vec![FrozenRecipient {
                    sns_neuron_id: None,
                    destination: state.config.jupiter_io_account.clone(),
                    io_e8s: amount,
                }],
                None,
            ))
        }
        ClaimBackingReceiptKind::TwoWeek {
            entitlement_generation,
        } => {
            let batch = state
                .pending_entitlement_batch
                .as_ref()
                .filter(|batch| batch.generation == *entitlement_generation)
                .ok_or_else(|| {
                    ApiError::Invalid("TwoWeek receipt has no matching entitlement batch".into())
                })?;
            let entitlements = batch
                .entries
                .iter()
                .map(|entry| {
                    io_reward_policy::entitlement_credit_from_bytes(
                        entry.sns_neuron_id.clone(),
                        entry.accumulated_eligible_credit,
                    )
                })
                .collect::<Vec<_>>();
            let plan = io_reward_policy::plan_claim_settlement(
                pre_backing,
                snapshot.claim_supply_e8s,
                request.net_liquid_credit_e8s,
                batch.policy_credit_total,
                &entitlements,
                snapshot.reserve_io_e8s,
                snapshot.io_fee_e8s,
            )
            .map_err(|error| ApiError::Invalid(format!("reward settlement failed: {error:?}")))?;
            let mut recipients = Vec::with_capacity(plan.rewards.allocations.len());
            for allocation in plan.rewards.allocations {
                let entry = batch
                    .entries
                    .iter()
                    .find(|entry| entry.sns_neuron_id == allocation.sns_neuron_id)
                    .ok_or_else(|| {
                        ApiError::Invalid("reward allocation lost its destination".into())
                    })?;
                recipients.push(FrozenRecipient {
                    sns_neuron_id: Some(allocation.sns_neuron_id),
                    destination: entry.destination.clone(),
                    io_e8s: allocation.io_e8s,
                });
            }
            Ok((recipients, Some(*entitlement_generation)))
        }
    }
}

fn receipt_pre_backing(
    _request: &PrepareClaimBackingReceiptArgs,
    current_backing_e8s: u128,
) -> Result<u128, String> {
    Ok(current_backing_e8s)
}

pub async fn prove_liquid(
    caller: Principal,
    args: ProveClaimBackingReceiptArgs,
) -> Result<ClaimBackingReceiptProgress, ApiError> {
    let snapshot = state::read();
    if caller != snapshot.config.nns_manager {
        return Err(ApiError::Unauthorized);
    }
    if let Some(completed) = &snapshot.last_completed_claim_receipt {
        if completed.stream_operation_sequence == args.stream_operation_sequence {
            return Ok(ClaimBackingReceiptProgress::Completed(
                completed.result.clone(),
            ));
        }
    }
    let operation = active()?;
    if operation.permit.stream_operation_sequence != args.stream_operation_sequence {
        return Err(ApiError::Invalid(
            "claim receipt sequence does not match".into(),
        ));
    }
    if let Some(block) = operation.liquid_block {
        return if block == args.block_index {
            Ok(progress(&operation))
        } else {
            Err(ApiError::Invalid("conflicting claim receipt block".into()))
        };
    }
    let exact = canonical::exact_icp_transfer(snapshot.config.icp_ledger, args.block_index)
        .await
        .map_err(ApiError::Ledger)?;
    let source = receipt_source_account(&operation.request, snapshot.config.nns_manager);
    if exact.from != canonical::icp_account_identifier(&source).map_err(ApiError::Invalid)?
        || exact.to
            != canonical::icp_account_identifier(&operation.permit.destination)
                .map_err(ApiError::Invalid)?
        || exact.amount_e8s != operation.permit.amount_e8s
        || exact.fee_e8s != snapshot.config.expected_icp_fee_e8s
        || exact.native_memo_u64 != 0
        || exact.icrc1_memo.as_deref() != Some(operation.permit.memo.as_slice())
        || exact.created_at_time == 0
        || exact.spender.is_some()
    {
        return Err(ApiError::Invalid(
            "exact ICP block differs from the claim receipt".into(),
        ));
    }
    let mut proved = operation.clone();
    proved.liquid_block = Some(args.block_index);
    persist(&operation, proved.clone())?;
    Ok(progress(&proved))
}

pub async fn resume(now: u64) -> Result<ClaimBackingReceiptProgress, ApiError> {
    let operation = active()?;
    if operation.liquid_block.is_none() {
        return Ok(ClaimBackingReceiptProgress::AwaitingLiquidProof(
            operation.permit,
        ));
    }
    let cursor = usize::try_from(operation.recipient_cursor)
        .map_err(|_| ApiError::Invalid("recipient cursor overflow".into()))?;
    if cursor == operation.recipients.len() {
        return complete(operation, now);
    }
    match operation
        .current_recipient
        .as_ref()
        .map(|attempt| attempt.state.clone())
    {
        None => submit_recipient(operation, now, DispatchEpoch(1), now).await,
        Some(TransferState::Submitted {
            epoch,
            first_submitted_at,
            last_submitted_at,
        }) => {
            if now.saturating_sub(last_submitted_at) < state::read().config.retry_delay_nanos {
                return Ok(ClaimBackingReceiptProgress::Pending);
            }
            let deadline = first_submitted_at
                .checked_add(state::read().config.ledger_deduplication_window_nanos)
                .ok_or_else(|| {
                    ApiError::Invalid("recipient deduplication deadline overflow".into())
                })?;
            if now >= deadline {
                return mark_stuck(operation, "recipient transfer proof window expired".into());
            }
            let next =
                DispatchEpoch(epoch.0.checked_add(1).ok_or_else(|| {
                    ApiError::Invalid("recipient dispatch epoch exhausted".into())
                })?);
            submit_recipient(operation, now, next, first_submitted_at).await
        }
        Some(TransferState::Succeeded { block }) => advance_recipient(operation, block),
        Some(TransferState::Stuck { reason }) => Err(ApiError::Stuck(reason)),
        Some(TransferState::Prepared) => {
            submit_recipient(operation, now, DispatchEpoch(1), now).await
        }
    }
}

async fn submit_recipient(
    operation: ClaimBackingReceipt,
    now: u64,
    epoch: DispatchEpoch,
    first_submitted_at: u64,
) -> Result<ClaimBackingReceiptProgress, ApiError> {
    let cursor = usize::try_from(operation.recipient_cursor)
        .map_err(|_| ApiError::Invalid("recipient cursor overflow".into()))?;
    let recipient = operation
        .recipients
        .get(cursor)
        .ok_or_else(|| ApiError::Invalid("recipient cursor is complete".into()))?;
    let mut submitted = operation.clone();
    let attempt = submitted.current_recipient.get_or_insert(
        TransferAttempt::prepared(recipient_intent(&operation, recipient, now)?)
            .map_err(ApiError::Invalid)?,
    );
    attempt.state = TransferState::Submitted {
        epoch,
        first_submitted_at,
        last_submitted_at: now,
    };
    let intent = attempt.intent.clone();
    persist(&operation, submitted.clone())?;
    let response = crate::api::submit(&intent).await;
    apply_callback(submitted, intent, epoch, response)
}

fn apply_callback(
    submitted: ClaimBackingReceipt,
    intent: OwnTransferIntent,
    epoch: DispatchEpoch,
    response: Result<crate::transfer::TransferResult, String>,
) -> Result<ClaimBackingReceiptProgress, ApiError> {
    let current = active()?;
    if current.request != submitted.request
        || current.permit.stream_operation_sequence != submitted.permit.stream_operation_sequence
    {
        return Err(ApiError::Busy);
    }
    let attempt = current.current_recipient.as_ref().ok_or(ApiError::Busy)?;
    if attempt.intent != intent
        || !matches!(attempt.state, TransferState::Submitted { epoch: value, .. } if value == epoch)
    {
        return Err(ApiError::Busy);
    }
    let classified = match response {
        Ok(result) => classify_result(result).map_err(ApiError::Ledger)?,
        Err(error) => return Err(ApiError::Pending(error)),
    };
    let mut replacement = current.clone();
    let attempt = replacement
        .current_recipient
        .as_mut()
        .expect("validated current recipient");
    match classified {
        ClassifiedResult::Succeeded(block) => attempt.state = TransferState::Succeeded { block },
        ClassifiedResult::Ambiguous(reason) => return Err(ApiError::Pending(reason)),
        ClassifiedResult::NoEffect(reason) => {
            attempt.state = TransferState::Stuck {
                reason: reason.clone(),
            };
            persist(&current, replacement)?;
            crate::api::pause();
            return Err(ApiError::Stuck(reason));
        }
    }
    persist(&current, replacement)?;
    Ok(ClaimBackingReceiptProgress::Pending)
}

fn advance_recipient(
    operation: ClaimBackingReceipt,
    block: u128,
) -> Result<ClaimBackingReceiptProgress, ApiError> {
    let mut replacement = operation.clone();
    if matches!(replacement.request.kind, ClaimBackingReceiptKind::Jupiter) {
        replacement.jupiter_recipient_block = Some(block);
    }
    replacement.recipient_cursor = replacement
        .recipient_cursor
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("recipient cursor exhausted".into()))?;
    replacement.current_recipient = None;
    persist(&operation, replacement)?;
    Ok(ClaimBackingReceiptProgress::Pending)
}

pub async fn prove_recipient(block_index: u128) -> Result<ClaimBackingReceiptProgress, ApiError> {
    let operation = active()?;
    let attempt = operation
        .current_recipient
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("no current recipient transfer".into()))?;
    if !matches!(
        attempt.state,
        TransferState::Submitted { .. } | TransferState::Stuck { .. }
    ) {
        return Err(ApiError::Invalid(
            "recipient transfer is not proof-recoverable".into(),
        ));
    }
    let exact = canonical::exact_icrc_transfer(attempt.intent.ledger(), block_index)
        .await
        .map_err(ApiError::Ledger)?;
    let OwnTransferIntent::Icrc1 {
        from_subaccount,
        to,
        amount,
        fee,
        memo,
        created_at_time,
        ..
    } = &attempt.intent
    else {
        return Err(ApiError::Invalid("recipient intent is not ICRC-1".into()));
    };
    let source = Account {
        owner: ic_cdk::api::canister_self(),
        subaccount: (*from_subaccount != [0; 32]).then(|| from_subaccount.to_vec()),
    };
    if !exact
        .matches(&io_ledger_boundary::ExpectedIcrcTransfer {
            from: &source,
            to,
            amount_e8s: *amount,
            fee_e8s: Some(*fee),
            memo: Some(memo),
            created_at_time: Some(*created_at_time),
            spender: None,
        })
        .map_err(ApiError::Invalid)?
    {
        return Err(ApiError::Invalid(
            "exact block differs from the recipient intent".into(),
        ));
    }
    let mut replacement = operation.clone();
    replacement
        .current_recipient
        .as_mut()
        .expect("validated recipient")
        .state = TransferState::Succeeded { block: block_index };
    persist(&operation, replacement)?;
    Ok(ClaimBackingReceiptProgress::Pending)
}

fn complete(
    operation: ClaimBackingReceipt,
    now: u64,
) -> Result<ClaimBackingReceiptProgress, ApiError> {
    let distributed = operation
        .recipients
        .iter()
        .try_fold(0u128, |sum, recipient| sum.checked_add(recipient.io_e8s))
        .ok_or_else(|| ApiError::Invalid("receipt distribution overflow".into()))?;
    let result = ClaimBackingReceiptResult {
        nns_operation_sequence: operation.request.nns_operation_sequence,
        kind: operation.request.kind.clone(),
        liquid_credit_e8s: operation.request.net_liquid_credit_e8s,
        distributed_io_e8s: distributed,
        recipient_transfer_block: operation.jupiter_recipient_block,
        io_fee_e8s: operation.economics.io_fee_e8s,
        completed_at_nanos: now,
    };
    let completed = CompletedClaimBackingReceipt {
        request: operation.request.clone(),
        stream_operation_sequence: operation.permit.stream_operation_sequence,
        result: result.clone(),
    };
    completed.validate().map_err(ApiError::Invalid)?;
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(StreamOperation::ClaimReceipt(active)) if **active == operation)
    {
        return Err(ApiError::Busy);
    }
    latest.active_operation = None;
    latest.last_completed_claim_receipt = Some(completed);
    latest.stake_observation_due = true;
    state::write(latest);
    Ok(ClaimBackingReceiptProgress::Completed(result))
}

fn mark_stuck(
    operation: ClaimBackingReceipt,
    reason: String,
) -> Result<ClaimBackingReceiptProgress, ApiError> {
    let mut replacement = operation.clone();
    replacement
        .current_recipient
        .as_mut()
        .ok_or_else(|| ApiError::Invalid("missing recipient".into()))?
        .state = TransferState::Stuck {
        reason: reason.clone(),
    };
    persist(&operation, replacement)?;
    crate::api::pause();
    Err(ApiError::Stuck(reason))
}

fn recipient_intent(
    operation: &ClaimBackingReceipt,
    recipient: &FrozenRecipient,
    now: u64,
) -> Result<OwnTransferIntent, ApiError> {
    let config = state::read().config;
    let mut hasher = Sha256::new();
    hasher.update(b"io-claim-receipt-recipient-v1");
    hasher.update(operation.request.nns_operation_sequence.to_be_bytes());
    hasher.update(operation.recipient_cursor.to_be_bytes());
    Ok(OwnTransferIntent::Icrc1 {
        ledger: config.io_ledger,
        from_subaccount: config
            .io_reserve
            .canonical()
            .map_err(ApiError::Invalid)?
            .subaccount,
        to: recipient.destination.clone(),
        amount: recipient.io_e8s,
        fee: operation.economics.io_fee_e8s,
        memo: hasher.finalize().to_vec(),
        created_at_time: now,
    })
}

fn validate_recipient_transfer(
    transfer: &TransferAttempt,
    recipient: &FrozenRecipient,
    operation: &ClaimBackingReceipt,
    config: &state::StreamConfig,
) -> Result<(), String> {
    match &transfer.intent {
        OwnTransferIntent::Icrc1 {
            ledger,
            from_subaccount,
            to,
            amount,
            fee,
            ..
        } if *ledger == config.io_ledger
            && *from_subaccount == config.io_reserve.canonical()?.subaccount
            && to.effective_eq(&recipient.destination)?
            && *amount == recipient.io_e8s
            && *fee == operation.economics.io_fee_e8s =>
        {
            Ok(())
        }
        _ => Err("current recipient transfer differs from the frozen receipt".into()),
    }
}

fn validate_request(
    request: &PrepareClaimBackingReceiptArgs,
    _config: &state::StreamConfig,
) -> Result<(), String> {
    if request.nns_operation_sequence == 0 || request.net_liquid_credit_e8s == 0 {
        return Err("claim receipt request is malformed".into());
    }
    match &request.kind {
        ClaimBackingReceiptKind::Jupiter => {}
        ClaimBackingReceiptKind::TwoWeek {
            entitlement_generation,
        } if *entitlement_generation == 0 => {
            return Err("two-week receipt generation is zero".into())
        }
        ClaimBackingReceiptKind::TwoWeek { .. } => {}
    }
    Ok(())
}

fn receipt_source_account(
    request: &PrepareClaimBackingReceiptArgs,
    nns_manager: Principal,
) -> Account {
    match request.kind {
        ClaimBackingReceiptKind::Jupiter => Account {
            owner: nns_manager,
            subaccount: None,
        },
        ClaimBackingReceiptKind::TwoWeek { .. } => {
            io_accounts::two_week_maturity_staging(nns_manager)
        }
    }
}

fn progress(operation: &ClaimBackingReceipt) -> ClaimBackingReceiptProgress {
    if operation.liquid_block.is_none() {
        ClaimBackingReceiptProgress::AwaitingLiquidProof(operation.permit.clone())
    } else {
        ClaimBackingReceiptProgress::Pending
    }
}

fn active() -> Result<ClaimBackingReceipt, ApiError> {
    match state::read().active_operation {
        Some(StreamOperation::ClaimReceipt(operation)) => Ok(*operation),
        Some(_) => Err(ApiError::Busy),
        None => Err(ApiError::Invalid("no active claim receipt".into())),
    }
}

fn persist(
    expected: &ClaimBackingReceipt,
    replacement: ClaimBackingReceipt,
) -> Result<(), ApiError> {
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(StreamOperation::ClaimReceipt(active)) if **active == *expected)
    {
        return Err(ApiError::Busy);
    }
    replacement
        .validate(&latest.config)
        .map_err(ApiError::Invalid)?;
    latest.active_operation = Some(StreamOperation::ClaimReceipt(Box::new(replacement)));
    state::write(latest);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_completion_reconstructs_the_exact_replay_permit() {
        let liquid = Account {
            owner: Principal::from_slice(&[1; 29]),
            subaccount: Some(vec![2; 32]),
        };
        let request = PrepareClaimBackingReceiptArgs {
            nns_operation_sequence: 3,
            kind: ClaimBackingReceiptKind::TwoWeek {
                entitlement_generation: 9,
            },
            net_liquid_credit_e8s: 100,
        };
        let permit = ClaimBackingReceiptPermit {
            stream_operation_sequence: 7,
            destination: liquid.clone(),
            amount_e8s: 100,
            memo: io_nns_types::receipt::receipt_memo(3),
        };
        let completed = CompletedClaimBackingReceipt {
            request,
            stream_operation_sequence: permit.stream_operation_sequence,
            result: ClaimBackingReceiptResult {
                nns_operation_sequence: 3,
                kind: ClaimBackingReceiptKind::TwoWeek {
                    entitlement_generation: 9,
                },
                liquid_credit_e8s: 100,
                distributed_io_e8s: 80,
                recipient_transfer_block: None,
                io_fee_e8s: 10,
                completed_at_nanos: 11,
            },
        };
        completed.validate().unwrap();
        assert_eq!(completed.replay_permit(&liquid), permit);
        assert!(candid::encode_one(completed).unwrap().len() < 512);
    }

    #[test]
    fn paired_ingress_preserves_pre_event_rate_until_permit() {
        let request = |kind| PrepareClaimBackingReceiptArgs {
            nns_operation_sequence: 1,
            kind,
            net_liquid_credit_e8s: 60,
        };
        assert_eq!(
            receipt_pre_backing(&request(ClaimBackingReceiptKind::Jupiter), 100),
            Ok(100)
        );
        assert_eq!(
            receipt_pre_backing(
                &request(ClaimBackingReceiptKind::TwoWeek {
                    entitlement_generation: 1,
                }),
                100,
            ),
            Ok(100)
        );
        let quote = io_core_model::redemption_quote(
            io_core_model::EconomicState {
                backing: io_core_model::Backing {
                    liquid: 100,
                    pooled: 0,
                    unwinding: 0,
                    transit: 0,
                },
                claims: 100,
                active_backing: 0,
                active_reward: 0,
            },
            10,
            0,
            0,
        )
        .unwrap();
        assert_eq!(quote.gross_icp, 10, "paired credit must not front-run IO");
    }

    #[test]
    fn jupiter_and_two_week_share_rate_one_and_rate_two_backed_issuance() {
        let (_, mut state) = crate::state::tests::valid_state();
        state.pending_entitlement_batch = Some(crate::state::PendingEntitlementBatch {
            generation: 1,
            frozen_at_timestamp_seconds: 1,
            through_event: crate::state::RewardEventId {
                end_timestamp_seconds: 1,
                round: 1,
            },
            target_icp_e8s: 100,
            entries: vec![crate::state::FrozenEntitlement {
                sns_neuron_id: vec![1; 32],
                destination: Account {
                    owner: Principal::from_slice(&[9; 29]),
                    subaccount: None,
                },
                accumulated_eligible_credit: 1,
            }],
            eligible_credit_total: 1,
            policy_credit_total: 1,
            processed_event_count: 1,
        });
        for (backing, expected_io) in [(100, 60), (200, 30)] {
            let snapshot = crate::redemption::ClaimSnapshot {
                total_supply_e8s: 1_100,
                reserve_io_e8s: 1_000,
                claim_supply_e8s: 100,
                total_claim_backing_e8s: backing,
                io_fee_e8s: 10,
                ..Default::default()
            };
            for kind in [
                ClaimBackingReceiptKind::Jupiter,
                ClaimBackingReceiptKind::TwoWeek {
                    entitlement_generation: 1,
                },
            ] {
                let request = PrepareClaimBackingReceiptArgs {
                    nns_operation_sequence: 1,
                    kind,
                    net_liquid_credit_e8s: 60,
                };
                let (recipients, _) =
                    plan_recipients(&state, &request, &snapshot, backing).unwrap();
                assert_eq!(
                    recipients.iter().map(|entry| entry.io_e8s).sum::<u128>(),
                    expected_io
                );
                assert_eq!(
                    recipients[0].sns_neuron_id.is_some(),
                    matches!(request.kind, ClaimBackingReceiptKind::TwoWeek { .. })
                );
            }
        }
    }

    #[test]
    fn oversized_two_week_donation_waits_without_partial_issuance() {
        let (_, mut state) = crate::state::tests::valid_state();
        state.pending_entitlement_batch = Some(crate::state::PendingEntitlementBatch {
            generation: 1,
            frozen_at_timestamp_seconds: 1,
            through_event: crate::state::RewardEventId {
                end_timestamp_seconds: 1,
                round: 1,
            },
            target_icp_e8s: 100,
            entries: vec![crate::state::FrozenEntitlement {
                sns_neuron_id: vec![1; 32],
                destination: Account {
                    owner: Principal::from_slice(&[9; 29]),
                    subaccount: None,
                },
                accumulated_eligible_credit: 1,
            }],
            eligible_credit_total: 1,
            policy_credit_total: 1,
            processed_event_count: 1,
        });
        let original = state.clone();
        let request = PrepareClaimBackingReceiptArgs {
            nns_operation_sequence: 1,
            kind: ClaimBackingReceiptKind::TwoWeek {
                entitlement_generation: 1,
            },
            net_liquid_credit_e8s: 1_000,
        };
        let snapshot = crate::redemption::ClaimSnapshot {
            total_supply_e8s: 200,
            reserve_io_e8s: 100,
            claim_supply_e8s: 100,
            total_claim_backing_e8s: 100,
            io_fee_e8s: 10,
            ..Default::default()
        };
        assert_eq!(
            io_core_model::backed_io(
                request.net_liquid_credit_e8s,
                snapshot.total_claim_backing_e8s,
                snapshot.claim_supply_e8s,
            ),
            Ok(1_000)
        );
        let error = plan_recipients(
            &state,
            &request,
            &snapshot,
            snapshot.total_claim_backing_e8s,
        )
        .unwrap_err();
        assert!(
            matches!(error, ApiError::Invalid(message) if message.contains("InsufficientIoReserve"))
        );
        assert_eq!(
            state, original,
            "failed planning must consume no entitlement or reserve state"
        );
    }
}
