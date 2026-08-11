#![cfg(target_family = "wasm")]

use crate::model::*;
use candid::{CandidType, Nat, Principal};
use io_ledger_types::Account;
use serde::Deserialize;
use std::collections::BTreeMap;

const CALL_TIMEOUT_SECONDS: u32 = 30;

async fn call<T: for<'de> Deserialize<'de> + CandidType>(
    canister: Principal,
    method: &str,
    arg: impl CandidType,
) -> Result<T, String> {
    ic_cdk::call::Call::bounded_wait(canister, method)
        .change_timeout(CALL_TIMEOUT_SECONDS)
        .with_arg(arg)
        .await
        .map_err(|err| format!("{method} call failed: {err}"))?
        .candid::<T>()
        .map_err(|err| format!("{method} response decode failed: {err}"))
}

fn nat_to_u128(value: Nat, field: &str) -> Result<u128, String> {
    let bytes = value.0.to_bytes_le();
    if bytes.len() > 16 {
        return Err(format!("{field} exceeds u128"));
    }
    let mut fixed = [0u8; 16];
    fixed[..bytes.len()].copy_from_slice(&bytes);
    Ok(u128::from_le_bytes(fixed))
}

pub async fn protocol(
    config: &ObservationConfig,
    generation: u64,
    now: u64,
) -> Result<ProtocolSnapshot, String> {
    let total = nat_to_u128(
        call(config.sns_ledger, "icrc1_total_supply", ()).await?,
        "total IO supply",
    )?;
    let reserve = balance(config.sns_ledger, &config.protocol_io_reserve).await?;
    let mut excluded = Vec::with_capacity(config.excluded_io_accounts.len());
    for named in &config.excluded_io_accounts {
        excluded.push(balance(config.sns_ledger, &named.account).await?);
    }
    let liquid = balance(config.icp_ledger, &config.liquid_icp_reserve).await?;
    coherent_protocol_snapshot(generation, total, reserve, &excluded, liquid, now)
}

async fn balance(ledger: Principal, account: &Account) -> Result<u128, String> {
    nat_to_u128(
        call(ledger, "icrc1_balance_of", account.clone()).await?,
        "ledger balance",
    )
}

#[derive(CandidType, Deserialize)]
struct RawStreamStatus {
    lifecycle: Lifecycle,
    operation_kind: Option<String>,
    operation_phase: Option<String>,
    latest_entitlement_batch_generation: u64,
    latest_processed_reward_event: Option<RewardEventId>,
    latest_reward_event_classification: Option<RewardEventClassification>,
    accumulated_eligible_credit: u128,
    accumulated_policy_credit: u128,
    processed_reward_event_count: u64,
    missed_reward_event_count: u64,
    reward_work_due: bool,
    reward_processing_paused: bool,
    governance_parameters_fresh: bool,
    pending_entitlement_batch_eligible_credit: Option<u128>,
    pending_entitlement_batch_policy_credit: Option<u128>,
}

pub async fn stream(config: &ObservationConfig, now: u64) -> Result<StreamStatus, String> {
    let raw: RawStreamStatus = call(config.stream_manager, "get_status", ()).await?;
    for value in [&raw.operation_kind, &raw.operation_phase]
        .into_iter()
        .flatten()
    {
        if value.len() > 128 {
            return Err("stream operation label exceeds 128 bytes".into());
        }
    }
    Ok(StreamStatus {
        lifecycle: raw.lifecycle,
        operation_kind: raw.operation_kind,
        operation_phase: raw.operation_phase,
        latest_entitlement_batch_generation: raw.latest_entitlement_batch_generation,
        latest_processed_reward_event: raw.latest_processed_reward_event,
        latest_reward_event_classification: raw.latest_reward_event_classification,
        accumulated_eligible_credit: raw.accumulated_eligible_credit,
        accumulated_policy_credit: raw.accumulated_policy_credit,
        processed_reward_event_count: raw.processed_reward_event_count,
        missed_reward_event_count: raw.missed_reward_event_count,
        reward_work_due: raw.reward_work_due,
        reward_processing_paused: raw.reward_processing_paused,
        governance_parameters_fresh: raw.governance_parameters_fresh,
        pending_entitlement_batch_eligible_credit: raw.pending_entitlement_batch_eligible_credit,
        pending_entitlement_batch_policy_credit: raw.pending_entitlement_batch_policy_credit,
        observed_at_timestamp_nanos: now,
    })
}

