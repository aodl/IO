use candid::{CandidType, Principal};
use io_nns_types::inflow::{
    effect_memo, BackingEffect, BackingInflowKind, BackingInflowPermit, BackingInflowProgress,
    FrozenInflowEconomics, FrozenRewardRecipient, PrepareBackingInflowArgs, ProveBackingEffectArgs,
};
use io_reward_policy::{ClaimRoute, EntitlementCredit, TwoWeekSettlementInput};
use serde::Deserialize;

use crate::{
    api::ApiError,
    canonical,
    state::{self, DispatchEpoch, Lifecycle, StreamOperation},
    transfer::{
        classify_result, ClassifiedResult, OwnTransferIntent, TransferAttempt, TransferState,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum BackingInflowPhase {
    AwaitingNnsEffects,
    AwaitingPooledTransfer,
    PooledTransferSubmitted,
    SettlingRewards,
    Stuck,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct InflowRecipient {
    pub frozen: FrozenRewardRecipient,
    pub transfer: Option<TransferAttempt>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct BackingInflowOperation {
    pub permit: BackingInflowPermit,
    pub phase: BackingInflowPhase,
    pub permanent_block: Option<u128>,
    pub first_claim_block: Option<u128>,
    pub pooled_transfer: Option<TransferAttempt>,
    pub recipients: Vec<InflowRecipient>,
    pub recipient_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct LastCompletedBackingInflow {
    pub permit: BackingInflowPermit,
    pub distributed_io_e8s: u128,
}

impl BackingInflowOperation {
    pub fn validate(&self) -> Result<(), String> {
        self.permit.validate()?;
        if usize::try_from(self.recipient_index).map_err(|_| "recipient index overflow")?
            > self.recipients.len()
        {
            return Err("backing-inflow recipient index exceeds its frozen list".into());
        }
        let expected = match &self.permit.economics {
            FrozenInflowEconomics::Permanent { .. } => Vec::new(),
            FrozenInflowEconomics::Pooled { recipients, .. } => recipients.clone(),
        };
        if self
            .recipients
            .iter()
            .map(|value| &value.frozen)
            .ne(expected.iter())
        {
            return Err("backing-inflow recipients differ from the frozen plan".into());
        }
        for recipient in &self.recipients {
            if let Some(transfer) = &recipient.transfer {
                transfer.validate()?;
            }
        }
        if let Some(transfer) = &self.pooled_transfer {
            transfer.validate()?;
        }
        let permanent_ready = self.permit.permanent_credit() == 0 || self.permanent_block.is_some();
        let claim_ready = self.first_claim_block.is_some();
        let pooled_succeeded = self
            .pooled_transfer
            .as_ref()
            .is_some_and(|transfer| matches!(transfer.state, TransferState::Succeeded { .. }));
        let route = self.permit.route().route;
        let phase_valid = match self.phase {
            BackingInflowPhase::AwaitingNnsEffects => !permanent_ready || !claim_ready,
            BackingInflowPhase::AwaitingPooledTransfer => {
                permanent_ready
                    && claim_ready
                    && route == ClaimRoute::Mixed
                    && self.pooled_transfer.is_none()
            }
            BackingInflowPhase::PooledTransferSubmitted => {
                permanent_ready
                    && claim_ready
                    && route == ClaimRoute::Mixed
                    && self.pooled_transfer.is_some()
            }
            BackingInflowPhase::SettlingRewards => {
                permanent_ready && claim_ready && (route != ClaimRoute::Mixed || pooled_succeeded)
            }
            BackingInflowPhase::Stuck => true,
        };
        if !phase_valid {
            return Err("backing-inflow phase is not supported by exact effect proofs".into());
        }
        for (index, recipient) in self.recipients.iter().enumerate() {
            let succeeded = recipient
                .transfer
                .as_ref()
                .is_some_and(|transfer| matches!(transfer.state, TransferState::Succeeded { .. }));
            if index < self.recipient_index as usize && !succeeded
                || index > self.recipient_index as usize && recipient.transfer.is_some()
            {
                return Err("reward settlement cursor contradicts transfer evidence".into());
            }
        }
        Ok(())
    }
}

impl LastCompletedBackingInflow {
    pub fn validate(&self) -> Result<(), String> {
        self.permit.validate()?;
        let planned = match &self.permit.economics {
            FrozenInflowEconomics::Permanent { .. } => 0,
            FrozenInflowEconomics::Pooled { settlement, .. } => settlement.distributed_io,
        };
        if self.distributed_io_e8s == planned {
            Ok(())
        } else {
            Err("completed backing inflow differs from its frozen settlement".into())
        }
    }
}

pub async fn prepare(
    caller: Principal,
    args: PrepareBackingInflowArgs,
) -> Result<BackingInflowPermit, ApiError> {
    let initial = state::read();
    if caller != initial.config.nns_manager {
        return Err(ApiError::Unauthorized);
    }
    if initial.lifecycle != Lifecycle::Ready {
        return Err(ApiError::Paused);
    }
    validate_args(&args, &initial)?;
    if let Some(StreamOperation::BackingInflow(active)) = &initial.active_operation {
        return if request_matches(&active.permit, &args) {
            Ok(active.permit.clone())
        } else if active.permit.source_operation_id == args.source_operation_id {
            Err(ApiError::Invalid(
                "backing-inflow replay conflicts with its frozen request".into(),
            ))
        } else {
            Err(ApiError::Busy)
        };
    }
    if initial.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    if let Some(completed) = &initial.last_completed_backing_inflow {
        if completed.permit.source_operation_id == args.source_operation_id {
            return if request_matches(&completed.permit, &args) {
                Ok(completed.permit.clone())
            } else {
                Err(ApiError::Invalid(
                    "completed backing-inflow replay conflicts with its frozen request".into(),
                ))
            };
        }
    }
    let snapshot = canonical::redemption_snapshot(&initial.config)
        .await
        .map_err(ApiError::Ledger)?;
    if snapshot.nns_fingerprint != args.nns_fingerprint {
        return Err(ApiError::Pending(
            "NNS observation changed before backing-inflow commitment".into(),
        ));
    }
    let permit = plan(&initial, &args, &snapshot)?;
    let recipients = match &permit.economics {
        FrozenInflowEconomics::Permanent { .. } => Vec::new(),
        FrozenInflowEconomics::Pooled { recipients, .. } => recipients
            .iter()
            .cloned()
            .map(|frozen| InflowRecipient {
                frozen,
                transfer: None,
            })
            .collect(),
    };
    let operation = BackingInflowOperation {
        permit: permit.clone(),
        phase: BackingInflowPhase::AwaitingNnsEffects,
        permanent_block: None,
        first_claim_block: None,
        pooled_transfer: None,
        recipients,
        recipient_index: 0,
    };
    operation.validate().map_err(ApiError::Invalid)?;
    let mut latest = state::read();
    if latest != initial || latest.active_operation.is_some() {
        return Err(ApiError::Busy);
    }
    latest.next_operation_sequence.0 = permit
        .stream_operation_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("Stream operation sequence overflow".into()))?;
    latest.active_operation = Some(StreamOperation::BackingInflow(Box::new(operation)));
    state::write(latest);
    Ok(permit)
}

fn validate_args(
    args: &PrepareBackingInflowArgs,
    state: &crate::state::StreamStateV1,
) -> Result<(), ApiError> {
    args.staging_account.validate().map_err(ApiError::Invalid)?;
    if args.source_operation_id.is_empty()
        || args.source_operation_id.len() > 64
        || args.actual_mint_e8s == 0
        || args.maturity_generation == 0
        || args.staging_account.owner != state.config.nns_manager
        || args.permanent_transfer_fee_e8s != state.config.expected_icp_fee_e8s
        || args.claim_transfer_fee_e8s != state.config.expected_icp_fee_e8s
        || args.nns_fingerprint.len() != 32
    {
        return Err(ApiError::Invalid(
            "backing-inflow request is malformed".into(),
        ));
    }
    Ok(())
}

fn plan(
    stream: &crate::state::StreamStateV1,
    args: &PrepareBackingInflowArgs,
    snapshot: &crate::redemption::CanonicalRedemptionSnapshot,
) -> Result<BackingInflowPermit, ApiError> {
    let excluded = snapshot
        .excluded_io_balances
        .iter()
        .try_fold(0u128, |sum, (_, balance)| sum.checked_add(*balance))
        .ok_or_else(|| ApiError::Invalid("nonredeemable IO overflow".into()))?;
    let claims = io_core_model::claim_supply(
        snapshot.total_supply_e8s,
        snapshot.reserve_io_e8s,
        &[excluded],
    )
    .map_err(|error| ApiError::Invalid(format!("claim supply failed: {error:?}")))?;
    let transit_increment = match args.kind {
        BackingInflowKind::PermanentMaturity => args.actual_mint_e8s,
        BackingInflowKind::PooledMaturity => {
            io_core_model::split_40_60(args.actual_mint_e8s)
                .map_err(|error| ApiError::Invalid(format!("maturity split failed: {error:?}")))?
                .claim
        }
    };
    let pre_transit = snapshot
        .transit_backing_e8s
        .checked_sub(transit_increment)
        .ok_or_else(|| ApiError::Invalid("NNS transit backing omits the proved Mint".into()))?;
    let economic = io_core_model::EconomicState {
        backing: io_core_model::Backing {
            liquid: snapshot.liquid_icp_e8s,
            pooled: snapshot.pooled_principal_e8s,
            unwinding: snapshot.unwinding_principal_e8s,
            transit: pre_transit,
        },
        claims,
        active_backing: snapshot.active_backing_io_e8s,
        active_reward: snapshot.active_reward_io_e8s,
    };
    let economics = match args.kind {
        BackingInflowKind::PermanentMaturity => FrozenInflowEconomics::Permanent {
            route: io_reward_policy::plan_permanent_maturity(
                economic,
                args.actual_mint_e8s,
                args.claim_transfer_fee_e8s,
                snapshot.pooled_parent_exists,
                snapshot.minimum_parent_stake_e8s,
            )
            .map_err(|error| {
                ApiError::Invalid(format!("permanent inflow plan failed: {error:?}"))
            })?,
        },
        BackingInflowKind::PooledMaturity => {
            let batch = stream
                .pending_entitlement_batch
                .as_ref()
                .filter(|batch| batch.generation == args.maturity_generation)
                .ok_or_else(|| {
                    ApiError::Invalid("pooled Mint lacks its frozen entitlement batch".into())
                })?;
            let entitlements = batch
                .entries
                .iter()
                .map(|entry| EntitlementCredit {
                    sns_neuron_id: entry.sns_neuron_id.clone(),
                    accumulated_eligible_credit: entry.accumulated_eligible_credit,
                })
                .collect::<Vec<_>>();
            let eligible = stream
                .backing_registry
                .iter()
                .filter(|record| {
                    matches!(
                        record.status,
                        crate::state::BackingRewardStatus::ActiveEligible { .. }
                    )
                })
                .map(|record| record.sns_neuron_id.clone())
                .collect::<Vec<_>>();
            let fingerprint: [u8; 32] = snapshot
                .observation_fingerprint
                .as_slice()
                .try_into()
                .map_err(|_| ApiError::Invalid("snapshot fingerprint is malformed".into()))?;
            let settlement = io_reward_policy::plan_two_week_settlement(TwoWeekSettlementInput {
                state: economic,
                actual_mint: args.actual_mint_e8s,
                permanent_transfer_fee: args.permanent_transfer_fee_e8s,
                claim_transfer_fee: args.claim_transfer_fee_e8s,
                parent_exists: snapshot.pooled_parent_exists,
                minimum_parent_credit: snapshot.minimum_parent_stake_e8s,
                policy_credit_total: batch.policy_credit_total,
                entitlements: &entitlements,
                reward_eligible_ids: &eligible,
                reserve_io_capacity: snapshot.reserve_io_e8s,
                io_fee: snapshot.io_fee_e8s,
                snapshot_fingerprint: fingerprint,
            })
            .map_err(|error| ApiError::Invalid(format!("pooled inflow plan failed: {error:?}")))?;
            let recipients = settlement
                .rewards
                .allocations
                .iter()
                .map(|allocation| {
                    let entry = batch
                        .entries
                        .binary_search_by(|entry| {
                            entry.sns_neuron_id.cmp(&allocation.sns_neuron_id)
                        })
                        .ok()
                        .map(|index| &batch.entries[index])
                        .ok_or_else(|| {
                            ApiError::Invalid("reward allocation lost its destination".into())
                        })?;
                    Ok(FrozenRewardRecipient {
                        sns_neuron_id: allocation.sns_neuron_id.clone(),
                        destination: entry.destination.clone(),
                        io_e8s: allocation.io_e8s,
                    })
                })
                .collect::<Result<Vec<_>, ApiError>>()?;
            FrozenInflowEconomics::Pooled {
                settlement: Box::new(settlement),
                recipients,
            }
        }
    };
    let permit = BackingInflowPermit {
        stream_operation_sequence: stream.next_operation_sequence.0,
        source_operation_id: args.source_operation_id.clone(),
        actual_mint_e8s: args.actual_mint_e8s,
        maturity_generation: args.maturity_generation,
        staging_account: args.staging_account.clone(),
        mint_block: args.mint_block,
        permanent_destination: snapshot.permanent_staking_account.clone(),
        pool_destination: snapshot.pool_staking_account.clone(),
        expected_parent_before_e8s: snapshot.pooled_principal_e8s,
        liquid_destination: stream.config.liquid_icp.clone(),
        permanent_transfer_fee_e8s: args.permanent_transfer_fee_e8s,
        claim_transfer_fee_e8s: args.claim_transfer_fee_e8s,
        economics,
        nns_fingerprint: args.nns_fingerprint.clone(),
        snapshot_fingerprint: snapshot.observation_fingerprint.clone(),
    };
    permit.validate().map_err(ApiError::Invalid)?;
    Ok(permit)
}

fn request_matches(permit: &BackingInflowPermit, args: &PrepareBackingInflowArgs) -> bool {
    permit.source_operation_id == args.source_operation_id
        && permit.actual_mint_e8s == args.actual_mint_e8s
        && permit.maturity_generation == args.maturity_generation
        && permit.staging_account == args.staging_account
        && permit.mint_block == args.mint_block
        && permit.permanent_transfer_fee_e8s == args.permanent_transfer_fee_e8s
        && permit.claim_transfer_fee_e8s == args.claim_transfer_fee_e8s
        && permit.nns_fingerprint == args.nns_fingerprint
        && matches!(
            (args.kind, &permit.economics),
            (
                BackingInflowKind::PermanentMaturity,
                FrozenInflowEconomics::Permanent { .. }
            ) | (
                BackingInflowKind::PooledMaturity,
                FrozenInflowEconomics::Pooled { .. }
            )
        )
}

pub async fn prove_effect(args: ProveBackingEffectArgs) -> Result<BackingInflowProgress, ApiError> {
    let operation = active()?;
    if args.stream_operation_sequence != operation.permit.stream_operation_sequence {
        return Err(ApiError::Invalid(
            "backing effect names a different operation".into(),
        ));
    }
    if existing_block(&operation, args.effect) == Some(args.block_index) {
        return progress(&operation);
    }
    if existing_block(&operation, args.effect).is_some() {
        return Err(ApiError::Invalid("conflicting backing effect block".into()));
    }
    verify_effect(&operation.permit, args.effect, args.block_index).await?;
    let mut updated = operation.clone();
    match args.effect {
        BackingEffect::PermanentCredit if updated.permit.permanent_credit() > 0 => {
            updated.permanent_block = Some(args.block_index)
        }
        BackingEffect::FirstClaimCredit => updated.first_claim_block = Some(args.block_index),
        BackingEffect::PooledCredit => {
            let Some(transfer) = updated.pooled_transfer.as_mut() else {
                return Err(ApiError::Invalid("no Stream pooled transfer exists".into()));
            };
            transfer.state = TransferState::Succeeded {
                block: args.block_index,
            };
        }
        _ => return Err(ApiError::Invalid("unexpected backing effect".into())),
    }
    advance(&mut updated)?;
    persist(&operation, updated.clone())?;
    progress(&updated)
}

async fn verify_effect(
    permit: &BackingInflowPermit,
    effect: BackingEffect,
    block: u128,
) -> Result<(), ApiError> {
    let route = permit.route();
    let (source, destination, amount, fee) = match effect {
        BackingEffect::PermanentCredit => (
            &permit.staging_account,
            &permit.permanent_destination,
            permit.permanent_credit(),
            permit.permanent_transfer_fee_e8s,
        ),
        BackingEffect::FirstClaimCredit => (
            &permit.staging_account,
            if route.route == ClaimRoute::AllPool {
                &permit.pool_destination
            } else {
                &permit.liquid_destination
            },
            permit
                .first_claim_credit()
                .ok_or_else(|| ApiError::Invalid("claim credit overflow".into()))?,
            permit.claim_transfer_fee_e8s,
        ),
        BackingEffect::PooledCredit => (
            &permit.liquid_destination,
            &permit.pool_destination,
            route.pooled_credit,
            permit.claim_transfer_fee_e8s,
        ),
    };
    let exact = canonical::exact_icp_transfer(state::read().config.icp_ledger, block)
        .await
        .map_err(ApiError::Ledger)?;
    if exact.from != canonical::icp_account_identifier(source).map_err(ApiError::Invalid)?
        || exact.to != canonical::icp_account_identifier(destination).map_err(ApiError::Invalid)?
        || exact.amount_e8s != amount
        || exact.fee_e8s != fee
        || exact.native_memo_u64 != 0
        || exact.icrc1_memo.as_deref()
            != Some(effect_memo(&permit.source_operation_id, effect).as_slice())
        || exact.created_at_time == 0
        || exact.spender.is_some()
    {
        return Err(ApiError::Invalid(
            "canonical ICP block does not match the frozen backing effect".into(),
        ));
    }
    Ok(())
}

fn existing_block(operation: &BackingInflowOperation, effect: BackingEffect) -> Option<u128> {
    match effect {
        BackingEffect::PermanentCredit => operation.permanent_block,
        BackingEffect::FirstClaimCredit => operation.first_claim_block,
        BackingEffect::PooledCredit => {
            operation
                .pooled_transfer
                .as_ref()
                .and_then(|transfer| match transfer.state {
                    TransferState::Succeeded { block } => Some(block),
                    _ => None,
                })
        }
    }
}

fn advance(operation: &mut BackingInflowOperation) -> Result<(), ApiError> {
    let permanent_ready =
        operation.permit.permanent_credit() == 0 || operation.permanent_block.is_some();
    if !permanent_ready || operation.first_claim_block.is_none() {
        operation.phase = BackingInflowPhase::AwaitingNnsEffects;
    } else if operation.permit.route().route == ClaimRoute::Mixed
        && operation
            .pooled_transfer
            .as_ref()
            .is_none_or(|transfer| !matches!(transfer.state, TransferState::Succeeded { .. }))
    {
        operation.phase = if operation.pooled_transfer.is_some() {
            BackingInflowPhase::PooledTransferSubmitted
        } else {
            BackingInflowPhase::AwaitingPooledTransfer
        };
    } else {
        operation.phase = BackingInflowPhase::SettlingRewards;
    }
    Ok(())
}

pub async fn resume(now: u64) -> Result<BackingInflowProgress, ApiError> {
    let operation = active()?;
    match operation.phase {
        BackingInflowPhase::AwaitingNnsEffects => progress(&operation),
        BackingInflowPhase::AwaitingPooledTransfer => submit_pool(operation, now).await,
        BackingInflowPhase::PooledTransferSubmitted => progress(&operation),
        BackingInflowPhase::SettlingRewards => settle_reward(operation, now).await,
        BackingInflowPhase::Stuck => Err(ApiError::Stuck(
            "backing-inflow effect requires exact proof".into(),
        )),
    }
}

async fn submit_pool(
    operation: BackingInflowOperation,
    now: u64,
) -> Result<BackingInflowProgress, ApiError> {
    let config = state::read().config;
    let intent = OwnTransferIntent::Icrc1 {
        ledger: config.icp_ledger,
        from_subaccount: config
            .liquid_icp
            .canonical()
            .map_err(ApiError::Invalid)?
            .subaccount,
        to: operation.permit.pool_destination.clone(),
        amount: operation.permit.route().pooled_credit,
        fee: operation.permit.claim_transfer_fee_e8s,
        memo: effect_memo(
            &operation.permit.source_operation_id,
            BackingEffect::PooledCredit,
        ),
        created_at_time: now,
    };
    let mut attempt = TransferAttempt::prepared(intent).map_err(ApiError::Invalid)?;
    attempt.state = TransferState::Submitted {
        epoch: DispatchEpoch(1),
        first_submitted_at: now,
        last_submitted_at: now,
    };
    let mut submitted = operation.clone();
    submitted.phase = BackingInflowPhase::PooledTransferSubmitted;
    submitted.pooled_transfer = Some(attempt.clone());
    persist(&operation, submitted.clone())?;
    match crate::api::submit(&attempt.intent).await {
        Err(error) => Err(ApiError::Pending(error)),
        Ok(result) => match classify_result(result).map_err(ApiError::Ledger)? {
            ClassifiedResult::Succeeded(block) => {
                Ok(BackingInflowProgress::AwaitingPooledProof { block_index: block })
            }
            ClassifiedResult::NoEffect(reason) => stick(submitted, reason),
            ClassifiedResult::Ambiguous(reason) => Err(ApiError::Pending(reason)),
        },
    }
}

async fn settle_reward(
    mut operation: BackingInflowOperation,
    now: u64,
) -> Result<BackingInflowProgress, ApiError> {
    let index = usize::try_from(operation.recipient_index)
        .map_err(|_| ApiError::Invalid("recipient index overflow".into()))?;
    if index == operation.recipients.len() {
        return complete(operation);
    }
    let config = state::read().config;
    if operation.recipients[index]
        .transfer
        .as_ref()
        .is_some_and(|transfer| matches!(transfer.state, TransferState::Succeeded { .. }))
    {
        operation.recipient_index = operation
            .recipient_index
            .checked_add(1)
            .ok_or_else(|| ApiError::Invalid("recipient index overflow".into()))?;
        let expected = active()?;
        persist(&expected, operation.clone())?;
        return progress(&operation);
    }
    if operation.recipients[index].transfer.is_none() {
        let recipient = &operation.recipients[index];
        let intent = OwnTransferIntent::Icrc1 {
            ledger: config.io_ledger,
            from_subaccount: config
                .io_reserve
                .canonical()
                .map_err(ApiError::Invalid)?
                .subaccount,
            to: recipient.frozen.destination.clone(),
            amount: recipient.frozen.io_e8s,
            fee: config.expected_io_fee_e8s,
            memo: crate::transfer::deterministic_memo(
                b"io-backing-reward-v1",
                ic_cdk::api::canister_self(),
                (operation.permit.stream_operation_sequence << 32)
                    | operation.recipient_index as u64,
            ),
            created_at_time: now,
        };
        operation.recipients[index].transfer =
            Some(TransferAttempt::prepared(intent).map_err(ApiError::Invalid)?);
    }
    let attempt = operation.recipients[index]
        .transfer
        .as_mut()
        .expect("reward transfer was prepared");
    let (epoch, first_submitted_at) = match attempt.state {
        TransferState::Prepared => (DispatchEpoch(1), now),
        TransferState::Submitted {
            first_submitted_at, ..
        } => match crate::receipt::retry_decision(
            attempt,
            now,
            config.retry_delay_nanos,
            config.ledger_deduplication_window_nanos,
        )
        .map_err(ApiError::Invalid)?
        {
            crate::receipt::RetryDecision::Wait => {
                return Ok(BackingInflowProgress::SettlingRewards)
            }
            crate::receipt::RetryDecision::Expired => {
                attempt.state = TransferState::Stuck {
                    reason: "reward settlement deduplication window expired".into(),
                };
                operation.phase = BackingInflowPhase::Stuck;
                let expected = active()?;
                persist(&expected, operation)?;
                return Err(ApiError::Stuck(
                    "reward settlement requires exact block proof".into(),
                ));
            }
            crate::receipt::RetryDecision::Dispatch(epoch) => (epoch, first_submitted_at),
        },
        TransferState::Stuck { ref reason } => return Err(ApiError::Stuck(reason.clone())),
        TransferState::Succeeded { .. } => unreachable!("handled above"),
    };
    attempt.state = TransferState::Submitted {
        epoch,
        first_submitted_at,
        last_submitted_at: now,
    };
    let intent = attempt.intent.clone();
    let expected = active()?;
    persist(&expected, operation.clone())?;
    match crate::api::submit(&intent).await {
        Err(error) => Err(ApiError::Pending(error)),
        Ok(result) => match classify_result(result).map_err(ApiError::Ledger)? {
            ClassifiedResult::Succeeded(block) => {
                let mut succeeded = operation.clone();
                succeeded.recipients[index]
                    .transfer
                    .as_mut()
                    .expect("submitted transfer")
                    .state = TransferState::Succeeded { block };
                persist(&operation, succeeded)?;
                Ok(BackingInflowProgress::SettlingRewards)
            }
            ClassifiedResult::NoEffect(reason) => stick(operation, reason),
            ClassifiedResult::Ambiguous(reason) => Err(ApiError::Pending(reason)),
        },
    }
}

pub async fn prove_active_transfer(block_index: u128) -> Result<(), ApiError> {
    let operation = active()?;
    if let Some(transfer) = &operation.pooled_transfer {
        if matches!(
            transfer.state,
            TransferState::Submitted { .. } | TransferState::Stuck { .. }
        ) {
            verify_effect(&operation.permit, BackingEffect::PooledCredit, block_index).await?;
            let mut succeeded = operation.clone();
            succeeded
                .pooled_transfer
                .as_mut()
                .expect("validated pooled transfer")
                .state = TransferState::Succeeded { block: block_index };
            advance(&mut succeeded)?;
            return persist(&operation, succeeded);
        }
    }
    let index = usize::try_from(operation.recipient_index)
        .map_err(|_| ApiError::Invalid("recipient index overflow".into()))?;
    let transfer = operation
        .recipients
        .get(index)
        .and_then(|recipient| recipient.transfer.as_ref())
        .ok_or_else(|| ApiError::Invalid("no active backing-inflow transfer".into()))?;
    if !matches!(
        transfer.state,
        TransferState::Submitted { .. } | TransferState::Stuck { .. }
    ) {
        return Err(ApiError::Invalid(
            "backing-inflow transfer does not accept proof".into(),
        ));
    }
    let exact = canonical::exact_icrc_transfer(transfer.intent.ledger(), block_index)
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
    } = &transfer.intent
    else {
        return Err(ApiError::Invalid("reward transfer is not ICRC-1".into()));
    };
    let source = crate::state::Account {
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
            "exact IO block differs from the frozen reward transfer".into(),
        ));
    }
    let mut succeeded = operation.clone();
    succeeded.recipients[index]
        .transfer
        .as_mut()
        .expect("validated reward transfer")
        .state = TransferState::Succeeded { block: block_index };
    succeeded.phase = BackingInflowPhase::SettlingRewards;
    persist(&operation, succeeded)
}

fn complete(operation: BackingInflowOperation) -> Result<BackingInflowProgress, ApiError> {
    let distributed = operation
        .recipients
        .iter()
        .try_fold(0u128, |sum, recipient| {
            sum.checked_add(recipient.frozen.io_e8s)
        })
        .ok_or_else(|| ApiError::Invalid("distributed IO overflow".into()))?;
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(StreamOperation::BackingInflow(active)) if **active == operation)
    {
        return Err(ApiError::Busy);
    }
    if matches!(
        operation.permit.economics,
        FrozenInflowEconomics::Pooled { .. }
    ) {
        latest.pending_entitlement_batch = None;
    }
    latest.last_completed_backing_inflow = Some(LastCompletedBackingInflow {
        permit: operation.permit.clone(),
        distributed_io_e8s: distributed,
    });
    latest.active_operation = None;
    state::write(latest);
    Ok(BackingInflowProgress::Completed {
        source_operation_id: operation.permit.source_operation_id,
        distributed_io_e8s: distributed,
    })
}

