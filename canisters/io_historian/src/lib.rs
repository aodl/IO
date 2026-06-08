use candid::CandidType;
use io_ledger_types::{
    AccountHistoryPageOrder, AccountHistoryScanState, IndexTransaction, LedgerKind,
};
use io_reward_policy::{eligible, participation_ratio, NeuronSnapshot};
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::BTreeMap;

pub const HISTORIAN_SCHEMA_VERSION: u32 = 1;
pub const MAX_STREAM_HISTORY: usize = 256;
pub const MAX_REDEMPTION_HISTORY: usize = 256;
pub const MAX_REWARD_HISTORY: usize = 256;
pub const MAX_NNS_LIFECYCLE_HISTORY: usize = 256;
pub const MAX_INDEX_HEALTH: usize = 32;
pub const MAX_CANISTER_STATUS: usize = 32;
pub const MAX_ARTIFACT_STATUS: usize = 32;
pub const MAX_GOVERNANCE_NEURON_SUMMARIES: usize = 512;
pub const MAX_PAGE_LIMIT: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum DataAvailability {
    Observed,
    Missing,
    NotApplicable,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DataCompleteness {
    pub total_io_supply: DataAvailability,
    pub protocol_reserve_io: DataAvailability,
    pub non_redeemable_governance_io: DataAvailability,
    pub redeemable_io_supply: DataAvailability,
    pub liquid_icp_reserve: DataAvailability,
    pub redemption_rate: DataAvailability,
    pub two_year_nns_principal: DataAvailability,
}

impl Default for DataCompleteness {
    fn default() -> Self {
        Self {
            total_io_supply: DataAvailability::Missing,
            protocol_reserve_io: DataAvailability::Missing,
            non_redeemable_governance_io: DataAvailability::Missing,
            redeemable_io_supply: DataAvailability::Missing,
            liquid_icp_reserve: DataAvailability::Missing,
            redemption_rate: DataAvailability::Missing,
            two_year_nns_principal: DataAvailability::Missing,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ProtocolSnapshot {
    pub total_io_supply_e8s: Option<u128>,
    pub protocol_reserve_io_e8s: Option<u128>,
    pub non_redeemable_governance_io_e8s: Option<u128>,
    pub redeemable_io_supply_e8s: Option<u128>,
    pub liquid_icp_reserve_e8s: Option<u128>,
    pub two_year_nns_principal_e8s: Option<u128>,
    pub redemption_rate: Option<RedemptionRateSnapshot>,
    pub last_updated_timestamp_nanos: Option<u64>,
    pub completeness: DataCompleteness,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ReserveSnapshot {
    pub liquid_icp_reserve_e8s: Option<u128>,
    pub two_year_nns_principal_e8s: Option<u128>,
    pub last_updated_timestamp_nanos: Option<u64>,
    pub completeness: DataCompleteness,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SupplySnapshot {
    pub total_io_supply_e8s: Option<u128>,
    pub protocol_reserve_io_e8s: Option<u128>,
    pub non_redeemable_governance_io_e8s: Option<u128>,
    pub redeemable_io_supply_e8s: Option<u128>,
    pub last_updated_timestamp_nanos: Option<u64>,
    pub completeness: DataCompleteness,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RedemptionRateSnapshot {
    pub liquid_icp_reserve_e8s: u128,
    pub redeemable_io_supply_e8s: u128,
    pub liquid_icp_per_io_e8s_numerator: u128,
    pub liquid_icp_per_io_e8s_denominator: u128,
    pub last_updated_timestamp_nanos: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct ProtocolObservation {
    pub total_io_supply_e8s: Option<u128>,
    pub protocol_reserve_io_e8s: Option<u128>,
    pub non_redeemable_governance_io_e8s: Option<u128>,
    pub liquid_icp_reserve_e8s: Option<u128>,
    pub two_year_nns_principal_e8s: Option<u128>,
    pub observed_at_timestamp_nanos: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum PublicStreamKind {
    JupiterFaucet,
    TwoYearMaturity,
    TwoWeekMaturity,
    UnknownIcpDeposit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum PublicRecipientPolicy {
    JupiterFaucet,
    EligibleIoSnsNeurons,
    None,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum PublicOperationPhase {
    Observed,
    Previewed,
    AwaitingIoIssuance,
    AwaitingIcpPayout,
    AwaitingIoReturn,
    PartiallyDistributed,
    Completed,
    FailedRetryable,
    FailedTerminal,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ObservedLedgerFlow {
    pub ledger_kind: LedgerKind,
    pub ledger_principal_text: Option<String>,
    pub block_index: u64,
    pub amount_e8s: u128,
    pub from_account: Option<String>,
    pub to_account: Option<String>,
    pub memo: Option<Vec<u8>>,
    pub timestamp_nanos: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StreamHistoryRecord {
    pub record_id: String,
    pub source_ledger: String,
    pub source_block_index: Option<u64>,
    pub stream_kind: PublicStreamKind,
    pub amount_e8s: u128,
    pub recipient_policy: PublicRecipientPolicy,
    pub io_issued_e8s: Option<u128>,
    pub phase: PublicOperationPhase,
    pub timestamp_nanos: Option<u64>,
    pub memo_label: Option<String>,
    pub safe_subaccount_label: Option<String>,
    pub terminal_rejection_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RedemptionHistoryRecord {
    pub record_id: String,
    pub io_burn_or_transfer_block: Option<u64>,
    pub user_account: Option<String>,
    pub io_amount_e8s: u128,
    pub icp_payout_amount_e8s: Option<u128>,
    pub gross_icp_payout_e8s: Option<u128>,
    pub icp_payout_fee_e8s: Option<u128>,
    pub net_user_icp_payout_e8s: Option<u128>,
    pub io_return_fee_e8s: Option<u128>,
    pub icp_payout_block: Option<u64>,
    pub io_return_block: Option<u64>,
    pub phase: PublicOperationPhase,
    pub timestamp_nanos: Option<u64>,
    pub retry_count: u32,
    pub retry_status: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardDistributionRecord {
    pub record_id: String,
    pub epoch_start_timestamp_nanos: Option<u64>,
    pub epoch_end_timestamp_nanos: Option<u64>,
    pub participation_summary_id: Option<String>,
    pub recipient_neuron_id: Option<u64>,
    pub recipient_account: Option<String>,
    pub eligible_stake_e8s: Option<u128>,
    pub reward_amount_e8s: u128,
    pub dust_unissued_e8s: Option<u128>,
    pub payout_block: Option<u64>,
    pub status: PublicOperationPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsLifecycleKind {
    TwoYearMaturityDisbursement,
    TwoWeekMaturityDisbursement,
    TwoWeekPoolRestake,
    TwoWeekPoolSplit,
    TwoWeekPoolStartDissolving,
    TwoWeekPoolStopDissolving,
    TwoWeekPoolMergeBack,
    TwoWeekUnwindPrincipalDisbursement,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsLifecycleSummary {
    pub record_id: String,
    pub kind: NnsLifecycleKind,
    pub neuron_id: Option<u64>,
    pub amount_e8s: Option<u128>,
    pub phase: PublicOperationPhase,
    pub timestamp_nanos: Option<u64>,
    pub retry_count: u32,
    pub safe_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IndexHealthSummary {
    pub record_id: String,
    pub ledger_kind: LedgerKind,
    pub account_label: String,
    pub latest_cursor: Option<u64>,
    pub oldest_cursor: Option<u64>,
    pub backfill_complete: bool,
    pub page_order: Option<AccountHistoryPageOrder>,
    pub last_success_timestamp_nanos: Option<u64>,
    pub unreadable_count: u64,
    pub invariant_broken_count: u64,
    pub lag_suspected: bool,
    pub page_cap_reached: bool,
    pub scan_incomplete: bool,
    pub last_observed_newest_tx_id: Option<u64>,
    pub last_observed_balance_e8s: Option<u128>,
    pub num_blocks_synced: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct GovernanceExcludedCount {
    pub reason: String,
    pub count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct GovernanceNeuronParticipation {
    pub neuron_id: u64,
    pub eligible_stake_e8s: u128,
    pub eligible_seconds: u64,
    pub eligible_closed_proposals: u64,
    pub voted_closed_proposals: u64,
    pub participation_numerator: u128,
    pub participation_denominator: u128,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct GovernanceParticipationSnapshot {
    pub sns_eligible_neuron_count: u64,
    pub sns_excluded_neuron_count_by_reason: Vec<GovernanceExcludedCount>,
    pub total_eligible_stake_e8s: u128,
    pub proposal_epoch_start: Option<u64>,
    pub proposal_epoch_end: Option<u64>,
    pub counted_proposals: u64,
    pub pending_nns_operation_count: Option<u64>,
    pub nns_lifecycle_status_summary: Option<String>,
    pub last_governance_snapshot_timestamp_nanos: Option<u64>,
    pub neuron_participation: Vec<GovernanceNeuronParticipation>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct GovernanceObservation {
    pub neurons: Vec<GovernanceNeuronObservation>,
    pub proposal_epoch_start: Option<u64>,
    pub proposal_epoch_end: Option<u64>,
    pub counted_proposals: u64,
    pub pending_nns_operation_count: Option<u64>,
    pub nns_lifecycle_status_summary: Option<String>,
    pub observed_at_timestamp_nanos: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct GovernanceNeuronObservation {
    pub neuron_id: u64,
    pub staked_io_e8s: u128,
    pub eligible_seconds: u64,
    pub eligible_closed_proposals: u64,
    pub voted_closed_proposals: u64,
    pub is_genesis_governance_neuron: bool,
    pub is_protocol_owned: bool,
    pub is_dissolving: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ReleaseManifestObservation {
    pub schema_version: u32,
    pub build_profile: String,
    pub target: String,
    pub git_commit: Option<String>,
    pub artifacts: Vec<ReleaseManifestArtifact>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ReleaseManifestArtifact {
    pub canister: String,
    pub raw_wasm_path: String,
    pub raw_wasm_sha256: String,
    pub raw_wasm_bytes: u64,
    pub gz_wasm_path: String,
    pub gz_wasm_sha256: String,
    pub gz_wasm_bytes: u64,
    pub build_profile: String,
    pub target: String,
    pub git_commit: Option<String>,
}

#[cfg(test)]
impl From<&io_sns_lifecycle::ArtifactManifest> for ReleaseManifestObservation {
    fn from(value: &io_sns_lifecycle::ArtifactManifest) -> Self {
        Self {
            schema_version: value.schema_version,
            build_profile: value.build_profile.clone(),
            target: value.target.clone(),
            git_commit: value.git_commit.clone(),
            artifacts: value
                .artifacts
                .iter()
                .map(|entry| ReleaseManifestArtifact {
                    canister: entry.canister.clone(),
                    raw_wasm_path: entry.raw_wasm_path.clone(),
                    raw_wasm_sha256: entry.raw_wasm_sha256.clone(),
                    raw_wasm_bytes: entry.raw_wasm_bytes,
                    gz_wasm_path: entry.gz_wasm_path.clone(),
                    gz_wasm_sha256: entry.gz_wasm_sha256.clone(),
                    gz_wasm_bytes: entry.gz_wasm_bytes,
                    build_profile: entry.build_profile.clone(),
                    target: entry.target.clone(),
                    git_commit: entry.git_commit.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ArtifactMatchStatus {
    Unknown,
    Matching,
    Mismatch,
    Unobserved,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CanisterArtifactStatus {
    pub canister_name: String,
    pub expected_canister_principal_text: Option<String>,
    pub raw_wasm_sha256: Option<String>,
    pub gz_wasm_sha256: Option<String>,
    pub artifact_byte_size: Option<u64>,
    pub gz_artifact_byte_size: Option<u64>,
    pub build_profile: Option<String>,
    pub target: Option<String>,
    pub git_commit: Option<String>,
    pub observed_module_hash: Option<String>,
    pub status: ArtifactMatchStatus,
    pub last_checked_timestamp_nanos: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct HistorianIngestionStatus {
    pub schema_version: u32,
    pub stream_record_count: u64,
    pub redemption_record_count: u64,
    pub reward_record_count: u64,
    pub nns_lifecycle_record_count: u64,
    pub index_health_record_count: u64,
    pub artifact_status_count: u64,
    pub canister_status_count: u64,
    pub last_ingested_timestamp_nanos: Option<u64>,
    pub retained_record_limits: RetentionLimits,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RetentionLimits {
    pub stream_history: u64,
    pub redemption_history: u64,
    pub reward_history: u64,
    pub nns_lifecycle_history: u64,
    pub index_health: u64,
    pub artifact_status: u64,
    pub canister_status: u64,
    pub governance_neuron_summaries: u64,
    pub max_page_limit: u64,
}

impl Default for RetentionLimits {
    fn default() -> Self {
        Self {
            stream_history: MAX_STREAM_HISTORY as u64,
            redemption_history: MAX_REDEMPTION_HISTORY as u64,
            reward_history: MAX_REWARD_HISTORY as u64,
            nns_lifecycle_history: MAX_NNS_LIFECYCLE_HISTORY as u64,
            index_health: MAX_INDEX_HEALTH as u64,
            artifact_status: MAX_ARTIFACT_STATUS as u64,
            canister_status: MAX_CANISTER_STATUS as u64,
            governance_neuron_summaries: MAX_GOVERNANCE_NEURON_SUMMARIES as u64,
            max_page_limit: MAX_PAGE_LIMIT as u64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PublicStatus {
    pub version: String,
    pub model: String,
    pub schema_version: u32,
    pub ingestion: HistorianIngestionStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PublicDashboardState {
    pub status: PublicStatus,
    pub protocol: ProtocolSnapshot,
    pub reserve: ReserveSnapshot,
    pub supply: SupplySnapshot,
    pub redemption_rate: Option<RedemptionRateSnapshot>,
    pub index_health: Vec<IndexHealthSummary>,
    pub governance: GovernanceParticipationSnapshot,
    pub release_artifacts: Vec<CanisterArtifactStatus>,
    pub canister_status: Vec<CanisterArtifactStatus>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ListStreamsRequest {
    pub start_after: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ListStreamsResponse {
    pub records: Vec<StreamHistoryRecord>,
    pub next_start_after: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ListRedemptionsRequest {
    pub start_after: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ListRedemptionsResponse {
    pub records: Vec<RedemptionHistoryRecord>,
    pub next_start_after: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ListRewardsRequest {
    pub start_after: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ListRewardsResponse {
    pub records: Vec<RewardDistributionRecord>,
    pub next_start_after: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ListNnsLifecycleEventsRequest {
    pub start_after: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ListNnsLifecycleEventsResponse {
    pub records: Vec<NnsLifecycleSummary>,
    pub next_start_after: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ListGovernanceParticipationRequest {
    pub start_after_neuron_id: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ListGovernanceParticipationResponse {
    pub records: Vec<GovernanceNeuronParticipation>,
    pub next_start_after_neuron_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StableState {
    pub schema_version: u32,
    pub protocol: ProtocolSnapshot,
    pub streams: Vec<StreamHistoryRecord>,
    pub redemptions: Vec<RedemptionHistoryRecord>,
    pub rewards: Vec<RewardDistributionRecord>,
    pub nns_lifecycle: Vec<NnsLifecycleSummary>,
    pub index_health: Vec<IndexHealthSummary>,
    pub governance: GovernanceParticipationSnapshot,
    pub release_artifacts: Vec<CanisterArtifactStatus>,
    pub canister_status: Vec<CanisterArtifactStatus>,
    pub last_ingested_timestamp_nanos: Option<u64>,
}

impl Default for StableState {
    fn default() -> Self {
        Self {
            schema_version: HISTORIAN_SCHEMA_VERSION,
            protocol: protocol_snapshot_from_observation(ProtocolObservation::default()),
            streams: Vec::new(),
            redemptions: Vec::new(),
            rewards: Vec::new(),
            nns_lifecycle: Vec::new(),
            index_health: Vec::new(),
            governance: GovernanceParticipationSnapshot::default(),
            release_artifacts: Vec::new(),
            canister_status: Vec::new(),
            last_ingested_timestamp_nanos: None,
        }
    }
}

thread_local! {
    static STATE: RefCell<StableState> = RefCell::new(StableState::default());
}

fn page_limit(limit: Option<u64>) -> usize {
    limit
        .unwrap_or(MAX_PAGE_LIMIT as u64)
        .min(MAX_PAGE_LIMIT as u64)
        .max(1) as usize
}

#[cfg_attr(not(any(test, debug_assertions)), allow(dead_code))]
fn upsert_bounded<T, F>(records: &mut Vec<T>, key: F, max_len: usize, record: T)
where
    F: Fn(&T) -> &str,
{
    let record_key = key(&record).to_string();
    if let Some(position) = records
        .iter()
        .position(|existing| key(existing) == record_key)
    {
        records[position] = record;
    } else {
        records.push(record);
    }
    records.sort_by(|a, b| key(a).cmp(key(b)));
    while records.len() > max_len {
        records.remove(0);
    }
}

fn page_by_key<T, F>(
    records: &[T],
    start_after: Option<String>,
    limit: Option<u64>,
    key: F,
) -> (Vec<T>, Option<String>)
where
    T: Clone,
    F: Fn(&T) -> &str,
{
    let limit = page_limit(limit);
    let start_after = start_after.as_deref();
    let start = records
        .iter()
        .position(|record| {
            start_after
                .map(|cursor| key(record) > cursor)
                .unwrap_or(true)
        })
        .unwrap_or(records.len());
    let page = records
        .iter()
        .skip(start)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let next = if start + page.len() < records.len() {
        page.last().map(|record| key(record).to_string())
    } else {
        None
    };
    (page, next)
}

#[cfg_attr(not(any(test, debug_assertions)), allow(dead_code))]
fn set_last_ingested(state: &mut StableState, timestamp: Option<u64>) {
    state.last_ingested_timestamp_nanos = timestamp.or(state.last_ingested_timestamp_nanos);
}

pub fn protocol_snapshot_from_observation(observation: ProtocolObservation) -> ProtocolSnapshot {
    let mut completeness = DataCompleteness {
        total_io_supply: availability(observation.total_io_supply_e8s),
        protocol_reserve_io: availability(observation.protocol_reserve_io_e8s),
        non_redeemable_governance_io: availability(observation.non_redeemable_governance_io_e8s),
        liquid_icp_reserve: availability(observation.liquid_icp_reserve_e8s),
        two_year_nns_principal: availability(observation.two_year_nns_principal_e8s),
        ..DataCompleteness::default()
    };

    let redeemable_io_supply_e8s = match (
        observation.total_io_supply_e8s,
        observation.protocol_reserve_io_e8s,
        observation.non_redeemable_governance_io_e8s,
    ) {
        (Some(total), Some(reserve), Some(governance)) => reserve
            .checked_add(governance)
            .and_then(|excluded| total.checked_sub(excluded)),
        _ => None,
    };
    completeness.redeemable_io_supply = availability(redeemable_io_supply_e8s);

    let redemption_rate = match (observation.liquid_icp_reserve_e8s, redeemable_io_supply_e8s) {
        (Some(liquid), Some(redeemable)) if redeemable > 0 => Some(RedemptionRateSnapshot {
            liquid_icp_reserve_e8s: liquid,
            redeemable_io_supply_e8s: redeemable,
            liquid_icp_per_io_e8s_numerator: liquid,
            liquid_icp_per_io_e8s_denominator: redeemable,
            last_updated_timestamp_nanos: observation.observed_at_timestamp_nanos,
        }),
        _ => None,
    };
    completeness.redemption_rate = availability(redemption_rate.as_ref());

    ProtocolSnapshot {
        total_io_supply_e8s: observation.total_io_supply_e8s,
        protocol_reserve_io_e8s: observation.protocol_reserve_io_e8s,
        non_redeemable_governance_io_e8s: observation.non_redeemable_governance_io_e8s,
        redeemable_io_supply_e8s,
        liquid_icp_reserve_e8s: observation.liquid_icp_reserve_e8s,
        two_year_nns_principal_e8s: observation.two_year_nns_principal_e8s,
        redemption_rate,
        last_updated_timestamp_nanos: observation.observed_at_timestamp_nanos,
        completeness,
    }
}

fn availability<T>(value: Option<T>) -> DataAvailability {
    if value.is_some() {
        DataAvailability::Observed
    } else {
        DataAvailability::Missing
    }
}

pub fn reserve_snapshot_from_protocol(protocol: &ProtocolSnapshot) -> ReserveSnapshot {
    ReserveSnapshot {
        liquid_icp_reserve_e8s: protocol.liquid_icp_reserve_e8s,
        two_year_nns_principal_e8s: protocol.two_year_nns_principal_e8s,
        last_updated_timestamp_nanos: protocol.last_updated_timestamp_nanos,
        completeness: protocol.completeness.clone(),
    }
}

pub fn supply_snapshot_from_protocol(protocol: &ProtocolSnapshot) -> SupplySnapshot {
    SupplySnapshot {
        total_io_supply_e8s: protocol.total_io_supply_e8s,
        protocol_reserve_io_e8s: protocol.protocol_reserve_io_e8s,
        non_redeemable_governance_io_e8s: protocol.non_redeemable_governance_io_e8s,
        redeemable_io_supply_e8s: protocol.redeemable_io_supply_e8s,
        last_updated_timestamp_nanos: protocol.last_updated_timestamp_nanos,
        completeness: protocol.completeness.clone(),
    }
}

pub fn stream_record_from_ledger_flow(
    flow: ObservedLedgerFlow,
    stream_kind: PublicStreamKind,
    recipient_policy: PublicRecipientPolicy,
    io_issued_e8s: Option<u128>,
    phase: PublicOperationPhase,
    terminal_rejection_reason: Option<String>,
) -> StreamHistoryRecord {
    let source_ledger = ledger_label(flow.ledger_kind, flow.ledger_principal_text);
    StreamHistoryRecord {
        record_id: format!("stream:{source_ledger}:{}", flow.block_index),
        source_ledger,
        source_block_index: Some(flow.block_index),
        stream_kind,
        amount_e8s: flow.amount_e8s,
        recipient_policy,
        io_issued_e8s,
        phase,
        timestamp_nanos: flow.timestamp_nanos,
        memo_label: flow.memo.as_ref().map(|memo| safe_memo_label(memo)),
        safe_subaccount_label: flow.to_account,
        terminal_rejection_reason,
    }
}

pub fn redemption_record_from_ledger_flow(
    flow: ObservedLedgerFlow,
    icp_payout_amount_e8s: Option<u128>,
    phase: PublicOperationPhase,
) -> RedemptionHistoryRecord {
    let source_ledger = ledger_label(flow.ledger_kind, flow.ledger_principal_text);
    RedemptionHistoryRecord {
        record_id: format!("redemption:{source_ledger}:{}", flow.block_index),
        io_burn_or_transfer_block: Some(flow.block_index),
        user_account: flow.from_account,
        io_amount_e8s: flow.amount_e8s,
        icp_payout_amount_e8s,
        gross_icp_payout_e8s: None,
        icp_payout_fee_e8s: None,
        net_user_icp_payout_e8s: None,
        io_return_fee_e8s: None,
        icp_payout_block: None,
        io_return_block: None,
        phase,
        timestamp_nanos: flow.timestamp_nanos,
        retry_count: 0,
        retry_status: None,
    }
}

pub fn reward_record_from_observation(
    record_id: String,
    recipient_neuron_id: Option<u64>,
    eligible_stake_e8s: Option<u128>,
    reward_amount_e8s: u128,
    dust_unissued_e8s: Option<u128>,
    status: PublicOperationPhase,
) -> RewardDistributionRecord {
    RewardDistributionRecord {
        record_id,
        epoch_start_timestamp_nanos: None,
        epoch_end_timestamp_nanos: None,
        participation_summary_id: None,
        recipient_neuron_id,
        recipient_account: None,
        eligible_stake_e8s,
        reward_amount_e8s,
        dust_unissued_e8s,
        payout_block: None,
        status,
    }
}

pub fn index_health_from_scan_state(
    record_id: String,
    ledger_kind: LedgerKind,
    account_label: String,
    scan: AccountHistoryScanState,
) -> IndexHealthSummary {
    IndexHealthSummary {
        record_id,
        ledger_kind,
        account_label,
        latest_cursor: scan.cursor.latest_cursor.map(|cursor| cursor.0),
        oldest_cursor: scan.cursor.oldest_cursor.map(|cursor| cursor.0),
        backfill_complete: scan.cursor.backfill_complete,
        page_order: scan.cursor.order,
        last_success_timestamp_nanos: scan.status.last_success_timestamp_nanos,
        unreadable_count: scan.status.latest_page_unreadable_count,
        invariant_broken_count: scan.status.invariant_broken_count,
        lag_suspected: scan.status.lag_suspected,
        page_cap_reached: scan.status.page_cap_reached,
        scan_incomplete: scan.status.scan_incomplete,
        last_observed_newest_tx_id: scan.status.last_observed_newest_tx_id.map(|block| block.0),
        last_observed_balance_e8s: scan.status.last_observed_account_balance_e8s,
        num_blocks_synced: scan.status.num_blocks_synced.map(|block| block.0),
        last_error: scan.status.last_error,
    }
}

pub fn governance_snapshot_from_observation(
    observation: GovernanceObservation,
) -> GovernanceParticipationSnapshot {
    let mut excluded = BTreeMap::<String, u64>::new();
    let mut participation = Vec::new();
    let mut total_stake = 0_u128;
    for neuron in observation.neurons {
        let policy_neuron = NeuronSnapshot {
            neuron_id: neuron.neuron_id,
            staked_io_e8s: neuron.staked_io_e8s,
            eligible_seconds: neuron.eligible_seconds,
            eligible_closed_proposals: neuron.eligible_closed_proposals,
            voted_closed_proposals: neuron.voted_closed_proposals,
            is_genesis_governance_neuron: neuron.is_genesis_governance_neuron,
            is_protocol_owned: neuron.is_protocol_owned,
            is_dissolving: neuron.is_dissolving,
        };
        if eligible(&policy_neuron) {
            let (num, den) = participation_ratio(&policy_neuron);
            total_stake = total_stake.saturating_add(policy_neuron.staked_io_e8s);
            participation.push(GovernanceNeuronParticipation {
                neuron_id: policy_neuron.neuron_id,
                eligible_stake_e8s: policy_neuron.staked_io_e8s,
                eligible_seconds: policy_neuron.eligible_seconds,
                eligible_closed_proposals: policy_neuron.eligible_closed_proposals,
                voted_closed_proposals: policy_neuron.voted_closed_proposals,
                participation_numerator: num,
                participation_denominator: den,
            });
        } else {
            for reason in exclusion_reasons(&policy_neuron) {
                *excluded.entry(reason).or_default() += 1;
            }
        }
    }
    participation.sort_by_key(|record| record.neuron_id);
    participation.truncate(MAX_GOVERNANCE_NEURON_SUMMARIES);

    GovernanceParticipationSnapshot {
        sns_eligible_neuron_count: participation.len() as u64,
        sns_excluded_neuron_count_by_reason: excluded
            .into_iter()
            .map(|(reason, count)| GovernanceExcludedCount { reason, count })
            .collect(),
        total_eligible_stake_e8s: total_stake,
        proposal_epoch_start: observation.proposal_epoch_start,
        proposal_epoch_end: observation.proposal_epoch_end,
        counted_proposals: observation.counted_proposals,
        pending_nns_operation_count: observation.pending_nns_operation_count,
        nns_lifecycle_status_summary: observation.nns_lifecycle_status_summary,
        last_governance_snapshot_timestamp_nanos: observation.observed_at_timestamp_nanos,
        neuron_participation: participation,
    }
}

pub fn release_artifacts_from_manifest(
    manifest: &ReleaseManifestObservation,
    observed_at_timestamp_nanos: Option<u64>,
) -> Vec<CanisterArtifactStatus> {
    manifest
        .artifacts
        .iter()
        .map(|entry| CanisterArtifactStatus {
            canister_name: entry.canister.clone(),
            expected_canister_principal_text: None,
            raw_wasm_sha256: Some(entry.raw_wasm_sha256.clone()),
            gz_wasm_sha256: Some(entry.gz_wasm_sha256.clone()),
            artifact_byte_size: Some(entry.raw_wasm_bytes),
            gz_artifact_byte_size: Some(entry.gz_wasm_bytes),
            build_profile: Some(entry.build_profile.clone()),
            target: Some(entry.target.clone()),
            git_commit: entry
                .git_commit
                .clone()
                .or_else(|| manifest.git_commit.clone()),
            observed_module_hash: None,
            status: ArtifactMatchStatus::Unobserved,
            last_checked_timestamp_nanos: observed_at_timestamp_nanos,
        })
        .collect()
}

pub fn model_artifact_status_mismatch(
    mut expected: CanisterArtifactStatus,
    observed_module_hash: Option<String>,
    observed_at_timestamp_nanos: Option<u64>,
) -> CanisterArtifactStatus {
    expected.status = match (&expected.raw_wasm_sha256, &observed_module_hash) {
        (Some(expected_hash), Some(observed_hash)) if expected_hash == observed_hash => {
            ArtifactMatchStatus::Matching
        }
        (Some(_), Some(_)) => ArtifactMatchStatus::Mismatch,
        _ => ArtifactMatchStatus::Unknown,
    };
    expected.observed_module_hash = observed_module_hash;
    expected.last_checked_timestamp_nanos = observed_at_timestamp_nanos;
    expected
}

pub fn observed_index_transactions_to_stream_records(
    transactions: Vec<IndexTransaction>,
    ledger_kind: LedgerKind,
) -> Vec<StreamHistoryRecord> {
    transactions
        .into_iter()
        .map(|tx| {
            stream_record_from_ledger_flow(
                ObservedLedgerFlow {
                    ledger_kind,
                    ledger_principal_text: None,
                    block_index: tx.block_index.0,
                    amount_e8s: tx.transaction.amount_e8s,
                    from_account: tx.transaction.from.map(|account| account.owner.to_text()),
                    to_account: tx.transaction.to.map(|account| account.owner.to_text()),
                    memo: tx.transaction.memo.map(|memo| memo.0),
                    timestamp_nanos: Some(tx.transaction.timestamp_nanos),
                },
                PublicStreamKind::UnknownIcpDeposit,
                PublicRecipientPolicy::Unknown,
                None,
                PublicOperationPhase::Observed,
                Some("unclassified ledger/index observation".to_string()),
            )
        })
        .collect()
}

fn exclusion_reasons(neuron: &NeuronSnapshot) -> Vec<String> {
    let mut reasons = Vec::new();
    if neuron.is_genesis_governance_neuron {
        reasons.push("genesis_governance_neuron".to_string());
    }
    if neuron.is_protocol_owned {
        reasons.push("protocol_owned".to_string());
    }
    if neuron.is_dissolving {
        reasons.push("dissolving".to_string());
    }
    if neuron.staked_io_e8s == 0 {
        reasons.push("zero_stake".to_string());
    }
    if neuron.eligible_seconds == 0 {
        reasons.push("zero_eligible_seconds".to_string());
    }
    if reasons.is_empty() {
        reasons.push("policy_ineligible".to_string());
    }
    reasons
}

fn ledger_label(kind: LedgerKind, principal: Option<String>) -> String {
    let kind = match kind {
        LedgerKind::IcpLedger => "icp",
        LedgerKind::IoLedger => "io",
    };
    principal.map_or_else(
        || kind.to_string(),
        |principal| format!("{kind}:{principal}"),
    )
}

fn safe_memo_label(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "empty".to_string();
    }
    if bytes
        .iter()
        .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
    {
        String::from_utf8_lossy(bytes).to_string()
    } else if bytes.len() == 8 {
        let mut fixed = [0_u8; 8];
        fixed.copy_from_slice(bytes);
        format!("u64:{}", u64::from_le_bytes(fixed))
    } else {
        format!("{} bytes", bytes.len())
    }
}

fn status_snapshot(state: &StableState) -> PublicStatus {
    PublicStatus {
        version: version().to_string(),
        model: "public-observation-read-model".to_string(),
        schema_version: state.schema_version,
        ingestion: HistorianIngestionStatus {
            schema_version: state.schema_version,
            stream_record_count: state.streams.len() as u64,
            redemption_record_count: state.redemptions.len() as u64,
            reward_record_count: state.rewards.len() as u64,
            nns_lifecycle_record_count: state.nns_lifecycle.len() as u64,
            index_health_record_count: state.index_health.len() as u64,
            artifact_status_count: state.release_artifacts.len() as u64,
            canister_status_count: state.canister_status.len() as u64,
            last_ingested_timestamp_nanos: state.last_ingested_timestamp_nanos,
            retained_record_limits: RetentionLimits::default(),
        },
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_public_status() -> PublicStatus {
    STATE.with(|cell| status_snapshot(&cell.borrow()))
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_protocol_snapshot() -> ProtocolSnapshot {
    STATE.with(|cell| cell.borrow().protocol.clone())
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_reserve_snapshot() -> ReserveSnapshot {
    STATE.with(|cell| reserve_snapshot_from_protocol(&cell.borrow().protocol))
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_redemption_rate() -> Option<RedemptionRateSnapshot> {
    STATE.with(|cell| cell.borrow().protocol.redemption_rate.clone())
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn list_streams(request: ListStreamsRequest) -> ListStreamsResponse {
    STATE.with(|cell| {
        let state = cell.borrow();
        let (records, next_start_after) = page_by_key(
            &state.streams,
            request.start_after,
            request.limit,
            |record| &record.record_id,
        );
        ListStreamsResponse {
            records,
            next_start_after,
        }
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn list_redemptions(request: ListRedemptionsRequest) -> ListRedemptionsResponse {
    STATE.with(|cell| {
        let state = cell.borrow();
        let (records, next_start_after) = page_by_key(
            &state.redemptions,
            request.start_after,
            request.limit,
            |record| &record.record_id,
        );
        ListRedemptionsResponse {
            records,
            next_start_after,
        }
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn list_rewards(request: ListRewardsRequest) -> ListRewardsResponse {
    STATE.with(|cell| {
        let state = cell.borrow();
        let (records, next_start_after) = page_by_key(
            &state.rewards,
            request.start_after,
            request.limit,
            |record| &record.record_id,
        );
        ListRewardsResponse {
            records,
            next_start_after,
        }
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn list_nns_lifecycle_events(
    request: ListNnsLifecycleEventsRequest,
) -> ListNnsLifecycleEventsResponse {
    STATE.with(|cell| {
        let state = cell.borrow();
        let (records, next_start_after) = page_by_key(
            &state.nns_lifecycle,
            request.start_after,
            request.limit,
            |record| &record.record_id,
        );
        ListNnsLifecycleEventsResponse {
            records,
            next_start_after,
        }
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_index_health() -> Vec<IndexHealthSummary> {
    STATE.with(|cell| cell.borrow().index_health.clone())
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_governance_summary() -> GovernanceParticipationSnapshot {
    STATE.with(|cell| cell.borrow().governance.clone())
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn list_governance_participation(
    request: ListGovernanceParticipationRequest,
) -> ListGovernanceParticipationResponse {
    STATE.with(|cell| {
        let state = cell.borrow();
        let limit = page_limit(request.limit);
        let records = state
            .governance
            .neuron_participation
            .iter()
            .filter(|record| {
                request
                    .start_after_neuron_id
                    .map(|cursor| record.neuron_id > cursor)
                    .unwrap_or(true)
            })
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        let next_start_after_neuron_id = if records.len() == limit
            && state.governance.neuron_participation.iter().any(|record| {
                records
                    .last()
                    .is_some_and(|last| record.neuron_id > last.neuron_id)
            }) {
            records.last().map(|record| record.neuron_id)
        } else {
            None
        };
        ListGovernanceParticipationResponse {
            records,
            next_start_after_neuron_id,
        }
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_release_artifacts() -> Vec<CanisterArtifactStatus> {
    STATE.with(|cell| cell.borrow().release_artifacts.clone())
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_canister_status_summary() -> Vec<CanisterArtifactStatus> {
    STATE.with(|cell| cell.borrow().canister_status.clone())
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_dashboard_state() -> PublicDashboardState {
    STATE.with(|cell| {
        let state = cell.borrow();
        PublicDashboardState {
            status: status_snapshot(&state),
            protocol: state.protocol.clone(),
            reserve: reserve_snapshot_from_protocol(&state.protocol),
            supply: supply_snapshot_from_protocol(&state.protocol),
            redemption_rate: state.protocol.redemption_rate.clone(),
            index_health: state.index_health.clone(),
            governance: state.governance.clone(),
            release_artifacts: state.release_artifacts.clone(),
            canister_status: state.canister_status.clone(),
        }
    })
}

fn export_stable_state() -> StableState {
    STATE.with(|cell| cell.borrow().clone())
}

fn import_stable_state(mut state: StableState) {
    if state.schema_version == 0 {
        state.schema_version = HISTORIAN_SCHEMA_VERSION;
    }
    STATE.with(|cell| *cell.borrow_mut() = state);
}

#[cfg_attr(target_family = "wasm", ic_cdk::pre_upgrade)]
pub fn pre_upgrade() {
    ic_cdk::storage::stable_save((export_stable_state(),))
        .expect("failed to save io_historian stable state");
}

#[cfg_attr(target_family = "wasm", ic_cdk::post_upgrade)]
pub fn post_upgrade() {
    if let Ok((state,)) = ic_cdk::storage::stable_restore::<(StableState,)>() {
        import_stable_state(state);
    }
}

#[cfg(any(test, debug_assertions))]
pub fn export_stable_state_for_tests() -> StableState {
    export_stable_state()
}

#[cfg(any(test, debug_assertions))]
pub fn import_stable_state_for_tests(state: StableState) {
    import_stable_state(state);
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_clear() {
    STATE.with(|cell| *cell.borrow_mut() = StableState::default());
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_ingest_protocol_snapshot(observation: ProtocolObservation) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let timestamp = observation.observed_at_timestamp_nanos;
        state.protocol = protocol_snapshot_from_observation(observation);
        set_last_ingested(&mut state, timestamp);
    });
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_ingest_ledger_flow(flow: ObservedLedgerFlow) {
    let record = stream_record_from_ledger_flow(
        flow,
        PublicStreamKind::UnknownIcpDeposit,
        PublicRecipientPolicy::Unknown,
        None,
        PublicOperationPhase::Observed,
        Some("unclassified ledger/index observation".to_string()),
    );
    debug_ingest_stream_record(record);
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_ingest_stream_record(record: StreamHistoryRecord) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let timestamp = record.timestamp_nanos;
        upsert_bounded(
            &mut state.streams,
            |record| &record.record_id,
            MAX_STREAM_HISTORY,
            record,
        );
        set_last_ingested(&mut state, timestamp);
    });
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_ingest_redemption_record(record: RedemptionHistoryRecord) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let timestamp = record.timestamp_nanos;
        upsert_bounded(
            &mut state.redemptions,
            |record| &record.record_id,
            MAX_REDEMPTION_HISTORY,
            record,
        );
        set_last_ingested(&mut state, timestamp);
    });
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_ingest_reward_record(record: RewardDistributionRecord) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        upsert_bounded(
            &mut state.rewards,
            |record| &record.record_id,
            MAX_REWARD_HISTORY,
            record,
        );
    });
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_ingest_nns_lifecycle(record: NnsLifecycleSummary) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let timestamp = record.timestamp_nanos;
        upsert_bounded(
            &mut state.nns_lifecycle,
            |record| &record.record_id,
            MAX_NNS_LIFECYCLE_HISTORY,
            record,
        );
        set_last_ingested(&mut state, timestamp);
    });
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_ingest_index_health(record: IndexHealthSummary) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let timestamp = record.last_success_timestamp_nanos;
        upsert_bounded(
            &mut state.index_health,
            |record| &record.record_id,
            MAX_INDEX_HEALTH,
            record,
        );
        set_last_ingested(&mut state, timestamp);
    });
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_ingest_governance_snapshot(observation: GovernanceObservation) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let timestamp = observation.observed_at_timestamp_nanos;
        state.governance = governance_snapshot_from_observation(observation);
        set_last_ingested(&mut state, timestamp);
    });
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_ingest_canister_artifact_status(record: CanisterArtifactStatus) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let timestamp = record.last_checked_timestamp_nanos;
        upsert_bounded(
            &mut state.canister_status,
            |record| &record.canister_name,
            MAX_CANISTER_STATUS,
            record,
        );
        set_last_ingested(&mut state, timestamp);
    });
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_ingest_release_artifacts(records: Vec<CanisterArtifactStatus>) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        for record in records {
            upsert_bounded(
                &mut state.release_artifacts,
                |record| &record.canister_name,
                MAX_ARTIFACT_STATUS,
                record,
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::{decode_one, encode_one};
    use io_ledger_types::{AccountHistoryCursor, AccountHistoryScanStatus, BlockIndex};
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("historian manifest lives under canisters/io_historian")
            .to_path_buf()
    }

    fn read_repo(path: &str) -> String {
        std::fs::read_to_string(repo_root().join(path)).unwrap()
    }

    fn protocol_observation() -> ProtocolObservation {
        ProtocolObservation {
            total_io_supply_e8s: Some(1_000),
            protocol_reserve_io_e8s: Some(100),
            non_redeemable_governance_io_e8s: Some(200),
            liquid_icp_reserve_e8s: Some(350),
            two_year_nns_principal_e8s: Some(9_999),
            observed_at_timestamp_nanos: Some(42),
        }
    }

    fn stream(id: u64) -> StreamHistoryRecord {
        StreamHistoryRecord {
            record_id: format!("stream:{id:04}"),
            source_ledger: "icp".to_string(),
            source_block_index: Some(id),
            stream_kind: PublicStreamKind::JupiterFaucet,
            amount_e8s: u128::from(id),
            recipient_policy: PublicRecipientPolicy::JupiterFaucet,
            io_issued_e8s: Some(u128::from(id)),
            phase: PublicOperationPhase::Completed,
            timestamp_nanos: Some(id),
            memo_label: None,
            safe_subaccount_label: None,
            terminal_rejection_reason: None,
        }
    }

    fn redemption(id: u64) -> RedemptionHistoryRecord {
        RedemptionHistoryRecord {
            record_id: format!("redemption:{id:04}"),
            io_burn_or_transfer_block: Some(id),
            user_account: Some("user".to_string()),
            io_amount_e8s: u128::from(id),
            icp_payout_amount_e8s: Some(u128::from(id)),
            gross_icp_payout_e8s: Some(u128::from(id)),
            icp_payout_fee_e8s: Some(0),
            net_user_icp_payout_e8s: Some(u128::from(id)),
            io_return_fee_e8s: Some(0),
            icp_payout_block: Some(id + 1),
            io_return_block: Some(id + 2),
            phase: PublicOperationPhase::Completed,
            timestamp_nanos: Some(id),
            retry_count: 0,
            retry_status: None,
        }
    }

    fn reward(id: u64) -> RewardDistributionRecord {
        reward_record_from_observation(
            format!("reward:{id:04}"),
            Some(id),
            Some(100),
            u128::from(id),
            Some(0),
            PublicOperationPhase::Completed,
        )
    }

    #[test]
    fn redemption_flow_observed_payout_does_not_infer_gross_net_or_fee() {
        let record = redemption_record_from_ledger_flow(
            ObservedLedgerFlow {
                ledger_kind: LedgerKind::IoLedger,
                ledger_principal_text: Some("ryjl3-tyaaa-aaaaa-aaaba-cai".to_string()),
                block_index: 42,
                amount_e8s: 1_000,
                from_account: Some("user".to_string()),
                to_account: Some("redemption".to_string()),
                memo: Some(b"redeem".to_vec()),
                timestamp_nanos: Some(99),
            },
            Some(700),
            PublicOperationPhase::Observed,
        );

        assert_eq!(record.icp_payout_amount_e8s, Some(700));
        assert_eq!(record.gross_icp_payout_e8s, None);
        assert_eq!(record.icp_payout_fee_e8s, None);
        assert_eq!(record.net_user_icp_payout_e8s, None);
        assert_eq!(record.io_return_fee_e8s, None);
    }

    fn lifecycle(id: u64) -> NnsLifecycleSummary {
        NnsLifecycleSummary {
            record_id: format!("nns:{id:04}"),
            kind: NnsLifecycleKind::TwoYearMaturityDisbursement,
            neuron_id: Some(id),
            amount_e8s: Some(u128::from(id)),
            phase: PublicOperationPhase::Completed,
            timestamp_nanos: Some(id),
            retry_count: 0,
            safe_error: None,
        }
    }

    #[test]
    fn public_dtos_candid_round_trip() {
        let dashboard = PublicDashboardState {
            status: get_public_status(),
            protocol: protocol_snapshot_from_observation(protocol_observation()),
            reserve: reserve_snapshot_from_protocol(&protocol_snapshot_from_observation(
                protocol_observation(),
            )),
            supply: supply_snapshot_from_protocol(&protocol_snapshot_from_observation(
                protocol_observation(),
            )),
            redemption_rate: protocol_snapshot_from_observation(protocol_observation())
                .redemption_rate,
            index_health: vec![],
            governance: GovernanceParticipationSnapshot::default(),
            release_artifacts: vec![],
            canister_status: vec![],
        };
        let encoded = encode_one(dashboard.clone()).unwrap();
        let decoded: PublicDashboardState = decode_one(&encoded).unwrap();
        assert_eq!(decoded, dashboard);
    }

    #[test]
    fn public_status_reports_observation_model() {
        debug_clear();
        let status = get_public_status();
        assert_eq!(status.schema_version, HISTORIAN_SCHEMA_VERSION);
        assert_eq!(status.model, "public-observation-read-model");
    }

    #[test]
    fn protocol_snapshot_complete_calculates_redeemable_supply_and_rate() {
        let snapshot = protocol_snapshot_from_observation(protocol_observation());
        assert_eq!(snapshot.redeemable_io_supply_e8s, Some(700));
        assert_eq!(
            snapshot.redemption_rate,
            Some(RedemptionRateSnapshot {
                liquid_icp_reserve_e8s: 350,
                redeemable_io_supply_e8s: 700,
                liquid_icp_per_io_e8s_numerator: 350,
                liquid_icp_per_io_e8s_denominator: 700,
                last_updated_timestamp_nanos: Some(42),
            })
        );
    }

    #[test]
    fn protocol_snapshot_missing_total_supply_is_incomplete() {
        let mut obs = protocol_observation();
        obs.total_io_supply_e8s = None;
        let snapshot = protocol_snapshot_from_observation(obs);
        assert_eq!(snapshot.redeemable_io_supply_e8s, None);
        assert_eq!(snapshot.redemption_rate, None);
        assert_eq!(
            snapshot.completeness.total_io_supply,
            DataAvailability::Missing
        );
    }

    #[test]
    fn protocol_snapshot_missing_liquid_reserve_is_incomplete() {
        let mut obs = protocol_observation();
        obs.liquid_icp_reserve_e8s = None;
        let snapshot = protocol_snapshot_from_observation(obs);
        assert_eq!(snapshot.redeemable_io_supply_e8s, Some(700));
        assert_eq!(snapshot.redemption_rate, None);
        assert_eq!(
            snapshot.completeness.liquid_icp_reserve,
            DataAvailability::Missing
        );
    }

    #[test]
    fn zero_redeemable_supply_has_no_fake_rate() {
        let mut obs = protocol_observation();
        obs.total_io_supply_e8s = Some(300);
        let snapshot = protocol_snapshot_from_observation(obs);
        assert_eq!(snapshot.redeemable_io_supply_e8s, Some(0));
        assert_eq!(snapshot.redemption_rate, None);
    }

    #[test]
    fn two_year_principal_is_excluded_from_liquid_nav() {
        let mut obs = protocol_observation();
        obs.two_year_nns_principal_e8s = Some(1_000_000);
        let snapshot = protocol_snapshot_from_observation(obs);
        assert_eq!(snapshot.liquid_icp_reserve_e8s, Some(350));
        assert_eq!(
            snapshot.redemption_rate.unwrap().liquid_icp_reserve_e8s,
            350
        );
    }

    #[test]
    fn protocol_reserve_and_governance_io_are_excluded_from_redeemable_supply() {
        let snapshot = protocol_snapshot_from_observation(protocol_observation());
        assert_eq!(snapshot.total_io_supply_e8s, Some(1_000));
        assert_eq!(snapshot.protocol_reserve_io_e8s, Some(100));
        assert_eq!(snapshot.non_redeemable_governance_io_e8s, Some(200));
        assert_eq!(snapshot.redeemable_io_supply_e8s, Some(700));
    }

    #[test]
    fn no_fee_or_dust_policy_is_applied_to_redemption_rate() {
        let snapshot = protocol_snapshot_from_observation(protocol_observation());
        let rate = snapshot.redemption_rate.unwrap();
        assert_eq!(rate.liquid_icp_per_io_e8s_numerator, 350);
        assert_eq!(rate.liquid_icp_per_io_e8s_denominator, 700);
        assert_eq!(io_core_model::BPS_DENOMINATOR, 10_000);
    }

    #[test]
    fn stream_redemption_reward_and_nns_history_are_paginated() {
        debug_clear();
        for id in 0..3 {
            debug_ingest_stream_record(stream(id));
            debug_ingest_redemption_record(redemption(id));
            debug_ingest_reward_record(reward(id));
            debug_ingest_nns_lifecycle(lifecycle(id));
        }
        let streams = list_streams(ListStreamsRequest {
            start_after: None,
            limit: Some(2),
        });
        assert_eq!(streams.records.len(), 2);
        assert_eq!(streams.next_start_after, Some("stream:0001".to_string()));
        assert_eq!(
            list_streams(ListStreamsRequest {
                start_after: streams.next_start_after,
                limit: Some(2),
            })
            .records
            .len(),
            1
        );
        assert_eq!(
            list_redemptions(ListRedemptionsRequest {
                start_after: None,
                limit: Some(2)
            })
            .records
            .len(),
            2
        );
        assert_eq!(
            list_rewards(ListRewardsRequest {
                start_after: None,
                limit: Some(2)
            })
            .records
            .len(),
            2
        );
        assert_eq!(
            list_nns_lifecycle_events(ListNnsLifecycleEventsRequest {
                start_after: None,
                limit: Some(2)
            })
            .records
            .len(),
            2
        );
    }

    #[test]
    fn bounded_retention_keeps_newest_deterministic_records() {
        debug_clear();
        for id in 0..(MAX_STREAM_HISTORY as u64 + 5) {
            debug_ingest_stream_record(stream(id));
        }
        let records = list_streams(ListStreamsRequest {
            start_after: None,
            limit: Some(MAX_PAGE_LIMIT as u64),
        });
        assert_eq!(
            get_public_status().ingestion.stream_record_count,
            MAX_STREAM_HISTORY as u64
        );
        assert_eq!(
            records.records.first().unwrap().record_id,
            "stream:0005".to_string()
        );
    }

    #[test]
    fn duplicate_observations_are_deduplicated_by_record_id() {
        debug_clear();
        let mut first = stream(1);
        first.amount_e8s = 10;
        debug_ingest_stream_record(first);
        let mut second = stream(1);
        second.amount_e8s = 20;
        debug_ingest_stream_record(second);
        let records = list_streams(ListStreamsRequest {
            start_after: None,
            limit: Some(10),
        });
        assert_eq!(records.records.len(), 1);
        assert_eq!(records.records[0].amount_e8s, 20);
    }

    #[test]
    fn index_health_status_ingestion_maps_scan_state() {
        debug_clear();
        let scan = AccountHistoryScanState {
            cursor: AccountHistoryCursor {
                order: Some(AccountHistoryPageOrder::Descending),
                latest_cursor: Some(BlockIndex(10)),
                oldest_cursor: Some(BlockIndex(2)),
                backfill_complete: true,
            },
            status: AccountHistoryScanStatus {
                last_success_timestamp_nanos: Some(99),
                latest_page_unreadable_count: 1,
                invariant_broken_count: 2,
                last_observed_newest_tx_id: Some(BlockIndex(11)),
                last_observed_account_balance_e8s: Some(123),
                num_blocks_synced: Some(BlockIndex(12)),
                page_cap_reached: true,
                lag_suspected: true,
                scan_incomplete: true,
                last_error: Some("safe".to_string()),
                safe_to_continue: true,
            },
        };
        debug_ingest_index_health(index_health_from_scan_state(
            "icp:deposit".to_string(),
            LedgerKind::IcpLedger,
            "deposit".to_string(),
            scan,
        ));
        let health = get_index_health();
        assert_eq!(health[0].latest_cursor, Some(10));
        assert_eq!(health[0].unreadable_count, 1);
        assert_eq!(health[0].last_error, Some("safe".to_string()));
    }

    #[test]
    fn governance_summary_ingestion_uses_reward_policy_eligibility() {
        debug_clear();
        debug_ingest_governance_snapshot(GovernanceObservation {
            neurons: vec![
                GovernanceNeuronObservation {
                    neuron_id: 1,
                    staked_io_e8s: 100,
                    eligible_seconds: 10,
                    eligible_closed_proposals: 4,
                    voted_closed_proposals: 2,
                    is_genesis_governance_neuron: false,
                    is_protocol_owned: false,
                    is_dissolving: false,
                },
                GovernanceNeuronObservation {
                    neuron_id: 2,
                    staked_io_e8s: 100,
                    eligible_seconds: 10,
                    eligible_closed_proposals: 4,
                    voted_closed_proposals: 4,
                    is_genesis_governance_neuron: true,
                    is_protocol_owned: false,
                    is_dissolving: false,
                },
            ],
            proposal_epoch_start: Some(1),
            proposal_epoch_end: Some(2),
            counted_proposals: 4,
            pending_nns_operation_count: Some(3),
            nns_lifecycle_status_summary: Some("observed".to_string()),
            observed_at_timestamp_nanos: Some(100),
        });
        let summary = get_governance_summary();
        assert_eq!(summary.sns_eligible_neuron_count, 1);
        assert_eq!(summary.total_eligible_stake_e8s, 100);
        assert_eq!(summary.neuron_participation[0].participation_numerator, 2);
        assert_eq!(
            summary.sns_excluded_neuron_count_by_reason[0].reason,
            "genesis_governance_neuron"
        );
    }

    #[test]
    fn release_manifest_parsing_models_artifact_status() {
        let lifecycle_manifest = io_sns_lifecycle::ArtifactManifest {
            schema_version: 1,
            build_profile: "release".to_string(),
            target: "wasm32-unknown-unknown".to_string(),
            git_commit: Some("abc".to_string()),
            artifacts: vec![io_sns_lifecycle::ArtifactManifestEntry {
                canister: "io_historian".to_string(),
                raw_wasm_path: "release-artifacts/io_historian.wasm".to_string(),
                raw_wasm_sha256: "raw".to_string(),
                raw_wasm_bytes: 10,
                gz_wasm_path: "release-artifacts/io_historian.wasm.gz".to_string(),
                gz_wasm_sha256: "gz".to_string(),
                gz_wasm_bytes: 5,
                build_profile: "release".to_string(),
                target: "wasm32-unknown-unknown".to_string(),
                git_commit: Some("abc".to_string()),
            }],
        };
        let manifest = ReleaseManifestObservation::from(&lifecycle_manifest);
        let statuses = release_artifacts_from_manifest(&manifest, Some(7));
        assert_eq!(statuses[0].canister_name, "io_historian");
        assert_eq!(statuses[0].status, ArtifactMatchStatus::Unobserved);
        assert_eq!(statuses[0].raw_wasm_sha256, Some("raw".to_string()));
    }

    #[test]
    fn artifact_status_mismatch_modeling_is_explicit() {
        let status = CanisterArtifactStatus {
            canister_name: "io_historian".to_string(),
            expected_canister_principal_text: None,
            raw_wasm_sha256: Some("expected".to_string()),
            gz_wasm_sha256: None,
            artifact_byte_size: Some(1),
            gz_artifact_byte_size: None,
            build_profile: Some("release".to_string()),
            target: Some("wasm32-unknown-unknown".to_string()),
            git_commit: None,
            observed_module_hash: None,
            status: ArtifactMatchStatus::Unobserved,
            last_checked_timestamp_nanos: None,
        };
        let observed = model_artifact_status_mismatch(status, Some("actual".to_string()), Some(9));
        assert_eq!(observed.status, ArtifactMatchStatus::Mismatch);
        assert_eq!(observed.last_checked_timestamp_nanos, Some(9));
    }

    #[test]
    fn stable_export_import_preserves_public_state() {
        debug_clear();
        debug_ingest_protocol_snapshot(protocol_observation());
        debug_ingest_stream_record(stream(1));
        let stable = export_stable_state_for_tests();
        debug_clear();
        assert_eq!(get_public_status().ingestion.stream_record_count, 0);
        import_stable_state_for_tests(stable);
        assert_eq!(get_protocol_snapshot().redeemable_io_supply_e8s, Some(700));
        assert_eq!(get_public_status().ingestion.stream_record_count, 1);
    }

    #[test]
    fn upgrade_persistence_uses_stable_import_export() {
        debug_clear();
        debug_ingest_stream_record(stream(44));
        let stable = export_stable_state_for_tests();
        import_stable_state_for_tests(stable);
        assert_eq!(
            list_streams(ListStreamsRequest {
                start_after: None,
                limit: Some(10)
            })
            .records[0]
                .record_id,
            "stream:0044"
        );
    }

    #[test]
    fn public_did_has_no_debug_ingestion_or_unbounded_history_methods() {
        let did = read_repo("canisters/io_historian/io_historian.did");
        assert!(!did.contains("debug_"));
        assert!(!did.contains("get_all"));
        assert!(did.contains("list_streams"));
        assert!(did.contains("ListStreamsRequest"));
    }

    #[test]
    fn value_moving_production_dids_stay_constructor_only() {
        for path in [
            "canisters/io_stream_manager/io_stream_manager.did",
            "canisters/io_nns_neuron_manager/io_nns_neuron_manager.did",
        ] {
            let did = read_repo(path);
            assert!(did.contains("service : (InitArgs) -> {}"));
            assert!(!did.contains("debug_"));
            assert!(!did.contains(" get_state :"));
            assert!(!did.contains(" get_events :"));
            assert!(!did.contains(" tick :"));
        }
    }

    #[test]
    fn historian_does_not_depend_on_value_moving_broad_query_apis() {
        let source = read_repo("canisters/io_historian/src/lib.rs");
        for forbidden in [
            "debug_get_state",
            "debug_get_redemption_rate",
            "process_stream_event",
            "get_events",
        ] {
            assert!(!source.contains(&format!("bounded_wait(canister, \"{forbidden}\"")));
        }
    }
}