#[derive(CandidType, Deserialize)]
struct RawNnsStatus {
    lifecycle: Lifecycle,
    active_operation: Option<String>,
    two_week_maturity_baseline_reconciled: bool,
    latest_started_two_week_generation: u64,
    latest_completed_two_week_generation: u64,
    latest_two_week_target: Option<TwoWeekTargetObservation>,
    unwinding_child_principal_e8s: u128,
}

pub async fn nns(config: &ObservationConfig, now: u64) -> Result<NnsManagerStatus, String> {
    let raw: RawNnsStatus = call(config.nns_manager, "get_status", ()).await?;
    if raw
        .active_operation
        .as_ref()
        .is_some_and(|value| value.len() > 128)
    {
        return Err("NNS manager operation label exceeds 128 bytes".into());
    }
    Ok(NnsManagerStatus {
        lifecycle: raw.lifecycle,
        active_operation: raw.active_operation,
        two_week_maturity_baseline_reconciled: raw.two_week_maturity_baseline_reconciled,
        latest_started_two_week_generation: raw.latest_started_two_week_generation,
        latest_completed_two_week_generation: raw.latest_completed_two_week_generation,
        latest_two_week_target: raw.latest_two_week_target,
        unwinding_child_principal_e8s: raw.unwinding_child_principal_e8s,
        observed_at_timestamp_nanos: now,
    })
}

#[derive(CandidType, Deserialize)]
enum NeuronInfoResult {
    Ok(RawNeuronInfo),
    Err(GovernanceError),
}

#[derive(CandidType, Deserialize)]
struct GovernanceError {
    error_type: i32,
    error_message: String,
}

#[derive(CandidType, Deserialize)]
struct RawNeuronInfo {
    stake_e8s: u64,
    staked_maturity_e8s_equivalent: Option<u64>,
    dissolve_delay_seconds: u64,
    state: i32,
}

pub async fn nns_governance(
    config: &ObservationConfig,
    now: u64,
) -> Result<NnsGovernanceStatus, String> {
    let build_metadata: String = call(config.nns_governance, "get_build_metadata", ()).await?;
    if build_metadata.len() > 4_096 {
        return Err("NNS Governance build metadata exceeds 4096 bytes".into());
    }
    let mut neurons = Vec::with_capacity(2);
    for (role, neuron_id) in [
        (
            NnsNeuronRole::RewardBacking,
            config.reward_backing_neuron_id,
        ),
        (NnsNeuronRole::TwoYearProtected, config.two_year_neuron_id),
    ] {
        let info = match call(config.nns_governance, "get_neuron_info", neuron_id).await? {
            NeuronInfoResult::Ok(info) => info,
            NeuronInfoResult::Err(error) => {
                return Err(format!(
                    "get_neuron_info({neuron_id}) failed {}: {}",
                    error.error_type, error.error_message
                ));
            }
        };
        neurons.push(NnsNeuronObservation {
            role,
            neuron_id,
            stake_e8s: info.stake_e8s,
            staked_maturity_e8s: info.staked_maturity_e8s_equivalent,
            dissolve_delay_seconds: info.dissolve_delay_seconds,
            state: info.state,
        });
    }
    Ok(NnsGovernanceStatus {
        build_metadata,
        neurons,
        observed_at_timestamp_nanos: now,
    })
}

#[derive(CandidType, Deserialize)]
struct SummaryRequest {
    update_canister_list: Option<bool>,
}

#[derive(CandidType, Deserialize)]
struct RootSummary {
    root: Option<CanisterSummary>,
    swap: Option<CanisterSummary>,
    ledger: Option<CanisterSummary>,
    index: Option<CanisterSummary>,
    governance: Option<CanisterSummary>,
    dapps: Option<Vec<CanisterSummary>>,
    archives: Option<Vec<CanisterSummary>>,
}

#[derive(CandidType, Deserialize)]
struct CanisterSummary {
    status: Option<CanisterStatus>,
    canister_id: Option<Principal>,
}

#[derive(CandidType, Deserialize)]
struct CanisterStatus {
    settings: Option<CanisterSettings>,
    module_hash: Option<Vec<u8>>,
}

#[derive(CandidType, Deserialize)]
struct CanisterSettings {
    controllers: Vec<Principal>,
}