fn stick(
    mut operation: BackingInflowOperation,
    reason: String,
) -> Result<BackingInflowProgress, ApiError> {
    operation.phase = BackingInflowPhase::Stuck;
    let expected = active()?;
    persist(&expected, operation)?;
    Ok(BackingInflowProgress::Stuck(reason))
}

fn active() -> Result<BackingInflowOperation, ApiError> {
    match state::read().active_operation {
        Some(StreamOperation::BackingInflow(operation)) => Ok(*operation),
        Some(_) => Err(ApiError::Busy),
        None => Err(ApiError::Invalid("no active backing inflow".into())),
    }
}

fn persist(
    expected: &BackingInflowOperation,
    replacement: BackingInflowOperation,
) -> Result<(), ApiError> {
    replacement.validate().map_err(ApiError::Invalid)?;
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(StreamOperation::BackingInflow(active)) if **active == *expected)
    {
        return Err(ApiError::Busy);
    }
    latest.active_operation = Some(StreamOperation::BackingInflow(Box::new(replacement)));
    state::write(latest);
    Ok(())
}

fn progress(operation: &BackingInflowOperation) -> Result<BackingInflowProgress, ApiError> {
    Ok(match operation.phase {
        BackingInflowPhase::AwaitingNnsEffects => {
            BackingInflowProgress::AwaitingNnsEffects(Box::new(operation.permit.clone()))
        }
        BackingInflowPhase::AwaitingPooledTransfer => BackingInflowProgress::AwaitingPooledTransfer,
        BackingInflowPhase::PooledTransferSubmitted => {
            let block = operation
                .pooled_transfer
                .as_ref()
                .and_then(|value| match value.state {
                    TransferState::Succeeded { block } => Some(block),
                    _ => None,
                });
            match block {
                Some(block_index) => BackingInflowProgress::AwaitingPooledProof { block_index },
                None => BackingInflowProgress::AwaitingPooledTransfer,
            }
        }
        BackingInflowPhase::SettlingRewards => BackingInflowProgress::SettlingRewards,
        BackingInflowPhase::Stuck => {
            BackingInflowProgress::Stuck("exact effect proof required".into())
        }
    })
}
