pub mod clients;
pub mod governance_snapshot;
pub mod logic;
pub mod scheduler;
pub mod state;

use candid::{CandidType, Principal};
use io_production_wiring::ProductionWiringConfig;
use io_stable_schema::IO_STREAM_MANAGER_SCHEMA_VERSION;
use serde::Deserialize;
use std::{cell::RefCell, collections::BTreeSet};

pub use io_core_model::{
    IoRecipientPolicy, ModelError, ProtocolState, RedemptionOutcome, RedemptionRate, Split,
    StreamKind, StreamOutcome, E8S_PER_TOKEN,
};
pub use logic::StreamManagerError;
pub use state::StreamManager;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct InitArgs {
    pub initial_total_io_supply_e8s: u128,
    pub initial_protocol_reserve_io_e8s: u128,
    pub non_redeemable_governance_io_e8s: u128,
    pub jupiter_faucet_principal_text: Option<String>,
    pub io_nns_neuron_manager_principal_text: Option<String>,
    pub icp_ledger_principal_text: Option<String>,
    pub icp_index_principal_text: Option<String>,
    pub io_ledger_principal_text: Option<String>,
    pub io_index_principal_text: Option<String>,
    pub io_sns_ledger_principal_text: Option<String>,
    pub io_sns_index_principal_text: Option<String>,
    pub sns_governance_principal_text: Option<String>,
    pub production_wiring: Option<ProductionWiringConfig>,
}

