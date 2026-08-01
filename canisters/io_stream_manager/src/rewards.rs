use candid::{CandidType, Principal};
use ic_cdk::call::Call;
use serde::Deserialize;

use crate::{
    api::ApiError,
    canonical,
    receipt::{
        CompletedReceiptResult, LastCompletedReceipt, LiquidReceiptOperation, ReceiptPhase,
        RewardRecipient, TwoWeekReceiptOperation, TwoWeekReceiptResult, TwoWeekSettlement,
    },
    reward_evidence::{
        canonical_eligible, capture_proposal_window, close_proposal_window, eligible_members,
        exact_neuron, list_all_neurons, participation,
    },
    state::{self, Account, Lifecycle, RewardCohort},
    transfer::{
        classify_result, deterministic_memo, ClassifiedResult, OwnTransferIntent, TransferAttempt,
        TransferState,
    },
};

#[derive(Clone, Debug, CandidType)]
struct SetTargetArgs {
    target_e8s: u128,
    generation: u64,
}

#[derive(Clone, Debug, CandidType)]
struct PrepareTwoWeekMaturityArgs {
    cohort_generation: u64,
    captured_at_timestamp_seconds: u64,
    closes_at_timestamp_seconds: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum PreparedMaturityProgress {
    Observed,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum NnsError {
    Unauthorized,
    Paused,
    Busy,
    Invalid(String),
    Pending(String),
    Stuck(String),
    BelowMaturityThreshold {
        remaining_e8s: u64,
        minimum_e8s: u64,
    },
    ImplementationIncomplete(String),
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum TargetStatus {
    UnderTarget,
    AtTarget,
    AtTargetWithinUnwindTolerance,
    OverTarget,
}

#[derive(Clone, Debug, CandidType)]
struct ManageNeuronRequest {
    subaccount: Vec<u8>,
    command: Option<ManageNeuronCommand>,
}

#[derive(Clone, Debug, CandidType)]
enum ManageNeuronCommand {
    ClaimOrRefresh(ClaimOrRefresh),
}

#[derive(Clone, Debug, CandidType)]
struct ClaimOrRefresh {
    by: Option<ClaimBy>,
}

#[derive(Clone, Debug, CandidType)]
enum ClaimBy {
    NeuronId(Empty),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Empty {}

pub async fn capture(now_seconds: u64) -> Result<RewardCohort, ApiError> {
    let snapshot = state::read();
    require_capture_slot(&snapshot, now_seconds)?;
    let neurons = list_all_neurons(snapshot.config.sns_governance).await?;
    let members = eligible_members(
        snapshot.config.sns_governance,
        &snapshot.config.excluded_io_accounts,
        &neurons,
    )?;
    if members.is_empty() {
        return Err(ApiError::Invalid(
            "reward cohort has no eligible members".into(),
        ));
    }
    let (latest_proposal_id_at_capture, open_proposal_ids_at_capture) =
        capture_proposal_window(snapshot.config.sns_governance).await?;
    let canonical = canonical::redemption_snapshot(&snapshot.config)
        .await
        .map_err(ApiError::Ledger)?;
    let excluded = canonical
        .excluded_io_balances
        .iter()
        .try_fold(0u128, |sum, (_, value)| sum.checked_add(*value))
        .ok_or_else(|| ApiError::Invalid("excluded IO balance overflow".into()))?;
    let redeemable = canonical
        .total_supply_e8s
        .checked_sub(canonical.reserve_io_e8s)
        .and_then(|value| value.checked_sub(excluded))
        .ok_or_else(|| ApiError::Invalid("invalid redeemable IO supply".into()))?;
    let active_io = members
        .iter()
        .try_fold(0u128, |sum, member| {
            sum.checked_add(member.frozen_stake_e8s)
        })
        .ok_or_else(|| ApiError::Invalid("eligible cohort stake overflow".into()))?;
    let target = io_core_model::two_week_target(active_io, canonical.liquid_icp_e8s, redeemable)
        .map_err(|error| ApiError::Invalid(format!("two-week target failed: {error:?}")))?;
    let generation = next_generation(&snapshot)?;
    let capacity = set_nns_target(snapshot.config.nns_manager, generation, target).await?;
    if capacity == TargetStatus::UnderTarget {
        return Err(ApiError::Pending(
            "canonical two-week NNS principal is UnderTarget".into(),
        ));
    }
    if state::read() != snapshot {
        return Err(ApiError::Busy);
    }
    let cohort = RewardCohort {
        generation,
        captured_at_timestamp_seconds: now_seconds,
        closes_at_timestamp_seconds: now_seconds
            .checked_add(io_core_model::TWO_WEEK_SECONDS)
            .ok_or_else(|| ApiError::Invalid("cohort close timestamp overflow".into()))?,
        target_icp_e8s: target,
        latest_proposal_id_at_capture,
        open_proposal_ids_at_capture,
        members,
    };
    cohort
        .validate(&snapshot.config)
        .map_err(ApiError::Invalid)?;
    let mut latest = snapshot;
    latest.latest_cohort_generation = generation;
    latest.next_cohort_timestamp_seconds = cohort.closes_at_timestamp_seconds;
    latest.active_reward_cohort = Some(cohort.clone());
    state::write(latest);
    crate::cohort_timer::install(Some(cohort.closes_at_timestamp_seconds));
    Ok(cohort)
}

pub async fn close(now_seconds: u64) -> Result<RewardCohort, ApiError> {
    let snapshot = state::read();
    if snapshot.lifecycle != Lifecycle::Ready
        || snapshot.active_operation.is_some()
        || snapshot.pending_reward_cohort.is_some()
    {
        return Err(ApiError::Busy);
    }
    let mut cohort = snapshot
        .active_reward_cohort
        .clone()
        .ok_or_else(|| ApiError::Invalid("no active reward cohort".into()))?;
    if now_seconds < cohort.closes_at_timestamp_seconds {
        return Err(ApiError::Pending(format!(
            "cohort closes at {}",
            cohort.closes_at_timestamp_seconds
        )));
    }
    let neurons = list_all_neurons(snapshot.config.sns_governance).await?;
    let proposals = close_proposal_window(snapshot.config.sns_governance, &cohort).await?;
    for member in &mut cohort.members {
        let current = neurons
            .iter()
            .find(|neuron| neuron.id.as_ref().map(|id| &id.id) == Some(&member.sns_neuron_id));
        member.destination_is_currently_eligible = current.is_some_and(canonical_eligible);
        member.observed_stake_e8s = current
            .map(|neuron| u128::from(neuron.cached_neuron_stake_e8s))
            .unwrap_or(member.frozen_stake_e8s);
        let (eligible, voted) = participation(
            &member.sns_neuron_id,
            cohort.captured_at_timestamp_seconds,
            cohort.closes_at_timestamp_seconds,
            &proposals,
        )?;
        member.eligible_closed_proposals = eligible;
        member.voted_closed_proposals = voted;
    }
    prepare_two_week_maturity(snapshot.config.nns_manager, &cohort).await?;
    if state::read() != snapshot {
        return Err(ApiError::Busy);
    }
    cohort
        .validate(&snapshot.config)
        .map_err(ApiError::Invalid)?;
    let mut latest = snapshot;
    latest.active_reward_cohort = None;
    latest.pending_reward_cohort = Some(cohort.clone());
    latest.next_cohort_timestamp_seconds = 0;
    state::write(latest);
    crate::cohort_timer::install(None);
    Ok(cohort)
}

async fn prepare_two_week_maturity(
    manager: Principal,
    cohort: &RewardCohort,
) -> Result<(), ApiError> {
    let result: Result<PreparedMaturityProgress, NnsError> =
        Call::bounded_wait(manager, "prepare_two_week_maturity")
            .with_arg(PrepareTwoWeekMaturityArgs {
                cohort_generation: cohort.generation,
                captured_at_timestamp_seconds: cohort.captured_at_timestamp_seconds,
                closes_at_timestamp_seconds: cohort.closes_at_timestamp_seconds,
            })
            .await
            .map_err(|error| {
                ApiError::Pending(format!(
                    "two-week maturity preparation ambiguous: {error:?}"
                ))
            })?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!(
                    "two-week maturity preparation decode failed: {error:?}"
                ))
            })?;
    match result {
        Ok(PreparedMaturityProgress::Observed) => Ok(()),
        Err(error) => Err(ApiError::Invalid(format!(
            "NNS rejected two-week maturity preparation: {error:?}"
        ))),
    }
}

fn require_capture_slot(
    state: &crate::state::StreamStateV1,
    now_seconds: u64,
) -> Result<(), ApiError> {
    if state.lifecycle != Lifecycle::Ready
        || state.active_operation.is_some()
        || state.active_reward_cohort.is_some()
    {
        return Err(ApiError::Busy);
    }
    if now_seconds == 0 {
        return Err(ApiError::Invalid("cohort capture time is zero".into()));
    }
    Ok(())
}

fn next_generation(state: &crate::state::StreamStateV1) -> Result<u64, ApiError> {
    state
        .latest_cohort_generation
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("cohort generation exhausted".into()))
}

