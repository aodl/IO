use candid::{CandidType, Principal};
use io_ledger_types::Account;
use serde::Deserialize;
use std::collections::BTreeSet;

pub const MAX_NONREDEEMABLE_ACCOUNTS: usize = 16;
pub const MAX_HISTORY_ACCOUNTS: usize = 8;
pub const MAX_EXPECTED_MODULES: usize = 12;
pub const MAX_RECENT_TRANSACTIONS: usize = 16;
pub const MIN_REFRESH_INTERVAL_SECONDS: u64 = 60;
pub const MAX_REFRESH_INTERVAL_SECONDS: u64 = 86_400;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ObservationConfig {
    pub stream_manager: Principal,
    pub nns_manager: Principal,
    pub sns_root: Principal,
    pub sns_governance: Principal,
    pub sns_ledger: Principal,
    pub sns_index: Principal,
    pub icp_ledger: Principal,
    pub nns_governance: Principal,
    pub two_year_neuron_id: u64,
    pub protocol_io_reserve: Account,
    pub liquid_icp_reserve: Account,
    pub nonredeemable_governance_io_accounts: Vec<NamedAccount>,
    pub history_accounts: Vec<NamedAccount>,
    pub expected_modules: Vec<ExpectedModule>,
    pub reward_share_capable_governance_sha256: Option<Vec<u8>>,
    pub refresh_interval_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NamedAccount {
    pub name: String,
    pub account: Account,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, CandidType, Deserialize)]