impl Default for InitArgs {
    fn default() -> Self {
        Self {
            initial_total_io_supply_e8s: 1_000_000 * E8S_PER_TOKEN,
            initial_protocol_reserve_io_e8s: 900_000 * E8S_PER_TOKEN,
            non_redeemable_governance_io_e8s: 100_000 * E8S_PER_TOKEN,
            jupiter_faucet_principal_text: None,
            io_nns_neuron_manager_principal_text: None,
            icp_ledger_principal_text: None,
            icp_index_principal_text: None,
            io_ledger_principal_text: None,
            io_index_principal_text: None,
            io_sns_ledger_principal_text: None,
            io_sns_index_principal_text: None,
            sns_governance_principal_text: None,
            production_wiring: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StreamManagerConfig {
    pub initial_total_io_supply_e8s: u128,
    pub initial_protocol_reserve_io_e8s: u128,
    pub non_redeemable_governance_io_e8s: u128,
    pub jupiter_faucet_principal_text: Option<String>,
    pub io_nns_neuron_manager_principal_text: Option<String>,
    pub icp_ledger_principal_text: Option<String>,
    pub icp_index_principal_text: Option<String>,
    pub io_ledger_principal_text: Option<String>,
    pub io_index_principal_text: Option<String>,
    pub io_sns_ledger_principal_text: Option<String>,
    pub io_sns_index_principal_text: Option<String>,
    pub sns_governance_principal_text: Option<String>,
    pub production_wiring: Option<ProductionWiringConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct LegacyPreV3StreamManagerConfig {
    initial_total_io_supply_e8s: u128,
    initial_protocol_reserve_io_e8s: u128,
    non_redeemable_governance_io_e8s: u128,
    two_week_pool_backing_bps: u128,
    jupiter_faucet_principal_text: Option<String>,
    io_nns_neuron_manager_principal_text: Option<String>,
    icp_ledger_principal_text: Option<String>,
    icp_index_principal_text: Option<String>,
    io_ledger_principal_text: Option<String>,
    io_index_principal_text: Option<String>,
    io_sns_ledger_principal_text: Option<String>,
    io_sns_index_principal_text: Option<String>,
    sns_governance_principal_text: Option<String>,
    production_wiring: Option<ProductionWiringConfig>,
}

impl From<LegacyPreV3StreamManagerConfig> for StreamManagerConfig {
    fn from(value: LegacyPreV3StreamManagerConfig) -> Self {
        Self {
            initial_total_io_supply_e8s: value.initial_total_io_supply_e8s,
            initial_protocol_reserve_io_e8s: value.initial_protocol_reserve_io_e8s,
            non_redeemable_governance_io_e8s: value.non_redeemable_governance_io_e8s,
            jupiter_faucet_principal_text: value.jupiter_faucet_principal_text,
            io_nns_neuron_manager_principal_text: value.io_nns_neuron_manager_principal_text,
            icp_ledger_principal_text: value.icp_ledger_principal_text,
            icp_index_principal_text: value.icp_index_principal_text,
            io_ledger_principal_text: value.io_ledger_principal_text,
            io_index_principal_text: value.io_index_principal_text,
            io_sns_ledger_principal_text: value.io_sns_ledger_principal_text,
            io_sns_index_principal_text: value.io_sns_index_principal_text,
            sns_governance_principal_text: value.sns_governance_principal_text,
            production_wiring: value.production_wiring,
        }
    }
}

impl Default for StreamManagerConfig {
    fn default() -> Self {
        InitArgs::default()
            .try_into()
            .expect("default stream-manager config must be valid")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitArgsError {
    ExcludedSupplyExceedsTotal,
    InvalidPrincipalText { field: &'static str, value: String },
    InvalidProductionWiring { message: String },
}

impl TryFrom<InitArgs> for StreamManagerConfig {
    type Error = InitArgsError;

    fn try_from(args: InitArgs) -> Result<Self, Self::Error> {
        let excluded_supply = args
            .initial_protocol_reserve_io_e8s
            .checked_add(args.non_redeemable_governance_io_e8s)
            .ok_or(InitArgsError::ExcludedSupplyExceedsTotal)?;
        if args.initial_total_io_supply_e8s < excluded_supply {
            return Err(InitArgsError::ExcludedSupplyExceedsTotal);
        }
        validate_optional_principal(
            "jupiter_faucet_principal_text",
            &args.jupiter_faucet_principal_text,
        )?;
        validate_optional_principal(
            "io_nns_neuron_manager_principal_text",
            &args.io_nns_neuron_manager_principal_text,
        )?;
        validate_optional_principal("icp_ledger_principal_text", &args.icp_ledger_principal_text)?;
        validate_optional_principal("icp_index_principal_text", &args.icp_index_principal_text)?;
        validate_optional_principal("io_ledger_principal_text", &args.io_ledger_principal_text)?;
        validate_optional_principal("io_index_principal_text", &args.io_index_principal_text)?;
        validate_optional_principal(
            "io_sns_ledger_principal_text",
            &args.io_sns_ledger_principal_text,
        )?;
        validate_optional_principal(
            "io_sns_index_principal_text",
            &args.io_sns_index_principal_text,
        )?;
        validate_optional_principal(
            "sns_governance_principal_text",
            &args.sns_governance_principal_text,
        )?;
        if let Some(production_wiring) = &args.production_wiring {
            production_wiring
                .validate()
                .map_err(|err| InitArgsError::InvalidProductionWiring {
                    message: format!("{err:?}"),
                })?;
        }

        Ok(Self {
            initial_total_io_supply_e8s: args.initial_total_io_supply_e8s,
            initial_protocol_reserve_io_e8s: args.initial_protocol_reserve_io_e8s,
            non_redeemable_governance_io_e8s: args.non_redeemable_governance_io_e8s,
            jupiter_faucet_principal_text: args.jupiter_faucet_principal_text,
            io_nns_neuron_manager_principal_text: args.io_nns_neuron_manager_principal_text,
            icp_ledger_principal_text: args.icp_ledger_principal_text,
            icp_index_principal_text: args.icp_index_principal_text,
            io_ledger_principal_text: args.io_ledger_principal_text,
            io_index_principal_text: args.io_index_principal_text,
            io_sns_ledger_principal_text: args.io_sns_ledger_principal_text,
            io_sns_index_principal_text: args.io_sns_index_principal_text,
            sns_governance_principal_text: args.sns_governance_principal_text,
            production_wiring: args.production_wiring,
        })
    }
}

fn validate_optional_principal(
    field: &'static str,
    value: &Option<String>,
) -> Result<(), InitArgsError> {
    if let Some(text) = value {
        if text.trim().is_empty() || Principal::from_text(text).is_err() {
            return Err(InitArgsError::InvalidPrincipalText {
                field,
                value: text.clone(),
            });
        }
    }
    Ok(())
}

#[cfg_attr(not(any(test, debug_assertions)), allow(dead_code))]
#[derive(Clone, Debug)]
struct CanisterState {
    config: StreamManagerConfig,
    manager: StreamManager,
    operation_journal: Vec<StreamOperation>,
    scheduler_cursors: SchedulerCursors,
    reward_cohort: Option<RewardCohort>,
    #[cfg(any(test, debug_assertions))]
    debug_failpoint: Option<DebugFailpoint>,
}

impl CanisterState {
    fn new(config: StreamManagerConfig) -> Self {
        let manager = StreamManager {
            state: ProtocolState::new(
                config.initial_total_io_supply_e8s,
                config.initial_protocol_reserve_io_e8s,
                config.non_redeemable_governance_io_e8s,
            ),
            processed_transactions: Default::default(),
            active_staked_io_e8s: 0,
        };
        Self {
            config,
            manager,
            operation_journal: Vec::new(),
            scheduler_cursors: SchedulerCursors::default(),
            reward_cohort: None,
            #[cfg(any(test, debug_assertions))]
            debug_failpoint: None,
        }
    }
}

impl Default for CanisterState {
    fn default() -> Self {
        Self::new(StreamManagerConfig::default())
    }
}

thread_local! {
    static CANISTER_STATE: RefCell<CanisterState> = RefCell::new(CanisterState::default());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StableProtocolState {
    pub liquid_icp_e8s: u128,
    pub two_year_staked_icp_e8s: u128,
    pub two_week_staked_icp_e8s: u128,
    pub total_io_supply_e8s: u128,
    pub protocol_reserve_io_e8s: u128,
    pub non_redeemable_governance_io_e8s: u128,
}

impl From<ProtocolState> for StableProtocolState {
    fn from(value: ProtocolState) -> Self {
        Self {
            liquid_icp_e8s: value.liquid_icp_e8s,
            two_year_staked_icp_e8s: value.two_year_staked_icp_e8s,
            two_week_staked_icp_e8s: value.two_week_staked_icp_e8s,
            total_io_supply_e8s: value.total_io_supply_e8s,
            protocol_reserve_io_e8s: value.protocol_reserve_io_e8s,
            non_redeemable_governance_io_e8s: value.non_redeemable_governance_io_e8s,
        }
    }
}

impl From<StableProtocolState> for ProtocolState {
    fn from(value: StableProtocolState) -> Self {
        Self {
            liquid_icp_e8s: value.liquid_icp_e8s,
            two_year_staked_icp_e8s: value.two_year_staked_icp_e8s,
            two_week_staked_icp_e8s: value.two_week_staked_icp_e8s,
            total_io_supply_e8s: value.total_io_supply_e8s,
            protocol_reserve_io_e8s: value.protocol_reserve_io_e8s,
            non_redeemable_governance_io_e8s: value.non_redeemable_governance_io_e8s,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardCohort {
    pub generation: u64,
    pub captured_at_timestamp_seconds: u64,
    pub members: Vec<RewardCohortMember>,
    pub consumed_by_operation_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardCohortMember {
    pub sns_neuron_id: Vec<u8>,
    pub frozen_stake_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StableState {
    pub config: StreamManagerConfig,
    pub protocol: StableProtocolState,
    pub processed_transactions: Vec<String>,
    pub active_staked_io_e8s: u128,
    pub reward_cohort: Option<RewardCohort>,
    pub operation_journal: Vec<StreamOperation>,
    pub scheduler_cursors: SchedulerCursors,
}

pub const STREAM_MANAGER_STABLE_SCHEMA_VERSION: u32 = IO_STREAM_MANAGER_SCHEMA_VERSION;
const REWARD_COHORT_MAX_MEMBERS: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct VersionedStableState {
    pub schema_version: u32,
    pub state: StableState,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct LegacyPreV3VersionedStableState {
    schema_version: u32,
    state: LegacyPreV3StableState,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
struct LegacyPreV3StableState {
    config: LegacyPreV3StreamManagerConfig,
    protocol: StableProtocolState,
    processed_transactions: Vec<String>,
    active_staked_io_e8s: u128,
    two_week_pool_backing_bps: u128,
    operation_journal: Vec<StreamOperation>,
    scheduler_cursors: SchedulerCursors,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StableMigrationError {
    UnsupportedFutureVersion {
        canister: &'static str,
        version: u32,
    },
    UnsupportedOldVersion {
        canister: &'static str,
        version: u32,
    },
    CorruptSnapshot {
        canister: &'static str,
        message: String,
    },
}

pub fn migrate_stable_state(
    snapshot: VersionedStableState,
) -> Result<StableState, StableMigrationError> {
    match snapshot.schema_version {
        0 | 1 => {
            let normalized = validate_reward_reservation_consistency(
                normalize_legacy_reward_reservations(snapshot.state)?,
                RewardValidationMode::LegacyNormalization,
            )?;
            validate_current_stable_state(normalized)
        }
        2 | STREAM_MANAGER_STABLE_SCHEMA_VERSION => validate_current_stable_state(snapshot.state),
        version if version > STREAM_MANAGER_STABLE_SCHEMA_VERSION => {
            Err(StableMigrationError::UnsupportedFutureVersion {
                canister: "io_stream_manager",
                version,
            })
        }
        version => Err(StableMigrationError::UnsupportedOldVersion {
            canister: "io_stream_manager",
            version,
        }),
    }
}

fn migrate_legacy_pre_v3_stable_state(
    snapshot: LegacyPreV3VersionedStableState,
) -> Result<StableState, StableMigrationError> {
    if !matches!(snapshot.schema_version, 0..=2) {
        return Err(StableMigrationError::UnsupportedOldVersion {
            canister: "io_stream_manager",
            version: snapshot.schema_version,
        });
    }
    if snapshot.state.two_week_pool_backing_bps != 10_000
        || snapshot.state.config.two_week_pool_backing_bps != 10_000
    {
        return Err(StableMigrationError::CorruptSnapshot {
            canister: "io_stream_manager",
            message: "legacy two_week_pool_backing_bps must be 10_000".to_string(),
        });
    }
    let state = StableState {
        config: snapshot.state.config.into(),
        protocol: snapshot.state.protocol,
        processed_transactions: snapshot.state.processed_transactions,
        active_staked_io_e8s: snapshot.state.active_staked_io_e8s,
        reward_cohort: None,
        operation_journal: snapshot.state.operation_journal,
        scheduler_cursors: snapshot.state.scheduler_cursors,
    };
    match snapshot.schema_version {
        0 | 1 => {
            let normalized = validate_reward_reservation_consistency(
                normalize_legacy_reward_reservations(state)?,
                RewardValidationMode::LegacyNormalization,
            )?;
            validate_current_stable_state(normalized)
        }
        2 => validate_current_stable_state(state),
        _ => unreachable!("pre-v3 schema version checked above"),
    }
}

fn validate_current_stable_state(state: StableState) -> Result<StableState, StableMigrationError> {
    let state = validate_reward_reservation_consistency(state, RewardValidationMode::Current)?;
    validate_reward_cohort(&state)?;
    Ok(state)
}

fn validate_reward_cohort(state: &StableState) -> Result<(), StableMigrationError> {
    let Some(cohort) = &state.reward_cohort else {
        return Ok(());
    };
    if cohort.generation == 0 {
        return Err(StableMigrationError::CorruptSnapshot {
            canister: "io_stream_manager",
            message: "reward cohort generation must be positive".to_string(),
        });
    }
    if cohort.members.len() > REWARD_COHORT_MAX_MEMBERS {
        return Err(StableMigrationError::CorruptSnapshot {
            canister: "io_stream_manager",
            message: "reward cohort member count exceeds governance pagination bound".to_string(),
        });
    }
    let mut seen = BTreeSet::new();
    let mut seen_compatibility_ids = BTreeSet::new();
    let mut previous: Option<&[u8]> = None;
    let mut total = 0u128;
    for member in &cohort.members {
        if member.sns_neuron_id.len() != 32 {
            return Err(StableMigrationError::CorruptSnapshot {
                canister: "io_stream_manager",
                message: "reward cohort member SNS neuron id is not 32 bytes".to_string(),
            });
        }
        if member.frozen_stake_e8s == 0 {
            return Err(StableMigrationError::CorruptSnapshot {
                canister: "io_stream_manager",
                message: "reward cohort member has zero frozen stake".to_string(),
            });
        }
        if previous.is_some_and(|id| id >= member.sns_neuron_id.as_slice()) {
            return Err(StableMigrationError::CorruptSnapshot {
                canister: "io_stream_manager",
                message: "reward cohort members are not strictly sorted by SNS neuron id"
                    .to_string(),
            });
        }
        if !seen.insert(member.sns_neuron_id.clone()) {
            return Err(StableMigrationError::CorruptSnapshot {
                canister: "io_stream_manager",
                message: "reward cohort has duplicate SNS neuron ids".to_string(),
            });
        }
        let compatibility_id = io_reward_policy::sns_neuron_id_to_u64(
            &io_governance_types::SnsNeuronId(member.sns_neuron_id.clone()),
        )
        .map_err(|err| StableMigrationError::CorruptSnapshot {
            canister: "io_stream_manager",
            message: format!("reward cohort member SNS neuron id is invalid: {err:?}"),
        })?;
        if !seen_compatibility_ids.insert(compatibility_id) {
            return Err(StableMigrationError::CorruptSnapshot {
                canister: "io_stream_manager",
                message: "reward cohort has duplicate compatibility SNS neuron ids".to_string(),
            });
        }
        total = total.checked_add(member.frozen_stake_e8s).ok_or_else(|| {
            StableMigrationError::CorruptSnapshot {
                canister: "io_stream_manager",
                message: "reward cohort frozen stake total overflows".to_string(),
            }
        })?;
        previous = Some(member.sns_neuron_id.as_slice());
    }
    if let Some(operation_id) = &cohort.consumed_by_operation_id {
        let matching = state.operation_journal.iter().filter(|op| {
            op.operation_id == *operation_id
                && op.kind == StreamOperationKind::TwoWeekMaturityStream
        });
        if matching.count() != 1 {
            return Err(StableMigrationError::CorruptSnapshot {
                canister: "io_stream_manager",
                message: "reward cohort consumed operation reference is missing or ambiguous"
                    .to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) fn reward_recipient_has_any_attempt_or_external_evidence(
    recipient: &TwoWeekRecipientTransfer,
) -> bool {
    recipient.reward_transfer_attempt.is_some()
        || recipient.transfer_block_index.is_some()
        || recipient.ledger_transfer_block.is_some()
        || recipient.ledger_transfer_proof_scan_state.is_some()
        || recipient
            .ledger_transfer_status
            .unwrap_or(recipient.transfer_status)
            == TransferStatus::Succeeded
        || matches!(
            recipient.governance_refresh_status,
            Some(TransferStatus::Succeeded)
        )
        || recipient.ledger_transfer_fee_e8s.is_some()
        || recipient.reward_amount_received_e8s.is_some()
        || recipient.reserve_debit_e8s.is_some()
        || recipient.observed_stake_after_e8s.is_some()
        || recipient.concurrent_stake_delta_e8s.is_some()
}

fn stable_reward_operation_has_external_effect_or_uncertainty(op: &StreamOperation) -> bool {
    op.two_week_recipients
        .iter()
        .any(reward_recipient_has_any_attempt_or_external_evidence)
}

fn corrupt_reward_snapshot(op: &StreamOperation, message: String) -> StableMigrationError {
    StableMigrationError::CorruptSnapshot {
        canister: "io_stream_manager",
        message: format!("reward operation {} {message}", op.operation_id),
    }
}

pub(crate) fn reward_operation_accounting_error(
    op: &StreamOperation,
    message: impl Into<String>,
) -> String {
    format!("reward operation {} {}", op.operation_id, message.into())
}

#[cfg_attr(not(any(test, target_family = "wasm")), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RewardValidationMode {
    Current,
    LegacyNormalization,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RewardAccountingError {
    Invalid {
        operation_id: String,
        message: String,
    },
}

impl RewardAccountingError {
    fn new(op: &StreamOperation, message: impl Into<String>) -> Self {
        Self::Invalid {
            operation_id: op.operation_id.clone(),
            message: message.into(),
        }
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}

impl std::fmt::Display for RewardAccountingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid {
                operation_id,
                message,
            } => write!(f, "reward operation {operation_id} {message}"),
        }
    }
}

impl From<RewardAccountingError> for String {
    fn from(value: RewardAccountingError) -> Self {
        value.to_string()
    }
}

fn reward_accounting_error(
    op: &StreamOperation,
    message: impl Into<String>,
) -> RewardAccountingError {
    RewardAccountingError::new(op, message)
}

#[cfg_attr(not(any(test, target_family = "wasm")), allow(dead_code))]
pub(crate) fn reward_attempt_submitted_or_proof_required(
    attempt: &RewardTransferAttemptRecord,
) -> bool {
    matches!(
        attempt.lifecycle,
        Some(
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { .. }
                | RewardTransferAttemptLifecycle::ProofRequired { .. }
        )
    )
}

#[cfg_attr(not(any(test, target_family = "wasm")), allow(dead_code))]
pub(crate) fn reward_recipient_has_submitted_or_proof_required_attempt(
    recipient: &TwoWeekRecipientTransfer,
) -> bool {
    recipient
        .reward_transfer_attempt
        .as_ref()
        .map(reward_attempt_submitted_or_proof_required)
        .unwrap_or(false)
}

#[cfg_attr(not(any(test, target_family = "wasm")), allow(dead_code))]
pub(crate) fn reward_recipient_attempt_is_proven(recipient: &TwoWeekRecipientTransfer) -> bool {
    recipient
        .reward_transfer_attempt
        .as_ref()
        .and_then(|attempt| attempt.lifecycle.as_ref())
        .map(|lifecycle| matches!(lifecycle, RewardTransferAttemptLifecycle::Proven { .. }))
        .unwrap_or(false)
}

#[cfg_attr(not(any(test, target_family = "wasm")), allow(dead_code))]
pub(crate) fn reward_recipient_has_spent_debit(recipient: &TwoWeekRecipientTransfer) -> bool {
    recipient
        .ledger_transfer_status
        .unwrap_or(recipient.transfer_status)
        == TransferStatus::Succeeded
        || reward_recipient_attempt_is_proven(recipient)
}

#[cfg_attr(not(any(test, target_family = "wasm")), allow(dead_code))]
pub(crate) fn reward_operation_submitted_or_proof_required_attempts(op: &StreamOperation) -> usize {
    op.two_week_recipients
        .iter()
        .filter(|recipient| reward_recipient_has_submitted_or_proof_required_attempt(recipient))
        .count()
}

#[cfg_attr(not(any(test, target_family = "wasm")), allow(dead_code))]
pub(crate) fn validate_reward_operation_accounting(
    op: &StreamOperation,
    processed_transactions: Option<&BTreeSet<String>>,
    mode: RewardValidationMode,
) -> Result<(), RewardAccountingError> {
    if let Some(processed_transactions) = processed_transactions {
        let is_processed = processed_transactions.contains(&op.source_transaction_id);
        if op.phase == OperationPhase::Completed {
            if !is_processed {
                return Err(reward_accounting_error(
                    op,
                    "is completed but missing processed transaction evidence",
                ));
            }
        } else if is_processed {
            return Err(reward_accounting_error(
                op,
                "is not completed but has processed transaction evidence",
            ));
        }
    }

    if reward_operation_submitted_or_proof_required_attempts(op) > 1 {
        return Err(reward_accounting_error(
            op,
            "has more than one submitted/proof-required reward attempt",
        ));
    }

    let stored_total = op
        .reward_reservation
        .and_then(|reservation| reservation.total_unavailable_reward_debit_e8s())
        .unwrap_or(op.reserved_reward_debit_e8s.unwrap_or(0));
    if op.phase == OperationPhase::Completed
        && (stored_total != 0 || op.reserved_reward_debit_e8s.unwrap_or(0) != 0)
    {
        return Err(reward_accounting_error(
            op,
            "is completed but has nonzero reward reservation",
        ));
    }
    if op.phase == OperationPhase::Completed {
        return Ok(());
    }

    let has_external_effect_or_uncertainty =
        stable_reward_operation_has_external_effect_or_uncertainty(op);
    let Some(preflight) = op.reward_preflight.as_ref() else {
        if has_external_effect_or_uncertainty || stored_total != 0 {
            return Err(reward_accounting_error(
                op,
                "has external transfer evidence or reservation without preflight fee evidence",
            ));
        }
        return Ok(());
    };

    match preflight.status {
        RewardPreflightStatus::Validated => {
            validate_reward_operation_preflight(op, preflight)
                .map_err(|message| reward_accounting_error(op, message))?;
            for recipient in &op.two_week_recipients {
                validate_reward_recipient_accounting(op, recipient, preflight, mode)
                    .map_err(|message| reward_accounting_error(op, message))?;
            }
        }
        RewardPreflightStatus::Pending => {
            if let Some(evidence) = op.reward_fee_repreflight {
                if op.reward_reservation.is_none() && op.reserved_reward_debit_e8s.is_none() {
                    return Err(reward_accounting_error(
                        op,
                        "pending re-preflight is missing prior reservation",
                    ));
                }
                validate_reward_operation_preflight(op, preflight)
                    .map_err(|message| reward_accounting_error(op, message))?;
                for recipient in &op.two_week_recipients {
                    validate_reward_recipient_accounting(op, recipient, preflight, mode)
                        .map_err(|message| reward_accounting_error(op, message))?;
                }
                if stored_total != evidence.prior_reserved_debit_e8s
                    || op.reserved_reward_debit_e8s.unwrap_or(stored_total)
                        != evidence.prior_reserved_debit_e8s
                {
                    return Err(reward_accounting_error(
                        op,
                        "pending re-preflight reservation disagrees with prior debit evidence",
                    ));
                }
                if evidence.prior_validated_fee_e8s != preflight.ledger_fee_e8s {
                    return Err(reward_accounting_error(
                        op,
                        "pending re-preflight prior fee disagrees with retained preflight fee",
                    ));
                }
                if evidence.prior_validated_fee_e8s == evidence.observed_current_fee_e8s {
                    return Err(reward_accounting_error(
                        op,
                        "pending re-preflight has no fee change evidence",
                    ));
                }
            } else {
                if has_external_effect_or_uncertainty || stored_total != 0 {
                    return Err(reward_accounting_error(
                        op,
                        "has pending preflight with external evidence or reservation but no re-preflight evidence",
                    ));
                }
                if mode != RewardValidationMode::LegacyNormalization {
                    for recipient in &op.two_week_recipients {
                        if recipient.reward_transfer_attempt.is_some()
                            || recipient.ledger_transfer_fee_e8s.is_some()
                            || recipient.reserve_debit_e8s.is_some()
                        {
                            return Err(reward_accounting_error(
                                op,
                                "has pending initial preflight with transfer accounting evidence",
                            ));
                        }
                    }
                }
            }
        }
        RewardPreflightStatus::FailedTerminal => {
            if has_external_effect_or_uncertainty {
                validate_reward_operation_preflight(op, preflight)
                    .map_err(|message| reward_accounting_error(op, message))?;
                for recipient in &op.two_week_recipients {
                    validate_reward_recipient_accounting(op, recipient, preflight, mode)
                        .map_err(|message| reward_accounting_error(op, message))?;
                }
            } else if stored_total != 0 {
                return Err(reward_accounting_error(
                    op,
                    "terminal preflight failure has nonzero reward reservation",
                ));
            }
        }
        RewardPreflightStatus::ManualReconciliationRequired => {
            if !has_external_effect_or_uncertainty {
                return Err(reward_accounting_error(
                    op,
                    "manual reconciliation requires external transfer evidence or uncertainty",
                ));
            }
            validate_reward_operation_preflight(op, preflight)
                .map_err(|message| reward_accounting_error(op, message))?;
            for recipient in &op.two_week_recipients {
                validate_reward_recipient_accounting(op, recipient, preflight, mode)
                    .map_err(|message| reward_accounting_error(op, message))?;
            }
            if stored_total == 0 {
                return Err(reward_accounting_error(
                    op,
                    "manual reconciliation is missing unavailable reward reservation",
                ));
            }
        }
    }
    Ok(())
}

#[cfg_attr(not(any(test, target_family = "wasm")), allow(dead_code))]
pub(crate) fn validate_reward_operation_preflight(
    op: &StreamOperation,
    preflight: &RewardDistributionPreflight,
) -> Result<(), String> {
    let recipient_count = u64::try_from(op.two_week_recipients.len()).map_err(|_| {
        reward_operation_accounting_error(op, "recipient count does not fit in u64")
    })?;
    if preflight.recipient_count != recipient_count {
        return Err(reward_operation_accounting_error(
            op,
            "preflight recipient count mismatch",
        ));
    }

    let mut total_reward = 0_u128;
    let mut canonical_ids = Vec::with_capacity(op.two_week_recipients.len());
    let mut compatibility_keys = Vec::with_capacity(op.two_week_recipients.len());
    let mut canonical_seen = BTreeSet::new();
    let mut compatibility_seen = BTreeSet::new();
    for recipient in &op.two_week_recipients {
        total_reward = total_reward
            .checked_add(recipient.amount_e8s)
            .ok_or_else(|| reward_operation_accounting_error(op, "total reward overflowed"))?;
        let canonical = recipient.sns_neuron_id.clone().ok_or_else(|| {
            reward_operation_accounting_error(op, "recipient missing canonical SNS neuron id")
        })?;
        if canonical.len() != 32 {
            return Err(reward_operation_accounting_error(
                op,
                format!(
                    "canonical SNS neuron id must be exactly 32 bytes, got {}",
                    canonical.len()
                ),
            ));
        }
        if !canonical_seen.insert(canonical.clone()) {
            return Err(reward_operation_accounting_error(
                op,
                "duplicate canonical SNS neuron id",
            ));
        }
        if !compatibility_seen.insert(recipient.neuron_id) {
            return Err(reward_operation_accounting_error(
                op,
                "duplicate compatibility reward recipient key",
            ));
        }
        canonical_ids.push(canonical);
        compatibility_keys.push(recipient.neuron_id);
    }
    let total_fee = preflight
        .ledger_fee_e8s
        .checked_mul(u128::from(recipient_count))
        .ok_or_else(|| reward_operation_accounting_error(op, "total fee overflowed"))?;
    let total_debit = total_reward
        .checked_add(total_fee)
        .ok_or_else(|| reward_operation_accounting_error(op, "total debit overflowed"))?;
    let dust = op
        .io_issued_e8s
        .checked_sub(total_reward)
        .ok_or_else(|| reward_operation_accounting_error(op, "reward dust underflowed"))?;

    if preflight.total_reward_e8s != total_reward
        || preflight.total_fee_e8s != total_fee
        || preflight.total_reserve_debit_e8s != total_debit
        || preflight.dust_e8s != dust
        || preflight.canonical_recipient_ids != canonical_ids
        || preflight.compatibility_keys != compatibility_keys
    {
        return Err(reward_operation_accounting_error(
            op,
            "preflight totals or recipient identity/order mismatch",
        ));
    }
    Ok(())
}

pub(crate) fn reward_recipient_authoritative_debit(
    op: &StreamOperation,
    recipient: &TwoWeekRecipientTransfer,
    preflight: &RewardDistributionPreflight,
) -> Result<u128, String> {
    let fee = recipient
        .reward_transfer_attempt
        .as_ref()
        .map(|attempt| attempt.fee_e8s)
        .or(recipient.ledger_transfer_fee_e8s)
        .unwrap_or(preflight.ledger_fee_e8s);
    recipient
        .amount_e8s
        .checked_add(fee)
        .ok_or_else(|| reward_operation_accounting_error(op, "recipient reserve debit overflowed"))
}

#[cfg_attr(not(any(test, target_family = "wasm")), allow(dead_code))]
pub(crate) fn validate_reward_recipient_accounting(
    op: &StreamOperation,
    recipient: &TwoWeekRecipientTransfer,
    preflight: &RewardDistributionPreflight,
    mode: RewardValidationMode,
) -> Result<(), String> {
    let expected_debit = reward_recipient_authoritative_debit(op, recipient, preflight)?;
    if let Some(debit) = recipient.reserve_debit_e8s {
        if debit != expected_debit {
            return Err(reward_operation_accounting_error(
                op,
                format!("recipient reserve debit {debit} does not equal amount plus fee {expected_debit}"),
            ));
        }
    }
    if let Some(fee) = recipient.ledger_transfer_fee_e8s {
        if fee != preflight.ledger_fee_e8s {
            return Err(reward_operation_accounting_error(
                op,
                "recipient ledger fee evidence does not match preflight fee",
            ));
        }
    }

    let ledger_status = recipient
        .ledger_transfer_status
        .unwrap_or(recipient.transfer_status);
    let Some(attempt) = recipient.reward_transfer_attempt.as_ref() else {
        if mode != RewardValidationMode::LegacyNormalization
            && (ledger_status == TransferStatus::Succeeded
                || recipient.transfer_block_index.is_some()
                || recipient.ledger_transfer_block.is_some()
                || recipient.ledger_transfer_proof_scan_state.is_some()
                || recipient.ledger_transfer_fee_e8s.is_some()
                || recipient.reward_amount_received_e8s.is_some()
                || recipient.reserve_debit_e8s.is_some()
                || matches!(
                    recipient.governance_refresh_status,
                    Some(TransferStatus::Succeeded)
                )
                || recipient.observed_stake_after_e8s.is_some()
                || recipient.concurrent_stake_delta_e8s.is_some())
        {
            return Err(reward_operation_accounting_error(
                op,
                "recipient has transfer/proof/refresh/stake accounting evidence without durable attempt",
            ));
        }
        return Ok(());
    };
    if attempt.amount_e8s != recipient.amount_e8s {
        return Err(reward_operation_accounting_error(
            op,
            "attempt amount does not match recipient planned amount",
        ));
    }
    if attempt.fee_e8s
        != recipient
            .ledger_transfer_fee_e8s
            .unwrap_or(preflight.ledger_fee_e8s)
    {
        return Err(reward_operation_accounting_error(
            op,
            "attempt fee does not match recipient/preflight fee evidence",
        ));
    }
    if attempt
        .amount_e8s
        .checked_add(attempt.fee_e8s)
        .ok_or_else(|| reward_operation_accounting_error(op, "attempt debit overflowed"))?
        != expected_debit
    {
        return Err(reward_operation_accounting_error(
            op,
            "attempt debit does not match recipient reserve debit",
        ));
    }
    match &attempt.lifecycle {
        None => Err(reward_operation_accounting_error(
            op,
            "current reward attempt is missing lifecycle",
        )),
        Some(RewardTransferAttemptLifecycle::Prepared) => {
            if ledger_status == TransferStatus::Succeeded
                || recipient.transfer_block_index.is_some()
                || recipient.ledger_transfer_block.is_some()
                || recipient.ledger_transfer_proof_scan_state.is_some()
            {
                return Err(reward_operation_accounting_error(
                    op,
                    "prepared attempt has submitted/proof/success evidence",
                ));
            }
            Ok(())
        }
        Some(RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation })
        | Some(RewardTransferAttemptLifecycle::ProofRequired { generation, .. }) => {
            if *generation != attempt.created_at_time {
                return Err(reward_operation_accounting_error(
                    op,
                    "attempt lifecycle generation mismatch",
                ));
            }
            if ledger_status == TransferStatus::Succeeded
                || recipient.transfer_block_index.is_some()
                || recipient.ledger_transfer_block.is_some()
            {
                return Err(reward_operation_accounting_error(
                    op,
                    "submitted/proof-required attempt has proven block evidence",
                ));
            }
            Ok(())
        }
        Some(RewardTransferAttemptLifecycle::Proven { generation, block }) => {
            if *generation != attempt.created_at_time {
                return Err(reward_operation_accounting_error(
                    op,
                    "attempt lifecycle generation mismatch",
                ));
            }
            if ledger_status != TransferStatus::Succeeded
                || recipient.transfer_block_index != Some(*block)
                || recipient.ledger_transfer_block != Some(*block)
            {
                return Err(reward_operation_accounting_error(
                    op,
                    "proven block evidence mismatch",
                ));
            }
            Ok(())
        }
    }
}

fn stable_reward_recipient_debit(
    op: &StreamOperation,
    recipient: &TwoWeekRecipientTransfer,
    preflight: &RewardDistributionPreflight,
) -> Result<u128, StableMigrationError> {
    reward_recipient_authoritative_debit(op, recipient, preflight)
        .map_err(|message| corrupt_reward_snapshot(op, message))
        .and_then(|expected| match recipient.reserve_debit_e8s {
            Some(debit) if debit == expected => Ok(debit),
            Some(debit) => Err(corrupt_reward_snapshot(
                op,
                format!(
                    "recipient reserve debit {debit} does not equal amount plus fee {expected}"
                ),
            )),
            None => Ok(expected),
        })
}

fn derive_stable_reward_reservation_with_mode(
    op: &StreamOperation,
    mode: RewardValidationMode,
) -> Result<RewardReservation, StableMigrationError> {
    validate_reward_operation_accounting(op, None, mode)
        .map_err(|err| corrupt_reward_snapshot(op, err.to_string()))?;

    let Some(preflight) = &op.reward_preflight else {
        return Ok(RewardReservation {
            unspent_reserved_reward_debit_e8s: op.reserved_reward_debit_e8s.unwrap_or(0),
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
    };

    if preflight.status == RewardPreflightStatus::Pending {
        if let Some(evidence) = op.reward_fee_repreflight {
            let stored =
                op.reward_reservation
                    .ok_or_else(|| StableMigrationError::CorruptSnapshot {
                        canister: "io_stream_manager",
                        message: format!(
                            "reward operation {} pending re-preflight is missing prior reservation",
                            op.operation_id
                        ),
                    })?;
            let stored_total = checked_reservation_total_for_stable(op, stored).map_err(|_| {
                StableMigrationError::CorruptSnapshot {
                    canister: "io_stream_manager",
                    message: format!(
                        "reward operation {} pending re-preflight reservation overflowed",
                        op.operation_id
                    ),
                }
            })?;
            if stored_total != evidence.prior_reserved_debit_e8s
                || op.reserved_reward_debit_e8s.unwrap_or(stored_total)
                    != evidence.prior_reserved_debit_e8s
            {
                return Err(StableMigrationError::CorruptSnapshot {
                    canister: "io_stream_manager",
                    message: format!(
                        "reward operation {} pending re-preflight reservation disagrees with prior debit evidence",
                        op.operation_id
                    ),
                });
            }
            if evidence.prior_validated_fee_e8s == evidence.observed_current_fee_e8s {
                return Err(StableMigrationError::CorruptSnapshot {
                    canister: "io_stream_manager",
                    message: format!(
                        "reward operation {} pending re-preflight has no fee change evidence",
                        op.operation_id
                    ),
                });
            }
            return Ok(stored);
        }
        if op
            .reward_reservation
            .and_then(|reservation| reservation.total_unavailable_reward_debit_e8s())
            .unwrap_or(0)
            != 0
            || op.reserved_reward_debit_e8s.unwrap_or(0) != 0
        {
            return Err(StableMigrationError::CorruptSnapshot {
                canister: "io_stream_manager",
                message: format!(
                    "reward operation {} has pending preflight with reservation but no re-preflight evidence",
                    op.operation_id
                ),
            });
        }
    }

    if preflight.status != RewardPreflightStatus::Validated
        && !stable_reward_operation_has_external_effect_or_uncertainty(op)
    {
        return Ok(RewardReservation::default());
    }

    op.two_week_recipients.iter().try_fold(
        RewardReservation::default(),
        |mut reservation, recipient| {
            let debit = stable_reward_recipient_debit(op, recipient, preflight)?;
            if reward_recipient_has_spent_debit(recipient) {
                reservation.externally_spent_but_uncommitted_reward_debit_e8s = reservation
                    .externally_spent_but_uncommitted_reward_debit_e8s
                    .checked_add(debit)
                    .ok_or_else(|| StableMigrationError::CorruptSnapshot {
                        canister: "io_stream_manager",
                        message: format!(
                            "spent reward reservation overflowed in operation {}",
                            op.operation_id
                        ),
                    })?;
            } else {
                reservation.unspent_reserved_reward_debit_e8s = reservation
                    .unspent_reserved_reward_debit_e8s
                    .checked_add(debit)
                    .ok_or_else(|| StableMigrationError::CorruptSnapshot {
                        canister: "io_stream_manager",
                        message: format!(
                            "unspent reward reservation overflowed in operation {}",
                            op.operation_id
                        ),
                    })?;
            }
            Ok(reservation)
        },
    )
}

fn checked_reservation_total_for_stable(
    op: &StreamOperation,
    reservation: RewardReservation,
) -> Result<u128, StableMigrationError> {
    reservation
        .checked_total_unavailable_reward_debit_e8s()
        .map_err(|message| StableMigrationError::CorruptSnapshot {
            canister: "io_stream_manager",
            message: format!("operation {} {message}", op.operation_id),
        })
}

fn validate_reward_reservation_consistency(
    state: StableState,
    mode: RewardValidationMode,
) -> Result<StableState, StableMigrationError> {
    for op in &state.operation_journal {
        if op.kind != StreamOperationKind::TwoWeekMaturityStream
            && op.reward_reservation.is_none()
            && op.reserved_reward_debit_e8s.is_none()
        {
            continue;
        }
        let stored = op.reward_reservation.unwrap_or(RewardReservation {
            unspent_reserved_reward_debit_e8s: op.reserved_reward_debit_e8s.unwrap_or(0),
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        let stored_total = checked_reservation_total_for_stable(op, stored)?;
        let is_processed = state
            .processed_transactions
            .contains(&op.source_transaction_id);

        if op.phase == OperationPhase::Completed {
            if !is_processed {
                return Err(StableMigrationError::CorruptSnapshot {
                    canister: "io_stream_manager",
                    message: format!(
                        "completed reward operation {} is missing processed transaction evidence",
                        op.operation_id
                    ),
                });
            }
            if stored_total != 0 || op.reserved_reward_debit_e8s.unwrap_or(0) != 0 {
                return Err(StableMigrationError::CorruptSnapshot {
                    canister: "io_stream_manager",
                    message: format!(
                        "completed reward operation {} has nonzero reward reservation",
                        op.operation_id
                    ),
                });
            }
            continue;
        }
        if is_processed {
            return Err(StableMigrationError::CorruptSnapshot {
                canister: "io_stream_manager",
                message: format!(
                    "non-completed reward operation {} has processed transaction evidence",
                    op.operation_id
                ),
            });
        }

        let derived = derive_stable_reward_reservation_with_mode(op, mode)?;
        let derived_total = checked_reservation_total_for_stable(op, derived)?;
        if stored != derived {
            return Err(StableMigrationError::CorruptSnapshot {
                canister: "io_stream_manager",
                message: format!(
                    "reward operation {} reservation split disagrees with external evidence: stored {:?}, derived {:?}",
                    op.operation_id, stored, derived
                ),
            });
        }
        if let Some(legacy) = op.reserved_reward_debit_e8s {
            if legacy != derived_total {
                return Err(StableMigrationError::CorruptSnapshot {
                    canister: "io_stream_manager",
                    message: format!(
                        "reward operation {} legacy reservation {legacy} disagrees with split total {derived_total}",
                        op.operation_id
                    ),
                });
            }
        }
    }
    Ok(state)
}

fn normalize_legacy_reward_reservations(
    mut state: StableState,
) -> Result<StableState, StableMigrationError> {
    for op in &mut state.operation_journal {
        normalize_legacy_reward_attempt_lifecycles(op)?;
        if op.reward_reservation.is_some() {
            continue;
        }
        if op.kind != StreamOperationKind::TwoWeekMaturityStream
            && op.reserved_reward_debit_e8s.is_none()
        {
            continue;
        }
        if op.phase == OperationPhase::Completed
            && state
                .processed_transactions
                .contains(&op.source_transaction_id)
        {
            op.reward_reservation = Some(RewardReservation::default());
            op.reserved_reward_debit_e8s = Some(0);
            continue;
        }

        let mut reservation = derive_stable_reward_reservation_with_mode(
            op,
            RewardValidationMode::LegacyNormalization,
        )?;
        let derived_total = checked_reservation_total_for_stable(op, reservation)?;
        if let Some(legacy) = op.reserved_reward_debit_e8s {
            if legacy < derived_total {
                return Err(StableMigrationError::CorruptSnapshot {
                    canister: "io_stream_manager",
                    message: format!(
                        "legacy reward reservation {legacy} is smaller than proven debit {derived_total} in operation {}",
                        op.operation_id
                    ),
                });
            }
            if legacy > derived_total {
                let excess = legacy.checked_sub(derived_total).ok_or_else(|| {
                    StableMigrationError::CorruptSnapshot {
                        canister: "io_stream_manager",
                        message: format!(
                            "legacy reward reservation underflow in operation {}",
                            op.operation_id
                        ),
                    }
                })?;
                reservation.unspent_reserved_reward_debit_e8s = reservation
                    .unspent_reserved_reward_debit_e8s
                    .checked_add(excess)
                    .ok_or_else(|| StableMigrationError::CorruptSnapshot {
                        canister: "io_stream_manager",
                        message: format!(
                            "legacy reward reservation excess overflowed in operation {}",
                            op.operation_id
                        ),
                    })?;
            }
        }
        op.reserved_reward_debit_e8s = Some(checked_reservation_total_for_stable(op, reservation)?);
        op.reward_reservation = Some(reservation);
    }
    Ok(state)
}

fn normalize_legacy_reward_attempt_lifecycles(
    op: &mut StreamOperation,
) -> Result<(), StableMigrationError> {
    for recipient_index in 0..op.two_week_recipients.len() {
        let recipient = &mut op.two_week_recipients[recipient_index];
        let Some(attempt) = recipient.reward_transfer_attempt.as_mut() else {
            continue;
        };
        if attempt.lifecycle.is_some() {
            continue;
        }
        let ledger_status = recipient
            .ledger_transfer_status
            .unwrap_or(recipient.transfer_status);
        let block = match (
            recipient.transfer_block_index,
            recipient.ledger_transfer_block,
        ) {
            (Some(a), Some(b)) if a == b => Some(a),
            (Some(a), None) | (None, Some(a)) => {
                recipient.transfer_block_index = Some(a);
                recipient.ledger_transfer_block = Some(a);
                Some(a)
            }
            (None, None) => None,
            (Some(a), Some(b)) => {
                return Err(corrupt_reward_snapshot(
                    op,
                    format!(
                        "legacy recipient {recipient_index} has conflicting transfer blocks {a} and {b}"
                    ),
                ));
            }
        };
        attempt.lifecycle = match (ledger_status, block) {
            (TransferStatus::Succeeded, Some(block)) => {
                Some(RewardTransferAttemptLifecycle::Proven {
                    generation: attempt.created_at_time,
                    block,
                })
            }
            _ => Some(RewardTransferAttemptLifecycle::ProofRequired {
                generation: attempt.created_at_time,
                reason: "legacy reward attempt lacked lifecycle; proof reconciliation required"
                    .to_string(),
            }),
        };
        if matches!(
            attempt.lifecycle,
            Some(RewardTransferAttemptLifecycle::ProofRequired { .. })
        ) && recipient.ledger_transfer_proof_scan_state.is_none()
        {
            recipient.ledger_transfer_proof_scan_state =
                Some(io_ledger_types::AccountHistoryScanState::default());
        }
    }
    Ok(())
}

#[cfg_attr(not(any(test, debug_assertions)), allow(dead_code))]
fn decode_stable_state_bytes(bytes: &[u8]) -> Result<StableState, StableMigrationError> {
    if let Ok((snapshot,)) = candid::decode_args::<(LegacyPreV3VersionedStableState,)>(bytes) {
        if matches!(snapshot.schema_version, 0..=2) {
            return migrate_legacy_pre_v3_stable_state(snapshot);
        }
    }

    let versioned_err = match candid::decode_args::<(VersionedStableState,)>(bytes) {
        Ok((snapshot,)) => return migrate_stable_state(snapshot),
        Err(err) => err,
    };

    match candid::decode_args::<(LegacyPreV3StableState,)>(bytes) {
        Ok((state,)) => migrate_legacy_pre_v3_stable_state(LegacyPreV3VersionedStableState {
            schema_version: 0,
            state,
        }),
        Err(legacy_unversioned_err) => match candid::decode_args::<(StableState,)>(bytes) {
            Ok((state,)) => migrate_stable_state(VersionedStableState {
                schema_version: 0,
                state,
            }),
            Err(unversioned_err) => Err(StableMigrationError::CorruptSnapshot {
                canister: "io_stream_manager",
                message: format!(
                    "failed to decode versioned stable state: {versioned_err}; failed to decode legacy unversioned stable state: {legacy_unversioned_err}; failed to decode current unversioned stable state: {unversioned_err}"
                ),
            }),
        },
    }
}

pub fn default_first_install_stable_state() -> StableState {
    CanisterState::default().into()
}

impl From<CanisterState> for StableState {
    fn from(state: CanisterState) -> Self {
        Self {
            config: state.config,
            protocol: state.manager.state.into(),
            processed_transactions: state.manager.processed_transactions.into_iter().collect(),
            active_staked_io_e8s: state.manager.active_staked_io_e8s,
            reward_cohort: state.reward_cohort,
            operation_journal: state.operation_journal,
            scheduler_cursors: state.scheduler_cursors,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum StreamOperationKind {
    JupiterFaucetStream,
    TwoYearMaturityStream,
    TwoWeekMaturityStream,
    Redemption,
    RejectedRedemption,
    PrincipalUnwind,
    UnknownIcpDeposit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum OperationPhase {
    Observed,
    Previewed,
    AwaitingIoIssuance,
    AwaitingIcpPayout,
    AwaitingIoReturn,
    PartiallyDistributed,
    Completed,
    FailedRetryable,
    FailedTerminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum TransferStatus {
    Pending,
    Succeeded,
    FailedRetryable,
    FailedTerminal,
    NotApplicable,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RejectedRefundAttemptRecord {
    pub attempted_refund_amount_e8s: u128,
    pub attempted_fee_e8s: u128,
    pub attempted_created_at_time: u64,
    pub memo: Option<io_ledger_types::Memo>,
    pub refund_source_account: io_ledger_types::Account,
    pub destination_account: io_ledger_types::Account,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RejectedFundDisposition {
    ReturnToSenderPending,
    ReturnToSenderSucceeded {
        block_index: u64,
        amount_e8s: u128,
    },
    ReturnToSenderProofPending {
        reason: String,
        original_created_at_time: Option<u64>,
        proof_scan_state: Option<io_ledger_types::AccountHistoryScanState>,
    },
    ReturnToSenderRetryable {
        error: String,
        next_attempt_created_at_time: Option<u64>,
    },
    ReturnToSenderManualReconciliationRequired {
        reason: String,
        original_created_at_time: Option<u64>,
    },
    QuarantinedTerminal {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardTransferAttemptRecord {
    pub amount_e8s: u128,
    pub fee_e8s: u128,
    pub created_at_time: u64,
    pub memo: Option<io_ledger_types::Memo>,
    pub source_account: io_ledger_types::Account,
    pub destination_account: io_ledger_types::Account,
    pub canonical_sns_neuron_id: Vec<u8>,
    pub lifecycle: Option<RewardTransferAttemptLifecycle>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RewardTransferAttemptLifecycle {
    Prepared,
    SubmittedAwaitingResult { generation: u64 },
    ProofRequired { generation: u64, reason: String },
    Proven { generation: u64, block: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RewardPreflightStatus {
    Pending,
    Validated,
    FailedTerminal,
    ManualReconciliationRequired,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardDistributionPreflight {
    pub status: RewardPreflightStatus,
    pub ledger_fee_e8s: u128,
    pub recipient_count: u64,
    pub total_reward_e8s: u128,
    pub total_fee_e8s: u128,
    pub total_reserve_debit_e8s: u128,
    pub protocol_reserve_available_e8s: u128,
    pub real_ledger_reserve_balance_e8s: u128,
    pub validated_at_timestamp_nanos: u64,
    pub canonical_recipient_ids: Vec<Vec<u8>>,
    pub compatibility_keys: Vec<u64>,
    pub dust_e8s: u128,
    pub failure_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardReservation {
    pub unspent_reserved_reward_debit_e8s: u128,
    pub externally_spent_but_uncommitted_reward_debit_e8s: u128,
}

impl RewardReservation {
    pub fn checked_total_unavailable_reward_debit_e8s(&self) -> Result<u128, String> {
        self.unspent_reserved_reward_debit_e8s
            .checked_add(self.externally_spent_but_uncommitted_reward_debit_e8s)
            .ok_or_else(|| "reward reservation total overflowed".to_string())
    }

    pub fn total_unavailable_reward_debit_e8s(&self) -> Option<u128> {
        self.checked_total_unavailable_reward_debit_e8s().ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardFeeRepreflightEvidence {
    pub prior_validated_fee_e8s: u128,
    pub observed_current_fee_e8s: u128,
    pub prior_reserved_debit_e8s: u128,
    pub invalidated_at_timestamp_nanos: u64,
    pub attempt_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TwoWeekRecipientTransfer {
    pub sns_neuron_id: Option<Vec<u8>>,
    pub neuron_id: u64,
    pub amount_e8s: u128,
    pub transfer_status: TransferStatus,
    pub transfer_block_index: Option<u64>,
    pub ledger_transfer_status: Option<TransferStatus>,
    pub ledger_transfer_block: Option<u64>,
    pub governance_refresh_status: Option<TransferStatus>,
    pub stake_before_e8s: Option<u128>,
    pub expected_stake_after_e8s: Option<u128>,
    pub minimum_expected_stake_after_e8s: Option<u128>,
    pub observed_stake_after_e8s: Option<u128>,
    pub concurrent_stake_delta_e8s: Option<u128>,
    pub refresh_retry_count: Option<u32>,
    pub refresh_last_error: Option<String>,
    pub reward_transfer_attempt: Option<RewardTransferAttemptRecord>,
    pub ledger_transfer_fee_e8s: Option<u128>,
    pub reward_amount_received_e8s: Option<u128>,
    pub reserve_debit_e8s: Option<u128>,
    pub ledger_transfer_proof_scan_state: Option<io_ledger_types::AccountHistoryScanState>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StreamOperation {
    pub operation_id: String,
    pub source_ledger: String,
    pub source_block_index: Option<u64>,
    pub source_transaction_id: String,
    pub kind: StreamOperationKind,
    pub phase: OperationPhase,
    pub amount_e8s: u128,
    pub created_at: u64,
    pub last_updated: u64,
    pub retry_count: u32,
    pub last_error: Option<String>,
    pub post_state: StableProtocolState,
    pub io_issued_e8s: u128,
    pub downstream_io_issuance_block: Option<u64>,
    pub two_week_recipients: Vec<TwoWeekRecipientTransfer>,
    pub io_redemption_block: Option<u64>,
    pub io_amount: u128,
    pub gross_icp_payout_e8s: u128,
    pub icp_payout_fee_e8s: u128,
    pub net_user_icp_payout_e8s: u128,
    pub io_return_fee_e8s: u128,
    pub icp_payout_status: TransferStatus,
    pub io_return_status: TransferStatus,
    pub icp_payout_block: Option<u64>,
    pub io_return_block: Option<u64>,
    pub user_account: Option<String>,
    pub source_account: Option<io_ledger_types::Account>,
    pub rejected_fund_disposition: Option<RejectedFundDisposition>,
    pub rejected_refund_attempt: Option<RejectedRefundAttemptRecord>,
    pub reward_preflight: Option<RewardDistributionPreflight>,
    pub reward_reservation: Option<RewardReservation>,
    pub reward_fee_repreflight: Option<RewardFeeRepreflightEvidence>,
    /// Legacy v1 scalar reservation. Current code derives live reservation
    /// state from `reward_reservation` and recipient transfer evidence.
    pub reserved_reward_debit_e8s: Option<u128>,
}

#[cfg(any(test, debug_assertions))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum DebugFailpoint {
    AfterRejectedRefundTransferBeforeJournalUpdate,
    AfterTwoWeekRewardPreflightBeforeTransfer,
    AfterTwoWeekRewardTransferBeforeJournalUpdate,
    AfterTwoWeekRewardTransferBeforeGovernanceRefresh,
    AfterTwoWeekGovernanceRefreshBeforeJournalCompletion,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct SchedulerCursors {
    pub last_scanned_icp_index_block: Option<u64>,
    pub last_scanned_io_index_block: Option<u64>,
    pub icp_account_history_scan: io_ledger_types::AccountHistoryScanState,
    pub io_account_history_scan: io_ledger_types::AccountHistoryScanState,
}

#[cfg(target_family = "wasm")]
fn canister_time() -> u64 {
    ic_cdk::api::time()
}

#[cfg(not(target_family = "wasm"))]
fn canister_time() -> u64 {
    0
}

impl StreamOperation {
    pub fn stream(
        source_ledger: impl Into<String>,
        source_block_index: u64,
        kind: StreamOperationKind,
        amount_e8s: u128,
        post_state: ProtocolState,
        io_issued_e8s: u128,
        phase: OperationPhase,
    ) -> Self {
        let source_ledger = source_ledger.into();
        let operation_id = format!("{source_ledger}:{source_block_index}");
        let now = canister_time();
        Self {
            operation_id: operation_id.clone(),
            source_ledger,
            source_block_index: Some(source_block_index),
            source_transaction_id: operation_id,
            kind,
            phase,
            amount_e8s,
            created_at: now,
            last_updated: now,
            retry_count: 0,
            last_error: None,
            post_state: post_state.into(),
            io_issued_e8s,
            downstream_io_issuance_block: None,
            two_week_recipients: Vec::new(),
            io_redemption_block: None,
            io_amount: 0,
            gross_icp_payout_e8s: 0,
            icp_payout_fee_e8s: 0,
            net_user_icp_payout_e8s: 0,
            io_return_fee_e8s: 0,
            icp_payout_status: TransferStatus::Pending,
            io_return_status: TransferStatus::Pending,
            icp_payout_block: None,
            io_return_block: None,
            user_account: None,
            source_account: None,
            rejected_fund_disposition: None,
            rejected_refund_attempt: None,
            reward_preflight: None,
            reward_reservation: None,
            reward_fee_repreflight: None,
            reserved_reward_debit_e8s: None,
        }
    }

    pub fn redemption(
        source_block_index: u64,
        io_amount: u128,
        icp_paid_e8s: u128,
        user_account: String,
        post_state: ProtocolState,
    ) -> Self {
        let mut op = Self::stream(
            "io",
            source_block_index,
            StreamOperationKind::Redemption,
            io_amount,
            post_state,
            0,
            OperationPhase::AwaitingIcpPayout,
        );
        op.io_redemption_block = Some(source_block_index);
        op.io_amount = io_amount;
        op.amount_e8s = icp_paid_e8s;
        op.gross_icp_payout_e8s = icp_paid_e8s;
        op.net_user_icp_payout_e8s = icp_paid_e8s;
        op.user_account = Some(user_account);
        op
    }

    pub fn effective_net_user_icp_payout_e8s(&self) -> u128 {
        if self.kind == StreamOperationKind::Redemption && self.net_user_icp_payout_e8s == 0 {
            self.amount_e8s
        } else {
            self.net_user_icp_payout_e8s
        }
    }

    #[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
    fn mark_retryable_error(&mut self, err: String, phase: OperationPhase) {
        self.phase = phase;
        self.retry_count = self.retry_count.saturating_add(1);
        self.last_error = Some(err);
        self.last_updated = canister_time();
    }

    #[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
    fn mark_updated(&mut self, phase: OperationPhase) {
        self.phase = phase;
        self.last_error = None;
        self.last_updated = canister_time();
    }

    #[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
    fn mark_terminal_error(&mut self, err: String, phase: OperationPhase) {
        self.retry_count = self.retry_count.saturating_add(1);
        self.last_updated = canister_time();
        self.phase = OperationPhase::FailedTerminal;
        self.last_error = Some(format!("{phase:?}: {err}"));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ApiStreamKind {
    JupiterFaucet,
    TwoYearMaturity,
    TwoWeekMaturity,
}

impl From<ApiStreamKind> for StreamKind {
    fn from(value: ApiStreamKind) -> Self {
        match value {
            ApiStreamKind::JupiterFaucet => StreamKind::JupiterFaucet,
            ApiStreamKind::TwoYearMaturity => StreamKind::TwoYearMaturity,
            ApiStreamKind::TwoWeekMaturity => StreamKind::TwoWeekMaturity,
        }
    }
}

impl From<StreamKind> for ApiStreamKind {
    fn from(value: StreamKind) -> Self {
        match value {
            StreamKind::JupiterFaucet => ApiStreamKind::JupiterFaucet,
            StreamKind::TwoYearMaturity => ApiStreamKind::TwoYearMaturity,
            StreamKind::TwoWeekMaturity => ApiStreamKind::TwoWeekMaturity,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ApiIoRecipientPolicy {
    JupiterFaucet,
    EligibleIoSnsNeurons,
    None,
}

impl From<IoRecipientPolicy> for ApiIoRecipientPolicy {
    fn from(value: IoRecipientPolicy) -> Self {
        match value {
            IoRecipientPolicy::JupiterFaucet => ApiIoRecipientPolicy::JupiterFaucet,
            IoRecipientPolicy::EligibleIoSnsNeurons => ApiIoRecipientPolicy::EligibleIoSnsNeurons,
            IoRecipientPolicy::None => ApiIoRecipientPolicy::None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ProcessStreamEventRequest {
    pub kind: ApiStreamKind,
    pub amount_e8s: u128,
    pub transaction_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiProtocolState {
    pub liquid_icp_e8s: u128,
    pub two_year_staked_icp_e8s: u128,
    pub two_week_staked_icp_e8s: u128,
    pub total_io_supply_e8s: u128,
    pub protocol_reserve_io_e8s: u128,
    pub non_redeemable_governance_io_e8s: u128,
}

impl From<ProtocolState> for ApiProtocolState {
    fn from(value: ProtocolState) -> Self {
        Self {
            liquid_icp_e8s: value.liquid_icp_e8s,
            two_year_staked_icp_e8s: value.two_year_staked_icp_e8s,
            two_week_staked_icp_e8s: value.two_week_staked_icp_e8s,
            total_io_supply_e8s: value.total_io_supply_e8s,
            protocol_reserve_io_e8s: value.protocol_reserve_io_e8s,
            non_redeemable_governance_io_e8s: value.non_redeemable_governance_io_e8s,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiRedemptionRate {
    pub liquid_icp_e8s: u128,
    pub redeemable_io_e8s: u128,
}

impl From<RedemptionRate> for ApiRedemptionRate {
    fn from(value: RedemptionRate) -> Self {
        Self {
            liquid_icp_e8s: value.liquid_icp_e8s,
            redeemable_io_e8s: value.redeemable_io_e8s,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiSplit {
    pub stake_e8s: u128,
    pub liquid_e8s: u128,
}

impl From<Split> for ApiSplit {
    fn from(value: Split) -> Self {
        Self {
            stake_e8s: value.stake_e8s,
            liquid_e8s: value.liquid_e8s,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiStreamOutcome {
    pub kind: ApiStreamKind,
    pub split: ApiSplit,
    pub recipient_policy: ApiIoRecipientPolicy,
    pub io_issued_e8s: u128,
    pub rate_before: ApiRedemptionRate,
    pub rate_after: ApiRedemptionRate,
}

impl From<StreamOutcome> for ApiStreamOutcome {
    fn from(value: StreamOutcome) -> Self {
        Self {
            kind: value.kind.into(),
            split: value.split.into(),
            recipient_policy: value.recipient_policy.into(),
            io_issued_e8s: value.io_issued_e8s,
            rate_before: value.rate_before.into(),
            rate_after: value.rate_after.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiRedemptionOutcome {
    pub io_redeemed_e8s: u128,
    pub icp_paid_e8s: u128,
    pub gross_icp_payout_e8s: u128,
    pub icp_ledger_fee_e8s: u128,
    pub net_user_icp_payout_e8s: u128,
    pub io_returned_to_reserve_e8s: u128,
    pub dust_retained_icp_e8s: u128,
    pub rate_before: ApiRedemptionRate,
    pub rate_after: ApiRedemptionRate,
}

impl From<RedemptionOutcome> for ApiRedemptionOutcome {
    fn from(value: RedemptionOutcome) -> Self {
        Self {
            io_redeemed_e8s: value.io_redeemed_e8s,
            icp_paid_e8s: value.icp_paid_e8s,
            gross_icp_payout_e8s: value.gross_icp_payout_e8s,
            icp_ledger_fee_e8s: value.icp_ledger_fee_e8s,
            net_user_icp_payout_e8s: value.net_user_icp_payout_e8s,
            io_returned_to_reserve_e8s: value.io_returned_to_reserve_e8s,
            dust_retained_icp_e8s: value.dust_retained_icp_e8s,
            rate_before: value.rate_before.into(),
            rate_after: value.rate_after.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiState {
    pub config: StreamManagerConfig,
    pub protocol: ApiProtocolState,
    pub processed_transaction_count: u64,
    pub active_staked_io_e8s: u128,
    pub reward_cohort: Option<RewardCohort>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugTickOutcome {
    pub scanned_icp_transactions: u64,
    pub scanned_io_transactions: u64,
    pub processed_authorized_streams: u64,
    pub processed_redemptions: u64,
    pub io_issued_e8s: u128,
    pub icp_paid_e8s: u128,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl ApiError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<ModelError> for ApiError {
    fn from(value: ModelError) -> Self {
        Self::new("model_error", format!("{value:?}"))
    }
}

impl From<StreamManagerError> for ApiError {
    fn from(value: StreamManagerError) -> Self {
        match value {
            StreamManagerError::DuplicateTransaction => {
                Self::new("duplicate_transaction", "transaction was already processed")
            }
            StreamManagerError::InvalidTransactionId => {
                Self::new("invalid_transaction_id", "transaction id must be non-empty")
            }
            StreamManagerError::UnknownOrUnauthorizedStream { source, memo } => Self::new(
                "unknown_or_unauthorized_stream",
                format!("stream source {source:?} with memo {memo:?} is not authorized"),
            ),
            StreamManagerError::Model(err) => err.into(),
            StreamManagerError::RewardPolicy(err) => {
                Self::new("reward_policy_error", format!("{err:?}"))
            }
        }
    }
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg_attr(target_family = "wasm", ic_cdk::init)]
pub fn init(args: InitArgs) {
    let config = StreamManagerConfig::try_from(args).expect("invalid io_stream_manager init args");
    CANISTER_STATE.with(|cell| {
        *cell.borrow_mut() = CanisterState::new(config);
    });
}

fn export_stable_state() -> StableState {
    CANISTER_STATE.with(|cell| {
        let state = cell.borrow();
        StableState {
            config: state.config.clone(),
            protocol: state.manager.state.into(),
            processed_transactions: state
                .manager
                .processed_transactions
                .iter()
                .cloned()
                .collect(),
            active_staked_io_e8s: state.manager.active_staked_io_e8s,
            reward_cohort: state.reward_cohort.clone(),
            operation_journal: state.operation_journal.clone(),
            scheduler_cursors: state.scheduler_cursors.clone(),
        }
    })
}

fn export_versioned_stable_state() -> VersionedStableState {
    VersionedStableState {
        schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
        state: export_stable_state(),
    }
}

fn import_stable_state(state: StableState) {
    CANISTER_STATE.with(|cell| {
        *cell.borrow_mut() = CanisterState {
            config: state.config,
            manager: StreamManager {
                state: state.protocol.into(),
                processed_transactions: state.processed_transactions.into_iter().collect(),
                active_staked_io_e8s: state.active_staked_io_e8s,
            },
            operation_journal: state.operation_journal,
            scheduler_cursors: state.scheduler_cursors,
            reward_cohort: state.reward_cohort,
            #[cfg(any(test, debug_assertions))]
            debug_failpoint: None,
        };
    });
}

#[cfg(any(test, debug_assertions))]
fn set_debug_failpoint_impl(failpoint: Option<DebugFailpoint>) {
    CANISTER_STATE.with(|cell| {
        cell.borrow_mut().debug_failpoint = failpoint;
    });
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
pub(crate) fn consume_debug_failpoint(failpoint: DebugFailpoint) -> bool {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.debug_failpoint == Some(failpoint) {
            state.debug_failpoint = None;
            true
        } else {
            false
        }
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::pre_upgrade)]
pub fn pre_upgrade() {
    ic_cdk::storage::stable_save((export_versioned_stable_state(),))
        .expect("failed to save io_stream_manager stable state");
}

#[cfg_attr(target_family = "wasm", ic_cdk::post_upgrade)]
pub fn post_upgrade() {
    let state = match ic_cdk::storage::stable_restore::<(LegacyPreV3VersionedStableState,)>() {
        Ok((snapshot,)) if matches!(snapshot.schema_version, 0..=2) => {
            migrate_legacy_pre_v3_stable_state(snapshot)
        }
        _ => match ic_cdk::storage::stable_restore::<(VersionedStableState,)>() {
            Ok((snapshot,)) => migrate_stable_state(snapshot),
            Err(versioned_err) => match ic_cdk::storage::stable_restore::<(LegacyPreV3StableState,)>() {
                Ok((state,)) => {
                    migrate_legacy_pre_v3_stable_state(LegacyPreV3VersionedStableState {
                        schema_version: 0,
                        state,
                    })
                }
                Err(legacy_unversioned_err) => {
                    match ic_cdk::storage::stable_restore::<(StableState,)>() {
                        Ok((state,)) => migrate_stable_state(VersionedStableState {
                            schema_version: 0,
                            state,
                        }),
                        Err(unversioned_err) => Err(StableMigrationError::CorruptSnapshot {
                            canister: "io_stream_manager",
                            message: format!(
                                "failed to restore versioned stable state: {versioned_err}; failed to restore legacy unversioned stable state: {legacy_unversioned_err}; failed to restore current unversioned stable state: {unversioned_err}"
                            ),
                        }),
                    }
                }
            },
        },
    }
    .expect("io_stream_manager stable state is missing, corrupt, or unsupported during upgrade");
    import_stable_state(state);
}

#[cfg(any(test, debug_assertions))]
pub fn export_stable_state_for_tests() -> StableState {
    export_stable_state()
}

#[cfg(any(test, debug_assertions))]
pub fn export_versioned_stable_state_for_tests() -> VersionedStableState {
    export_versioned_stable_state()
}

#[cfg(any(test, debug_assertions))]
pub fn import_stable_state_for_tests(state: StableState) {
    import_stable_state(state);
}

#[cfg(any(test, debug_assertions))]
pub fn migrate_stable_state_for_tests(
    snapshot: VersionedStableState,
) -> Result<StableState, StableMigrationError> {
    migrate_stable_state(snapshot)
}

#[cfg(any(test, debug_assertions))]
pub fn decode_stable_state_bytes_for_tests(
    bytes: &[u8],
) -> Result<StableState, StableMigrationError> {
    decode_stable_state_bytes(bytes)
}

#[cfg(any(test, debug_assertions))]
fn state_snapshot() -> ApiState {
    CANISTER_STATE.with(|cell| {
        let state = cell.borrow();
        ApiState {
            config: state.config.clone(),
            protocol: state.manager.state.into(),
            processed_transaction_count: state.manager.processed_transactions.len() as u64,
            active_staked_io_e8s: state.manager.active_staked_io_e8s,
            reward_cohort: state.reward_cohort.clone(),
        }
    })
}

#[cfg(any(test, debug_assertions))]
fn redemption_rate() -> Result<ApiRedemptionRate, ApiError> {
    CANISTER_STATE.with(|cell| {
        cell.borrow()
            .manager
            .state
            .redemption_rate()
            .map(ApiRedemptionRate::from)
            .map_err(ApiError::from)
    })
}

#[cfg(any(test, debug_assertions))]
fn process_stream_event_impl(
    request: ProcessStreamEventRequest,
) -> Result<ApiStreamOutcome, ApiError> {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state
            .manager
            .process_authorized_stream(
                request.kind.into(),
                request.amount_e8s,
                request.transaction_id,
            )
            .map(ApiStreamOutcome::from)
            .map_err(ApiError::from)
    })
}

#[cfg(any(test, debug_assertions))]
fn redeem_impl(io_e8s: u128) -> Result<ApiRedemptionOutcome, ApiError> {
    CANISTER_STATE.with(|cell| {
        cell.borrow_mut()
            .manager
            .redeem(io_e8s)
            .map(ApiRedemptionOutcome::from)
            .map_err(ApiError::from)
    })
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_state() -> ApiState {
    state_snapshot()
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_stable_state() -> StableState {
    export_stable_state()
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_redemption_rate() -> Result<ApiRedemptionRate, ApiError> {
    redemption_rate()
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_process_stream_event(
    request: ProcessStreamEventRequest,
) -> Result<ApiStreamOutcome, ApiError> {
    process_stream_event_impl(request)
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_redeem(io_e8s: u128) -> Result<ApiRedemptionOutcome, ApiError> {
    redeem_impl(io_e8s)
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn debug_tick() -> DebugTickOutcome {
    scheduler::scheduler_tick_once().await
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_failpoint(failpoint: Option<DebugFailpoint>) {
    set_debug_failpoint_impl(failpoint);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{
        IO_NNS_NEURON_MANAGER_SOURCE, JUPITER_FAUCET_SOURCE, TWO_YEAR_MATURITY_MEMO,
    };
    fn t(n: u128) -> u128 {
        n * E8S_PER_TOKEN
    }

    #[test]
    fn manager_accepts_faucet_stream() {
        let mut m = StreamManager::default_for_tests();
        let out = m
            .process_authorized_stream(StreamKind::JupiterFaucet, t(100), "tx-1")
            .unwrap();
        assert_eq!(out.io_issued_e8s, t(60));
        assert!(matches!(
            m.process_authorized_stream(StreamKind::JupiterFaucet, t(100), "tx-1"),
            Err(StreamManagerError::DuplicateTransaction)
        ));
    }

    #[test]
    fn manager_redeems_to_reserve() {
        let mut m = StreamManager::default_for_tests();
        m.process_authorized_stream(StreamKind::JupiterFaucet, t(100), "tx-1")
            .unwrap();
        let out = m.redeem(t(10)).unwrap();
        assert_eq!(out.icp_paid_e8s, t(10));
        assert_eq!(m.state.protocol_reserve_io_e8s, t(899_950));
    }

    #[test]
    fn scanned_source_and_memo_classify_streams() {
        assert_eq!(
            StreamManager::classify_stream(JUPITER_FAUCET_SOURCE, "").unwrap(),
            StreamKind::JupiterFaucet
        );
        assert_eq!(
            StreamManager::classify_stream(IO_NNS_NEURON_MANAGER_SOURCE, TWO_YEAR_MATURITY_MEMO)
                .unwrap(),
            StreamKind::TwoYearMaturity
        );
        assert!(matches!(
            StreamManager::classify_stream("unknown", ""),
            Err(StreamManagerError::UnknownOrUnauthorizedStream { .. })
        ));
    }

    #[test]
    fn failed_stream_does_not_mark_transaction_processed() {
        let mut m = StreamManager::default_for_tests();
        m.state.protocol_reserve_io_e8s = t(1);
        let err = m
            .process_authorized_stream(StreamKind::JupiterFaucet, t(100), "bad-tx")
            .unwrap_err();
        assert!(matches!(
            err,
            StreamManagerError::Model(ModelError::InsufficientProtocolReserve { .. })
        ));
        assert!(!m.processed_transactions.contains("bad-tx"));
    }

    #[test]
    fn canister_api_initializes_and_reports_state() {
        init(InitArgs::default());
        let state = debug_get_state();
        assert_eq!(
            state.protocol.total_io_supply_e8s,
            1_000_000 * E8S_PER_TOKEN
        );
        assert_eq!(state.processed_transaction_count, 0);
        assert_eq!(
            debug_get_redemption_rate().unwrap(),
            RedemptionRate::one_to_one().into()
        );
    }

    #[test]
    fn canister_api_processes_stream_and_redeems() {
        init(InitArgs::default());
        let outcome = debug_process_stream_event(ProcessStreamEventRequest {
            kind: ApiStreamKind::JupiterFaucet,
            amount_e8s: t(100),
            transaction_id: "api-tx-1".to_string(),
        })
        .unwrap();
        assert_eq!(outcome.io_issued_e8s, t(60));
        assert_eq!(
            debug_process_stream_event(ProcessStreamEventRequest {
                kind: ApiStreamKind::JupiterFaucet,
                amount_e8s: t(100),
                transaction_id: "api-tx-1".to_string(),
            })
            .unwrap_err()
            .code,
            "duplicate_transaction"
        );

        let redemption = debug_redeem(t(10)).unwrap();
        assert_eq!(redemption.icp_paid_e8s, t(10));
        assert_eq!(debug_get_state().processed_transaction_count, 1);
    }

    #[test]
    fn init_rejects_supply_config_that_cannot_be_valid() {
        let args = InitArgs {
            initial_total_io_supply_e8s: 10,
            initial_protocol_reserve_io_e8s: 9,
            non_redeemable_governance_io_e8s: 2,
            ..InitArgs::default()
        };
        assert_eq!(
            StreamManagerConfig::try_from(args).unwrap_err(),
            InitArgsError::ExcludedSupplyExceedsTotal
        );
    }

    #[test]
    fn init_rejects_invalid_optional_principal_text() {
        let args = InitArgs {
            jupiter_faucet_principal_text: Some("not a principal".to_string()),
            ..InitArgs::default()
        };
        assert_eq!(
            StreamManagerConfig::try_from(args).unwrap_err(),
            InitArgsError::InvalidPrincipalText {
                field: "jupiter_faucet_principal_text",
                value: "not a principal".to_string()
            }
        );
    }

    #[test]
    fn init_accepts_local_sns_shaped_principals() {
        let config = StreamManagerConfig::try_from(InitArgs {
            jupiter_faucet_principal_text: Some("aaaaa-aa".to_string()),
            io_nns_neuron_manager_principal_text: Some("oae4c-3iaaa-aaaar-qb5qq-cai".to_string()),
            icp_ledger_principal_text: Some("bkyz2-fmaaa-aaaaa-qaaaq-cai".to_string()),
            icp_index_principal_text: Some("bd3sg-teaaa-aaaaa-qaaba-cai".to_string()),
            io_ledger_principal_text: Some("br5f7-7uaaa-aaaaa-qaaca-cai".to_string()),
            io_index_principal_text: Some("be2us-64aaa-aaaaa-qaabq-cai".to_string()),
            io_sns_ledger_principal_text: Some("bw4dl-smaaa-aaaaa-qaacq-cai".to_string()),
            io_sns_index_principal_text: Some("b77ix-eeaaa-aaaaa-qaada-cai".to_string()),
            sns_governance_principal_text: Some("by6od-j4aaa-aaaaa-qaadq-cai".to_string()),
            ..InitArgs::default()
        })
        .unwrap();

        assert_eq!(
            config.sns_governance_principal_text.as_deref(),
            Some("by6od-j4aaa-aaaaa-qaadq-cai")
        );
    }

    #[test]
    fn init_rejects_malformed_local_sns_principals() {
        assert_eq!(
            StreamManagerConfig::try_from(InitArgs {
                sns_governance_principal_text: Some("not-sns-governance".to_string()),
                ..InitArgs::default()
            })
            .unwrap_err(),
            InitArgsError::InvalidPrincipalText {
                field: "sns_governance_principal_text",
                value: "not-sns-governance".to_string()
            }
        );
        assert_eq!(
            StreamManagerConfig::try_from(InitArgs {
                io_sns_ledger_principal_text: Some("not-sns-ledger".to_string()),
                ..InitArgs::default()
            })
            .unwrap_err(),
            InitArgsError::InvalidPrincipalText {
                field: "io_sns_ledger_principal_text",
                value: "not-sns-ledger".to_string()
            }
        );
        assert_eq!(
            StreamManagerConfig::try_from(InitArgs {
                io_sns_index_principal_text: Some("not-sns-index".to_string()),
                ..InitArgs::default()
            })
            .unwrap_err(),
            InitArgsError::InvalidPrincipalText {
                field: "io_sns_index_principal_text",
                value: "not-sns-index".to_string()
            }
        );
    }

    fn dry_run_wiring() -> ProductionWiringConfig {
        use io_production_wiring::{
            DeploymentTargets, FeePolicyWiring, IoLedgerRole, PrincipalWiring, ProtectedReferences,
            WiringMode, ICP_INDEX_PRINCIPAL, ICP_LEDGER_PRINCIPAL, ICP_TRANSFER_FEE_E8S,
            NNS_GOVERNANCE_PRINCIPAL, PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
            PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID, PROTECTED_IO_NEURON_OWNER_CANISTER,
            PROTECTED_IO_NNS_NEURON_ID,
        };

        ProductionWiringConfig {
            mode: WiringMode::DryRun,
            io_ledger_role: IoLedgerRole::FutureCanonicalSnsIo,
            fixture_marked: false,
            principals: PrincipalWiring {
                icp_ledger_principal_text: Some(ICP_LEDGER_PRINCIPAL.to_string()),
                icp_index_principal_text: Some(ICP_INDEX_PRINCIPAL.to_string()),
                nns_governance_principal_text: Some(NNS_GOVERNANCE_PRINCIPAL.to_string()),
                nns_ledger_principal_text: Some(ICP_LEDGER_PRINCIPAL.to_string()),
                nns_index_principal_text: Some(ICP_INDEX_PRINCIPAL.to_string()),
                sns_root_principal_text: Some("qaa6y-5yaaa-aaaaa-aaafa-cai".to_string()),
                sns_governance_principal_text: Some("r7inp-6aaaa-aaaaa-aaabq-cai".to_string()),
                sns_ledger_principal_text: Some("qjdve-lqaaa-aaaaa-aaaeq-cai".to_string()),
                sns_index_principal_text: Some("renrk-eyaaa-aaaaa-aaada-cai".to_string()),
                io_ledger_principal_text: Some("qjdve-lqaaa-aaaaa-aaaeq-cai".to_string()),
                io_index_principal_text: Some("renrk-eyaaa-aaaaa-aaada-cai".to_string()),
            },
            fee_policy: FeePolicyWiring {
                icp_transfer_fee_e8s: Some(ICP_TRANSFER_FEE_E8S),
                io_ledger_transfer_fee_e8s: Some(10_000),
                tiny_value_policy_max_fee_e8s: Some(1_000_000),
                allow_zero_fees_for_mock_or_local: false,
            },
            protected: ProtectedReferences {
                neuron_owner_canister_principal_text: Some(
                    PROTECTED_IO_NEURON_OWNER_CANISTER.to_string(),
                ),
                io_nns_neuron_id: Some(PROTECTED_IO_NNS_NEURON_ID),
            },
            deployment_targets: DeploymentTargets {
                io_stream_manager_principal_text: Some(
                    PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID.to_string(),
                ),
                io_nns_neuron_manager_principal_text: Some(
                    PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID.to_string(),
                ),
                mutation_target_principal_texts: Vec::new(),
                mutation_target_nns_neuron_ids: Vec::new(),
            },
        }
    }

    #[test]
    fn install_args_accept_valid_dry_run_wiring() {
        let config = StreamManagerConfig::try_from(InitArgs {
            production_wiring: Some(dry_run_wiring()),
            ..InitArgs::default()
        })
        .unwrap();

        assert!(config.production_wiring.is_some());
    }

    #[test]
    fn install_args_reject_invalid_production_planned_wiring() {
        let mut wiring = dry_run_wiring();
        wiring.mode = io_production_wiring::WiringMode::ProductionPlanned;
        wiring.principals.icp_ledger_principal_text = None;

        assert!(matches!(
            StreamManagerConfig::try_from(InitArgs {
                production_wiring: Some(wiring),
                ..InitArgs::default()
            })
            .unwrap_err(),
            InitArgsError::InvalidProductionWiring { .. }
        ));
    }

    #[test]
    fn default_install_args_do_not_enable_production_wiring() {
        let config = StreamManagerConfig::try_from(InitArgs::default()).unwrap();

        assert!(config.production_wiring.is_none());
    }

    #[test]
    fn stable_state_round_trip_preserves_config_accounting_and_processed_txs() {
        init(InitArgs {
            initial_total_io_supply_e8s: t(2_000),
            initial_protocol_reserve_io_e8s: t(1_200),
            non_redeemable_governance_io_e8s: t(300),
            jupiter_faucet_principal_text: Some("oae4c-3iaaa-aaaar-qb5qq-cai".to_string()),
            ..InitArgs::default()
        });
        debug_process_stream_event(ProcessStreamEventRequest {
            kind: ApiStreamKind::JupiterFaucet,
            amount_e8s: t(100),
            transaction_id: "stable-tx-1".to_string(),
        })
        .unwrap();
        debug_redeem(t(10)).unwrap();
        let before_state = debug_get_state();
        let before_rate = debug_get_redemption_rate().unwrap();
        let stable = export_stable_state_for_tests();

        init(InitArgs::default());
        assert_ne!(debug_get_state(), before_state);

        import_stable_state_for_tests(stable);
        assert_eq!(debug_get_state(), before_state);
        assert_eq!(debug_get_redemption_rate().unwrap(), before_rate);
        assert_eq!(debug_get_state().processed_transaction_count, 1);
    }

    #[test]
    fn stable_state_round_trip_preserves_operation_journal_and_cursors() {
        init(InitArgs::default());
        let mut op = StreamOperation::stream(
            "icp",
            7,
            StreamOperationKind::TwoWeekMaturityStream,
            t(100),
            ProtocolState::new(t(1_000_000), t(900_000), t(100_000)),
            t(60),
            OperationPhase::PartiallyDistributed,
        );
        op.two_week_recipients = vec![
            TwoWeekRecipientTransfer {
                sns_neuron_id: Some(10_u64.to_be_bytes().to_vec()),
                neuron_id: 10,
                amount_e8s: t(40),
                transfer_status: TransferStatus::Succeeded,
                transfer_block_index: Some(1),
                ledger_transfer_status: Some(TransferStatus::Succeeded),
                ledger_transfer_block: Some(1),
                governance_refresh_status: Some(TransferStatus::Succeeded),
                stake_before_e8s: Some(t(100)),
                expected_stake_after_e8s: Some(t(140)),
                minimum_expected_stake_after_e8s: Some(t(140)),
                observed_stake_after_e8s: Some(t(140)),
                concurrent_stake_delta_e8s: None,
                refresh_retry_count: Some(0),
                refresh_last_error: None,
                reward_transfer_attempt: Some(RewardTransferAttemptRecord {
                    amount_e8s: t(40),
                    fee_e8s: 10_000,
                    created_at_time: 88,
                    memo: Some(io_ledger_types::Memo::from("two_week_reward:icp:7")),
                    source_account: io_ledger_types::Account::new(
                        candid::Principal::anonymous(),
                        Some(io_ledger_types::Subaccount([1; 32])),
                    ),
                    destination_account: io_ledger_types::Account::new(
                        candid::Principal::anonymous(),
                        Some(io_ledger_types::Subaccount([10; 32])),
                    ),
                    canonical_sns_neuron_id: vec![10; 32],
                    lifecycle: Some(RewardTransferAttemptLifecycle::Proven {
                        generation: 88,
                        block: 1,
                    }),
                }),
                ledger_transfer_fee_e8s: Some(10_000),
                reward_amount_received_e8s: Some(t(40)),
                reserve_debit_e8s: Some(t(40) + 10_000),
                ledger_transfer_proof_scan_state: None,
                last_error: None,
            },
            TwoWeekRecipientTransfer {
                sns_neuron_id: Some(11_u64.to_be_bytes().to_vec()),
                neuron_id: 11,
                amount_e8s: t(20),
                transfer_status: TransferStatus::FailedRetryable,
                transfer_block_index: None,
                ledger_transfer_status: Some(TransferStatus::FailedRetryable),
                ledger_transfer_block: None,
                governance_refresh_status: Some(TransferStatus::Pending),
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
                last_error: Some("reject".to_string()),
            },
        ];
        let redemption = StreamOperation::redemption(
            9,
            t(10),
            t(10),
            "user".to_string(),
            ProtocolState::new(t(1_000_000), t(900_000), t(100_000)),
        );
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal.push(op);
            state.operation_journal.push(redemption);
            state.scheduler_cursors.last_scanned_icp_index_block = Some(7);
            state.scheduler_cursors.last_scanned_io_index_block = Some(9);
        });

        let stable = export_stable_state_for_tests();
        init(InitArgs::default());
        import_stable_state_for_tests(stable.clone());
        assert_eq!(
            export_stable_state_for_tests().operation_journal,
            stable.operation_journal
        );
        assert_eq!(
            export_stable_state_for_tests().scheduler_cursors,
            stable.scheduler_cursors
        );
    }

    fn pending_redemption_fixture() -> StableState {
        init(InitArgs::default());
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            let mut redemption = StreamOperation::redemption(
                99,
                t(10),
                t(9),
                "user-account".to_string(),
                ProtocolState::new(t(1_000_000), t(900_000), t(100_000)),
            );
            redemption.phase = OperationPhase::FailedRetryable;
            redemption.retry_count = 2;
            redemption.last_error = Some("transient icp ledger failure".to_string());
            redemption.gross_icp_payout_e8s = t(9);
            redemption.icp_payout_fee_e8s = 10_000;
            redemption.net_user_icp_payout_e8s = t(9) - 10_000;
            redemption.io_return_fee_e8s = 10_000;
            redemption.icp_payout_status = TransferStatus::FailedRetryable;
            redemption.io_return_status = TransferStatus::FailedRetryable;
            state.operation_journal.push(redemption);
            state
                .manager
                .processed_transactions
                .insert("duplicate-proof:99".to_string());
            state.scheduler_cursors.last_scanned_icp_index_block = Some(99);
            state.scheduler_cursors.last_scanned_io_index_block = Some(100);
            state
                .scheduler_cursors
                .icp_account_history_scan
                .cursor
                .latest_cursor = Some(io_ledger_types::BlockIndex(99));
        });
        export_stable_state_for_tests()
    }

    fn legacy_pre_v3_snapshot_from_current(
        schema_version: u32,
        state: StableState,
        config_backing_bps: u128,
        state_backing_bps: u128,
    ) -> LegacyPreV3VersionedStableState {
        LegacyPreV3VersionedStableState {
            schema_version,
            state: LegacyPreV3StableState {
                config: LegacyPreV3StreamManagerConfig {
                    initial_total_io_supply_e8s: state.config.initial_total_io_supply_e8s,
                    initial_protocol_reserve_io_e8s: state.config.initial_protocol_reserve_io_e8s,
                    non_redeemable_governance_io_e8s: state.config.non_redeemable_governance_io_e8s,
                    two_week_pool_backing_bps: config_backing_bps,
                    jupiter_faucet_principal_text: state.config.jupiter_faucet_principal_text,
                    io_nns_neuron_manager_principal_text: state
                        .config
                        .io_nns_neuron_manager_principal_text,
                    icp_ledger_principal_text: state.config.icp_ledger_principal_text,
                    icp_index_principal_text: state.config.icp_index_principal_text,
                    io_ledger_principal_text: state.config.io_ledger_principal_text,
                    io_index_principal_text: state.config.io_index_principal_text,
                    io_sns_ledger_principal_text: state.config.io_sns_ledger_principal_text,
                    io_sns_index_principal_text: state.config.io_sns_index_principal_text,
                    sns_governance_principal_text: state.config.sns_governance_principal_text,
                    production_wiring: state.config.production_wiring,
                },
                protocol: state.protocol,
                processed_transactions: state.processed_transactions,
                active_staked_io_e8s: state.active_staked_io_e8s,
                two_week_pool_backing_bps: state_backing_bps,
                operation_journal: state.operation_journal,
                scheduler_cursors: state.scheduler_cursors,
            },
        }
    }

    fn legacy_pre_v3_unversioned_from_current(
        state: StableState,
        config_backing_bps: u128,
        state_backing_bps: u128,
    ) -> LegacyPreV3StableState {
        legacy_pre_v3_snapshot_from_current(0, state, config_backing_bps, state_backing_bps).state
    }

    #[test]
    fn stream_manager_migrates_previous_stable_fixture() {
        let fixture = pending_redemption_fixture();
        let migrated = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: 0,
            state: fixture.clone(),
        })
        .unwrap();

        assert_eq!(migrated, fixture);
        assert!(migrated.config.production_wiring.is_none());
    }

    #[test]
    fn stream_manager_decodes_legacy_unversioned_stable_root() {
        let fixture = pending_redemption_fixture();
        let bytes = candid::encode_args((fixture.clone(),)).unwrap();
        let migrated = decode_stable_state_bytes_for_tests(&bytes).unwrap();

        assert_eq!(migrated, fixture);
        assert!(migrated.config.production_wiring.is_none());
    }

    #[test]
    fn current_versioned_snapshot_restores() {
        let fixture = pending_redemption_fixture();
        let bytes = candid::encode_args((VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state: fixture.clone(),
        },))
        .unwrap();
        let migrated = decode_stable_state_bytes_for_tests(&bytes).unwrap();

        assert_eq!(migrated, fixture);
    }

    #[test]
    fn v0_full_backing_unversioned_migrates_without_cohort() {
        let fixture = pending_redemption_fixture();
        let legacy = legacy_pre_v3_unversioned_from_current(fixture.clone(), 10_000, 10_000);
        let bytes = candid::encode_args((legacy,)).unwrap();
        let migrated = decode_stable_state_bytes_for_tests(&bytes).unwrap();

        assert_eq!(migrated.reward_cohort, None);
        assert_eq!(migrated.operation_journal, fixture.operation_journal);
        assert_eq!(migrated.scheduler_cursors, fixture.scheduler_cursors);
    }

    #[test]
    fn v0_non_full_backing_unversioned_fails_closed() {
        for (config_bps, state_bps) in [(7_500, 10_000), (10_000, 7_500)] {
            let legacy = legacy_pre_v3_unversioned_from_current(
                pending_redemption_fixture(),
                config_bps,
                state_bps,
            );
            let bytes = candid::encode_args((legacy,)).unwrap();
            let err = decode_stable_state_bytes_for_tests(&bytes).unwrap_err();
            assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
        }
    }

    #[test]
    fn v1_full_backing_versioned_migrates_without_cohort() {
        let fixture = pending_redemption_fixture();
        let legacy = legacy_pre_v3_snapshot_from_current(1, fixture.clone(), 10_000, 10_000);
        let bytes = candid::encode_args((legacy,)).unwrap();
        let migrated = decode_stable_state_bytes_for_tests(&bytes).unwrap();

        assert_eq!(migrated.reward_cohort, None);
        assert_eq!(migrated.operation_journal, fixture.operation_journal);
        assert_eq!(migrated.scheduler_cursors, fixture.scheduler_cursors);
    }

    #[test]
    fn v1_non_full_backing_versioned_fails_closed() {
        for (config_bps, state_bps) in [(7_500, 10_000), (10_000, 7_500)] {
            let legacy = legacy_pre_v3_snapshot_from_current(
                1,
                pending_redemption_fixture(),
                config_bps,
                state_bps,
            );
            let bytes = candid::encode_args((legacy,)).unwrap();
            let err = decode_stable_state_bytes_for_tests(&bytes).unwrap_err();
            assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
        }
    }

    #[test]
    fn v2_full_backing_versioned_migrates_without_cohort() {
        let fixture = pending_redemption_fixture();
        let legacy = legacy_pre_v3_snapshot_from_current(2, fixture.clone(), 10_000, 10_000);
        let bytes = candid::encode_args((legacy,)).unwrap();
        let migrated = decode_stable_state_bytes_for_tests(&bytes).unwrap();

        assert_eq!(migrated.reward_cohort, None);
        assert_eq!(migrated.operation_journal, fixture.operation_journal);
        assert_eq!(migrated.scheduler_cursors, fixture.scheduler_cursors);
    }

    #[test]
    fn v2_non_full_backing_versioned_fails_closed() {
        for (config_bps, state_bps) in [(7_500, 10_000), (10_000, 7_500)] {
            let legacy = legacy_pre_v3_snapshot_from_current(
                2,
                pending_redemption_fixture(),
                config_bps,
                state_bps,
            );
            let bytes = candid::encode_args((legacy,)).unwrap();
            let err = decode_stable_state_bytes_for_tests(&bytes).unwrap_err();

            assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
        }
    }

    fn cohort_id(value: u8) -> Vec<u8> {
        vec![value; 32]
    }

    #[test]
    fn cohort_roundtrip_preserves_generation_members_and_consumed_operation() {
        let mut state = default_first_install_stable_state();
        let mut op = StreamOperation::stream(
            "icp",
            1,
            StreamOperationKind::TwoWeekMaturityStream,
            t(10),
            ProtocolState::new(t(1_000_000), t(900_000), t(100_000)),
            t(6),
            OperationPhase::PartiallyDistributed,
        );
        op.operation_id = "cohort-op".to_string();
        state.operation_journal.push(op);
        state.reward_cohort = Some(RewardCohort {
            generation: 7,
            captured_at_timestamp_seconds: 100,
            members: vec![
                RewardCohortMember {
                    sns_neuron_id: cohort_id(1),
                    frozen_stake_e8s: 10,
                },
                RewardCohortMember {
                    sns_neuron_id: cohort_id(2),
                    frozen_stake_e8s: 20,
                },
            ],
            consumed_by_operation_id: Some("cohort-op".to_string()),
        });

        let migrated = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state: state.clone(),
        })
        .unwrap();

        assert_eq!(migrated.reward_cohort, state.reward_cohort);
    }

    #[test]
    fn corrupt_cohort_duplicate_ids_fail_restore() {
        let mut state = default_first_install_stable_state();
        state.reward_cohort = Some(RewardCohort {
            generation: 1,
            captured_at_timestamp_seconds: 1,
            members: vec![
                RewardCohortMember {
                    sns_neuron_id: cohort_id(1),
                    frozen_stake_e8s: 10,
                },
                RewardCohortMember {
                    sns_neuron_id: cohort_id(1),
                    frozen_stake_e8s: 20,
                },
            ],
            consumed_by_operation_id: None,
        });

        let err = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state,
        })
        .unwrap_err();

        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn corrupt_cohort_unknown_consuming_operation_fails_restore() {
        let mut state = default_first_install_stable_state();
        state.reward_cohort = Some(RewardCohort {
            generation: 1,
            captured_at_timestamp_seconds: 1,
            members: vec![RewardCohortMember {
                sns_neuron_id: cohort_id(1),
                frozen_stake_e8s: 10,
            }],
            consumed_by_operation_id: Some("missing".to_string()),
        });

        let err = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state,
        })
        .unwrap_err();

        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn v1_reward_reservation_migrates_to_split_buckets() {
        let mut state = default_first_install_stable_state();
        let post_state = ProtocolState {
            liquid_icp_e8s: 300_000_000,
            two_year_staked_icp_e8s: 0,
            two_week_staked_icp_e8s: 200_000_000,
            total_io_supply_e8s: 100_000_000_000,
            protocol_reserve_io_e8s: 99_700_000_000,
            non_redeemable_governance_io_e8s: 0,
        };
        let mut op = StreamOperation::stream(
            "icp",
            77,
            StreamOperationKind::TwoWeekMaturityStream,
            500_000_000,
            post_state,
            300_000_000,
            OperationPhase::PartiallyDistributed,
        );
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
            canonical_recipient_ids: vec![vec![1; 32], vec![2; 32]],
            compatibility_keys: vec![1, 2],
            dust_e8s: 0,
            failure_reason: None,
        });
        op.two_week_recipients = vec![
            TwoWeekRecipientTransfer {
                sns_neuron_id: Some(vec![1; 32]),
                neuron_id: 1,
                amount_e8s: 100_000_000,
                transfer_status: TransferStatus::Succeeded,
                transfer_block_index: Some(42),
                ledger_transfer_status: Some(TransferStatus::Succeeded),
                ledger_transfer_block: Some(42),
                governance_refresh_status: Some(TransferStatus::Pending),
                stake_before_e8s: None,
                expected_stake_after_e8s: None,
                minimum_expected_stake_after_e8s: None,
                observed_stake_after_e8s: None,
                concurrent_stake_delta_e8s: None,
                refresh_retry_count: None,
                refresh_last_error: None,
                reward_transfer_attempt: None,
                ledger_transfer_fee_e8s: Some(10_000),
                reward_amount_received_e8s: Some(100_000_000),
                reserve_debit_e8s: Some(100_010_000),
                ledger_transfer_proof_scan_state: None,
                last_error: None,
            },
            TwoWeekRecipientTransfer {
                sns_neuron_id: Some(vec![2; 32]),
                neuron_id: 2,
                amount_e8s: 200_000_000,
                transfer_status: TransferStatus::Pending,
                transfer_block_index: None,
                ledger_transfer_status: Some(TransferStatus::Pending),
                ledger_transfer_block: None,
                governance_refresh_status: Some(TransferStatus::Pending),
                stake_before_e8s: None,
                expected_stake_after_e8s: None,
                minimum_expected_stake_after_e8s: None,
                observed_stake_after_e8s: None,
                concurrent_stake_delta_e8s: None,
                refresh_retry_count: None,
                refresh_last_error: None,
                reward_transfer_attempt: None,
                ledger_transfer_fee_e8s: None,
                reward_amount_received_e8s: None,
                reserve_debit_e8s: None,
                ledger_transfer_proof_scan_state: None,
                last_error: None,
            },
        ];
        add_current_attempt(
            &mut op,
            RewardTransferAttemptLifecycle::Proven {
                generation: 1234,
                block: 42,
            },
        );
        op.two_week_recipients[0].transfer_block_index = Some(42);
        op.two_week_recipients[0].ledger_transfer_block = Some(42);
        op.reserved_reward_debit_e8s = Some(300_020_000);
        op.reward_reservation = None;
        state.operation_journal = vec![op];

        let migrated = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: 1,
            state,
        })
        .unwrap();

        assert_eq!(
            migrated.operation_journal[0].reward_reservation,
            Some(RewardReservation {
                unspent_reserved_reward_debit_e8s: 200_010_000,
                externally_spent_but_uncommitted_reward_debit_e8s: 100_010_000,
            })
        );
        assert_eq!(
            migrated.operation_journal[0].reserved_reward_debit_e8s,
            Some(300_020_000)
        );
    }

    fn stable_reward_operation_with_one_recipient(
        phase: OperationPhase,
        recipient_status: TransferStatus,
    ) -> StreamOperation {
        let post_state = ProtocolState {
            liquid_icp_e8s: 300_000_000,
            two_year_staked_icp_e8s: 0,
            two_week_staked_icp_e8s: 200_000_000,
            total_io_supply_e8s: 100_000_000_000,
            protocol_reserve_io_e8s: 99_700_000_000,
            non_redeemable_governance_io_e8s: 0,
        };
        let mut op = StreamOperation::stream(
            "icp",
            177,
            StreamOperationKind::TwoWeekMaturityStream,
            500_000_000,
            post_state,
            100_000_000,
            phase,
        );
        op.reward_preflight = Some(RewardDistributionPreflight {
            status: RewardPreflightStatus::Validated,
            ledger_fee_e8s: 10_000,
            recipient_count: 1,
            total_reward_e8s: 100_000_000,
            total_fee_e8s: 10_000,
            total_reserve_debit_e8s: 100_010_000,
            protocol_reserve_available_e8s: 100_010_000,
            real_ledger_reserve_balance_e8s: 100_010_000,
            validated_at_timestamp_nanos: 123,
            canonical_recipient_ids: vec![vec![1; 32]],
            compatibility_keys: vec![1],
            dust_e8s: 0,
            failure_reason: None,
        });
        op.two_week_recipients = vec![TwoWeekRecipientTransfer {
            sns_neuron_id: Some(vec![1; 32]),
            neuron_id: 1,
            amount_e8s: 100_000_000,
            transfer_status: recipient_status,
            transfer_block_index: (recipient_status == TransferStatus::Succeeded).then_some(42),
            ledger_transfer_status: Some(recipient_status),
            ledger_transfer_block: (recipient_status == TransferStatus::Succeeded).then_some(42),
            governance_refresh_status: Some(TransferStatus::Pending),
            stake_before_e8s: None,
            expected_stake_after_e8s: None,
            minimum_expected_stake_after_e8s: None,
            observed_stake_after_e8s: None,
            concurrent_stake_delta_e8s: None,
            refresh_retry_count: None,
            refresh_last_error: None,
            reward_transfer_attempt: None,
            ledger_transfer_fee_e8s: (recipient_status == TransferStatus::Succeeded)
                .then_some(10_000),
            reward_amount_received_e8s: (recipient_status == TransferStatus::Succeeded)
                .then_some(100_000_000),
            reserve_debit_e8s: (recipient_status == TransferStatus::Succeeded)
                .then_some(100_010_000),
            ledger_transfer_proof_scan_state: None,
            last_error: None,
        }];
        op
    }

    #[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
    struct LegacyV1VersionedStableState {
        schema_version: u32,
        state: LegacyV1StableState,
    }

    #[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
    struct LegacyV1StableState {
        config: StreamManagerConfig,
        protocol: StableProtocolState,
        processed_transactions: Vec<String>,
        active_staked_io_e8s: u128,
        two_week_pool_backing_bps: u128,
        operation_journal: Vec<LegacyV1StreamOperation>,
        scheduler_cursors: SchedulerCursors,
    }

    #[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
    struct LegacyV1StreamOperation {
        operation_id: String,
        source_ledger: String,
        source_block_index: Option<u64>,
        source_transaction_id: String,
        kind: StreamOperationKind,
        phase: OperationPhase,
        amount_e8s: u128,
        created_at: u64,
        last_updated: u64,
        retry_count: u32,
        last_error: Option<String>,
        post_state: StableProtocolState,
        io_issued_e8s: u128,
        downstream_io_issuance_block: Option<u64>,
        two_week_recipients: Vec<LegacyV1TwoWeekRecipientTransfer>,
        io_redemption_block: Option<u64>,
        io_amount: u128,
        gross_icp_payout_e8s: u128,
        icp_payout_fee_e8s: u128,
        net_user_icp_payout_e8s: u128,
        io_return_fee_e8s: u128,
        icp_payout_status: TransferStatus,
        io_return_status: TransferStatus,
        icp_payout_block: Option<u64>,
        io_return_block: Option<u64>,
        user_account: Option<String>,
        source_account: Option<io_ledger_types::Account>,
        rejected_fund_disposition: Option<RejectedFundDisposition>,
        rejected_refund_attempt: Option<RejectedRefundAttemptRecord>,
        reward_preflight: Option<RewardDistributionPreflight>,
        reserved_reward_debit_e8s: Option<u128>,
    }

    #[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
    struct LegacyV1RewardTransferAttemptRecord {
        amount_e8s: u128,
        fee_e8s: u128,
        created_at_time: u64,
        memo: Option<io_ledger_types::Memo>,
        source_account: io_ledger_types::Account,
        destination_account: io_ledger_types::Account,
        canonical_sns_neuron_id: Vec<u8>,
    }

    #[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
    struct LegacyV1TwoWeekRecipientTransfer {
        sns_neuron_id: Option<Vec<u8>>,
        neuron_id: u64,
        amount_e8s: u128,
        transfer_status: TransferStatus,
        transfer_block_index: Option<u64>,
        ledger_transfer_status: Option<TransferStatus>,
        ledger_transfer_block: Option<u64>,
        governance_refresh_status: Option<TransferStatus>,
        stake_before_e8s: Option<u128>,
        expected_stake_after_e8s: Option<u128>,
        minimum_expected_stake_after_e8s: Option<u128>,
        observed_stake_after_e8s: Option<u128>,
        concurrent_stake_delta_e8s: Option<u128>,
        refresh_retry_count: Option<u32>,
        refresh_last_error: Option<String>,
        reward_transfer_attempt: Option<LegacyV1RewardTransferAttemptRecord>,
        ledger_transfer_fee_e8s: Option<u128>,
        reward_amount_received_e8s: Option<u128>,
        reserve_debit_e8s: Option<u128>,
        ledger_transfer_proof_scan_state: Option<io_ledger_types::AccountHistoryScanState>,
        last_error: Option<String>,
    }

    impl From<StableState> for LegacyV1StableState {
        fn from(state: StableState) -> Self {
            Self {
                config: state.config,
                protocol: state.protocol,
                processed_transactions: state.processed_transactions,
                active_staked_io_e8s: state.active_staked_io_e8s,
                two_week_pool_backing_bps: 10_000,
                operation_journal: state
                    .operation_journal
                    .into_iter()
                    .map(LegacyV1StreamOperation::from)
                    .collect(),
                scheduler_cursors: state.scheduler_cursors,
            }
        }
    }

    impl From<StreamOperation> for LegacyV1StreamOperation {
        fn from(op: StreamOperation) -> Self {
            Self {
                operation_id: op.operation_id,
                source_ledger: op.source_ledger,
                source_block_index: op.source_block_index,
                source_transaction_id: op.source_transaction_id,
                kind: op.kind,
                phase: op.phase,
                amount_e8s: op.amount_e8s,
                created_at: op.created_at,
                last_updated: op.last_updated,
                retry_count: op.retry_count,
                last_error: op.last_error,
                post_state: op.post_state,
                io_issued_e8s: op.io_issued_e8s,
                downstream_io_issuance_block: op.downstream_io_issuance_block,
                two_week_recipients: op
                    .two_week_recipients
                    .into_iter()
                    .map(LegacyV1TwoWeekRecipientTransfer::from)
                    .collect(),
                io_redemption_block: op.io_redemption_block,
                io_amount: op.io_amount,
                gross_icp_payout_e8s: op.gross_icp_payout_e8s,
                icp_payout_fee_e8s: op.icp_payout_fee_e8s,
                net_user_icp_payout_e8s: op.net_user_icp_payout_e8s,
                io_return_fee_e8s: op.io_return_fee_e8s,
                icp_payout_status: op.icp_payout_status,
                io_return_status: op.io_return_status,
                icp_payout_block: op.icp_payout_block,
                io_return_block: op.io_return_block,
                user_account: op.user_account,
                source_account: op.source_account,
                rejected_fund_disposition: op.rejected_fund_disposition,
                rejected_refund_attempt: op.rejected_refund_attempt,
                reward_preflight: op.reward_preflight,
                reserved_reward_debit_e8s: op.reserved_reward_debit_e8s,
            }
        }
    }

    impl From<RewardTransferAttemptRecord> for LegacyV1RewardTransferAttemptRecord {
        fn from(attempt: RewardTransferAttemptRecord) -> Self {
            Self {
                amount_e8s: attempt.amount_e8s,
                fee_e8s: attempt.fee_e8s,
                created_at_time: attempt.created_at_time,
                memo: attempt.memo,
                source_account: attempt.source_account,
                destination_account: attempt.destination_account,
                canonical_sns_neuron_id: attempt.canonical_sns_neuron_id,
            }
        }
    }

    impl From<TwoWeekRecipientTransfer> for LegacyV1TwoWeekRecipientTransfer {
        fn from(recipient: TwoWeekRecipientTransfer) -> Self {
            Self {
                sns_neuron_id: recipient.sns_neuron_id,
                neuron_id: recipient.neuron_id,
                amount_e8s: recipient.amount_e8s,
                transfer_status: recipient.transfer_status,
                transfer_block_index: recipient.transfer_block_index,
                ledger_transfer_status: recipient.ledger_transfer_status,
                ledger_transfer_block: recipient.ledger_transfer_block,
                governance_refresh_status: recipient.governance_refresh_status,
                stake_before_e8s: recipient.stake_before_e8s,
                expected_stake_after_e8s: recipient.expected_stake_after_e8s,
                minimum_expected_stake_after_e8s: recipient.minimum_expected_stake_after_e8s,
                observed_stake_after_e8s: recipient.observed_stake_after_e8s,
                concurrent_stake_delta_e8s: recipient.concurrent_stake_delta_e8s,
                refresh_retry_count: recipient.refresh_retry_count,
                refresh_last_error: recipient.refresh_last_error,
                reward_transfer_attempt: recipient
                    .reward_transfer_attempt
                    .map(LegacyV1RewardTransferAttemptRecord::from),
                ledger_transfer_fee_e8s: recipient.ledger_transfer_fee_e8s,
                reward_amount_received_e8s: recipient.reward_amount_received_e8s,
                reserve_debit_e8s: recipient.reserve_debit_e8s,
                ledger_transfer_proof_scan_state: recipient.ledger_transfer_proof_scan_state,
                last_error: recipient.last_error,
            }
        }
    }

    fn decode_legacy_v1_bytes(state: StableState) -> Result<StableState, StableMigrationError> {
        let bytes = candid::encode_args((LegacyV1VersionedStableState {
            schema_version: 1,
            state: LegacyV1StableState::from(state),
        },))
        .unwrap();
        decode_stable_state_bytes_for_tests(&bytes)
    }

    fn legacy_state_with_reward_op(op: StreamOperation) -> StableState {
        let mut state = default_first_install_stable_state();
        state.operation_journal = vec![op];
        state
    }

    fn current_reward_state_with_op(op: StreamOperation) -> StableState {
        legacy_state_with_reward_op(op)
    }

    fn migrate_current_reward_state(
        state: StableState,
    ) -> Result<StableState, StableMigrationError> {
        migrate_stable_state_for_tests(VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state,
        })
    }

    fn assert_state_passes_current_live_validation(state: &StableState) {
        let processed = state
            .processed_transactions
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        for op in &state.operation_journal {
            validate_reward_operation_accounting(
                op,
                Some(&processed),
                RewardValidationMode::Current,
            )
            .unwrap();
        }
    }

    fn expect_current_reward_restore_rejects(op: StreamOperation) {
        assert!(matches!(
            migrate_current_reward_state(current_reward_state_with_op(op)),
            Err(StableMigrationError::CorruptSnapshot { .. })
        ));
    }

    fn add_current_attempt(op: &mut StreamOperation, lifecycle: RewardTransferAttemptLifecycle) {
        op.two_week_recipients[0].reward_transfer_attempt = Some(RewardTransferAttemptRecord {
            amount_e8s: 100_000_000,
            fee_e8s: 10_000,
            created_at_time: 1234,
            memo: None,
            source_account: io_ledger_types::Account::new(
                candid::Principal::anonymous(),
                Some(io_ledger_types::Subaccount([1; 32])),
            ),
            destination_account: io_ledger_types::Account::new(
                candid::Principal::anonymous(),
                Some(io_ledger_types::Subaccount([2; 32])),
            ),
            canonical_sns_neuron_id: vec![1; 32],
            lifecycle: Some(lifecycle),
        });
        op.two_week_recipients[0].ledger_transfer_fee_e8s = Some(10_000);
        op.two_week_recipients[0].reward_amount_received_e8s = Some(100_000_000);
        op.two_week_recipients[0].reserve_debit_e8s = Some(100_010_000);
    }

    #[test]
    fn legacy_terminal_transfer_without_attempt_fails_closed() {
        let mut state = default_first_install_stable_state();
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::FailedTerminal,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);
        state.operation_journal = vec![op];

        let err = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: 1,
            state,
        })
        .unwrap_err();

        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn completed_reward_without_processed_transaction_rejects_restore() {
        let mut state = default_first_install_stable_state();
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::Completed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = Some(RewardReservation::default());
        op.reserved_reward_debit_e8s = Some(0);
        state.operation_journal = vec![op];

        let err = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state,
        })
        .unwrap_err();
        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn current_v2_inconsistent_reward_reservation_rejects_restore() {
        let mut state = default_first_install_stable_state();
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 100_010_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        op.reserved_reward_debit_e8s = Some(100_010_000);
        state.operation_journal = vec![op];

        let err = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state,
        })
        .unwrap_err();
        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn v2_inconsistent_split_fails_closed() {
        current_v2_inconsistent_reward_reservation_rejects_restore();
    }

    #[test]
    fn legacy_scalar_less_than_proven_spent_debit_rejects_restore() {
        let mut state = default_first_install_stable_state();
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(1);
        state.operation_journal = vec![op];

        let err = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: 1,
            state,
        })
        .unwrap_err();
        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn v1_validated_scalar_no_transfers_decodes_and_migrates() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);

        let migrated = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap();

        assert_eq!(
            migrated.operation_journal[0].reward_reservation,
            Some(RewardReservation {
                unspent_reserved_reward_debit_e8s: 100_010_000,
                externally_spent_but_uncommitted_reward_debit_e8s: 0,
            })
        );
    }

    #[test]
    fn legacy_noncompleted_success_without_attempt_fails_closed() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);

        let err = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap_err();

        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn v1_proof_pending_decodes_and_retains_full_debit() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::FailedRetryable,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);
        op.two_week_recipients[0].reward_transfer_attempt = Some(RewardTransferAttemptRecord {
            amount_e8s: 100_000_000,
            fee_e8s: 10_000,
            created_at_time: 1234,
            memo: None,
            source_account: io_ledger_types::Account::new(
                candid::Principal::anonymous(),
                Some(io_ledger_types::Subaccount([1; 32])),
            ),
            destination_account: io_ledger_types::Account::new(
                candid::Principal::anonymous(),
                Some(io_ledger_types::Subaccount([2; 32])),
            ),
            canonical_sns_neuron_id: vec![1; 32],
            lifecycle: None,
        });
        op.two_week_recipients[0].ledger_transfer_proof_scan_state =
            Some(io_ledger_types::AccountHistoryScanState::default());

        let migrated = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap();

        assert_eq!(
            migrated.operation_journal[0].reward_reservation,
            Some(RewardReservation {
                unspent_reserved_reward_debit_e8s: 100_010_000,
                externally_spent_but_uncommitted_reward_debit_e8s: 0,
            })
        );
    }

    #[test]
    fn v1_attempt_without_lifecycle_migrates_to_proof_required() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::FailedRetryable,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);
        add_current_attempt(
            &mut op,
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 1234 },
        );
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::FailedRetryable);
        op.two_week_recipients[0].transfer_status = TransferStatus::FailedRetryable;
        op.two_week_recipients[0].transfer_block_index = None;
        op.two_week_recipients[0].ledger_transfer_block = None;

        let migrated = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap();
        let lifecycle = migrated.operation_journal[0].two_week_recipients[0]
            .reward_transfer_attempt
            .as_ref()
            .unwrap()
            .lifecycle
            .as_ref()
            .unwrap();

        assert!(matches!(
            lifecycle,
            RewardTransferAttemptLifecycle::ProofRequired {
                generation: 1234,
                ..
            }
        ));
    }

    #[test]
    fn v1_success_with_two_matching_blocks_migrates_to_proven() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);
        add_current_attempt(
            &mut op,
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 1234 },
        );
        op.two_week_recipients[0].transfer_block_index = Some(42);
        op.two_week_recipients[0].ledger_transfer_block = Some(42);

        let migrated = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap();

        assert_eq!(
            migrated.operation_journal[0].two_week_recipients[0]
                .reward_transfer_attempt
                .as_ref()
                .unwrap()
                .lifecycle,
            Some(RewardTransferAttemptLifecycle::Proven {
                generation: 1234,
                block: 42,
            })
        );
    }

    #[test]
    fn v1_success_with_only_transfer_block_migrates_to_proven() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);
        add_current_attempt(
            &mut op,
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 1234 },
        );
        op.two_week_recipients[0].transfer_block_index = Some(42);
        op.two_week_recipients[0].ledger_transfer_block = None;

        let migrated = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap();
        let recipient = &migrated.operation_journal[0].two_week_recipients[0];

        assert_eq!(recipient.transfer_block_index, Some(42));
        assert_eq!(recipient.ledger_transfer_block, Some(42));
        assert_eq!(
            recipient
                .reward_transfer_attempt
                .as_ref()
                .unwrap()
                .lifecycle,
            Some(RewardTransferAttemptLifecycle::Proven {
                generation: 1234,
                block: 42,
            })
        );
    }

    #[test]
    fn v1_success_with_only_ledger_block_migrates_to_proven() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);
        add_current_attempt(
            &mut op,
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 1234 },
        );
        op.two_week_recipients[0].transfer_block_index = None;
        op.two_week_recipients[0].ledger_transfer_block = Some(42);

        let migrated = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap();
        let recipient = &migrated.operation_journal[0].two_week_recipients[0];

        assert_eq!(recipient.transfer_block_index, Some(42));
        assert_eq!(recipient.ledger_transfer_block, Some(42));
        assert_eq!(
            recipient
                .reward_transfer_attempt
                .as_ref()
                .unwrap()
                .lifecycle,
            Some(RewardTransferAttemptLifecycle::Proven {
                generation: 1234,
                block: 42,
            })
        );
    }

    #[test]
    fn legacy_attempt_with_complete_evidence_migrates_and_passes_current_validation() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);
        add_current_attempt(
            &mut op,
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 1234 },
        );
        op.two_week_recipients[0].transfer_block_index = Some(42);
        op.two_week_recipients[0].ledger_transfer_block = Some(42);

        let migrated = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap();

        assert_state_passes_current_live_validation(&migrated);
        assert!(migrate_current_reward_state(migrated).is_ok());
    }

    #[test]
    fn v1_attempt_with_proof_cursor_migrates_to_proof_required() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::FailedRetryable,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);
        add_current_attempt(
            &mut op,
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 1234 },
        );
        op.two_week_recipients[0].ledger_transfer_status = Some(TransferStatus::FailedRetryable);
        op.two_week_recipients[0].transfer_status = TransferStatus::FailedRetryable;
        op.two_week_recipients[0].ledger_transfer_proof_scan_state =
            Some(io_ledger_types::AccountHistoryScanState::default());

        let migrated = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap();

        assert!(matches!(
            migrated.operation_journal[0].two_week_recipients[0]
                .reward_transfer_attempt
                .as_ref()
                .unwrap()
                .lifecycle,
            Some(RewardTransferAttemptLifecycle::ProofRequired { .. })
        ));
    }

    #[test]
    fn v1_success_without_block_never_becomes_prepared() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);
        add_current_attempt(
            &mut op,
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 1234 },
        );
        op.two_week_recipients[0].ledger_transfer_fee_e8s = Some(10_000);
        op.two_week_recipients[0].transfer_block_index = None;
        op.two_week_recipients[0].ledger_transfer_block = None;

        let err = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap_err();

        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn v1_success_with_conflicting_blocks_fails_closed() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);
        add_current_attempt(
            &mut op,
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 1234 },
        );
        op.two_week_recipients[0].transfer_block_index = Some(42);
        op.two_week_recipients[0].ledger_transfer_block = Some(43);

        let err = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap_err();

        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn v1_terminal_after_proven_transfer_decodes_and_retains_spent_debit() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::FailedTerminal,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);
        add_current_attempt(
            &mut op,
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 1234 },
        );
        op.two_week_recipients[0].transfer_block_index = Some(42);
        op.two_week_recipients[0].ledger_transfer_block = Some(42);

        let migrated = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap();

        assert_eq!(
            migrated.operation_journal[0].reward_reservation,
            Some(RewardReservation {
                unspent_reserved_reward_debit_e8s: 0,
                externally_spent_but_uncommitted_reward_debit_e8s: 100_010_000,
            })
        );
    }

    #[test]
    fn legacy_completed_processed_operation_remains_valid() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::Completed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);
        let source_transaction_id = op.source_transaction_id.clone();
        let mut state = legacy_state_with_reward_op(op);
        state.processed_transactions = vec![source_transaction_id];

        let migrated = decode_legacy_v1_bytes(state).unwrap();

        assert_eq!(
            migrated.operation_journal[0].reward_reservation,
            Some(RewardReservation::default())
        );
        assert_eq!(
            migrated.operation_journal[0].reserved_reward_debit_e8s,
            Some(0)
        );
        assert_state_passes_current_live_validation(&migrated);
        assert!(migrate_current_reward_state(migrated).is_ok());
    }

    #[test]
    fn every_successful_v0_migration_output_passes_current_stable_validation() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);

        let migrated = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: 0,
            state: legacy_state_with_reward_op(op),
        })
        .unwrap();

        assert!(migrate_current_reward_state(migrated).is_ok());
    }

    #[test]
    fn every_successful_v1_migration_output_passes_current_stable_validation() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);

        let migrated = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap();

        assert!(migrate_current_reward_state(migrated).is_ok());
    }

    #[test]
    fn every_successful_legacy_migration_output_passes_current_live_validation() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);

        let migrated = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap();

        assert_state_passes_current_live_validation(&migrated);
    }

    #[test]
    fn v1_prepreflight_nonzero_reservation_without_preflight_fails_closed() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_preflight = None;
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);

        let err = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap_err();

        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn v1_external_effect_without_exact_fee_evidence_fails_closed() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        op.reward_preflight = None;
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_010_000);

        let err = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap_err();

        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn v1_scalar_less_than_proven_debit_fails_closed() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(100_009_999);

        let err = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap_err();

        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn v1_overflow_fails_closed() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = Some(u128::MAX);
        op.reward_preflight.as_mut().unwrap().ledger_fee_e8s = 1;
        op.two_week_recipients[0].amount_e8s = u128::MAX;
        op.two_week_recipients[0].reserve_debit_e8s = None;

        let err = decode_legacy_v1_bytes(legacy_state_with_reward_op(op)).unwrap_err();

        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn v2_processed_noncompleted_operation_fails_closed() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.two_week_recipients[0].stake_before_e8s = Some(1_000_000_000);
        op.two_week_recipients[0].expected_stake_after_e8s = Some(1_100_000_000);
        op.two_week_recipients[0].minimum_expected_stake_after_e8s = Some(1_100_000_000);
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 100_010_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        op.reserved_reward_debit_e8s = Some(100_010_000);
        let source_transaction_id = op.source_transaction_id.clone();
        let mut state = legacy_state_with_reward_op(op);
        state.processed_transactions = vec![source_transaction_id];

        let err = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state,
        })
        .unwrap_err();

        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn restore_rejects_recipient_debit_smaller_than_amount_plus_fee() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.two_week_recipients[0].reserve_debit_e8s = Some(100_009_999);
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn restore_rejects_recipient_debit_larger_than_amount_plus_fee() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.two_week_recipients[0].reserve_debit_e8s = Some(100_010_001);
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn restore_rejects_attempt_amount_mismatch() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        add_current_attempt(&mut op, RewardTransferAttemptLifecycle::Prepared);
        op.two_week_recipients[0]
            .reward_transfer_attempt
            .as_mut()
            .unwrap()
            .amount_e8s = 99_000_000;
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn restore_rejects_attempt_fee_mismatch() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        add_current_attempt(&mut op, RewardTransferAttemptLifecycle::Prepared);
        op.two_week_recipients[0].ledger_transfer_fee_e8s = Some(20_000);
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn restore_rejects_lifecycle_generation_mismatch() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        add_current_attempt(
            &mut op,
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 999 },
        );
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn restore_rejects_proven_block_mismatch() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        add_current_attempt(
            &mut op,
            RewardTransferAttemptLifecycle::Proven {
                generation: 1234,
                block: 43,
            },
        );
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn restore_rejects_prepared_attempt_with_success_evidence() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        add_current_attempt(&mut op, RewardTransferAttemptLifecycle::Prepared);
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn restore_rejects_submitted_attempt_with_proven_block() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        add_current_attempt(
            &mut op,
            RewardTransferAttemptLifecycle::SubmittedAwaitingResult { generation: 1234 },
        );
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn restore_rejects_preflight_recipient_count_mismatch() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_preflight.as_mut().unwrap().recipient_count = 2;
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn restore_rejects_preflight_total_reward_mismatch() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_preflight.as_mut().unwrap().total_reward_e8s -= 1;
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn restore_rejects_preflight_total_fee_mismatch() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_preflight.as_mut().unwrap().total_fee_e8s += 1;
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn restore_rejects_preflight_total_debit_mismatch() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_preflight
            .as_mut()
            .unwrap()
            .total_reserve_debit_e8s += 1;
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn restore_rejects_preflight_dust_mismatch() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_preflight.as_mut().unwrap().dust_e8s += 1;
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn restore_rejects_preflight_recipient_identity_mismatch() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_preflight
            .as_mut()
            .unwrap()
            .canonical_recipient_ids[0] = vec![9; 32];
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn valid_current_reward_state_passes_shared_validator() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        add_current_attempt(&mut op, RewardTransferAttemptLifecycle::Prepared);
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 100_010_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        op.reserved_reward_debit_e8s = Some(100_010_000);

        assert!(migrate_current_reward_state(current_reward_state_with_op(op)).is_ok());
    }

    #[test]
    fn stable_and_live_validators_accept_and_reject_same_fixtures() {
        let mut valid = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        add_current_attempt(&mut valid, RewardTransferAttemptLifecycle::Prepared);
        valid.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 100_010_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        valid.reserved_reward_debit_e8s = Some(100_010_000);

        assert!(
            validate_reward_operation_accounting(&valid, None, RewardValidationMode::Current)
                .is_ok()
        );
        assert!(migrate_current_reward_state(current_reward_state_with_op(valid.clone())).is_ok());

        let mut invalid = valid;
        invalid.reward_preflight.as_mut().unwrap().total_reward_e8s += 1;

        assert!(validate_reward_operation_accounting(
            &invalid,
            None,
            RewardValidationMode::Current
        )
        .is_err());
        expect_current_reward_restore_rejects(invalid);
    }

    #[test]
    fn stable_rejects_attempt_fee_different_from_preflight_fee() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        add_current_attempt(&mut op, RewardTransferAttemptLifecycle::Prepared);
        op.two_week_recipients[0].ledger_transfer_fee_e8s = Some(20_000);
        op.two_week_recipients[0]
            .reward_transfer_attempt
            .as_mut()
            .unwrap()
            .fee_e8s = 20_000;
        op.two_week_recipients[0].reserve_debit_e8s = Some(100_020_000);

        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn stable_and_live_reject_same_attempt_fee_drift() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        add_current_attempt(&mut op, RewardTransferAttemptLifecycle::Prepared);
        op.two_week_recipients[0].ledger_transfer_fee_e8s = Some(20_000);
        op.two_week_recipients[0]
            .reward_transfer_attempt
            .as_mut()
            .unwrap()
            .fee_e8s = 20_000;
        op.two_week_recipients[0].reserve_debit_e8s = Some(100_020_000);

        assert!(
            validate_reward_operation_accounting(&op, None, RewardValidationMode::Current).is_err()
        );
        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn live_rejects_external_attempt_without_preflight() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_preflight = None;
        add_current_attempt(&mut op, RewardTransferAttemptLifecycle::Prepared);

        let err = validate_reward_operation_accounting(&op, None, RewardValidationMode::Current)
            .unwrap_err();

        assert!(err.contains("without preflight"));
    }

    #[test]
    fn stable_rejects_external_attempt_without_preflight() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_preflight = None;
        add_current_attempt(&mut op, RewardTransferAttemptLifecycle::Prepared);

        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn live_rejects_success_without_durable_attempt() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 0,
            externally_spent_but_uncommitted_reward_debit_e8s: 100_010_000,
        });
        op.reserved_reward_debit_e8s = Some(100_010_000);

        let err = validate_reward_operation_accounting(&op, None, RewardValidationMode::Current)
            .unwrap_err();

        assert!(err.contains("without durable attempt"));
    }

    #[test]
    fn stable_rejects_success_without_durable_attempt() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Succeeded,
        );
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 0,
            externally_spent_but_uncommitted_reward_debit_e8s: 100_010_000,
        });
        op.reserved_reward_debit_e8s = Some(100_010_000);

        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn live_rejects_block_without_durable_attempt() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.two_week_recipients[0].transfer_block_index = Some(42);
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 100_010_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        op.reserved_reward_debit_e8s = Some(100_010_000);

        let err = validate_reward_operation_accounting(&op, None, RewardValidationMode::Current)
            .unwrap_err();

        assert!(err.contains("without durable attempt"));
    }

    #[test]
    fn stable_rejects_block_without_durable_attempt() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.two_week_recipients[0].ledger_transfer_block = Some(42);
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 100_010_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        op.reserved_reward_debit_e8s = Some(100_010_000);

        expect_current_reward_restore_rejects(op);
    }

    #[test]
    fn live_rejects_proof_cursor_without_durable_attempt() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.two_week_recipients[0].ledger_transfer_proof_scan_state =
            Some(io_ledger_types::AccountHistoryScanState::default());
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 100_010_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        op.reserved_reward_debit_e8s = Some(100_010_000);

        let err = validate_reward_operation_accounting(&op, None, RewardValidationMode::Current)
            .unwrap_err();

        assert!(err.contains("without durable attempt"));
    }

    #[test]
    fn live_rejects_refresh_or_stake_evidence_without_durable_attempt() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.two_week_recipients[0].governance_refresh_status = Some(TransferStatus::Succeeded);
        op.two_week_recipients[0].expected_stake_after_e8s = Some(200_000_000);
        op.two_week_recipients[0].observed_stake_after_e8s = Some(200_000_000);
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 100_010_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        op.reserved_reward_debit_e8s = Some(100_010_000);

        let err = validate_reward_operation_accounting(&op, None, RewardValidationMode::Current)
            .unwrap_err();

        assert!(err.contains("without durable attempt"));
    }

    #[test]
    fn valid_preflight_recipient_before_attempt_remains_valid() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 100_010_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        op.reserved_reward_debit_e8s = Some(100_010_000);

        assert!(
            validate_reward_operation_accounting(&op, None, RewardValidationMode::Current).is_ok()
        );
        assert!(migrate_current_reward_state(current_reward_state_with_op(op)).is_ok());
    }

    #[test]
    fn preflight_not_started_zero_effect_state_is_valid() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_preflight = None;
        op.reward_reservation = Some(RewardReservation::default());
        op.reserved_reward_debit_e8s = Some(0);

        assert!(
            validate_reward_operation_accounting(&op, None, RewardValidationMode::Current).is_ok()
        );
        assert!(migrate_current_reward_state(current_reward_state_with_op(op)).is_ok());
    }

    #[test]
    fn failed_terminal_invalid_plan_is_status_aware_and_restorable() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::FailedTerminal,
            TransferStatus::Pending,
        );
        op.reward_preflight = Some(RewardDistributionPreflight {
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
            failure_reason: Some("recipient canonical id missing".to_string()),
        });
        op.reward_reservation = Some(RewardReservation::default());
        op.reserved_reward_debit_e8s = Some(0);

        assert!(migrate_current_reward_state(current_reward_state_with_op(op)).is_ok());
    }

    #[test]
    fn bad_fee_pending_repreflight_stable_roundtrip_preserves_reservation() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_preflight.as_mut().unwrap().status = RewardPreflightStatus::Pending;
        op.reward_preflight.as_mut().unwrap().failure_reason =
            Some("reward transfer BadFee before definitive success".to_string());
        op.reward_reservation = Some(RewardReservation {
            unspent_reserved_reward_debit_e8s: 100_010_000,
            externally_spent_but_uncommitted_reward_debit_e8s: 0,
        });
        op.reserved_reward_debit_e8s = Some(100_010_000);
        op.reward_fee_repreflight = Some(RewardFeeRepreflightEvidence {
            prior_validated_fee_e8s: 10_000,
            observed_current_fee_e8s: 20_000,
            prior_reserved_debit_e8s: 100_010_000,
            invalidated_at_timestamp_nanos: 999,
            attempt_generation: 55,
        });

        let migrated = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state: legacy_state_with_reward_op(op),
        })
        .unwrap();

        assert_eq!(
            migrated.operation_journal[0].reward_reservation,
            Some(RewardReservation {
                unspent_reserved_reward_debit_e8s: 100_010_000,
                externally_spent_but_uncommitted_reward_debit_e8s: 0,
            })
        );
        assert_eq!(
            migrated.operation_journal[0].reward_fee_repreflight,
            Some(RewardFeeRepreflightEvidence {
                prior_validated_fee_e8s: 10_000,
                observed_current_fee_e8s: 20_000,
                prior_reserved_debit_e8s: 100_010_000,
                invalidated_at_timestamp_nanos: 999,
                attempt_generation: 55,
            })
        );
    }

    #[test]
    fn pending_repreflight_missing_prior_reservation_fails_closed() {
        let mut op = stable_reward_operation_with_one_recipient(
            OperationPhase::PartiallyDistributed,
            TransferStatus::Pending,
        );
        op.reward_preflight.as_mut().unwrap().status = RewardPreflightStatus::Pending;
        op.reward_reservation = None;
        op.reserved_reward_debit_e8s = None;
        op.reward_fee_repreflight = Some(RewardFeeRepreflightEvidence {
            prior_validated_fee_e8s: 10_000,
            observed_current_fee_e8s: 20_000,
            prior_reserved_debit_e8s: 100_010_000,
            invalidated_at_timestamp_nanos: 999,
            attempt_generation: 55,
        });

        let err = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state: legacy_state_with_reward_op(op),
        })
        .unwrap_err();

        assert!(matches!(err, StableMigrationError::CorruptSnapshot { .. }));
    }

    #[test]
    fn old_stable_snapshots_decode_without_not_applicable() {
        let fixture = pending_redemption_fixture();
        assert!(fixture.operation_journal.iter().all(|op| {
            op.icp_payout_status != TransferStatus::NotApplicable
                && op.io_return_status != TransferStatus::NotApplicable
        }));

        let bytes = candid::encode_args((fixture.clone(),)).unwrap();
        let migrated = decode_stable_state_bytes_for_tests(&bytes).unwrap();

        assert_eq!(migrated, fixture);
    }

    #[test]
    fn new_stable_snapshots_with_not_applicable_round_trip() {
        let mut fixture = pending_redemption_fixture();
        let mut rejected = StreamOperation::stream(
            "io",
            101,
            StreamOperationKind::RejectedRedemption,
            E8S_PER_TOKEN,
            ProtocolState::new(1_000_000 * E8S_PER_TOKEN, 900_000 * E8S_PER_TOKEN, 0),
            0,
            OperationPhase::Completed,
        );
        rejected.icp_payout_status = TransferStatus::NotApplicable;
        rejected.io_return_status = TransferStatus::Succeeded;
        rejected.rejected_fund_disposition =
            Some(RejectedFundDisposition::ReturnToSenderSucceeded {
                block_index: 102,
                amount_e8s: E8S_PER_TOKEN - 10_000,
            });
        fixture.operation_journal.push(rejected);

        let bytes = candid::encode_args((VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state: fixture.clone(),
        },))
        .unwrap();
        let migrated = decode_stable_state_bytes_for_tests(&bytes).unwrap();

        assert_eq!(migrated, fixture);
    }

    #[test]
    fn proof_pending_scan_progress_round_trips_in_stable_snapshot() {
        let mut fixture = pending_redemption_fixture();
        let mut scan = io_ledger_types::AccountHistoryScanState::default();
        scan.cursor.order = Some(io_ledger_types::AccountHistoryPageOrder::Descending);
        scan.cursor.latest_cursor = Some(io_ledger_types::BlockIndex(91));
        scan.cursor.oldest_cursor = Some(io_ledger_types::BlockIndex(77));
        scan.status.num_blocks_synced = Some(io_ledger_types::BlockIndex(91));
        let mut rejected = StreamOperation::stream(
            "io",
            101,
            StreamOperationKind::RejectedRedemption,
            E8S_PER_TOKEN,
            ProtocolState::new(1_000_000 * E8S_PER_TOKEN, 900_000 * E8S_PER_TOKEN, 0),
            0,
            OperationPhase::AwaitingIoReturn,
        );
        rejected.icp_payout_status = TransferStatus::NotApplicable;
        rejected.io_return_status = TransferStatus::FailedRetryable;
        rejected.rejected_fund_disposition =
            Some(RejectedFundDisposition::ReturnToSenderProofPending {
                reason: "TooOld; refund proof pending".to_string(),
                original_created_at_time: Some(500),
                proof_scan_state: Some(scan.clone()),
            });
        fixture.operation_journal.push(rejected);

        let bytes = candid::encode_args((VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state: fixture.clone(),
        },))
        .unwrap();
        let migrated = decode_stable_state_bytes_for_tests(&bytes).unwrap();
        let restored = migrated
            .operation_journal
            .iter()
            .find(|op| op.source_block_index == Some(101))
            .expect("proof-pending operation should restore");

        assert!(matches!(
            &restored.rejected_fund_disposition,
            Some(RejectedFundDisposition::ReturnToSenderProofPending {
                proof_scan_state: Some(restored_scan),
                ..
            }) if restored_scan == &scan
        ));
    }

    #[test]
    fn original_refund_request_survives_same_wasm_upgrade() {
        let mut fixture = pending_redemption_fixture();
        let principal = candid::Principal::from_text("aaaaa-aa").unwrap();
        let refund_source =
            io_ledger_types::Account::new(principal, Some(io_ledger_types::Subaccount([9; 32])));
        let destination =
            io_ledger_types::Account::new(principal, Some(io_ledger_types::Subaccount([7; 32])));
        let attempt = RejectedRefundAttemptRecord {
            attempted_refund_amount_e8s: E8S_PER_TOKEN - 10_000,
            attempted_fee_e8s: 10_000,
            attempted_created_at_time: 88,
            memo: Some(io_ledger_types::Memo::from("rejected_io_refund:io:101")),
            refund_source_account: refund_source,
            destination_account: destination,
        };
        let mut rejected = StreamOperation::stream(
            "io",
            101,
            StreamOperationKind::RejectedRedemption,
            E8S_PER_TOKEN,
            ProtocolState::new(1_000_000 * E8S_PER_TOKEN, 900_000 * E8S_PER_TOKEN, 0),
            0,
            OperationPhase::AwaitingIoReturn,
        );
        rejected.icp_payout_status = TransferStatus::NotApplicable;
        rejected.io_return_status = TransferStatus::FailedRetryable;
        rejected.rejected_fund_disposition =
            Some(RejectedFundDisposition::ReturnToSenderProofPending {
                reason: "TooOld; refund proof pending".to_string(),
                original_created_at_time: Some(88),
                proof_scan_state: None,
            });
        rejected.rejected_refund_attempt = Some(attempt.clone());
        fixture.operation_journal.push(rejected);

        let bytes = candid::encode_args((VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state: fixture,
        },))
        .unwrap();
        let migrated = decode_stable_state_bytes_for_tests(&bytes).unwrap();
        let restored = migrated
            .operation_journal
            .iter()
            .find(|op| op.source_block_index == Some(101))
            .expect("proof-pending operation should restore");

        assert_eq!(restored.rejected_refund_attempt, Some(attempt));
    }

    #[test]
    fn legacy_unversioned_snapshot_migrates() {
        let fixture = pending_redemption_fixture();
        let bytes = candid::encode_args((fixture.clone(),)).unwrap();

        assert_eq!(
            decode_stable_state_bytes_for_tests(&bytes).unwrap(),
            fixture
        );
    }

    #[test]
    fn stream_manager_current_fixture_round_trips_unchanged() {
        let fixture = pending_redemption_fixture();
        let snapshot = VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state: fixture.clone(),
        };

        assert_eq!(migrate_stable_state_for_tests(snapshot).unwrap(), fixture);
    }

    #[test]
    fn stream_manager_rejects_future_schema_version() {
        let err = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION + 1,
            state: default_first_install_stable_state(),
        })
        .unwrap_err();

        assert!(matches!(
            err,
            StableMigrationError::UnsupportedFutureVersion {
                canister: "io_stream_manager",
                ..
            }
        ));
    }

    #[test]
    fn future_schema_fails_closed() {
        stream_manager_rejects_future_schema_version();
    }

    #[test]
    fn stream_manager_rejects_corrupt_stable_fixture() {
        let decoded = decode_stable_state_bytes_for_tests(b"not candid stable state");

        assert!(decoded.is_err());
    }

    #[test]
    fn corrupt_snapshot_fails_closed() {
        stream_manager_rejects_corrupt_stable_fixture();
    }

    #[test]
    fn trailing_invalid_bytes_do_not_silently_initialize_empty_state() {
        let fixture = pending_redemption_fixture();
        let mut bytes = candid::encode_args((VersionedStableState {
            schema_version: STREAM_MANAGER_STABLE_SCHEMA_VERSION,
            state: fixture,
        },))
        .unwrap();
        bytes.extend_from_slice(b"trailing garbage");

        assert!(decode_stable_state_bytes_for_tests(&bytes).is_err());
    }

    #[test]
    fn stream_manager_empty_first_install_state_defaults_safely() {
        let stable = default_first_install_stable_state();

        assert!(stable.config.production_wiring.is_none());
        assert!(stable.operation_journal.is_empty());
        assert!(stable.processed_transactions.is_empty());
    }

    #[test]
    fn stream_manager_preserves_pending_redemption_retry_intent() {
        let migrated = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: 0,
            state: pending_redemption_fixture(),
        })
        .unwrap();
        let op = migrated.operation_journal.first().unwrap();

        assert_eq!(op.kind, StreamOperationKind::Redemption);
        assert_eq!(op.phase, OperationPhase::FailedRetryable);
        assert_eq!(op.retry_count, 2);
        assert_eq!(op.gross_icp_payout_e8s, t(9));
        assert_eq!(op.icp_payout_fee_e8s, 10_000);
        assert_eq!(op.net_user_icp_payout_e8s, t(9) - 10_000);
        assert_eq!(op.io_return_fee_e8s, 10_000);
        assert_eq!(op.icp_payout_status, TransferStatus::FailedRetryable);
        assert_eq!(op.io_return_status, TransferStatus::FailedRetryable);
    }

    #[test]
    fn terminal_redemption_survives_same_wasm_upgrade_without_retry() {
        init(InitArgs::default());
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            let mut redemption = StreamOperation::redemption(
                111,
                t(1),
                0,
                "user-account".to_string(),
                state.manager.state,
            );
            redemption.phase = OperationPhase::FailedTerminal;
            redemption.icp_payout_status = TransferStatus::FailedTerminal;
            redemption.io_return_status = TransferStatus::FailedTerminal;
            redemption.last_error = Some("below IO return fee".to_string());
            state.operation_journal.push(redemption);
        });

        let stable = export_stable_state_for_tests();
        import_stable_state_for_tests(stable);
        let restored = export_stable_state_for_tests();
        let op = restored.operation_journal.first().unwrap();

        assert_eq!(op.phase, OperationPhase::FailedTerminal);
        assert_eq!(op.icp_payout_status, TransferStatus::FailedTerminal);
        assert_eq!(op.io_return_status, TransferStatus::FailedTerminal);
    }

    #[test]
    fn stream_manager_preserves_processed_transaction_cursors() {
        let migrated = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: 0,
            state: pending_redemption_fixture(),
        })
        .unwrap();

        assert!(migrated
            .processed_transactions
            .contains(&"duplicate-proof:99".to_string()));
        assert_eq!(
            migrated.scheduler_cursors.last_scanned_icp_index_block,
            Some(99)
        );
        assert_eq!(
            migrated.scheduler_cursors.last_scanned_io_index_block,
            Some(100)
        );
        assert_eq!(
            migrated
                .scheduler_cursors
                .icp_account_history_scan
                .cursor
                .latest_cursor,
            Some(io_ledger_types::BlockIndex(99))
        );
    }

    #[test]
    fn legacy_pending_redemption_defaults_retry_with_gross_amount() {
        let mut redemption = StreamOperation::redemption(
            9,
            t(10),
            t(10),
            "user".to_string(),
            ProtocolState::new(t(1_000_000), t(900_000), t(100_000)),
        );
        redemption.net_user_icp_payout_e8s = 0;

        assert_eq!(redemption.effective_net_user_icp_payout_e8s(), t(10));
    }

    #[test]
    fn scheduler_tick_does_not_mutate_value_moving_state() {
        init(InitArgs::default());
        let before = export_stable_state_for_tests();
        let outcome = crate::scheduler::scheduler_tick_plan_only();
        assert_eq!(outcome.processed_authorized_streams, 0);
        assert_eq!(export_stable_state_for_tests(), before);
    }
}