async fn set_nns_target(
    manager: Principal,
    generation: u64,
    target: u128,
) -> Result<TargetStatus, ApiError> {
    let result: Result<TargetStatus, NnsError> = Call::bounded_wait(manager, "set_two_week_target")
        .with_arg(SetTargetArgs {
            target_e8s: target,
            generation,
        })
        .await
        .map_err(|error| ApiError::Pending(format!("NNS target call ambiguous: {error:?}")))?
        .candid()
        .map_err(|error| ApiError::Invalid(format!("NNS target decode failed: {error:?}")))?;
    result.map_err(|error| ApiError::Invalid(format!("NNS rejected target: {error:?}")))
}

pub(crate) async fn resume_two_week(
    operation: TwoWeekReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    match operation.phase {
        ReceiptPhase::AwaitingReceipt => Ok(crate::api::LiquidReceiptProgress::AwaitingReceipt),
        ReceiptPhase::ReceiptProved => prepare_settlement(operation).await,
        ReceiptPhase::Settling => resume_recipient(operation, now).await,
        ReceiptPhase::Stuck => Err(ApiError::Stuck(
            "two-week recipient transfer requires exact proof or reviewed recovery".into(),
        )),
        ReceiptPhase::Completed => Err(ApiError::Invalid(
            "completed two-week receipt must be available through replay".into(),
        )),
    }
}