pub enum CanisterRole {
    StreamManager,
    NnsManager,
    Historian,
    Frontend,
    SnsGovernance,
    SnsRoot,
    SnsLedger,
    SnsIndex,
    SnsSwap,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ExpectedModule {
    pub role: CanisterRole,
    pub canister_id: Principal,
    pub wasm_sha256: Vec<u8>,
}

pub fn validate_config(
    config: &ObservationConfig,
    self_id: Option<Principal>,
) -> Result<(), String> {
    if !(MIN_REFRESH_INTERVAL_SECONDS..=MAX_REFRESH_INTERVAL_SECONDS)
        .contains(&config.refresh_interval_seconds)
    {
        return Err(format!(
            "refresh_interval_seconds must be in {MIN_REFRESH_INTERVAL_SECONDS}..={MAX_REFRESH_INTERVAL_SECONDS}"
        ));
    }
    if config.nonredeemable_governance_io_accounts.len() > MAX_NONREDEEMABLE_ACCOUNTS
        || config.history_accounts.len() > MAX_HISTORY_ACCOUNTS
        || config.expected_modules.len() > MAX_EXPECTED_MODULES
    {
        return Err("observation configuration exceeds a bounded collection limit".into());
    }
    let principals = [
        config.stream_manager,
        config.nns_manager,
        config.sns_root,
        config.sns_governance,
        config.sns_ledger,
        config.sns_index,
        config.icp_ledger,
        config.nns_governance,
    ];
    if principals
        .iter()
        .any(|principal| *principal == Principal::anonymous())
    {
        return Err("source principals must not be anonymous".into());
    }
    if principals.iter().copied().collect::<BTreeSet<_>>().len() != principals.len() {
        return Err("source principals must be distinct".into());
    }
    if config.two_year_neuron_id == 0 {
        return Err("permanent NNS neuron ID must be nonzero".into());
    }
    let mut nonredeemable_accounts = BTreeSet::new();
    let mut nonredeemable_names = BTreeSet::new();
    for named in &config.nonredeemable_governance_io_accounts {
        validate_name(&named.name)?;
        if !nonredeemable_names.insert(named.name.as_str()) {
            return Err("duplicate nonredeemable governance IO Account name".into());
        }
        let bytes = candid::encode_one(&named.account)
            .map_err(|err| format!("failed to encode account: {err}"))?;
        if !nonredeemable_accounts.insert(bytes) {
            return Err("duplicate nonredeemable governance IO Account".into());
        }
    }
    let reserve_bytes = candid::encode_one(&config.protocol_io_reserve)
        .map_err(|err| format!("failed to encode account: {err}"))?;
    if nonredeemable_accounts.contains(&reserve_bytes) {
        return Err(
            "protocol reserve must not also be an nonredeemable governance IO Account".into(),
        );
    }
    let mut history_accounts = BTreeSet::new();
    let mut history_names = BTreeSet::new();
    for named in &config.history_accounts {
        validate_name(&named.name)?;
        if !history_names.insert(named.name.as_str()) {
            return Err("duplicate history Account name".into());
        }
        let bytes = candid::encode_one(&named.account)
            .map_err(|err| format!("failed to encode account: {err}"))?;
        if !history_accounts.insert(bytes) {
            return Err("duplicate history Account".into());
        }
    }
    let mut roles = BTreeSet::new();
    let mut module_principals = BTreeSet::new();
    for expected in &config.expected_modules {
        if expected.wasm_sha256.len() != 32 {
            return Err("every expected Wasm SHA-256 must contain exactly 32 bytes".into());
        }
        if !roles.insert(expected.role) || !module_principals.insert(expected.canister_id) {
            return Err("expected module roles and canister IDs must be unique".into());
        }
    }
    for (role, principal) in [
        (CanisterRole::StreamManager, config.stream_manager),
        (CanisterRole::NnsManager, config.nns_manager),
        (CanisterRole::SnsRoot, config.sns_root),
        (CanisterRole::SnsGovernance, config.sns_governance),
        (CanisterRole::SnsLedger, config.sns_ledger),
        (CanisterRole::SnsIndex, config.sns_index),
    ] {
        if !config
            .expected_modules
            .iter()
            .any(|expected| expected.role == role && expected.canister_id == principal)
        {
            return Err(format!(
                "missing or mismatched expected module for {role:?}"
            ));
        }
    }
    if let Some(hash) = &config.reward_share_capable_governance_sha256 {
        if hash.len() != 32
            || !config.expected_modules.iter().any(|expected| {
                expected.role == CanisterRole::SnsGovernance && &expected.wasm_sha256 == hash
            })
        {
            return Err(
                "reward-share capable Governance hash must equal the expected Governance module"
                    .into(),
            );
        }
    }
    if let Some(self_id) = self_id {
        if !config.expected_modules.iter().any(|expected| {
            expected.role == CanisterRole::Historian && expected.canister_id == self_id
        }) {
            return Err("historian expected module must name this canister".into());
        }
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 64 || !name.bytes().all(|byte| byte.is_ascii_graphic()) {
        Err("Account names must be non-empty printable ASCII and at most 64 bytes".into())
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ObservationFreshness {
    Fresh,
    Stale,
    Missing,
    ErrorRetryable,
    PrelaunchNotConfigured,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SourceHealth {
    pub source: String,
    pub freshness: ObservationFreshness,
    pub last_attempt_timestamp_nanos: Option<u64>,
    pub last_success_timestamp_nanos: Option<u64>,
    pub error: Option<String>,
}

impl SourceHealth {
    pub fn prelaunch(source: &str) -> Self {
        Self {
            source: source.into(),
            freshness: ObservationFreshness::PrelaunchNotConfigured,
            last_attempt_timestamp_nanos: None,
            last_success_timestamp_nanos: None,
            error: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct DataCompleteness {
    pub total_io_supply: bool,
    pub protocol_reserve_io: bool,
    pub nonredeemable_governance_io: bool,
    pub claim_io_supply: bool,
    pub claim_backing: bool,
    pub active_backing_io: bool,
    pub active_reward_io: bool,
    pub claim_rate: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct ProtocolSnapshot {
    pub generation: u64,
    pub total_io_supply_e8s: Option<u128>,
    pub protocol_reserve_io_e8s: Option<u128>,
    pub nonredeemable_governance_io_e8s: Option<u128>,
    pub claim_io_supply_e8s: Option<u128>,
    pub liquid_claim_backing_e8s: Option<u128>,
    pub pooled_parent_principal_e8s: Option<u128>,
    pub live_child_principal_e8s: Option<u128>,
    pub in_transit_backing_e8s: Option<u128>,
    pub total_claim_backing_e8s: Option<u128>,
    pub claim_rate: Option<ClaimRateSnapshot>,
    pub active_backing_io_e8s: Option<u128>,
    pub active_reward_io_e8s: Option<u128>,
    pub pooled_target_e8s: Option<u128>,
    pub pooled_target_delta: Option<PooledTargetDelta>,
    pub live_cohort_count: Option<u32>,
    pub oldest_ready_at_seconds: Option<u64>,
    pub permanent_productive_capital_e8s: Option<u128>,
    pub observed_at_timestamp_nanos: Option<u64>,
    pub completeness: DataCompleteness,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ClaimRateSnapshot {
    pub backing_numerator_e8s: u128,
    pub claim_denominator_e8s: u128,
    pub available_liquid_e8s: u128,
    pub observed_at_timestamp_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum PooledTargetDelta {
    UnderTarget(u128),
    AtTarget,
    OverTarget(u128),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ReconciliationProjection {
    pub generation: u64,
    pub observed_at_nanos: u64,
    pub claim_supply_e8s: u128,
    pub liquid_backing_e8s: u128,
    pub pooled_backing_e8s: u128,
    pub unwinding_backing_e8s: u128,
    pub transit_backing_e8s: u128,
    pub total_claim_backing_e8s: u128,
    pub active_backing_io_e8s: u128,
    pub active_reward_io_e8s: u128,
    pub live_cohort_count: u32,
    pub oldest_ready_at_seconds: Option<u64>,
    pub pooled_target_e8s: u128,
    pub observed_pooled_e8s: u128,
}

pub struct ProtocolSnapshotInput<'a> {
    pub generation: u64,
    pub total: u128,
    pub reserve: u128,
    pub nonredeemable: &'a [u128],
    pub liquid: u128,
    pub reconciliation: Option<&'a ReconciliationProjection>,
    pub permanent_productive_capital_e8s: Option<u128>,
    pub observed_at: u64,
}

pub fn coherent_protocol_snapshot(
    input: ProtocolSnapshotInput<'_>,
) -> Result<ProtocolSnapshot, String> {
    let nonredeemable_total = input
        .nonredeemable
        .iter()
        .try_fold(0u128, |sum, value| sum.checked_add(*value))
        .ok_or_else(|| "nonredeemable governance IO balance sum overflow".to_string())?;
    let non_redeemable = input
        .reserve
        .checked_add(nonredeemable_total)
        .ok_or_else(|| "non-redeemable IO balance sum overflow".to_string())?;
    let claims = input.total.checked_sub(non_redeemable).ok_or_else(|| {
        "total IO supply is less than protocol reserve plus nonredeemable governance balances"
            .to_string()
    })?;
    let projection = input
        .reconciliation
        .filter(|value| value.claim_supply_e8s == claims);
    let rate = projection.and_then(|value| {
        (value.claim_supply_e8s > 0).then_some(ClaimRateSnapshot {
            backing_numerator_e8s: value.total_claim_backing_e8s,
            claim_denominator_e8s: value.claim_supply_e8s,
            available_liquid_e8s: input.liquid,
            observed_at_timestamp_nanos: value.observed_at_nanos,
        })
    });
    let delta = projection.map(|value| {
        if value.observed_pooled_e8s < value.pooled_target_e8s {
            PooledTargetDelta::UnderTarget(value.pooled_target_e8s - value.observed_pooled_e8s)
        } else if value.observed_pooled_e8s > value.pooled_target_e8s {
            PooledTargetDelta::OverTarget(value.observed_pooled_e8s - value.pooled_target_e8s)
        } else {
            PooledTargetDelta::AtTarget
        }
    });
    Ok(ProtocolSnapshot {
        generation: input.generation,
        total_io_supply_e8s: Some(input.total),
        protocol_reserve_io_e8s: Some(input.reserve),
        nonredeemable_governance_io_e8s: Some(nonredeemable_total),
        claim_io_supply_e8s: Some(claims),
        liquid_claim_backing_e8s: projection.map(|value| value.liquid_backing_e8s),
        pooled_parent_principal_e8s: projection.map(|value| value.pooled_backing_e8s),
        live_child_principal_e8s: projection.map(|value| value.unwinding_backing_e8s),
        in_transit_backing_e8s: projection.map(|value| value.transit_backing_e8s),
        total_claim_backing_e8s: projection.map(|value| value.total_claim_backing_e8s),
        claim_rate: rate.clone(),
        active_backing_io_e8s: projection.map(|value| value.active_backing_io_e8s),
        active_reward_io_e8s: projection.map(|value| value.active_reward_io_e8s),
        pooled_target_e8s: projection.map(|value| value.pooled_target_e8s),
        pooled_target_delta: delta,
        live_cohort_count: projection.map(|value| value.live_cohort_count),
        oldest_ready_at_seconds: projection.and_then(|value| value.oldest_ready_at_seconds),
        permanent_productive_capital_e8s: input.permanent_productive_capital_e8s,
        observed_at_timestamp_nanos: Some(input.observed_at),
        completeness: DataCompleteness {
            total_io_supply: true,
            protocol_reserve_io: true,
            nonredeemable_governance_io: true,
            claim_io_supply: true,
            claim_backing: projection.is_some(),
            active_backing_io: projection.is_some(),
            active_reward_io: projection.is_some(),
            claim_rate: rate.is_some(),
        },
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum Lifecycle {
    Paused,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RewardEventClassification {
    ProposalBearing,
    NoProposalFallback,
    ZeroEligibleParticipation,
    MissedSkipped,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardEventId {
    pub end_timestamp_seconds: u64,
    pub round: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StreamStatus {
    pub lifecycle: Lifecycle,
    pub operation_kind: Option<String>,
    pub operation_phase: Option<String>,
    pub latest_entitlement_batch_generation: u64,
    pub latest_processed_reward_event: Option<RewardEventId>,
    pub latest_reward_event_classification: Option<RewardEventClassification>,
    pub accumulated_eligible_credit: u128,
    pub accumulated_policy_credit: u128,
    pub processed_reward_event_count: u64,
    pub missed_reward_event_count: u64,
    pub reward_work_due: bool,
    pub reward_processing_paused: bool,
    pub governance_parameters_fresh: bool,
    pub pending_entitlement_batch_eligible_credit: Option<u128>,
    pub pending_entitlement_batch_policy_credit: Option<u128>,
    pub latest_reconciliation_checkpoint: Option<ReconciliationProjection>,
    pub observed_at_timestamp_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsManagerStatus {
    pub lifecycle: Lifecycle,
    pub active_operation: Option<String>,
    pub permanent_maturity_baseline_reconciled: bool,
    pub latest_started_two_week_generation: u64,
    pub latest_completed_two_week_generation: u64,
    pub latest_pooled_target: Option<PooledTargetObservation>,
    pub unwinding_child_principal_e8s: u128,
    pub observed_at_timestamp_nanos: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum TwoWeekTargetStatus {
    UnderTarget,
    AtTarget,
    AtTargetWithinUnwindTolerance,
    OverTarget,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PooledTargetObservation {
    pub target_e8s: u128,
    pub status: TwoWeekTargetStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsNeuronRole {
    TwoYearProtected,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsNeuronObservation {
    pub role: NnsNeuronRole,
    pub neuron_id: u64,
    pub stake_e8s: u64,
    pub staked_maturity_e8s: Option<u64>,
    pub dissolve_delay_seconds: u64,
    pub state: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsGovernanceStatus {
    pub build_metadata: String,
    pub neurons: Vec<NnsNeuronObservation>,
    pub observed_at_timestamp_nanos: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ModuleMatch {
    Matching,
    Mismatch,
    Unavailable,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CanisterObservation {
    pub role: CanisterRole,
    pub canister_id: Principal,
    pub expected_module_hash: Vec<u8>,
    pub observed_module_hash: Option<Vec<u8>>,
    pub module_match: ModuleMatch,
    pub controllers: Option<Vec<Principal>>,
    pub observed_at_timestamp_nanos: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum CapabilityState {
    ExpectedGovernanceModuleMatching,
    ModuleMismatch,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SnsStatus {
    pub max_number_of_neurons: Option<u64>,
    pub native_initial_reward_rate_basis_points: Option<u64>,
    pub native_final_reward_rate_basis_points: Option<u64>,
    pub latest_reward_event_round: Option<u64>,
    pub latest_reward_event_end_timestamp_seconds: Option<u64>,
    pub settled_proposal_count: Option<u64>,
    pub reward_share_capability: CapabilityState,
    pub archive_canisters: Vec<Principal>,
    pub observed_at_timestamp_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RecentTransaction {
    pub block_index: u128,
    pub kind: String,
    pub amount_e8s: Option<u128>,
    pub from: Option<Account>,
    pub to: Option<Account>,
    pub timestamp_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct AccountHistoryObservation {
    pub name: String,
    pub account: Account,
    pub index_balance_e8s: u128,
    pub newest_transaction_id: Option<u128>,
    pub oldest_transaction_id: Option<u128>,
    pub transactions: Vec<RecentTransaction>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct IndexStatus {
    pub num_blocks_synced: u128,
    pub accounts: Vec<AccountHistoryObservation>,
    pub observed_at_timestamp_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PublicStatus {
    pub version: String,
    pub schema_version: u32,
    pub configured: bool,
    pub refresh_active: bool,
    pub refresh_generation: u64,
    pub last_attempt_timestamp_nanos: Option<u64>,
    pub last_success_timestamp_nanos: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct Dashboard {
    pub status: PublicStatus,
    pub protocol: ProtocolSnapshot,
    pub source_health: Vec<SourceHealth>,
    pub canisters: Vec<CanisterObservation>,
    pub stream: Option<StreamStatus>,
    pub nns_manager: Option<NnsManagerStatus>,
    pub nns_governance: Option<NnsGovernanceStatus>,
    pub sns: Option<SnsStatus>,
    pub index: Option<IndexStatus>,
}