#[cfg(test)]
mod additional_stream_manager_tests {
    use super::*;
    use crate::state::{
        IO_NNS_NEURON_MANAGER_SOURCE, JUPITER_FAUCET_SOURCE, TWO_WEEK_MATURITY_MEMO,
        TWO_YEAR_MATURITY_MEMO,
    };
    use io_governance_types::{SnsNeuronEligibility, SnsNeuronId};
    use io_reward_policy::RewardParticipant;

    fn t(n: u128) -> u128 {
        n * E8S_PER_TOKEN
    }

    fn neuron(id: u64, stake: u128, voted: u64, total: u64) -> RewardParticipant {
        RewardParticipant {
            sns_neuron_id: SnsNeuronId(id.to_be_bytes().to_vec()),
            neuron_id: id,
            frozen_stake_e8s: stake,
            eligible_closed_proposals: total,
            voted_closed_proposals: voted,
            destination_is_currently_eligible: true,
        }
    }

    #[test]
    fn unknown_memo_from_authorized_nns_manager_is_rejected() {
        let mut m = StreamManager::default_for_tests();
        let err = m
            .process_scanned_icp(
                IO_NNS_NEURON_MANAGER_SOURCE,
                "unexpected",
                t(100),
                "bad-memo",
            )
            .unwrap_err();
        assert!(matches!(
            err,
            StreamManagerError::UnknownOrUnauthorizedStream { .. }
        ));
        assert!(!m.processed_transactions.contains("bad-memo"));
    }