async fn prepare_settlement(
    operation: TwoWeekReceiptOperation,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let snapshot = state::read();
    let generation = operation
        .context
        .request
        .cohort_generation
        .ok_or_else(|| ApiError::Invalid("two-week receipt lacks cohort".into()))?;
    let cohort = snapshot
        .pending_reward_cohort
        .as_ref()
        .filter(|cohort| cohort.generation == generation)
        .ok_or_else(|| ApiError::Invalid("two-week receipt lost pending cohort".into()))?;
    let canonical = canonical::redemption_snapshot(&snapshot.config)
        .await
        .map_err(ApiError::Ledger)?;
    crate::receipt_preparation::validate_post_receipt_snapshot(
        &operation.context.backing_snapshot,
        &canonical,
        operation.context.request.liquid_amount_e8s,
        0,
    )?;
    let excluded = operation
        .context
        .backing_snapshot
        .excluded_io_balances
        .iter()
        .try_fold(0u128, |sum, (_, value)| sum.checked_add(*value))
        .ok_or_else(|| ApiError::Invalid("excluded IO overflow".into()))?;
    let redeemable = operation
        .context
        .backing_snapshot
        .total_io_supply_e8s
        .checked_sub(operation.context.backing_snapshot.reserve_io_e8s)
        .and_then(|value| value.checked_sub(excluded))
        .ok_or_else(|| ApiError::Invalid("invalid redeemable IO supply".into()))?;
    let pool = io_core_model::backed_io(
        operation.context.request.liquid_amount_e8s,
        operation.context.backing_snapshot.liquid_icp_e8s,
        redeemable,
    )
    .map_err(|error| ApiError::Invalid(format!("reward backing failed: {error:?}")))?;
    let participants = cohort
        .members
        .iter()
        .map(|member| {
            io_reward_policy::participant_from_bytes(
                member.sns_neuron_id.clone(),
                member.frozen_stake_e8s,
                member.eligible_closed_proposals,
                member.voted_closed_proposals,
                member.destination_is_currently_eligible,
            )
            .map_err(|error| ApiError::Invalid(format!("reward participant failed: {error:?}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let allocation = io_reward_policy::allocate_rewards(pool, &participants)
        .map_err(|error| ApiError::Invalid(format!("reward allocation failed: {error:?}")))?;
    let recipients = allocation
        .allocations
        .iter()
        .map(|allocation| {
            let id = allocation.sns_neuron_id.0.clone();
            let member = cohort
                .members
                .iter()
                .find(|member| member.sns_neuron_id == id)
                .ok_or_else(|| ApiError::Invalid("allocation lacks frozen member".into()))?;
            Ok(RewardRecipient {
                sns_neuron_id: id,
                destination: member.account.clone(),
                before_stake_e8s: member.observed_stake_e8s,
                io_e8s: allocation.io_e8s,
                eligibility_checked: false,
                forfeited: false,
                transfer: None,
                refresh_submitted: false,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let fees = operation
        .context
        .backing_snapshot
        .io_fee_e8s
        .checked_mul(recipients.len() as u128)
        .ok_or_else(|| ApiError::Invalid("reward fee total overflow".into()))?;
    let issued = recipients
        .iter()
        .try_fold(0u128, |sum, recipient| sum.checked_add(recipient.io_e8s))
        .ok_or_else(|| ApiError::Invalid("reward issue total overflow".into()))?;
    let required_reserve = issued
        .checked_add(fees)
        .ok_or_else(|| ApiError::Invalid("reward reserve requirement overflow".into()))?;
    if canonical.reserve_io_e8s < required_reserve {
        return Err(ApiError::Invalid(
            "IO reserve does not cover rewards plus one fee per recipient".into(),
        ));
    }
    if state::read() != snapshot {
        return Err(ApiError::Busy);
    }
    let mut replacement = operation.clone();
    replacement.phase = ReceiptPhase::Settling;
    replacement.settlement = Some(TwoWeekSettlement {
        backed_io_pool_e8s: pool,
        recipients,
        recipient_index: 0,
        distributed_io_e8s: 0,
        forfeited_io_e8s: allocation.forfeited_reward_e8s,
        rounding_dust_io_e8s: allocation.rounding_dust_e8s,
        total_dust_io_e8s: allocation.dust_e8s,
    });
    crate::receipt::persist_exact(
        &LiquidReceiptOperation::TwoWeek(Box::new(operation)),
        LiquidReceiptOperation::TwoWeek(Box::new(replacement)),
    )?;
    Ok(crate::api::LiquidReceiptProgress::Settling)
}

async fn resume_recipient(
    operation: TwoWeekReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let settlement = operation
        .settlement
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("two-week settlement is missing".into()))?;
    let index = settlement.recipient_index as usize;
    if index == settlement.recipients.len() {
        return complete_settlement(operation, now);
    }
    let recipient = &settlement.recipients[index];
    if !recipient.eligibility_checked {
        return check_recipient_eligibility(operation).await;
    }
    if recipient.forfeited {
        return Err(ApiError::Invalid(
            "forfeited reward recipient index was not advanced".into(),
        ));
    }
    match &recipient.transfer {
        None => submit_recipient(operation, now).await,
        Some(transfer) => match transfer.state {
            TransferState::Prepared | TransferState::Submitted { .. } => {
                submit_recipient(operation, now).await
            }
            TransferState::Succeeded { .. } if !recipient.refresh_submitted => {
                refresh_recipient(operation).await
            }
            TransferState::Succeeded { .. } => observe_refresh(operation).await,
            TransferState::Stuck { ref reason } => {
                Ok(crate::api::LiquidReceiptProgress::Stuck(reason.clone()))
            }
        },
    }
}

async fn check_recipient_eligibility(
    operation: TwoWeekReceiptOperation,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let snapshot = state::read();
    let settlement = operation.settlement.as_ref().expect("validated settlement");
    let index = settlement.recipient_index as usize;
    let recipient = &settlement.recipients[index];
    let neuron = exact_neuron(snapshot.config.sns_governance, &recipient.sns_neuron_id).await?;
    if active_two_week()? != operation {
        return Err(ApiError::Busy);
    }
    let account = Account {
        owner: snapshot.config.sns_governance,
        subaccount: Some(recipient.sns_neuron_id.clone()),
    };
    if !account
        .effective_eq(&recipient.destination)
        .map_err(ApiError::Invalid)?
    {
        return Err(ApiError::Invalid(
            "reward destination does not match its SNS neuron ID".into(),
        ));
    }
    let excluded = snapshot
        .config
        .excluded_io_accounts
        .iter()
        .try_fold(false, |matched, excluded| {
            account.effective_eq(excluded).map(|same| matched || same)
        })
        .map_err(ApiError::Invalid)?;
    let mut replacement = operation.clone();
    let settlement = replacement
        .settlement
        .as_mut()
        .expect("validated settlement");
    let recipient = &mut settlement.recipients[index];
    recipient.eligibility_checked = true;
    if let Some(neuron) = neuron.filter(|value| canonical_eligible(value) && !excluded) {
        recipient.before_stake_e8s = u128::from(neuron.cached_neuron_stake_e8s);
    } else {
        forfeit_current_recipient(settlement)?;
    }
    crate::receipt::persist_exact(
        &LiquidReceiptOperation::TwoWeek(Box::new(operation)),
        LiquidReceiptOperation::TwoWeek(Box::new(replacement)),
    )?;
    Ok(crate::api::LiquidReceiptProgress::Settling)
}

fn forfeit_current_recipient(settlement: &mut TwoWeekSettlement) -> Result<(), ApiError> {
    let index = settlement.recipient_index as usize;
    let amount = settlement.recipients[index].io_e8s;
    settlement.recipients[index].forfeited = true;
    settlement.forfeited_io_e8s = settlement
        .forfeited_io_e8s
        .checked_add(amount)
        .ok_or_else(|| ApiError::Invalid("reward forfeiture overflow".into()))?;
    settlement.total_dust_io_e8s = settlement
        .total_dust_io_e8s
        .checked_add(amount)
        .ok_or_else(|| ApiError::Invalid("reward dust overflow".into()))?;
    settlement.recipient_index = settlement
        .recipient_index
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("reward recipient index overflow".into()))?;
    Ok(())
}

async fn submit_recipient(
    operation: TwoWeekReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let config = state::read().config;
    let settlement = operation.settlement.as_ref().expect("validated settlement");
    let index = settlement.recipient_index as usize;
    let recipient = &settlement.recipients[index];
    let mut attempt = match &recipient.transfer {
        None => TransferAttempt::prepared(OwnTransferIntent::Icrc1 {
            ledger: config.io_ledger,
            from_subaccount: config
                .io_reserve
                .canonical()
                .map_err(ApiError::Invalid)?
                .subaccount,
            to: recipient.destination.clone(),
            amount: recipient.io_e8s,
            fee: config.expected_io_fee_e8s,
            memo: deterministic_memo(
                b"io-two-week-reward-v1",
                ic_cdk::api::canister_self(),
                (operation.context.request.receipt_sequence << 32) | index as u64,
            ),
            created_at_time: now,
        })
        .map_err(ApiError::Invalid)?,
        Some(attempt) => attempt.clone(),
    };
    let (epoch, first_submitted_at) =
        match attempt.state {
            TransferState::Prepared => (crate::state::DispatchEpoch(1), now),
            TransferState::Submitted {
                epoch,
                first_submitted_at,
                last_submitted_at,
            } => {
                if now
                    .checked_sub(last_submitted_at)
                    .ok_or_else(|| ApiError::Invalid("reward retry clock regressed".into()))?
                    < config.retry_delay_nanos
                {
                    return Ok(crate::api::LiquidReceiptProgress::Settling);
                }
                let deadline = attempt
                    .intent
                    .created_at_time()
                    .checked_add(config.ledger_deduplication_window_nanos)
                    .ok_or_else(|| ApiError::Invalid("reward retry deadline overflow".into()))?;
                if now >= deadline {
                    return stick_recipient(operation, "reward transfer retry window expired");
                }
                (
                    crate::state::DispatchEpoch(epoch.0.checked_add(1).ok_or_else(|| {
                        ApiError::Invalid("reward dispatch epoch exhausted".into())
                    })?),
                    first_submitted_at,
                )
            }
            _ => return Err(ApiError::Busy),
        };
    attempt.state = TransferState::Submitted {
        epoch,
        first_submitted_at,
        last_submitted_at: now,
    };
    let intent = attempt.intent.clone();
    let mut submitted = operation.clone();
    submitted
        .settlement
        .as_mut()
        .expect("validated settlement")
        .recipients[index]
        .transfer = Some(attempt.clone());
    crate::receipt::persist_exact(
        &LiquidReceiptOperation::TwoWeek(Box::new(operation)),
        LiquidReceiptOperation::TwoWeek(Box::new(submitted.clone())),
    )?;
    let response = crate::api::submit(&intent).await;
    if active_two_week()? != submitted {
        return Err(ApiError::Busy);
    }
    match response {
        Err(error) => Err(ApiError::Pending(error)),
        Ok(result) => match classify_result(result).map_err(ApiError::Ledger)? {
            ClassifiedResult::Succeeded(block) => {
                let mut succeeded = submitted.clone();
                succeeded
                    .settlement
                    .as_mut()
                    .expect("validated settlement")
                    .recipients[index]
                    .transfer
                    .as_mut()
                    .expect("submitted transfer")
                    .state = TransferState::Succeeded { block };
                crate::receipt::persist_exact(
                    &LiquidReceiptOperation::TwoWeek(Box::new(submitted)),
                    LiquidReceiptOperation::TwoWeek(Box::new(succeeded)),
                )?;
                Ok(crate::api::LiquidReceiptProgress::Settling)
            }
            ClassifiedResult::NoEffect(error) => stick_recipient(submitted, &error),
            ClassifiedResult::Ambiguous(error) => Err(ApiError::Pending(error)),
        },
    }
}

fn stick_recipient(
    operation: TwoWeekReceiptOperation,
    reason: &str,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let mut stuck = operation.clone();
    stuck.phase = ReceiptPhase::Stuck;
    let settlement = stuck.settlement.as_mut().expect("validated settlement");
    let index = settlement.recipient_index as usize;
    let attempt = settlement.recipients[index]
        .transfer
        .as_mut()
        .ok_or_else(|| ApiError::Invalid("Stuck reward transfer is missing".into()))?;
    attempt.state = TransferState::Stuck {
        reason: reason.into(),
    };
    crate::receipt::persist_exact(
        &LiquidReceiptOperation::TwoWeek(Box::new(operation)),
        LiquidReceiptOperation::TwoWeek(Box::new(stuck)),
    )?;
    crate::api::pause();
    Err(ApiError::Stuck(reason.into()))
}

async fn refresh_recipient(
    operation: TwoWeekReceiptOperation,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let settlement = operation.settlement.as_ref().expect("validated settlement");
    let index = settlement.recipient_index as usize;
    let neuron_id = settlement.recipients[index].sns_neuron_id.clone();
    let mut submitted = operation.clone();
    submitted
        .settlement
        .as_mut()
        .expect("validated settlement")
        .recipients[index]
        .refresh_submitted = true;
    crate::receipt::persist_exact(
        &LiquidReceiptOperation::TwoWeek(Box::new(operation)),
        LiquidReceiptOperation::TwoWeek(Box::new(submitted.clone())),
    )?;
    let result = Call::bounded_wait(state::read().config.sns_governance, "manage_neuron")
        .with_arg(ManageNeuronRequest {
            subaccount: neuron_id,
            command: Some(ManageNeuronCommand::ClaimOrRefresh(ClaimOrRefresh {
                by: Some(ClaimBy::NeuronId(Empty {})),
            })),
        })
        .await;
    if active_two_week()? != submitted {
        return Err(ApiError::Busy);
    }
    result
        .map(|_| crate::api::LiquidReceiptProgress::Settling)
        .map_err(|error| ApiError::Pending(format!("SNS reward refresh ambiguous: {error:?}")))
}

async fn observe_refresh(
    operation: TwoWeekReceiptOperation,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let settlement = operation.settlement.as_ref().expect("validated settlement");
    let index = settlement.recipient_index as usize;
    let recipient = &settlement.recipients[index];
    let neuron = exact_neuron(
        state::read().config.sns_governance,
        &recipient.sns_neuron_id,
    )
    .await?
    .ok_or_else(|| ApiError::Pending("refreshed SNS neuron is absent".into()))?;
    if active_two_week()? != operation {
        return Err(ApiError::Busy);
    }
    let expected = recipient
        .before_stake_e8s
        .checked_add(recipient.io_e8s)
        .ok_or_else(|| ApiError::Invalid("reward stake expectation overflow".into()))?;
    if u128::from(neuron.cached_neuron_stake_e8s) < expected {
        return Err(ApiError::Pending(
            "SNS reward refresh stake increase is not canonically observable".into(),
        ));
    }
    let mut replacement = operation.clone();
    let settlement = replacement
        .settlement
        .as_mut()
        .expect("validated settlement");
    settlement.distributed_io_e8s = settlement
        .distributed_io_e8s
        .checked_add(settlement.recipients[index].io_e8s)
        .ok_or_else(|| ApiError::Invalid("distributed reward overflow".into()))?;
    settlement.recipient_index = settlement
        .recipient_index
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("reward recipient index overflow".into()))?;
    crate::receipt::persist_exact(
        &LiquidReceiptOperation::TwoWeek(Box::new(operation)),
        LiquidReceiptOperation::TwoWeek(Box::new(replacement)),
    )?;
    Ok(crate::api::LiquidReceiptProgress::Settling)
}

fn complete_settlement(
    operation: TwoWeekReceiptOperation,
    now: u64,
) -> Result<crate::api::LiquidReceiptProgress, ApiError> {
    let settlement = operation.settlement.as_ref().expect("validated settlement");
    let receipt_block = operation
        .receipt_block
        .ok_or_else(|| ApiError::Invalid("two-week receipt block is missing".into()))?;
    if settlement
        .distributed_io_e8s
        .checked_add(settlement.total_dust_io_e8s)
        .ok_or_else(|| ApiError::Invalid("two-week settlement total overflow".into()))?
        != settlement.backed_io_pool_e8s
    {
        return Err(ApiError::Invalid(
            "two-week settlement does not reconcile".into(),
        ));
    }
    let result = CompletedReceiptResult::TwoWeek(TwoWeekReceiptResult {
        request_fingerprint: operation.context.request_fingerprint.clone(),
        receipt_block,
        backed_io_pool_e8s: settlement.backed_io_pool_e8s,
        distributed_io_e8s: settlement.distributed_io_e8s,
        forfeited_io_e8s: settlement.forfeited_io_e8s,
        rounding_dust_io_e8s: settlement.rounding_dust_io_e8s,
        total_dust_io_e8s: settlement.total_dust_io_e8s,
        completed_at_nanos: now,
    });
    let expected = LiquidReceiptOperation::TwoWeek(Box::new(operation.clone()));
    let mut latest = state::read();
    if !matches!(&latest.active_operation, Some(crate::state::StreamOperation::LiquidReceipt(active)) if **active == expected)
    {
        return Err(ApiError::Busy);
    }
    let generation = operation.context.request.cohort_generation;
    if latest
        .pending_reward_cohort
        .as_ref()
        .map(|cohort| cohort.generation)
        != generation
    {
        return Err(ApiError::Busy);
    }
    latest.last_completed_receipt = Some(LastCompletedReceipt {
        request: operation.context.request,
        request_fingerprint: operation.context.request_fingerprint,
        permit: operation.context.permit,
        backing_snapshot: operation.context.backing_snapshot,
        receipt_block,
        result: result.clone(),
    });
    latest.next_nns_receipt_sequence = latest
        .next_nns_receipt_sequence
        .checked_add(1)
        .ok_or_else(|| ApiError::Invalid("receipt sequence overflow".into()))?;
    latest.active_operation = None;
    latest.pending_reward_cohort = None;
    state::write(latest);
    Ok(crate::api::LiquidReceiptProgress::Completed(result))
}

fn active_two_week() -> Result<TwoWeekReceiptOperation, ApiError> {
    match state::read().active_operation {
        Some(crate::state::StreamOperation::LiquidReceipt(operation)) => match *operation {
            LiquidReceiptOperation::TwoWeek(operation) => Ok(*operation),
            LiquidReceiptOperation::Jupiter(_) => Err(ApiError::Busy),
        },
        _ => Err(ApiError::Busy),
    }
}

pub(crate) async fn prove_recipient_transfer(block_index: u128) -> Result<(), ApiError> {
    let operation = active_two_week()?;
    if operation.phase != ReceiptPhase::Stuck {
        return Err(ApiError::Invalid("two-week receipt is not Stuck".into()));
    }
    let settlement = operation.settlement.as_ref().expect("validated settlement");
    let index = settlement.recipient_index as usize;
    let attempt = settlement.recipients[index]
        .transfer
        .as_ref()
        .ok_or_else(|| ApiError::Invalid("reward transfer proof slot is empty".into()))?;
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
        return Err(ApiError::Invalid(
            "reward intent has wrong transfer kind".into(),
        ));
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
            "exact block does not match reward transfer".into(),
        ));
    }
    if active_two_week()? != operation {
        return Err(ApiError::Busy);
    }
    let mut replacement = operation.clone();
    replacement.phase = ReceiptPhase::Settling;
    replacement
        .settlement
        .as_mut()
        .expect("validated settlement")
        .recipients[index]
        .transfer
        .as_mut()
        .expect("reward transfer")
        .state = TransferState::Succeeded { block: block_index };
    crate::receipt::persist_exact(
        &LiquidReceiptOperation::TwoWeek(Box::new(operation)),
        LiquidReceiptOperation::TwoWeek(Box::new(replacement)),
    )
}

pub use io_reward_policy::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payout_ineligibility_preserves_rounding_and_adds_exact_forfeiture() {
        let recipient = RewardRecipient {
            sns_neuron_id: vec![1; 32],
            destination: Account {
                owner: Principal::from_slice(&[2; 29]),
                subaccount: Some(vec![1; 32]),
            },
            before_stake_e8s: 0,
            io_e8s: 50,
            eligibility_checked: true,
            forfeited: false,
            transfer: None,
            refresh_submitted: false,
        };
        let mut settlement = TwoWeekSettlement {
            backed_io_pool_e8s: 101,
            recipients: vec![recipient],
            recipient_index: 0,
            distributed_io_e8s: 50,
            forfeited_io_e8s: 0,
            rounding_dust_io_e8s: 1,
            total_dust_io_e8s: 1,
        };
        forfeit_current_recipient(&mut settlement).unwrap();
        assert_eq!(settlement.forfeited_io_e8s, 50);
        assert_eq!(settlement.rounding_dust_io_e8s, 1);
        assert_eq!(settlement.total_dust_io_e8s, 51);
        assert_eq!(
            settlement.distributed_io_e8s + settlement.total_dust_io_e8s,
            101
        );
    }
}
