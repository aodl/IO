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
    state::{self, Account, Lifecycle, RewardCohort, RewardMember},
    transfer::{
        classify_result, deterministic_memo, ClassifiedResult, OwnTransferIntent, TransferAttempt,
        TransferState,
    },
};

const PAGE_SIZE: u32 = 100;
const MAX_PAGES: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct NeuronId {
    id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum DissolveState {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Neuron {
    id: Option<NeuronId>,
    cached_neuron_stake_e8s: u64,
    dissolve_state: Option<DissolveState>,
}

#[derive(Clone, Debug, CandidType)]
struct ListNeuronsRequest {
    of_principal: Option<Principal>,
    limit: u32,
    start_page_at: Option<NeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ListNeuronsResponse {
    neurons: Vec<Neuron>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct ProposalId {
    id: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Ballot {
    vote: i32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Proposal {
    id: Option<ProposalId>,
    ballots: Vec<(String, Ballot)>,
    decided_timestamp_seconds: u64,
    is_eligible_for_rewards: bool,
}

#[derive(Clone, Debug, CandidType)]
struct ListProposalsRequest {
    include_reward_status: Vec<i32>,
    before_proposal: Option<ProposalId>,
    limit: u32,
    exclude_type: Vec<u64>,
    include_status: Vec<i32>,
    include_topics: Option<Vec<ReservedTopicSelector>>,
}

#[derive(Clone, Debug, CandidType)]
struct ReservedTopicSelector {
    topic: Option<SnsTopic>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, CandidType)]
enum SnsTopic {
    DaoCommunitySettings,
    SnsFrameworkManagement,
    DappCanisterManagement,
    ApplicationBusinessLogic,
    Governance,
    TreasuryAssetManagement,
    CriticalDappOperations,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ListProposalsResponse {
    proposals: Vec<Proposal>,
}

#[derive(Clone, Debug, CandidType)]
struct SetTargetArgs {
    target_e8s: u128,
    generation: u64,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
enum TargetStatus {
    UnderTarget,
    AtTarget,
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

#[derive(Clone, Debug, CandidType, Deserialize)]
struct GovernanceError {
    error_message: String,
    error_type: i32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ClaimOrRefreshResponse {
    refreshed_neuron_id: Option<NeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum ManageNeuronCommandResponse {
    Error(GovernanceError),
    ClaimOrRefresh(ClaimOrRefreshResponse),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct ManageNeuronResponse {
    command: Option<ManageNeuronCommandResponse>,
}

pub async fn capture(now_seconds: u64) -> Result<RewardCohort, ApiError> {
    let snapshot = state::read();
    require_capture_slot(&snapshot, now_seconds)?;
    let neurons = list_all_neurons(snapshot.config.sns_governance).await?;
    let members = eligible_members(snapshot.config.sns_governance, &neurons)?;
    if members.is_empty() {
        return Err(ApiError::Invalid(
            "reward cohort has no eligible members".into(),
        ));
    }
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
        members,
    };
    cohort.validate().map_err(ApiError::Invalid)?;
    let mut latest = snapshot;
    latest.latest_cohort_generation = generation;
    latest.next_cohort_timestamp_seconds = cohort.closes_at_timestamp_seconds;
    latest.active_reward_cohort = Some(cohort.clone());
    state::write(latest);
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
    let proposals = list_all_proposals(snapshot.config.sns_governance).await?;
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
        );
        member.eligible_closed_proposals = eligible;
        member.voted_closed_proposals = voted;
    }
    if state::read() != snapshot {
        return Err(ApiError::Busy);
    }
    cohort.validate().map_err(ApiError::Invalid)?;
    let mut latest = snapshot;
    latest.active_reward_cohort = None;
    latest.pending_reward_cohort = Some(cohort.clone());
    latest.next_cohort_timestamp_seconds = 0;
    state::write(latest);
    Ok(cohort)
}

fn require_capture_slot(
    state: &crate::state::StreamStateV1,
    now_seconds: u64,
) -> Result<(), ApiError> {
    if state.lifecycle != Lifecycle::Ready
        || state.active_operation.is_some()
        || state.active_reward_cohort.is_some()
        || state.pending_reward_cohort.is_some()
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

fn eligible_members(
    governance: Principal,
    neurons: &[Neuron],
) -> Result<Vec<RewardMember>, ApiError> {
    neurons
        .iter()
        .filter(|neuron| canonical_eligible(neuron))
        .map(|neuron| {
            let id = neuron
                .id
                .as_ref()
                .ok_or_else(|| ApiError::Invalid("eligible SNS neuron lacks ID".into()))?
                .id
                .clone();
            if id.len() != 32 {
                return Err(ApiError::Invalid(
                    "eligible SNS neuron ID is not a canonical staking subaccount".into(),
                ));
            }
            Ok(RewardMember {
                account: Account {
                    owner: governance,
                    subaccount: Some(id.clone()),
                },
                sns_neuron_id: id,
                frozen_stake_e8s: u128::from(neuron.cached_neuron_stake_e8s),
                observed_stake_e8s: u128::from(neuron.cached_neuron_stake_e8s),
                eligible_closed_proposals: 0,
                voted_closed_proposals: 0,
                destination_is_currently_eligible: true,
            })
        })
        .collect()
}

fn canonical_eligible(neuron: &Neuron) -> bool {
    neuron.cached_neuron_stake_e8s > 0
        && matches!(
            neuron.dissolve_state.as_ref(),
            Some(DissolveState::DissolveDelaySeconds(
                io_core_model::TWO_WEEK_SECONDS
            ))
        )
}

fn participation(id: &[u8], start: u64, end: u64, proposals: &[Proposal]) -> (u64, u64) {
    let id = crate::transfer::hex(id);
    let mut eligible = 0u64;
    let mut voted = 0u64;
    for proposal in proposals {
        let decided = proposal.decided_timestamp_seconds;
        if decided < start || decided > end || decided == 0 || !proposal.is_eligible_for_rewards {
            continue;
        }
        eligible = eligible.saturating_add(1);
        if proposal.ballots.iter().any(|(neuron, ballot)| {
            neuron.eq_ignore_ascii_case(&id) && matches!(ballot.vote, 1..=4)
        }) {
            voted = voted.saturating_add(1);
        }
    }
    (eligible, voted)
}

async fn list_all_neurons(governance: Principal) -> Result<Vec<Neuron>, ApiError> {
    let mut neurons = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_PAGES {
        let response: ListNeuronsResponse = Call::bounded_wait(governance, "list_neurons")
            .with_arg(ListNeuronsRequest {
                of_principal: None,
                limit: PAGE_SIZE,
                start_page_at: cursor.clone(),
            })
            .await
            .map_err(|error| ApiError::Pending(format!("SNS list_neurons failed: {error:?}")))?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("SNS list_neurons decode failed: {error:?}"))
            })?;
        let count = response.neurons.len();
        if count > PAGE_SIZE as usize {
            return Err(ApiError::Invalid("SNS neuron page exceeds bound".into()));
        }
        let next = response.neurons.last().and_then(|neuron| neuron.id.clone());
        if count == PAGE_SIZE as usize && next == cursor {
            return Err(ApiError::Invalid(
                "SNS neuron pagination did not progress".into(),
            ));
        }
        neurons.extend(response.neurons);
        if count < PAGE_SIZE as usize {
            return Ok(neurons);
        }
        cursor = next;
    }
    Err(ApiError::Invalid(
        "SNS neuron evidence exceeds bounded pages".into(),
    ))
}

async fn list_all_proposals(governance: Principal) -> Result<Vec<Proposal>, ApiError> {
    let mut proposals = Vec::new();
    let mut before = None;
    for _ in 0..MAX_PAGES {
        let response: ListProposalsResponse = Call::bounded_wait(governance, "list_proposals")
            .with_arg(ListProposalsRequest {
                include_reward_status: Vec::new(),
                before_proposal: before.clone(),
                limit: PAGE_SIZE,
                exclude_type: Vec::new(),
                include_status: Vec::new(),
                include_topics: None,
            })
            .await
            .map_err(|error| ApiError::Pending(format!("SNS list_proposals failed: {error:?}")))?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("SNS list_proposals decode failed: {error:?}"))
            })?;
        let count = response.proposals.len();
        let next = response
            .proposals
            .last()
            .and_then(|proposal| proposal.id.clone());
        if count > PAGE_SIZE as usize || (count == PAGE_SIZE as usize && next == before) {
            return Err(ApiError::Invalid(
                "SNS proposal pagination is invalid".into(),
            ));
        }
        proposals.extend(response.proposals);
        if count < PAGE_SIZE as usize {
            return Ok(proposals);
        }
        before = next;
    }
    Err(ApiError::Invalid(
        "SNS proposal evidence exceeds bounded pages".into(),
    ))
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

pub(crate) fn validate_settlement(
    settlement: &TwoWeekSettlement,
    config: &crate::state::StreamConfig,
) -> Result<(), String> {
    let recipients = settlement
        .recipients
        .iter()
        .try_fold(0u128, |sum, recipient| sum.checked_add(recipient.io_e8s))
        .ok_or("two-week recipient total overflow")?;
    if settlement.backed_io_pool_e8s
        != recipients
            .checked_add(settlement.dust_io_e8s)
            .ok_or("two-week settlement total overflow")?
        || settlement.recipient_index as usize > settlement.recipients.len()
        || settlement.forfeited_io_e8s > settlement.dust_io_e8s
    {
        return Err("two-week reward settlement totals are inconsistent".into());
    }
    let reserve = config.io_reserve.canonical()?.subaccount;
    for recipient in &settlement.recipients {
        if recipient.sns_neuron_id.len() != 32
            || recipient.io_e8s == 0
            || recipient.destination.owner != config.sns_governance
        {
            return Err("two-week reward recipient is inconsistent".into());
        }
        let Some(attempt) = &recipient.transfer else {
            continue;
        };
        attempt.validate()?;
        if !matches!(
            &attempt.intent,
            OwnTransferIntent::Icrc1 {
                ledger,
                from_subaccount,
                to,
                amount,
                fee,
                ..
            } if *ledger == config.io_ledger
                && *from_subaccount == reserve
                && to.effective_eq(&recipient.destination)?
                && *amount == recipient.io_e8s
                && *fee == config.expected_io_fee_e8s
        ) {
            return Err("two-week reward transfer intent is inconsistent".into());
        }
    }
    Ok(())
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
    let pre_liquid = canonical
        .liquid_icp_e8s
        .checked_sub(operation.context.request.liquid_amount_e8s)
        .ok_or_else(|| ApiError::Invalid("two-week receipt exceeds liquid ICP".into()))?;
    let excluded = canonical
        .excluded_io_balances
        .iter()
        .try_fold(0u128, |sum, (_, value)| sum.checked_add(*value))
        .ok_or_else(|| ApiError::Invalid("excluded IO overflow".into()))?;
    let redeemable = canonical
        .total_supply_e8s
        .checked_sub(canonical.reserve_io_e8s)
        .and_then(|value| value.checked_sub(excluded))
        .ok_or_else(|| ApiError::Invalid("invalid redeemable IO supply".into()))?;
    let pool = io_core_model::backed_io(
        operation.context.request.liquid_amount_e8s,
        pre_liquid,
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
                transfer: None,
                refreshed: false,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let fees = snapshot
        .config
        .expected_io_fee_e8s
        .checked_mul(recipients.len() as u128)
        .ok_or_else(|| ApiError::Invalid("reward fee total overflow".into()))?;
    let issued = recipients
        .iter()
        .try_fold(0u128, |sum, recipient| sum.checked_add(recipient.io_e8s))
        .ok_or_else(|| ApiError::Invalid("reward issue total overflow".into()))?;
    if canonical.reserve_io_e8s < issued.saturating_add(fees) {
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
        dust_io_e8s: allocation.dust_e8s,
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
    match &recipient.transfer {
        None => submit_recipient(operation, now).await,
        Some(transfer) => match transfer.state {
            TransferState::Prepared | TransferState::Submitted { .. } => {
                submit_recipient(operation, now).await
            }
            TransferState::Succeeded { .. } if !recipient.refreshed => {
                refresh_recipient(operation).await
            }
            TransferState::Succeeded { .. } => Err(ApiError::Invalid(
                "refreshed recipient index was not advanced".into(),
            )),
            TransferState::Stuck { ref reason } => {
                Ok(crate::api::LiquidReceiptProgress::Stuck(reason.clone()))
            }
        },
    }
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
                if now.saturating_sub(last_submitted_at) < config.retry_delay_nanos {
                    return Ok(crate::api::LiquidReceiptProgress::Settling);
                }
                if now
                    >= attempt
                        .intent
                        .created_at_time()
                        .saturating_add(config.ledger_deduplication_window_nanos)
                {
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
    let recipient = &settlement.recipients[index];
    let response: ManageNeuronResponse =
        Call::bounded_wait(state::read().config.sns_governance, "manage_neuron")
            .with_arg(ManageNeuronRequest {
                subaccount: recipient.sns_neuron_id.clone(),
                command: Some(ManageNeuronCommand::ClaimOrRefresh(ClaimOrRefresh {
                    by: Some(ClaimBy::NeuronId(Empty {})),
                })),
            })
            .await
            .map_err(|error| ApiError::Pending(format!("SNS reward refresh ambiguous: {error:?}")))?
            .candid()
            .map_err(|error| {
                ApiError::Invalid(format!("SNS reward refresh decode failed: {error:?}"))
            })?;
    if active_two_week()? != operation {
        return Err(ApiError::Busy);
    }
    match response.command {
        Some(ManageNeuronCommandResponse::ClaimOrRefresh(value))
            if value.refreshed_neuron_id.as_ref().map(|id| &id.id)
                == Some(&recipient.sns_neuron_id) => {}
        Some(ManageNeuronCommandResponse::Error(error)) => {
            return Err(ApiError::Invalid(format!(
                "SNS reward refresh rejected ({}): {}",
                error.error_type, error.error_message
            )))
        }
        _ => {
            return Err(ApiError::Invalid(
                "SNS reward refresh returned wrong result".into(),
            ))
        }
    }
    let mut replacement = operation.clone();
    let settlement = replacement
        .settlement
        .as_mut()
        .expect("validated settlement");
    settlement.recipients[index].refreshed = true;
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
        .saturating_add(settlement.dust_io_e8s)
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
        dust_io_e8s: settlement.dust_io_e8s,
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

    fn id(byte: u8) -> Vec<u8> {
        vec![byte; 32]
    }

    fn neuron(byte: u8, stake: u64, delay: u64) -> Neuron {
        Neuron {
            id: Some(NeuronId { id: id(byte) }),
            cached_neuron_stake_e8s: stake,
            dissolve_state: Some(DissolveState::DissolveDelaySeconds(delay)),
        }
    }

    fn proposal(decided: u64, votes: &[(u8, i32)]) -> Proposal {
        Proposal {
            id: Some(ProposalId { id: decided }),
            ballots: votes
                .iter()
                .map(|(neuron, vote)| (crate::transfer::hex(&id(*neuron)), Ballot { vote: *vote }))
                .collect(),
            decided_timestamp_seconds: decided,
            is_eligible_for_rewards: true,
        }
    }

    #[test]
    fn eligibility_is_exact_two_week_non_dissolving_positive_stake() {
        assert!(canonical_eligible(&neuron(
            1,
            1,
            io_core_model::TWO_WEEK_SECONDS
        )));
        assert!(!canonical_eligible(&neuron(
            1,
            0,
            io_core_model::TWO_WEEK_SECONDS
        )));
        assert!(!canonical_eligible(&neuron(
            1,
            1,
            io_core_model::TWO_WEEK_SECONDS + 1
        )));
        let mut dissolving = neuron(1, 1, io_core_model::TWO_WEEK_SECONDS);
        dissolving.dissolve_state = Some(DissolveState::WhenDissolvedTimestampSeconds(10));
        assert!(!canonical_eligible(&dissolving));
    }

    #[test]
    fn capture_freezes_stake_and_canonical_staking_account() {
        let governance = Principal::from_slice(&[9; 29]);
        let members = eligible_members(
            governance,
            &[neuron(1, 123, io_core_model::TWO_WEEK_SECONDS)],
        )
        .unwrap();
        assert_eq!(members[0].frozen_stake_e8s, 123);
        assert_eq!(members[0].observed_stake_e8s, 123);
        assert_eq!(members[0].account.owner, governance);
        assert_eq!(members[0].account.subaccount, Some(id(1)));
    }

    #[test]
    fn exact_interval_counts_direct_and_followed_but_not_late_or_non_vote() {
        let start = 100;
        let end = start + io_core_model::TWO_WEEK_SECONDS;
        let proposals = vec![
            proposal(start, &[(1, 1), (2, 3)]),
            proposal(end, &[(1, 2), (2, 4)]),
            proposal(end + 1, &[(1, 1), (2, 1)]),
        ];
        assert_eq!(participation(&id(1), start, end, &proposals), (2, 2));
        assert_eq!(participation(&id(2), start, end, &proposals), (2, 2));
        assert_eq!(participation(&id(3), start, end, &proposals), (2, 0));
    }
}
