use candid::{CandidType, Principal};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
use serde::Deserialize;
use std::{borrow::Cow, cell::RefCell};

use crate::{
    pool_reconciliation::PoolTopUpOperation,
    receipt::{ClaimBackingReceipt, CompletedClaimBackingReceipt},
    redemption::{RedemptionOperation, RedemptionPreparation},
};
pub use io_accounts::Account;

type Memory = VirtualMemory<DefaultMemoryImpl>;
pub(crate) const LAUNCH_SCHEMA_MARKER: u8 = 4;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StreamConfig {
    pub io_ledger: Principal,
    pub icp_ledger: Principal,
    pub nns_manager: Principal,
    pub jupiter_receipt_source: Account,
    pub jupiter_io_account: Account,
    pub sns_governance: Principal,
    pub sns_root: Principal,
    pub expected_sns_governance_module_hash: Vec<u8>,
    pub approved_reward_event_duration_seconds: u64,
    pub io_reserve: Account,
    pub liquid_icp: Account,
    pub nonredeemable_governance_io_accounts: Vec<Account>,
    pub minimum_redemption_io_e8s: u128,
    pub expected_io_fee_e8s: u128,
    pub expected_icp_fee_e8s: u128,
    pub maximum_request_lifetime_nanos: u64,
    pub retry_delay_nanos: u64,
    pub ledger_deduplication_window_nanos: u64,
}

impl StreamConfig {
    pub const MAX_EXCLUDED_ACCOUNTS: usize = 32;
    pub const MAX_FEE_E8S: u128 = 100_000_000;