    #[test]
    fn same_transaction_id_cannot_be_reused_across_stream_kinds() {
        let mut m = StreamManager::default_for_tests();
        m.process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "ledger-block-1")
            .unwrap();
        assert_eq!(
            m.process_scanned_icp(
                IO_NNS_NEURON_MANAGER_SOURCE,
                TWO_YEAR_MATURITY_MEMO,
                t(100),
                "ledger-block-1"
            )
            .unwrap_err(),
            StreamManagerError::DuplicateTransaction
        );
    }

    #[test]
    fn two_year_stream_does_not_consume_io_reserve() {
        let mut m = StreamManager::default_for_tests();
        let before_reserve = m.state.protocol_reserve_io_e8s;
        m.process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_YEAR_MATURITY_MEMO,
            t(100),
            "2y",
        )
        .unwrap();
        assert_eq!(m.state.protocol_reserve_io_e8s, before_reserve);
    }

    #[test]
    fn two_week_stream_consumes_io_reserve_but_preserves_rate() {
        let mut m = StreamManager::default_for_tests();
        m.process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet")
            .unwrap();
        m.process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_YEAR_MATURITY_MEMO,
            t(100),
            "2y",
        )
        .unwrap();
        let rate_before = m.state.redemption_rate().unwrap();
        let reserve_before = m.state.protocol_reserve_io_e8s;
        let out = m
            .process_scanned_icp(
                IO_NNS_NEURON_MANAGER_SOURCE,
                TWO_WEEK_MATURITY_MEMO,
                t(100),
                "2w",
            )
            .unwrap();
        assert!(out.io_issued_e8s > 0);
        assert_eq!(
            m.state.protocol_reserve_io_e8s,
            reserve_before - out.io_issued_e8s
        );
        assert_eq!(m.state.redemption_rate().unwrap(), rate_before);
    }

    #[test]
    fn two_week_target_is_full_redemption_claim() {
        let mut m = StreamManager::default_for_tests();
        m.process_authorized_stream(StreamKind::JupiterFaucet, t(100), "faucet")
            .unwrap();
        m.refresh_active_staked_io_from_neurons(&[neuron(1, t(20), 1, 1)]);
        assert_eq!(m.target_two_week_pool_e8s().unwrap(), t(20));
    }

    #[test]
    fn active_stake_reward_consistency_excludes_invalid_sns_neuron_ids() {
        let mut m = StreamManager::default_for_tests();
        let valid = SnsNeuronEligibility {
            neuron_id: SnsNeuronId({
                let mut bytes = [0_u8; 32];
                bytes[24..].copy_from_slice(&42_u64.to_be_bytes());
                bytes.to_vec()
            }),
            owner: None,
            eligible_stake_e8s: t(20),
            dissolve_delay_seconds: 14 * 24 * 60 * 60,
            is_non_dissolving: true,
            excluded_reason: None,
        };
        let invalid = SnsNeuronEligibility {
            neuron_id: SnsNeuronId(vec![7]),
            owner: None,
            eligible_stake_e8s: t(30),
            dissolve_delay_seconds: 14 * 24 * 60 * 60,
            is_non_dissolving: true,
            excluded_reason: None,
        };

        m.refresh_active_staked_io_from_sns_eligibility(&[valid, invalid]);

        assert_eq!(m.active_staked_io_e8s, t(20));
    }

    #[test]
    fn reward_allocation_with_no_eligible_neurons_keeps_pool_as_dust() {
        let m = StreamManager::default_for_tests();
        let mut genesis = neuron(1, t(10), 1, 1);
        genesis.destination_is_currently_eligible = false;
        let out = m.allocate_two_week_maturity_io(t(5), &[genesis]).unwrap();
        assert!(out.allocations.is_empty());
        assert_eq!(out.dust_e8s, t(5));
    }

    #[test]
    fn redemption_failure_is_retryable_with_same_user_intent() {
        let mut m = StreamManager::default_for_tests();
        m.process_authorized_stream(StreamKind::JupiterFaucet, t(100), "faucet")
            .unwrap();
        let before = m.state;
        let err = m.redeem(t(100)).unwrap_err();
        assert!(matches!(
            err,
            StreamManagerError::Model(ModelError::InsufficientRedeemableSupply { .. })
        ));
        assert_eq!(m.state, before);
        let ok = m.redeem(t(10)).unwrap();
        assert_eq!(ok.icp_paid_e8s, t(10));
    }

    #[test]
    fn empty_or_whitespace_transaction_ids_are_rejected_before_state_changes() {
        let mut m = StreamManager::default_for_tests();
        let before = m.state;
        assert_eq!(
            m.process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "   ")
                .unwrap_err(),
            StreamManagerError::InvalidTransactionId
        );
        assert_eq!(m.state, before);
        assert!(m.processed_transactions.is_empty());
    }

    #[test]
    fn source_classification_is_case_sensitive_and_strict() {
        assert!(StreamManager::classify_stream("JUPITER_FAUCET", "").is_err());
        assert!(
            StreamManager::classify_stream(JUPITER_FAUCET_SOURCE, TWO_YEAR_MATURITY_MEMO).is_err()
        );
        assert!(StreamManager::classify_stream(IO_NNS_NEURON_MANAGER_SOURCE, "").is_err());
    }
}
