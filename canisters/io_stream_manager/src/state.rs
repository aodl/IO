use candid::{CandidType, Principal};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
use serde::Deserialize;
use std::{borrow::Cow, cell::RefCell};

use crate::{
    receipt::{LastCompletedReceipt, LiquidReceiptOperation, ReceiptPreparation},
    redemption::{RedemptionOperation, RedemptionPreparation},
};
pub use io_accounts::Account;

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StreamConfig {
    pub io_ledger: Principal,
    pub icp_ledger: Principal,
    pub nns_manager: Principal,
    pub jupiter_receipt_source: Account,
    pub two_week_receipt_source: Account,
    pub jupiter_io_account: Account,
    pub sns_governance: Principal,
    pub sns_root: Principal,
    pub expected_sns_governance_module_hash: Vec<u8>,
    pub approved_reward_event_duration_seconds: u64,
    pub io_reserve: Account,
    pub liquid_icp: Account,
    pub excluded_io_accounts: Vec<Account>,
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
        if self.jupiter_receipt_source.owner != self.nns_manager
            || self.two_week_receipt_source.owner != self.nns_manager
        {
            return Err("receipt source owners must equal NNS manager".into());
        }
        self.io_reserve.validate()?;
        self.liquid_icp.validate()?;
        self.jupiter_receipt_source.validate()?;
        self.two_week_receipt_source.validate()?;
        self.jupiter_io_account.validate()?;
        for (name, account) in [
            ("IO reserve", &self.io_reserve),
            ("liquid ICP", &self.liquid_icp),
            ("Jupiter receipt source", &self.jupiter_receipt_source),
            ("two-week receipt source", &self.two_week_receipt_source),
            ("Jupiter IO account", &self.jupiter_io_account),
        ] {
            if account.owner == Principal::anonymous() || account.owner == management {
                return Err(format!("{name} owner is forbidden"));
            }
        }
        if self
            .jupiter_receipt_source
            .effective_eq(&self.two_week_receipt_source)?
            || self.jupiter_receipt_source.effective_eq(&self.liquid_icp)?
            || self
                .two_week_receipt_source
                .effective_eq(&self.liquid_icp)?
            || self.jupiter_io_account.effective_eq(&self.io_reserve)?
        {
            return Err("receipt sources, reserve and liquid accounts must be distinct".into());
        }
        if self.excluded_io_accounts.len() > Self::MAX_EXCLUDED_ACCOUNTS {
            return Err("too many excluded IO accounts".into());
        }
        let mut canonical_excluded = std::collections::BTreeSet::new();
        for account in &self.excluded_io_accounts {
            account.validate()?;
            if account.owner == Principal::anonymous() || account.owner == management {
                return Err("excluded IO account owner is forbidden".into());
            }
            if account.effective_eq(&self.io_reserve)? {
                return Err("reserve account cannot be excluded".into());
            }
            if account.effective_eq(&self.jupiter_io_account)? {
                return Err("Jupiter IO account cannot be excluded".into());
            }
            if !canonical_excluded.insert(account.canonical()?) {
                return Err("excluded IO accounts must be unique".into());
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
    LiquidReceipt(Box<LiquidReceiptStreamOperation>),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RedemptionStreamOperation {
    Preparing(Box<RedemptionPreparation>),
    Active(Box<RedemptionOperation>),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum LiquidReceiptStreamOperation {
    Preparing(Box<ReceiptPreparation>),
    Active(Box<LiquidReceiptOperation>),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
#[rustfmt::skip]
pub struct RewardEntitlementEntry { pub sns_neuron_id: Vec<u8>, pub destination: Account, pub accumulated_eligible_credit: u128 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RewardEventClassification {
    ProposalBearing,
    NoProposalFallback,
    ZeroEligibleParticipation,
    MissedSkipped,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
#[rustfmt::skip]
pub struct RewardEventCredit { pub sns_neuron_id: Vec<u8>, pub destination: Account, pub event_credit: u128 }

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
#[rustfmt::skip]
pub struct RewardEventObservation { pub event: RewardEventId, pub proposal_count: u64, pub classification: RewardEventClassification, pub credits: Vec<RewardEventCredit>, pub policy_credit: u128, pub eligible_credit_total: u128, pub observed_at_nanos: u64 }

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
#[rustfmt::skip]
pub struct SkippedRewardEvent { pub previous_event: Option<RewardEventId>, pub observed_event: RewardEventId, pub ambiguous_event_count: u64, pub rounds_since_last_distribution: u64, pub observed_at_nanos: u64 }

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
#[rustfmt::skip]
pub struct RewardEntitlementAccumulator { pub last_processed_event: Option<RewardEventId>, pub entries: Vec<RewardEntitlementEntry>, pub accumulated_policy_credit: u128, pub processed_event_count: u64, pub missed_event_count: u64, pub reward_work_due: bool, pub reward_processing_paused: bool, pub latest_observation: Option<RewardEventObservation>, pub latest_skipped_event: Option<SkippedRewardEvent>, pub governance_parameters_fresh: bool }

impl Default for RewardEntitlementAccumulator {
    fn default() -> Self {
        Self {
            last_processed_event: None,
            entries: Vec::new(),
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
#[rustfmt::skip]
pub struct PendingEntitlementBatch { pub generation: u64, pub frozen_at_timestamp_seconds: u64, pub through_event: RewardEventId, pub target_icp_e8s: u128, pub entries: Vec<RewardEntitlementEntry>, pub eligible_credit_total: u128, pub policy_credit_total: u128, pub processed_event_count: u64 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
#[rustfmt::skip]
pub struct RewardEventId { pub end_timestamp_seconds: u64, pub round: u64 }

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
#[rustfmt::skip]
pub struct StreamStateV1 { pub launch_schema_marker: u8, pub config: StreamConfig, pub lifecycle: Lifecycle, pub active_operation: Option<StreamOperation>, pub reward_entitlements: RewardEntitlementAccumulator, pub pending_entitlement_batch: Option<PendingEntitlementBatch>, pub latest_entitlement_batch_generation: u64, pub next_nns_receipt_sequence: u64, pub next_operation_sequence: OperationSequence, pub control_epoch: u64, pub last_completed_receipt: Option<LastCompletedReceipt> }

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
            launch_schema_marker: 1,
            config: StreamConfig {
                io_ledger: anonymous,
                icp_ledger: anonymous,
                nns_manager: anonymous,
                jupiter_receipt_source: account.clone(),
                two_week_receipt_source: account.clone(),
                jupiter_io_account: account.clone(),
                sns_governance: anonymous,
                sns_root: anonymous,
                expected_sns_governance_module_hash: Vec::new(),
                approved_reward_event_duration_seconds: 0,
                io_reserve: account.clone(),
                liquid_icp: account,
                excluded_io_accounts: Vec::new(),
                minimum_redemption_io_e8s: 1,
                expected_io_fee_e8s: 0,
                expected_icp_fee_e8s: 0,
                maximum_request_lifetime_nanos: 1,
                retry_delay_nanos: 1,
                ledger_deduplication_window_nanos: 2,
            },
            lifecycle: Lifecycle::Paused,
            active_operation: None,
            reward_entitlements: RewardEntitlementAccumulator::default(),
            pending_entitlement_batch: None,
            latest_entitlement_batch_generation: 0,
            next_nns_receipt_sequence: 0,
            next_operation_sequence: OperationSequence(0),
            control_epoch: 0,
            last_completed_receipt: None,
        }
    }
}

impl StreamStateV1 {
    pub fn validate(&self, canister_self: Principal) -> Result<(), String> {
        if self.launch_schema_marker != 1 {
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
                        || self.config.excluded_io_accounts.iter().try_fold(
                            false,
                            |matched, account| {
                                value
                                    .account
                                    .effective_eq(account)
                                    .map(|same| matched || same)
                            },
                        )?
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
            Some(StreamOperation::LiquidReceipt(operation)) => match operation.as_ref() {
                LiquidReceiptStreamOperation::Preparing(value) => {
                    value.validate(&self.config)?;
                    if value.captured_control_epoch != self.control_epoch {
                        return Err("receipt preparation control epoch is stale".into());
                    }
                }
                LiquidReceiptStreamOperation::Active(value) => value.validate(&self.config)?,
            },
            None => {}
        }
        self.reward_entitlements.validate(&self.config)?;
        if let Some(batch) = &self.pending_entitlement_batch {
            batch.validate(&self.config)?;
            if batch.generation != self.latest_entitlement_batch_generation {
                return Err("pending entitlement batch generation is inconsistent".into());
            }
        }
        if let Some(completed) = &self.last_completed_receipt {
            completed.validate(&self.config, self.next_nns_receipt_sequence)?;
        }
        Ok(())
    }
}

impl RewardEntitlementAccumulator {
    pub const MAX_ENTRIES: usize = 1_000;

    pub fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        validate_entitlement_entries(&self.entries, config)?;
        let eligible_credit_total = self
            .entries
            .iter()
            .try_fold(0u128, |sum, entry| {
                sum.checked_add(entry.accumulated_eligible_credit)
            })
            .ok_or("entitlement accumulator total overflow")?;
        if eligible_credit_total > self.accumulated_policy_credit {
            return Err("eligible entitlement credit exceeds policy credit".into());
        }
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
        let entries = self
            .credits
            .iter()
            .map(|weight| RewardEntitlementEntry {
                sns_neuron_id: weight.sns_neuron_id.clone(),
                destination: weight.destination.clone(),
                accumulated_eligible_credit: weight.event_credit,
            })
            .collect::<Vec<_>>();
        validate_entitlement_entries(&entries, config)?;
        let eligible_credit_total = entries.iter().try_fold(0u128, |sum, entry| {
            sum.checked_add(entry.accumulated_eligible_credit)
        });
        if eligible_credit_total != Some(self.eligible_credit_total)
            || self.eligible_credit_total > self.policy_credit
            || (self.classification == RewardEventClassification::MissedSkipped
                && (self.policy_credit != 0 || self.eligible_credit_total != 0))
            || (self.classification != RewardEventClassification::MissedSkipped
                && self.policy_credit != io_reward_policy::DAILY_EVENT_CREDIT)
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
    entries: &[RewardEntitlementEntry],
    config: &StreamConfig,
) -> Result<(), String> {
    if entries.len() > RewardEntitlementAccumulator::MAX_ENTRIES {
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
                .excluded_io_accounts
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
    ) || matches!(
        &reopened.active_operation,
        Some(StreamOperation::LiquidReceipt(operation))
            if matches!(operation.as_ref(), LiquidReceiptStreamOperation::Preparing(_))
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

    #[derive(candid::CandidType)]
    enum FutureStableStreamState {
        V2(StreamStateV1),
    }

    #[derive(candid::CandidType)]
    struct PreviousStreamStateV1 {
        config: StreamConfig,
        lifecycle: Lifecycle,
        active_operation: Option<StreamOperation>,
        reward_entitlements: RewardEntitlementAccumulator,
        pending_entitlement_batch: Option<PendingEntitlementBatch>,
        latest_entitlement_batch_generation: u64,
        next_nns_receipt_sequence: u64,
        next_operation_sequence: OperationSequence,
        control_epoch: u64,
        last_completed_receipt: Option<LastCompletedReceipt>,
    }

    #[derive(candid::CandidType)]
    enum PreviousStableStreamState {
        V1(PreviousStreamStateV1),
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
        let governance = principal(5);
        let entry = RewardEntitlementEntry {
            sns_neuron_id: vec![1; 32],
            destination: account(governance, 1),
            accumulated_eligible_credit: 100,
        };
        let event = RewardEventId {
            end_timestamp_seconds: 86_400,
            round: 1,
        };
        (
            canister_self,
            StreamStateV1 {
                launch_schema_marker: 1,
                config: StreamConfig {
                    io_ledger: principal(2),
                    icp_ledger: principal(3),
                    nns_manager: principal(4),
                    jupiter_receipt_source: account(principal(4), 1),
                    two_week_receipt_source: account(principal(4), 2),
                    jupiter_io_account: account(principal(7), 3),
                    sns_governance: governance,
                    sns_root: principal(6),
                    expected_sns_governance_module_hash: vec![8; 32],
                    approved_reward_event_duration_seconds: 86_400,
                    io_reserve: account(canister_self, 4),
                    liquid_icp: account(canister_self, 5),
                    excluded_io_accounts: vec![account(governance, 9)],
                    minimum_redemption_io_e8s: 20_000,
                    expected_io_fee_e8s: 10_000,
                    expected_icp_fee_e8s: 10_000,
                    maximum_request_lifetime_nanos: 1_000_000,
                    retry_delay_nanos: 1,
                    ledger_deduplication_window_nanos: 2_000_000,
                },
                lifecycle: Lifecycle::Ready,
                active_operation: None,
                reward_entitlements: RewardEntitlementAccumulator {
                    last_processed_event: Some(event),
                    entries: vec![entry.clone()],
                    accumulated_policy_credit: io_reward_policy::DAILY_EVENT_CREDIT,
                    processed_event_count: 1,
                    missed_event_count: 0,
                    reward_work_due: false,
                    reward_processing_paused: false,
                    latest_observation: Some(RewardEventObservation {
                        event,
                        proposal_count: 1,
                        classification: RewardEventClassification::ProposalBearing,
                        credits: vec![RewardEventCredit {
                            sns_neuron_id: entry.sns_neuron_id,
                            destination: entry.destination,
                            event_credit: 100,
                        }],
                        policy_credit: io_reward_policy::DAILY_EVENT_CREDIT,
                        eligible_credit_total: 100,
                        observed_at_nanos: 1,
                    }),
                    latest_skipped_event: None,
                    governance_parameters_fresh: true,
                },
                pending_entitlement_batch: None,
                latest_entitlement_batch_generation: 0,
                next_nns_receipt_sequence: 0,
                next_operation_sequence: OperationSequence(1),
                control_epoch: 0,
                last_completed_receipt: None,
            },
        )
    }

    #[test]
    fn daily_reward_configuration_is_required() {
        let (canister_self, mut state) = valid_state();
        state.validate(canister_self).unwrap();
        state.config.approved_reward_event_duration_seconds = io_core_model::TWO_WEEK_SECONDS;
        assert!(state
            .validate(canister_self)
            .unwrap_err()
            .contains("one day"));
    }

    #[test]
    fn every_control_principal_pair_must_be_distinct() {
        let roles = ["self", "io", "icp", "nns", "governance", "root"];
        for left in 0..roles.len() {
            for right in left + 1..roles.len() {
                let (canister_self, mut state) = valid_state();
                let value = match roles[left] {
                    "self" => canister_self,
                    "io" => state.config.io_ledger,
                    "icp" => state.config.icp_ledger,
                    "nns" => state.config.nns_manager,
                    "governance" => state.config.sns_governance,
                    "root" => state.config.sns_root,
                    _ => unreachable!(),
                };
                match roles[right] {
                    "io" => state.config.io_ledger = value,
                    "icp" => state.config.icp_ledger = value,
                    "nns" => state.config.nns_manager = value,
                    "governance" => state.config.sns_governance = value,
                    "root" => state.config.sns_root = value,
                    _ => unreachable!(),
                }
                assert!(
                    state.validate(canister_self).is_err(),
                    "{} may not alias {}",
                    roles[left],
                    roles[right]
                );
            }
        }
    }

    #[test]
    fn io_and_icp_account_collisions_are_rejected_within_each_ledger() {
        let (canister_self, mut state) = valid_state();
        state.config.jupiter_io_account = state.config.io_reserve.clone();
        assert!(state.validate(canister_self).is_err());

        let (canister_self, mut state) = valid_state();
        state.config.excluded_io_accounts[0] = state.config.io_reserve.clone();
        assert!(state.validate(canister_self).is_err());

        let (canister_self, mut state) = valid_state();
        state.config.excluded_io_accounts[0] = state.config.jupiter_io_account.clone();
        assert!(state.validate(canister_self).is_err());

        let (canister_self, mut state) = valid_state();
        state.config.two_week_receipt_source = state.config.jupiter_receipt_source.clone();
        assert!(state.validate(canister_self).is_err());
    }

    #[test]
    fn accumulator_requires_sorted_unique_neurons_and_destinations() {
        let (canister_self, mut state) = valid_state();
        let entry = state.reward_entitlements.entries[0].clone();
        state.reward_entitlements.entries.push(entry);
        assert!(state.validate(canister_self).is_err());

        let (_, mut state) = valid_state();
        let governance = state.config.sns_governance;
        state.reward_entitlements.entries = vec![
            RewardEntitlementEntry {
                sns_neuron_id: vec![2; 32],
                destination: account(governance, 2),
                accumulated_eligible_credit: 1,
            },
            RewardEntitlementEntry {
                sns_neuron_id: vec![1; 32],
                destination: account(governance, 1),
                accumulated_eligible_credit: 1,
            },
        ];
        assert!(state.validate(canister_self).is_err());
    }

    #[test]
    fn one_zero_credit_pending_batch_is_valid_without_a_parallel_status_machine() {
        let (canister_self, mut state) = valid_state();
        state.reward_entitlements.entries.clear();
        state.reward_entitlements.accumulated_policy_credit = 0;
        state.pending_entitlement_batch = Some(PendingEntitlementBatch {
            generation: 1,
            frozen_at_timestamp_seconds: 1,
            through_event: state.reward_entitlements.last_processed_event.unwrap(),
            target_icp_e8s: 0,
            entries: Vec::new(),
            eligible_credit_total: 0,
            policy_credit_total: io_reward_policy::DAILY_EVENT_CREDIT,
            processed_event_count: 1,
        });
        state.latest_entitlement_batch_generation = 1;
        state.validate(canister_self).unwrap();
        state.pending_entitlement_batch = None;
        state.validate(canister_self).unwrap();
    }

    #[test]
    fn frozen_batch_and_later_live_entitlements_coexist() {
        let (canister_self, mut state) = valid_state();
        let governance = state.config.sns_governance;
        let first_event = state.reward_entitlements.last_processed_event.unwrap();
        let frozen_entry = state.reward_entitlements.entries[0].clone();
        state.pending_entitlement_batch = Some(PendingEntitlementBatch {
            generation: 1,
            frozen_at_timestamp_seconds: first_event.end_timestamp_seconds,
            through_event: first_event,
            target_icp_e8s: 1,
            entries: vec![frozen_entry],
            eligible_credit_total: 100,
            policy_credit_total: io_reward_policy::DAILY_EVENT_CREDIT,
            processed_event_count: 1,
        });
        state.latest_entitlement_batch_generation = 1;
        state.reward_entitlements.processed_event_count = 2;
        state.reward_entitlements.last_processed_event = Some(RewardEventId {
            end_timestamp_seconds: 172_800,
            round: 2,
        });
        state.reward_entitlements.entries = vec![RewardEntitlementEntry {
            sns_neuron_id: vec![2; 32],
            destination: account(governance, 2),
            accumulated_eligible_credit: 200,
        }];
        state.reward_entitlements.latest_observation = None;
        state.validate(canister_self).unwrap();
    }

    #[test]
    fn skipped_event_evidence_is_bounded_and_cumulative() {
        let (canister_self, mut state) = valid_state();
        let skipped = SkippedRewardEvent {
            previous_event: state.reward_entitlements.last_processed_event,
            observed_event: RewardEventId {
                end_timestamp_seconds: 259_200,
                round: 3,
            },
            ambiguous_event_count: 2,
            rounds_since_last_distribution: 1,
            observed_at_nanos: 2,
        };
        state.reward_entitlements.last_processed_event = Some(skipped.observed_event);
        state.reward_entitlements.missed_event_count = 2;
        state.reward_entitlements.latest_skipped_event = Some(skipped.clone());
        state.reward_entitlements.latest_observation = Some(RewardEventObservation {
            event: skipped.observed_event,
            proposal_count: 0,
            classification: RewardEventClassification::MissedSkipped,
            credits: Vec::new(),
            policy_credit: 0,
            eligible_credit_total: 0,
            observed_at_nanos: 2,
        });
        state.validate(canister_self).unwrap();
    }

    #[test]
    fn stable_reopen_preserves_entitlements_and_forces_paused() {
        let (canister_self, state) = valid_state();
        let expected = state.reward_entitlements.clone();
        initialize(state, canister_self).unwrap();
        reopen(canister_self);
        let reopened = read();
        assert_eq!(reopened.lifecycle, Lifecycle::Paused);
        assert_eq!(reopened.reward_entitlements, expected);
        assert!(reopened.active_operation.is_none());
    }

    #[test]
    fn maximum_simultaneously_valid_state_fits_the_stable_cell_bound() {
        let (canister_self, mut state) = valid_state();
        let governance = state.config.sns_governance;
        let entries = (0..RewardEntitlementAccumulator::MAX_ENTRIES)
            .map(|index| {
                let mut id = [0_u8; 32];
                id[24..].copy_from_slice(&(index as u64 + 1).to_be_bytes());
                RewardEntitlementEntry {
                    sns_neuron_id: id.to_vec(),
                    destination: Account {
                        owner: governance,
                        subaccount: Some(id.to_vec()),
                    },
                    accumulated_eligible_credit: 1,
                }
            })
            .collect::<Vec<_>>();
        state.reward_entitlements.entries = entries.clone();
        state.reward_entitlements.accumulated_policy_credit = entries.len() as u128;
        state.reward_entitlements.latest_observation = None;
        state.pending_entitlement_batch = Some(PendingEntitlementBatch {
            generation: 1,
            frozen_at_timestamp_seconds: 1,
            through_event: state.reward_entitlements.last_processed_event.unwrap(),
            target_icp_e8s: 1,
            entries: entries.clone(),
            eligible_credit_total: RewardEntitlementAccumulator::MAX_ENTRIES as u128,
            policy_credit_total: entries.len() as u128,
            processed_event_count: 1,
        });
        state.latest_entitlement_batch_generation = 1;
        let active_request = crate::receipt::PrepareLiquidReceiptArgs {
            receipt_sequence: 1,
            receipt_kind: crate::receipt::ReceiptKind::TwoWeekMaturity,
            source_operation_id: vec![9; 64],
            liquid_amount_e8s: entries.len() as u128,
            entitlement_batch_generation: Some(1),
        };
        let active_fingerprint = crate::receipt::request_fingerprint(&active_request);
        let backing_snapshot = crate::receipt::BackingSnapshot {
            total_io_supply_e8s: 10_000,
            reserve_io_e8s: 1_000,
            excluded_io_balances: vec![(state.config.excluded_io_accounts[0].clone(), 1_000)],
            liquid_icp_e8s: 8_000,
            io_fee_e8s: state.config.expected_io_fee_e8s,
            observed_at_nanos: u64::MAX,
        };
        let recipients = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let intent = crate::transfer::OwnTransferIntent::Icrc1 {
                    ledger: state.config.io_ledger,
                    from_subaccount: state.config.io_reserve.canonical().unwrap().subaccount,
                    to: entry.destination.clone(),
                    amount: 1,
                    fee: state.config.expected_io_fee_e8s,
                    memo: vec![8; crate::transfer::MAX_MEMO_BYTES],
                    created_at_time: index as u64 + 1,
                };
                crate::receipt::RewardRecipient {
                    sns_neuron_id: entry.sns_neuron_id.clone(),
                    destination: entry.destination.clone(),
                    io_e8s: 1,
                    transfer: Some(crate::transfer::TransferAttempt {
                        fingerprint: intent.fingerprint(),
                        intent,
                        state: crate::transfer::TransferState::Succeeded {
                            block: index as u128,
                        },
                    }),
                    refresh_attempted: true,
                }
            })
            .collect();
        state.active_operation = Some(StreamOperation::LiquidReceipt(Box::new(
            LiquidReceiptStreamOperation::Active(Box::new(
                crate::receipt::LiquidReceiptOperation::TwoWeek(Box::new(
                    crate::receipt::TwoWeekReceiptOperation {
                        context: crate::receipt::ReceiptContext {
                            request: active_request,
                            request_fingerprint: active_fingerprint,
                            source: state.config.two_week_receipt_source.clone(),
                            permit: crate::receipt::LiquidReceiptPermit {
                                sequence: 1,
                                destination: state.config.liquid_icp.clone(),
                                memo: crate::receipt::receipt_memo(state.config.nns_manager, 1),
                            },
                            backing_snapshot: backing_snapshot.clone(),
                        },
                        phase: crate::receipt::ReceiptPhase::Settling,
                        receipt_block: Some(u128::MAX),
                        settlement: Some(crate::receipt::TwoWeekSettlement {
                            backed_io_pool_e8s: entries.len() as u128,
                            recipients,
                            recipient_index: entries.len() as u32,
                            distributed_io_e8s: entries.len() as u128,
                            forfeited_io_e8s: 0,
                            rounding_dust_io_e8s: 0,
                        }),
                    },
                )),
            )),
        )));

        let completed_request = crate::receipt::PrepareLiquidReceiptArgs {
            receipt_sequence: 0,
            receipt_kind: crate::receipt::ReceiptKind::Jupiter,
            source_operation_id: vec![7; 64],
            liquid_amount_e8s: 1,
            entitlement_batch_generation: None,
        };
        let completed_fingerprint = crate::receipt::request_fingerprint(&completed_request);
        state.next_nns_receipt_sequence = 1;
        state.last_completed_receipt = Some(crate::receipt::LastCompletedReceipt {
            request: completed_request,
            request_fingerprint: completed_fingerprint.clone(),
            permit: crate::receipt::LiquidReceiptPermit {
                sequence: 0,
                destination: state.config.liquid_icp.clone(),
                memo: crate::receipt::receipt_memo(state.config.nns_manager, 0),
            },
            backing_snapshot,
            receipt_block: u128::MAX - 1,
            result: crate::receipt::CompletedReceiptResult::Jupiter(
                crate::receipt::JupiterReceiptResult {
                    request_fingerprint: completed_fingerprint,
                    receipt_block: u128::MAX - 1,
                    backed_io_e8s: u128::MAX,
                    io_transfer_block: u128::MAX,
                    io_fee_e8s: state.config.expected_io_fee_e8s,
                    completed_at_nanos: u64::MAX,
                },
            ),
        });
        state.validate(canister_self).unwrap();
        let stable = StableStreamState::V1(state);
        let encoded = stable.to_bytes();
        let Bound::Bounded { max_size, .. } = <StableStreamState as Storable>::BOUND else {
            panic!("stream state must remain bounded");
        };
        eprintln!(
            "maximum simultaneous Stream state encodes to {} bytes of the {}-byte stable bound",
            encoded.len(),
            max_size
        );
        assert!(encoded.len() <= max_size as usize);
    }

    #[test]
    fn strict_launch_v1_rejects_corrupt_and_future_state() {
        assert!(candid::decode_one::<StableStreamState>(b"not candid").is_err());
        let (_, state) = valid_state();
        let future = candid::encode_one(FutureStableStreamState::V2(state.clone())).unwrap();
        assert!(candid::decode_one::<StableStreamState>(&future).is_err());
        let previous = candid::encode_one(PreviousStableStreamState::V1(PreviousStreamStateV1 {
            config: state.config,
            lifecycle: state.lifecycle,
            active_operation: state.active_operation,
            reward_entitlements: state.reward_entitlements,
            pending_entitlement_batch: state.pending_entitlement_batch,
            latest_entitlement_batch_generation: state.latest_entitlement_batch_generation,
            next_nns_receipt_sequence: state.next_nns_receipt_sequence,
            next_operation_sequence: state.next_operation_sequence,
            control_epoch: state.control_epoch,
            last_completed_receipt: state.last_completed_receipt,
        }))
        .unwrap();
        assert!(candid::decode_one::<StableStreamState>(&previous).is_err());
    }
}