pub struct TopologyObservation {
    pub canisters: Vec<CanisterObservation>,
    pub archives: Vec<Principal>,
}

pub async fn topology(config: &ObservationConfig, now: u64) -> Result<TopologyObservation, String> {
    let summary: RootSummary = call(
        config.sns_root,
        "get_sns_canisters_summary",
        SummaryRequest {
            update_canister_list: Some(false),
        },
    )
    .await?;
    if summary.dapps.as_ref().is_some_and(|items| items.len() > 64)
        || summary
            .archives
            .as_ref()
            .is_some_and(|items| items.len() > 64)
    {
        return Err("SNS Root summary exceeds bounded canister inventory".into());
    }
    let archives = summary
        .archives
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter_map(|item| item.canister_id)
        .collect::<Vec<_>>();
    let all = [
        summary.root,
        summary.swap,
        summary.ledger,
        summary.index,
        summary.governance,
    ]
    .into_iter()
    .flatten()
    .chain(summary.dapps.unwrap_or_default())
    .chain(summary.archives.unwrap_or_default())
    .filter_map(|item| item.canister_id.map(|id| (id, item.status)))
    .collect::<BTreeMap<_, _>>();
    let mut canisters = config
        .expected_modules
        .iter()
        .map(|expected| {
            let status = all.get(&expected.canister_id).and_then(Option::as_ref);
            if status
                .and_then(|value| value.settings.as_ref())
                .is_some_and(|settings| settings.controllers.len() > 16)
            {
                return Err("canister controller inventory exceeds 16".to_string());
            }
            let observed = status.and_then(|status| status.module_hash.clone());
            Ok(CanisterObservation {
                role: expected.role,
                canister_id: expected.canister_id,
                expected_module_hash: expected.wasm_sha256.clone(),
                module_match: match observed.as_ref() {
                    Some(hash) if hash == &expected.wasm_sha256 => ModuleMatch::Matching,
                    Some(_) => ModuleMatch::Mismatch,
                    None if all.contains_key(&expected.canister_id) => ModuleMatch::Unavailable,
                    None => ModuleMatch::Unknown,
                },
                observed_module_hash: observed,
                controllers: status
                    .and_then(|status| status.settings.as_ref())
                    .map(|settings| settings.controllers.clone()),
                observed_at_timestamp_nanos: now,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    canisters.sort_by_key(|item| item.role);
    Ok(TopologyObservation {
        canisters,
        archives,
    })
}

#[derive(CandidType, Deserialize)]
struct NervousSystemParameters {
    max_number_of_neurons: Option<u64>,
    voting_rewards_parameters: Option<VotingRewardsParameters>,
}

#[derive(CandidType, Deserialize)]
struct VotingRewardsParameters {
    initial_reward_rate_basis_points: Option<u64>,
    final_reward_rate_basis_points: Option<u64>,
}

#[derive(CandidType, Deserialize)]
struct RewardEvent {
    round: u64,
    end_timestamp_seconds: Option<u64>,
    settled_proposals: Vec<ProposalId>,
}

#[derive(CandidType, Deserialize)]
struct ProposalId {
    id: u64,
}

pub async fn sns(
    config: &ObservationConfig,
    topology: Option<&TopologyObservation>,
    now: u64,
) -> Result<SnsStatus, String> {
    let parameters: NervousSystemParameters =
        call(config.sns_governance, "get_nervous_system_parameters", ()).await?;
    let reward: RewardEvent = call(config.sns_governance, "get_latest_reward_event", ()).await?;
    let module = topology.and_then(|topology| {
        topology
            .canisters
            .iter()
            .find(|item| item.role == CanisterRole::SnsGovernance)
    });
    let reward_share_capability = match (
        config.reward_share_capable_governance_sha256.as_ref(),
        module.map(|item| item.module_match),
    ) {
        (Some(_), Some(ModuleMatch::Matching)) => CapabilityState::ExpectedGovernanceModuleMatching,
        (Some(_), Some(ModuleMatch::Mismatch)) => CapabilityState::ModuleMismatch,
        _ => CapabilityState::Unavailable,
    };
    let voting = parameters.voting_rewards_parameters;
    Ok(SnsStatus {
        max_number_of_neurons: parameters.max_number_of_neurons,
        native_initial_reward_rate_basis_points: voting
            .as_ref()
            .and_then(|value| value.initial_reward_rate_basis_points),
        native_final_reward_rate_basis_points: voting
            .as_ref()
            .and_then(|value| value.final_reward_rate_basis_points),
        latest_reward_event_round: Some(reward.round),
        latest_reward_event_end_timestamp_seconds: reward.end_timestamp_seconds,
        settled_proposal_count: Some(reward.settled_proposals.len() as u64),
        reward_share_capability,
        archive_canisters: topology
            .map(|value| value.archives.clone())
            .unwrap_or_default(),
        observed_at_timestamp_nanos: now,
    })
}

#[derive(CandidType, Deserialize)]
struct IndexRawStatus {
    num_blocks_synced: Nat,
}

#[derive(CandidType)]
struct HistoryArgs {
    account: Account,
    start: Option<Nat>,
    max_results: Nat,
}

#[derive(CandidType, Deserialize)]
enum HistoryResult {
    Ok(HistoryPage),
    Err(HistoryError),
}

#[derive(CandidType, Deserialize)]
struct HistoryError {
    message: String,
}

#[derive(CandidType, Deserialize)]
struct HistoryPage {
    balance: Nat,
    transactions: Vec<TransactionWithId>,
    oldest_tx_id: Option<Nat>,
}

#[derive(CandidType, Deserialize)]
struct TransactionWithId {
    id: Nat,
    transaction: CompactTransaction,
}

#[derive(CandidType, Deserialize)]
struct CompactTransaction {
    kind: String,
    timestamp: u64,
    transfer: Option<CompactTransfer>,
    mint: Option<CompactMint>,
    burn: Option<CompactBurn>,
}

#[derive(CandidType, Deserialize)]
struct CompactTransfer {
    from: Account,
    to: Account,
    amount: Nat,
}

#[derive(CandidType, Deserialize)]
struct CompactMint {
    to: Account,
    amount: Nat,
}

#[derive(CandidType, Deserialize)]
struct CompactBurn {
    from: Account,
    amount: Nat,
}

pub async fn index(config: &ObservationConfig, now: u64) -> Result<IndexStatus, String> {
    let status: IndexRawStatus = call(config.sns_index, "status", ()).await?;
    let mut accounts = Vec::with_capacity(config.history_accounts.len());
    for named in &config.history_accounts {
        let result: HistoryResult = call(
            config.sns_index,
            "get_account_transactions",
            HistoryArgs {
                account: named.account.clone(),
                start: None,
                max_results: Nat::from(MAX_RECENT_TRANSACTIONS as u64),
            },
        )
        .await?;
        let page = match result {
            HistoryResult::Ok(page) => page,
            HistoryResult::Err(error) => return Err(error.message),
        };
        let mut transactions = Vec::with_capacity(page.transactions.len());
        for item in page.transactions {
            if item.transaction.kind.len() > 64 {
                return Err("index transaction kind exceeds 64 bytes".into());
            }
            let (amount, from, to) = if let Some(tx) = item.transaction.transfer {
                (Some(tx.amount), Some(tx.from), Some(tx.to))
            } else if let Some(tx) = item.transaction.mint {
                (Some(tx.amount), None, Some(tx.to))
            } else if let Some(tx) = item.transaction.burn {
                (Some(tx.amount), Some(tx.from), None)
            } else {
                (None, None, None)
            };
            transactions.push(RecentTransaction {
                block_index: nat_to_u128(item.id, "index transaction ID")?,
                kind: item.transaction.kind,
                amount_e8s: amount
                    .map(|value| nat_to_u128(value, "index transaction amount"))
                    .transpose()?,
                from,
                to,
                timestamp_nanos: item.transaction.timestamp,
            });
        }
        accounts.push(AccountHistoryObservation {
            name: named.name.clone(),
            account: named.account.clone(),
            index_balance_e8s: nat_to_u128(page.balance, "index Account balance")?,
            newest_transaction_id: transactions.first().map(|item| item.block_index),
            oldest_transaction_id: page
                .oldest_tx_id
                .map(|value| nat_to_u128(value, "oldest index transaction ID"))
                .transpose()?,
            transactions,
        });
    }
    Ok(IndexStatus {
        num_blocks_synced: nat_to_u128(status.num_blocks_synced, "index block height")?,
        accounts,
        observed_at_timestamp_nanos: now,
    })
}