    pub fn validate(&self, canister_self: Principal) -> Result<(), String> {
        let management = Principal::management_canister();
        let principals = [
            ("canister self", canister_self),
            ("IO ledger", self.io_ledger),
            ("ICP ledger", self.icp_ledger),
            ("NNS manager", self.nns_manager),
            ("SNS governance", self.sns_governance),
            ("SNS root", self.sns_root),
        ];
        for (name, principal) in principals {
            if principal == Principal::anonymous() || principal == management {
                return Err(format!("{name} principal is forbidden"));
            }
        }
        for (index, (left_name, left)) in principals.iter().enumerate() {
            for (right_name, right) in principals.iter().skip(index + 1) {
                if left == right {
                    return Err(format!(
                        "{left_name} and {right_name} principals must be distinct"
                    ));
                }
            }
        }
        if self.expected_sns_governance_module_hash.len() != 32 {
            return Err("expected SNS Governance module hash must contain 32 bytes".into());
        }
        if self.approved_reward_event_duration_seconds != 86_400 {
            return Err("approved reward-event duration must equal one day".into());
        }
        if self.io_reserve.owner != canister_self || self.liquid_icp.owner != canister_self {
            return Err("reserve and liquid accounts must be owned by this canister".into());
        }
        if self.jupiter_receipt_source.owner != self.nns_manager {
            return Err("Jupiter receipt source owner must equal NNS manager".into());
        }
        self.io_reserve.validate()?;
        self.liquid_icp.validate()?;
        self.jupiter_receipt_source.validate()?;
        self.jupiter_io_account.validate()?;
        for (name, account) in [
            ("IO reserve", &self.io_reserve),
            ("liquid ICP", &self.liquid_icp),
            ("Jupiter receipt source", &self.jupiter_receipt_source),
            ("Jupiter IO account", &self.jupiter_io_account),
        ] {
            if account.owner == Principal::anonymous() || account.owner == management {
                return Err(format!("{name} owner is forbidden"));
            }
        }
        if self.jupiter_receipt_source.effective_eq(&self.liquid_icp)?
            || self.jupiter_io_account.effective_eq(&self.io_reserve)?
        {
            return Err("receipt sources, reserve and liquid accounts must be distinct".into());
        }
        if self.nonredeemable_governance_io_accounts.len() > Self::MAX_EXCLUDED_ACCOUNTS {
            return Err("too many nonredeemable governance IO accounts".into());
        }
        let mut canonical_excluded = std::collections::BTreeSet::new();
        for account in &self.nonredeemable_governance_io_accounts {
            account.validate()?;
            if account.owner == Principal::anonymous() || account.owner == management {
                return Err("nonredeemable governance IO account owner is forbidden".into());
            }
            if account.effective_eq(&self.io_reserve)? {
                return Err("reserve account cannot be excluded".into());
            }
            if account.effective_eq(&self.jupiter_io_account)? {
                return Err("Jupiter IO account cannot be excluded".into());
            }
            if !canonical_excluded.insert(account.canonical()?) {
                return Err("nonredeemable governance IO accounts must be unique".into());
            }
        }
        if self.minimum_redemption_io_e8s <= self.expected_io_fee_e8s {
            return Err("minimum redemption must exceed the IO fee".into());
        }
        for fee in [self.expected_io_fee_e8s, self.expected_icp_fee_e8s] {
            if fee == 0 || fee > Self::MAX_FEE_E8S {
                return Err("configured fee is outside launch bounds".into());
            }
        }
        if self.maximum_request_lifetime_nanos == 0
            || self.retry_delay_nanos == 0
            || self.retry_delay_nanos >= self.ledger_deduplication_window_nanos
            || self.maximum_request_lifetime_nanos > self.ledger_deduplication_window_nanos
        {
            return Err("request/retry windows are invalid".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum Lifecycle {
    Paused,
    Ready,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct OperationSequence(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct DispatchEpoch(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum StreamOperation {
    Redemption(Box<RedemptionStreamOperation>),
    ClaimReceipt(Box<ClaimBackingReceipt>),
    PoolTopUp(Box<PoolTopUpOperation>),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RedemptionStreamOperation {
    Preparing(Box<RedemptionPreparation>),
    Active(Box<RedemptionOperation>),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct FrozenEntitlement {
    pub sns_neuron_id: Vec<u8>,
    pub destination: Account,
    pub accumulated_eligible_credit: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RewardEventClassification {
    ProposalBearing,
    NoProposalFallback,
    ZeroEligibleParticipation,
    MissedSkipped,
    StructuralOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardEventCredit {
    pub sns_neuron_id: Vec<u8>,
    pub destination: Account,
    pub event_credit: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardEventObservation {
    pub event: RewardEventId,
    pub proposal_count: u64,
    pub classification: RewardEventClassification,
    pub policy_credit: u128,
    pub eligible_credit_total: u128,
    pub observed_at_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SkippedRewardEvent {
    pub previous_event: Option<RewardEventId>,
    pub observed_event: RewardEventId,
    pub ambiguous_event_count: u64,
    pub rounds_since_last_distribution: u64,
    pub observed_at_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardCheckpoint {
    pub last_processed_event: Option<RewardEventId>,
    pub accumulated_policy_credit: u128,
    pub processed_event_count: u64,
    pub missed_event_count: u64,
    pub reward_work_due: bool,
    pub reward_processing_paused: bool,
    pub latest_observation: Option<RewardEventObservation>,
    pub latest_skipped_event: Option<SkippedRewardEvent>,
    pub governance_parameters_fresh: bool,
}

impl Default for RewardCheckpoint {
    fn default() -> Self {
        Self {
            last_processed_event: None,
            accumulated_policy_credit: 0,
            processed_event_count: 0,
            missed_event_count: 0,
            reward_work_due: true,
            reward_processing_paused: false,
            latest_observation: None,
            latest_skipped_event: None,
            governance_parameters_fresh: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PendingEntitlementBatch {
    pub generation: u64,
    pub frozen_at_timestamp_seconds: u64,
    pub through_event: RewardEventId,
    pub target_icp_e8s: u128,
    pub entries: Vec<FrozenEntitlement>,
    pub eligible_credit_total: u128,
    pub policy_credit_total: u128,
    pub processed_event_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum StructuralStakeState {
    Active,
    IneligibleActive,
    Dissolving,
    LiquidOrDissolved,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum BackingRewardStatus {
    ActiveEligible { eligible_from_event: u64 },
    ActiveIneligible,
    ExitCommitted { generation: u64 },
    ExitObserved,
    ReentryPending { eligible_from_event: u64 },
    Inactive,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct BackingRewardRecord {
    pub sns_neuron_id: Vec<u8>,
    pub staking_account: Account,
    pub accumulated_eligible_credit: u128,
    pub latest_structural_state: StructuralStakeState,
    pub status: BackingRewardStatus,
    pub unresolved_cohort_generation: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ReconciliationCheckpoint {
    pub generation: u64,
    pub event_marker: u64,
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
    pub snapshot_fingerprint: Vec<u8>,
}

pub fn validate_backing_registry(
    records: &[BackingRewardRecord],
    config: &StreamConfig,
) -> Result<(), String> {
    if records.len() > 1_000 {
        return Err("backing/reward registry exceeds 1,000 entries".into());
    }
    let mut previous: Option<&[u8]> = None;
    let mut accounts = std::collections::BTreeSet::new();
    for record in records {
        let account = record.staking_account.canonical()?;
        if record.sns_neuron_id.len() != 32
            || previous.is_some_and(|value| value >= record.sns_neuron_id.as_slice())
            || account.owner != config.sns_governance
            || account.subaccount.as_slice() != record.sns_neuron_id
            || !accounts.insert(account)
            || match (&record.status, record.unresolved_cohort_generation) {
                (BackingRewardStatus::ExitCommitted { generation }, Some(bound)) => {
                    *generation != bound || bound == 0
                }
                (BackingRewardStatus::ExitCommitted { .. }, None) => true,
                (_, Some(_)) => true,
                (_, None) => false,
            }
        {
            return Err("backing/reward registry is malformed or unsorted".into());
        }
        previous = Some(&record.sns_neuron_id);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardEventId {
    pub end_timestamp_seconds: u64,
    pub round: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StreamStateV1 {
    pub launch_schema_marker: u8,
    pub config: StreamConfig,
    pub lifecycle: Lifecycle,
    pub active_operation: Option<StreamOperation>,
    pub reward_checkpoint: RewardCheckpoint,
    pub pending_entitlement_batch: Option<PendingEntitlementBatch>,
    pub neuron_registry: Vec<BackingRewardRecord>,
    pub stake_observation_due: bool,
    pub latest_reconciliation_checkpoint: Option<ReconciliationCheckpoint>,
    pub latest_reconciliation_generation: u64,
    pub latest_entitlement_batch_generation: u64,
    pub next_operation_sequence: OperationSequence,
    pub control_epoch: u64,
    pub last_completed_claim_receipt: Option<CompletedClaimBackingReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum StableStreamState {
    V1(StreamStateV1),
}

impl StreamStateV1 {
    fn decode_placeholder() -> Self {
        let anonymous = Principal::anonymous();
        let account = Account {
            owner: anonymous,
            subaccount: None,
        };
        Self {
            launch_schema_marker: LAUNCH_SCHEMA_MARKER,
            config: StreamConfig {
                io_ledger: anonymous,
                icp_ledger: anonymous,
                nns_manager: anonymous,
                jupiter_receipt_source: account.clone(),
                jupiter_io_account: account.clone(),
                sns_governance: anonymous,
                sns_root: anonymous,
                expected_sns_governance_module_hash: Vec::new(),
                approved_reward_event_duration_seconds: 0,
                io_reserve: account.clone(),
                liquid_icp: account,
                nonredeemable_governance_io_accounts: Vec::new(),
                minimum_redemption_io_e8s: 1,
                expected_io_fee_e8s: 0,
                expected_icp_fee_e8s: 0,
                maximum_request_lifetime_nanos: 1,
                retry_delay_nanos: 1,
                ledger_deduplication_window_nanos: 2,
            },
            lifecycle: Lifecycle::Paused,
            active_operation: None,
            reward_checkpoint: RewardCheckpoint::default(),
            pending_entitlement_batch: None,
            neuron_registry: Vec::new(),
            stake_observation_due: true,
            latest_reconciliation_checkpoint: None,
            latest_reconciliation_generation: 0,
            latest_entitlement_batch_generation: 0,
            next_operation_sequence: OperationSequence(0),
            control_epoch: 0,
            last_completed_claim_receipt: None,
        }
    }
}

impl StreamStateV1 {
    pub fn validate(&self, canister_self: Principal) -> Result<(), String> {
        if self.launch_schema_marker != LAUNCH_SCHEMA_MARKER {
            return Err("invalid Stream launch schema marker".into());
        }
        self.config.validate(canister_self)?;
        match &self.active_operation {
            Some(StreamOperation::Redemption(operation)) => match operation.as_ref() {
                RedemptionStreamOperation::Preparing(value) => {
                    value.validate()?;
                    if value.sequence.0 >= self.next_operation_sequence.0
                        || value.captured_control_epoch != self.control_epoch
                        || value.request.io_amount_e8s < self.config.minimum_redemption_io_e8s
                        || value.request.max_io_fee_e8s < self.config.expected_io_fee_e8s
                        || value.request.max_icp_fee_e8s < self.config.expected_icp_fee_e8s
                        || value
                            .request
                            .expires_at_nanos
                            .checked_sub(value.prepared_at_nanos)
                            .is_none_or(|lifetime| {
                                lifetime > self.config.maximum_request_lifetime_nanos
                            })
                        || value.account.effective_eq(&self.config.io_reserve)?
                        || self
                            .config
                            .nonredeemable_governance_io_accounts
                            .iter()
                            .try_fold(false, |matched, account| {
                                value
                                    .account
                                    .effective_eq(account)
                                    .map(|same| matched || same)
                            })?
                    {
                        return Err("redemption preparation does not match stream state".into());
                    }
                }
                RedemptionStreamOperation::Active(value) => {
                    value.validate(&self.config)?;
                    if value.sequence.0 >= self.next_operation_sequence.0 {
                        return Err("active redemption sequence was not reserved".into());
                    }
                }
            },
            Some(StreamOperation::ClaimReceipt(operation)) => {
                operation.validate(&self.config)?;
                if operation.permit.stream_operation_sequence >= self.next_operation_sequence.0 {
                    return Err("active claim-receipt sequence was not reserved".into());
                }
            }
            Some(StreamOperation::PoolTopUp(operation)) => operation.validate(&self.config)?,
            None => {}
        }
        self.reward_checkpoint.validate(&self.config)?;
        validate_backing_registry(&self.neuron_registry, &self.config)?;
        if self
            .latest_reconciliation_checkpoint
            .as_ref()
            .is_some_and(|checkpoint| {
                checkpoint.generation == 0
                    || checkpoint.generation > self.latest_reconciliation_generation
                    || checkpoint.event_marker == 0
                    || checkpoint.observed_at_nanos == 0
                    || checkpoint.snapshot_fingerprint.len() != 32
                    || checkpoint.live_cohort_count
                        > io_nns_types::backing::MAX_LIVE_UNWIND_COHORTS as u32
                    || io_core_model::claim_backing(io_core_model::Backing {
                        liquid: checkpoint.liquid_backing_e8s,
                        pooled: checkpoint.pooled_backing_e8s,
                        unwinding: checkpoint.unwinding_backing_e8s,
                        transit: checkpoint.transit_backing_e8s,
                    }) != Ok(checkpoint.total_claim_backing_e8s)
                    || io_core_model::target(
                        checkpoint.active_backing_io_e8s,
                        checkpoint.total_claim_backing_e8s,
                        checkpoint.claim_supply_e8s,
                    ) != Ok(checkpoint.pooled_target_e8s)
            })
        {
            return Err("reconciliation checkpoint fingerprint is malformed".into());
        }
        if let Some(batch) = &self.pending_entitlement_batch {
            batch.validate(&self.config)?;
            if batch.generation != self.latest_entitlement_batch_generation {
                return Err("pending entitlement batch generation is inconsistent".into());
            }
        }
        if let Some(completed) = &self.last_completed_claim_receipt {
            completed.validate()?;
            if completed.stream_operation_sequence >= self.next_operation_sequence.0 {
                return Err("completed claim-receipt sequence was not reserved".into());
            }
        }
        Ok(())
    }
}

impl RewardCheckpoint {
    pub const MAX_ENTRIES: usize = 1_000;

    pub fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        let _ = config;
        if self
            .last_processed_event
            .is_some_and(|event| event.end_timestamp_seconds == 0 || event.round == 0)
        {
            return Err("entitlement checkpoint is invalid".into());
        }
        if let Some(observation) = &self.latest_observation {
            observation.validate(config)?;
            if self.last_processed_event != Some(observation.event) {
                return Err("latest reward observation is not the checkpoint".into());
            }
        }
        if let Some(skipped) = &self.latest_skipped_event {
            if skipped.observed_at_nanos == 0
                || skipped.observed_event.end_timestamp_seconds == 0
                || skipped.ambiguous_event_count == 0
                || skipped.rounds_since_last_distribution == 0
                || self.missed_event_count < skipped.ambiguous_event_count
            {
                return Err("latest skipped reward event is invalid".into());
            }
        }
        Ok(())
    }
}

impl RewardEventObservation {
    fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        if self.event.end_timestamp_seconds == 0
            || self.event.round == 0
            || self.observed_at_nanos == 0
        {
            return Err("latest reward observation is invalid".into());
        }
        let _ = config;
        if self.eligible_credit_total > self.policy_credit
            || (matches!(
                self.classification,
                RewardEventClassification::MissedSkipped
                    | RewardEventClassification::StructuralOnly
            ) && (self.policy_credit != 0 || self.eligible_credit_total != 0))
            || (!matches!(
                self.classification,
                RewardEventClassification::MissedSkipped
                    | RewardEventClassification::StructuralOnly
            ) && self.policy_credit != io_reward_policy::DAILY_EVENT_CREDIT)
        {
            return Err("latest reward observation credit totals are inconsistent".into());
        }
        Ok(())
    }
}

impl PendingEntitlementBatch {
    pub fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        if self.generation == 0
            || self.frozen_at_timestamp_seconds == 0
            || self.through_event.end_timestamp_seconds == 0
            || self.through_event.round == 0
            || self.processed_event_count == 0
        {
            return Err("pending entitlement batch metadata is invalid".into());
        }
        validate_entitlement_entries(&self.entries, config)?;
        let total = self.entries.iter().try_fold(0u128, |sum, entry| {
            sum.checked_add(entry.accumulated_eligible_credit)
        });
        if total != Some(self.eligible_credit_total)
            || self.eligible_credit_total > self.policy_credit_total
            || self.policy_credit_total == 0
        {
            return Err("pending entitlement batch total is inconsistent".into());
        }
        Ok(())
    }
}

fn validate_entitlement_entries(
    entries: &[FrozenEntitlement],
    config: &StreamConfig,
) -> Result<(), String> {
    if entries.len() > RewardCheckpoint::MAX_ENTRIES {
        return Err("entitlement accumulator exceeds 1,000 entries".into());
    }
    let mut previous_id: Option<&[u8]> = None;
    let mut accounts = std::collections::BTreeSet::new();
    for entry in entries {
        let account = entry.destination.canonical()?;
        if entry.sns_neuron_id.len() != 32
            || previous_id.is_some_and(|previous| previous >= entry.sns_neuron_id.as_slice())
            || !accounts.insert(account)
            || account.owner != config.sns_governance
            || account.subaccount.as_slice() != entry.sns_neuron_id
            || entry.accumulated_eligible_credit == 0
            || config
                .nonredeemable_governance_io_accounts
                .iter()
                .try_fold(false, |matched, excluded| {
                    entry
                        .destination
                        .effective_eq(excluded)
                        .map(|same| matched || same)
                })?
        {
            return Err("entitlement entry is invalid or not canonically sorted".into());
        }
        previous_id = Some(&entry.sns_neuron_id);
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct CallerRedemptionState {
    pub next_nonce: u64,
    pub last_request_fingerprint: Option<Vec<u8>>,
    pub last_result: Option<RedemptionResult>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RedemptionResult {
    pub request_fingerprint: Vec<u8>,
    pub nonce: u64,
    pub io_block: u128,
    pub icp_block: u128,
    pub net_icp_e8s: u128,
    pub gross_icp_e8s: u128,
    pub io_fee_e8s: u128,
    pub icp_fee_e8s: u128,
    pub completed_at_nanos: u64,
}

impl CallerRedemptionState {
    pub fn validate(&self) -> Result<(), String> {
        match (&self.last_request_fingerprint, &self.last_result) {
            (None, None) => {}
            (Some(fingerprint), Some(result))
                if fingerprint.len() == 32
                    && result.request_fingerprint == *fingerprint
                    && result.request_fingerprint.len() == 32
                    && result.nonce.checked_add(1) == Some(self.next_nonce)
                    && result.completed_at_nanos > 0 => {}
            _ => return Err("caller redemption replay state is inconsistent".into()),
        }
        Ok(())
    }
}

macro_rules! candid_storable {
    ($type:ty, $max:expr) => {
        impl Storable for $type {
            fn to_bytes(&self) -> Cow<'_, [u8]> {
                Cow::Owned(candid::encode_one(self).expect("stable value must encode"))
            }

            fn into_bytes(self) -> Vec<u8> {
                candid::encode_one(self).expect("stable value must encode")
            }

            fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
                candid::decode_one(bytes.as_ref()).expect("stable V1 value must decode")
            }

            const BOUND: Bound = Bound::Bounded {
                max_size: $max,
                is_fixed_size: false,
            };
        }
    };
}

candid_storable!(StableStreamState, 2_000_000);
candid_storable!(CallerRedemptionState, 1_024);

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static STATE: RefCell<Option<StableCell<StableStreamState, Memory>>> =
        const { RefCell::new(None) };
    static REDEMPTIONS: RefCell<Option<StableBTreeMap<Principal, CallerRedemptionState, Memory>>> =
        const { RefCell::new(None) };
}

pub fn initialize(state: StreamStateV1, canister_self: Principal) -> Result<(), String> {
    state.validate(canister_self)?;
    STATE.with(|slot| {
        let memory = MEMORY_MANAGER.with(|manager| manager.borrow().get(MemoryId::new(0)));
        let cell = StableCell::init(memory, StableStreamState::V1(state));
        *slot.borrow_mut() = Some(cell);
    });
    REDEMPTIONS.with(|slot| {
        let memory = MEMORY_MANAGER.with(|manager| manager.borrow().get(MemoryId::new(1)));
        *slot.borrow_mut() = Some(StableBTreeMap::init(memory));
    });
    Ok(())
}

pub fn reopen(canister_self: Principal) {
    STATE.with(|slot| {
        let memory = MEMORY_MANAGER.with(|manager| manager.borrow().get(MemoryId::new(0)));
        *slot.borrow_mut() = Some(StableCell::init(
            memory,
            StableStreamState::V1(StreamStateV1::decode_placeholder()),
        ));
    });
    REDEMPTIONS.with(|slot| {
        let memory = MEMORY_MANAGER.with(|manager| manager.borrow().get(MemoryId::new(1)));
        *slot.borrow_mut() = Some(StableBTreeMap::init(memory));
    });
    let mut reopened = read();
    reopened
        .validate(canister_self)
        .unwrap_or_else(|error| panic!("invalid stable stream V1 state: {error}"));
    reopened.lifecycle = Lifecycle::Paused;
    if matches!(
        &reopened.active_operation,
        Some(StreamOperation::Redemption(operation))
            if matches!(operation.as_ref(), RedemptionStreamOperation::Preparing(_))
    ) {
        reopened.active_operation = None;
    }
    write(reopened);
}

pub fn read() -> StreamStateV1 {
    STATE.with(|slot| {
        slot.borrow()
            .as_ref()
            .expect("stream state is not initialized")
            .get()
            .clone()
            .into_v1()
    })
}

pub fn write(state: StreamStateV1) {
    STATE.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .expect("stream state is not initialized")
            .set(StableStreamState::V1(state));
    });
}

impl StableStreamState {
    fn into_v1(self) -> StreamStateV1 {
        match self {
            Self::V1(state) => state,
        }
    }
}

pub fn caller_state(caller: Principal) -> CallerRedemptionState {
    let value = REDEMPTIONS.with(|slot| {
        slot.borrow()
            .as_ref()
            .expect("redemption map is not initialized")
            .get(&caller)
            .unwrap_or_default()
    });
    value
        .validate()
        .unwrap_or_else(|error| panic!("invalid caller redemption state: {error}"));
    value
}

pub fn set_caller_state(caller: Principal, state: CallerRedemptionState) {
    state
        .validate()
        .unwrap_or_else(|error| panic!("invalid caller redemption state write: {error}"));
    REDEMPTIONS.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .expect("redemption map is not initialized")
            .insert(caller, state);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(CandidType)]
    enum PriorStableStreamState {
        V1(PriorStreamState),
    }

    #[derive(CandidType)]
    struct PriorStreamState {
        launch_schema_marker: u8,
        config: StreamConfig,
        lifecycle: Lifecycle,
    }

    #[derive(CandidType)]
    enum FutureStableStreamState {
        V2(StreamStateV1),
    }

    fn principal(value: u8) -> Principal {
        Principal::from_slice(&[value; 29])
    }

    fn account(owner: Principal, value: u8) -> Account {
        Account {
            owner,
            subaccount: Some(vec![value; 32]),
        }
    }

    fn valid_state() -> (Principal, StreamStateV1) {
        let canister_self = principal(1);
        let nns_manager = principal(4);
        (
            canister_self,
            StreamStateV1 {
                launch_schema_marker: LAUNCH_SCHEMA_MARKER,
                config: StreamConfig {
                    io_ledger: principal(2),
                    icp_ledger: principal(3),
                    nns_manager,
                    jupiter_receipt_source: account(nns_manager, 1),
                    jupiter_io_account: account(principal(7), 2),
                    sns_governance: principal(5),
                    sns_root: principal(6),
                    expected_sns_governance_module_hash: vec![8; 32],
                    approved_reward_event_duration_seconds: 86_400,
                    io_reserve: account(canister_self, 3),
                    liquid_icp: account(canister_self, 4),
                    nonredeemable_governance_io_accounts: vec![account(principal(5), 5)],
                    minimum_redemption_io_e8s: 100,
                    expected_io_fee_e8s: 10,
                    expected_icp_fee_e8s: 10,
                    maximum_request_lifetime_nanos: 1_000,
                    retry_delay_nanos: 10,
                    ledger_deduplication_window_nanos: 2_000,
                },
                lifecycle: Lifecycle::Paused,
                active_operation: None,
                reward_checkpoint: RewardCheckpoint::default(),
                pending_entitlement_batch: None,
                neuron_registry: Vec::new(),
                stake_observation_due: true,
                latest_reconciliation_checkpoint: None,
                latest_reconciliation_generation: 0,
                latest_entitlement_batch_generation: 0,
                next_operation_sequence: OperationSequence(1),
                control_epoch: 0,
                last_completed_claim_receipt: None,
            },
        )
    }

    fn registry_record(governance: Principal, index: u32) -> BackingRewardRecord {
        let mut id = vec![0; 32];
        id[28..].copy_from_slice(&index.to_be_bytes());
        BackingRewardRecord {
            sns_neuron_id: id.clone(),
            staking_account: Account {
                owner: governance,
                subaccount: Some(id),
            },
            accumulated_eligible_credit: 1,
            latest_structural_state: StructuralStakeState::Active,
            status: BackingRewardStatus::ActiveEligible {
                eligible_from_event: 1,
            },
            unresolved_cohort_generation: None,
        }
    }

    #[test]
    fn current_launch_state_round_trips_and_validates() {
        let (canister_self, state) = valid_state();
        assert_eq!(state.validate(canister_self), Ok(()));
        let bytes = candid::encode_one(StableStreamState::V1(state.clone())).unwrap();
        let decoded: StableStreamState = candid::decode_one(&bytes).unwrap();
        assert_eq!(decoded, StableStreamState::V1(state));
    }

    #[test]
    fn prior_and_future_launch_shapes_are_rejected() {
        let (_, state) = valid_state();
        let prior = PriorStableStreamState::V1(PriorStreamState {
            launch_schema_marker: LAUNCH_SCHEMA_MARKER - 1,
            config: state.config.clone(),
            lifecycle: Lifecycle::Paused,
        });
        assert!(
            candid::decode_one::<StableStreamState>(&candid::encode_one(prior).unwrap()).is_err()
        );
        let future = FutureStableStreamState::V2(state);
        assert!(
            candid::decode_one::<StableStreamState>(&candid::encode_one(future).unwrap()).is_err()
        );
    }

    #[test]
    fn registry_is_sorted_unique_and_bounded() {
        let (canister_self, mut state) = valid_state();
        state.neuron_registry = (0..1_000)
            .map(|index| registry_record(state.config.sns_governance, index))
            .collect();
        assert_eq!(state.validate(canister_self), Ok(()));
        let encoded = candid::encode_one(StableStreamState::V1(state.clone())).unwrap();
        assert!(encoded.len() < 2_000_000);

        state
            .neuron_registry
            .push(registry_record(state.config.sns_governance, 1_000));
        assert!(state.validate(canister_self).is_err());
        state.neuron_registry.truncate(1_000);
        state.neuron_registry[1].sns_neuron_id = state.neuron_registry[0].sns_neuron_id.clone();
        assert!(state.validate(canister_self).is_err());
    }

    #[test]
    fn upgrade_policy_reopens_paused() {
        let (_, mut state) = valid_state();
        state.lifecycle = Lifecycle::Ready;
        state.lifecycle = Lifecycle::Paused;
        assert_eq!(state.lifecycle, Lifecycle::Paused);
    }
}
