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
        for (name, principal) in [
            ("canister self", canister_self),
            ("IO ledger", self.io_ledger),
            ("ICP ledger", self.icp_ledger),
            ("NNS manager", self.nns_manager),
            ("SNS governance", self.sns_governance),
        ] {
            if principal == Principal::anonymous() || principal == management {
                return Err(format!("{name} principal is forbidden"));
            }
        }
        if self.sns_root == Principal::anonymous()
            || self.sns_root == management
            || self.sns_root == self.sns_governance
            || self.sns_root == self.nns_manager
        {
            return Err("SNS Root principal is forbidden or aliases another boundary".into());
        }
        if self.expected_sns_governance_module_hash.len() != 32 {
            return Err("expected SNS Governance module hash must contain 32 bytes".into());
        }
        if self.approved_reward_event_duration_seconds != io_core_model::TWO_WEEK_SECONDS {
            return Err("approved reward-event duration must equal two weeks".into());
        }
        if self.io_ledger == self.icp_ledger {
            return Err("IO and ICP ledgers must be distinct".into());
        }
        if self.nns_manager == self.sns_governance {
            return Err("NNS manager and SNS governance must be distinct".into());
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
        if self.io_reserve.effective_eq(&self.liquid_icp)? {
            return Err("reserve and liquid accounts must be distinct".into());
        }
        if self
            .jupiter_receipt_source
            .effective_eq(&self.two_week_receipt_source)?
            || self.jupiter_receipt_source.effective_eq(&self.io_reserve)?
            || self.jupiter_receipt_source.effective_eq(&self.liquid_icp)?
            || self
                .two_week_receipt_source
                .effective_eq(&self.io_reserve)?
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
    CohortCapture(Box<CohortCaptureOperation>),
    CohortClose(Box<RewardCohort>),
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
pub struct RewardCohort {
    pub generation: u64,
    pub captured_at_timestamp_seconds: u64,
    pub closes_at_timestamp_seconds: u64,
    pub target_icp_e8s: u128,
    pub reward_event_at_capture: Option<RewardEventId>,
    pub reward_share_snapshot: Option<RewardShareSnapshot>,
    pub members: Vec<RewardMember>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardEventId {
    pub end_timestamp_seconds: u64,
    pub round: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardShareSnapshot {
    pub event: RewardEventId,
    pub settled_proposal_count: u64,
    pub total_eligible_reward_shares: u128,
    pub captured_at_nanos: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardMember {
    pub sns_neuron_id: Vec<u8>,
    pub account: Account,
    pub frozen_stake_e8s: u128,
    pub observed_stake_e8s: u128,
    pub reward_shares: Option<u128>,
    pub destination_is_currently_eligible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum CohortCaptureOperation {
    Prepared {
        generation: u64,
        captured_at_timestamp_seconds: u64,
    },
    TargetSubmitted {
        cohort: RewardCohort,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StreamStateV1 {
    pub config: StreamConfig,
    pub lifecycle: Lifecycle,
    pub active_operation: Option<StreamOperation>,
    pub active_reward_cohort: Option<RewardCohort>,
    pub pending_reward_cohort: Option<RewardCohort>,
    #[serde(default)]
    pub pending_maturity_prepared: bool,
    pub latest_cohort_generation: u64,
    pub next_nns_receipt_sequence: u64,
    pub next_cohort_timestamp_seconds: u64,
    pub next_operation_sequence: OperationSequence,
    pub control_epoch: u64,
    pub last_completed_receipt: Option<LastCompletedReceipt>,
    pub last_consumed_reward_event: Option<RewardEventId>,
    pub reward_work_due: bool,
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
            active_reward_cohort: None,
            pending_reward_cohort: None,
            pending_maturity_prepared: false,
            latest_cohort_generation: 0,
            next_nns_receipt_sequence: 0,
            next_cohort_timestamp_seconds: 0,
            next_operation_sequence: OperationSequence(0),
            control_epoch: 0,
            last_completed_receipt: None,
            last_consumed_reward_event: None,
            reward_work_due: false,
        }
    }
}

impl StreamStateV1 {
    pub fn validate(&self, canister_self: Principal) -> Result<(), String> {
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
            Some(StreamOperation::CohortCapture(operation)) => match operation.as_ref() {
                CohortCaptureOperation::Prepared {
                    generation,
                    captured_at_timestamp_seconds,
                } => {
                    if *captured_at_timestamp_seconds == 0
                        || self.latest_cohort_generation.checked_add(1) != Some(*generation)
                        || self.active_reward_cohort.is_some()
                    {
                        return Err("prepared cohort capture is inconsistent".into());
                    }
                }
                CohortCaptureOperation::TargetSubmitted { cohort } => {
                    cohort.validate(&self.config)?;
                    if self.latest_cohort_generation.checked_add(1) != Some(cohort.generation)
                        || self.active_reward_cohort.is_some()
                    {
                        return Err("submitted cohort capture is inconsistent".into());
                    }
                }
            },
            Some(StreamOperation::CohortClose(cohort)) => {
                cohort.validate(&self.config)?;
                if cohort.generation != self.latest_cohort_generation
                    || self.active_reward_cohort.is_some()
                    || self.pending_reward_cohort.is_some()
                {
                    return Err("cohort close operation is inconsistent".into());
                }
            }
            None => {}
        }
        for cohort in [
            self.active_reward_cohort.as_ref(),
            self.pending_reward_cohort.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            cohort.validate(&self.config)?;
            if cohort.generation > self.latest_cohort_generation {
                return Err("reward cohort generation exceeds state generation".into());
            }
        }
        if let (Some(active), Some(pending)) = (
            self.active_reward_cohort.as_ref(),
            self.pending_reward_cohort.as_ref(),
        ) {
            if pending.generation.checked_add(1) != Some(active.generation) {
                return Err("active reward cohort must follow pending cohort".into());
            }
        }
        if self.pending_reward_cohort.is_none() && self.pending_maturity_prepared {
            return Err("prepared maturity lacks a pending reward cohort".into());
        }
        if self
            .active_reward_cohort
            .as_ref()
            .or(self.pending_reward_cohort.as_ref())
            .is_some_and(|cohort| cohort.generation != self.latest_cohort_generation)
        {
            return Err("latest reward cohort slot does not match state generation".into());
        }
        match &self.active_reward_cohort {
            Some(active)
                if self.next_cohort_timestamp_seconds != active.closes_at_timestamp_seconds =>
            {
                return Err("active cohort deadline is inconsistent".into())
            }
            None if self.next_cohort_timestamp_seconds != 0 => {
                return Err("cohort deadline exists without an active cohort".into())
            }
            _ => {}
        }
        if let Some(completed) = &self.last_completed_receipt {
            completed.validate(&self.config, self.next_nns_receipt_sequence)?;
        }
        Ok(())
    }
}

impl RewardCohort {
    pub const MAX_MEMBERS: usize = 1_000;

    pub fn validate(&self, config: &StreamConfig) -> Result<(), String> {
        if self.generation == 0
            || self.captured_at_timestamp_seconds == 0
            || self.closes_at_timestamp_seconds
                != self
                    .captured_at_timestamp_seconds
                    .checked_add(io_core_model::TWO_WEEK_SECONDS)
                    .ok_or("reward cohort close timestamp overflow")?
            || self.members.is_empty()
            || self.members.len() > Self::MAX_MEMBERS
        {
            return Err("reward cohort timestamp or capacity is invalid".into());
        }
        if let Some(event) = self.reward_event_at_capture {
            if event.end_timestamp_seconds == 0 {
                return Err("reward cohort capture event is invalid".into());
            }
        }
        if let Some(snapshot) = &self.reward_share_snapshot {
            if snapshot.event.end_timestamp_seconds == 0
                || snapshot.captured_at_nanos == 0
                || self.reward_event_at_capture.is_some_and(|before| {
                    snapshot.event.round <= before.round
                        || snapshot.event.end_timestamp_seconds <= before.end_timestamp_seconds
                })
            {
                return Err("reward-share snapshot is invalid".into());
            }
        }
        let mut neuron_ids = std::collections::BTreeSet::new();
        let mut accounts = std::collections::BTreeSet::new();
        for member in &self.members {
            let account = member.account.canonical()?;
            if member.sns_neuron_id.len() != 32
                || !neuron_ids.insert(member.sns_neuron_id.clone())
                || !accounts.insert(account)
                || account.owner != config.sns_governance
                || account.subaccount.as_slice() != member.sns_neuron_id
                || config
                    .excluded_io_accounts
                    .iter()
                    .try_fold(false, |matched, excluded| {
                        member
                            .account
                            .effective_eq(excluded)
                            .map(|same| matched || same)
                    })?
                || member.frozen_stake_e8s == 0
                || (member.destination_is_currently_eligible
                    && member.observed_stake_e8s < member.frozen_stake_e8s)
                || (self.reward_share_snapshot.is_none() && member.reward_shares.is_some())
            {
                return Err("reward cohort member is invalid".into());
            }
        }
        if let Some(snapshot) = &self.reward_share_snapshot {
            let exact_total = self.members.iter().try_fold(0u128, |sum, member| {
                sum.checked_add(member.reward_shares.unwrap_or(0))
            });
            if exact_total != Some(snapshot.total_eligible_reward_shares)
                || self
                    .members
                    .iter()
                    .any(|member| member.reward_shares.is_none())
            {
                return Err(
                    "reward-share snapshot total does not match exact member shares".into(),
                );
            }
        }
        Ok(())
    }
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
    ) || matches!(
        &reopened.active_operation,
        Some(StreamOperation::CohortCapture(operation))
            if matches!(operation.as_ref(), CohortCaptureOperation::Prepared { .. })
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
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll, Waker};

    fn poll_ready<F: Future>(future: F) -> F::Output {
        let mut context = Context::from_waker(Waker::noop());
        let mut future = Box::pin(future);
        match Pin::new(&mut future).poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("fail-closed capability check unexpectedly reached an await"),
        }
    }

    #[test]
    fn reward_readiness_fails_closed_without_canister_calls() {
        let canister = Principal::from_slice(&[42; 29]);
        let owner = Principal::from_slice(&[43; 29]);
        let nns_manager = Principal::from_slice(&[3; 29]);
        let config = StreamConfig {
            io_ledger: Principal::from_slice(&[1; 29]),
            icp_ledger: Principal::from_slice(&[2; 29]),
            nns_manager,
            jupiter_receipt_source: Account {
                owner: nns_manager,
                subaccount: Some(vec![2; 32]),
            },
            two_week_receipt_source: Account {
                owner: nns_manager,
                subaccount: Some(vec![3; 32]),
            },
            jupiter_io_account: Account {
                owner,
                subaccount: Some(vec![4; 32]),
            },
            sns_governance: Principal::from_slice(&[4; 29]),
            sns_root: Principal::from_slice(&[5; 29]),
            expected_sns_governance_module_hash: vec![0; 32],
            approved_reward_event_duration_seconds: io_core_model::TWO_WEEK_SECONDS,
            io_reserve: Account {
                owner: canister,
                subaccount: Some(vec![5; 32]),
            },
            liquid_icp: Account {
                owner: canister,
                subaccount: Some(vec![6; 32]),
            },
            excluded_io_accounts: Vec::new(),
            minimum_redemption_io_e8s: 2,
            expected_io_fee_e8s: 1,
            expected_icp_fee_e8s: 1,
            maximum_request_lifetime_nanos: 100,
            retry_delay_nanos: 1,
            ledger_deduplication_window_nanos: 200,
        };
        initialize(
            StreamStateV1 {
                config,
                lifecycle: Lifecycle::Paused,
                active_operation: None,
                active_reward_cohort: None,
                pending_reward_cohort: None,
                pending_maturity_prepared: false,
                latest_cohort_generation: 0,
                next_nns_receipt_sequence: 0,
                next_cohort_timestamp_seconds: 0,
                next_operation_sequence: OperationSequence(0),
                control_epoch: 0,
                last_completed_receipt: None,
                last_consumed_reward_event: None,
                reward_work_due: false,
            },
            canister,
        )
        .unwrap();
        let error = poll_ready(crate::lifecycle::readiness_preflight(canister, 0)).unwrap_err();
        assert_eq!(
            error,
            crate::api::ApiError::Pending(
                "SNS get_sns_canisters_summary readiness failed: canister calls are unavailable on host".into()
            )
        );
        assert_eq!(read().lifecycle, Lifecycle::Paused);
    }

    #[test]
    fn v1_cell_cancels_no_effect_preparation_and_preserves_caller_nonce() {
        let principal = Principal::from_slice(&[42; 29]);
        let io_ledger = Principal::from_slice(&[1; 29]);
        let icp_ledger = Principal::from_slice(&[2; 29]);
        let manager = Principal::from_slice(&[3; 29]);
        let governance = Principal::from_slice(&[4; 29]);
        let account = Account {
            owner: principal,
            subaccount: None,
        };
        initialize(
            StreamStateV1 {
                config: StreamConfig {
                    io_ledger,
                    icp_ledger,
                    nns_manager: manager,
                    jupiter_receipt_source: Account {
                        owner: manager,
                        subaccount: Some(vec![2; 32]),
                    },
                    two_week_receipt_source: Account {
                        owner: manager,
                        subaccount: Some(vec![3; 32]),
                    },
                    jupiter_io_account: Account {
                        owner: manager,
                        subaccount: Some(vec![4; 32]),
                    },
                    sns_governance: governance,
                    sns_root: Principal::from_slice(&[5; 29]),
                    expected_sns_governance_module_hash: vec![0; 32],
                    approved_reward_event_duration_seconds: io_core_model::TWO_WEEK_SECONDS,
                    io_reserve: account.clone(),
                    liquid_icp: Account {
                        owner: principal,
                        subaccount: Some(vec![1; 32]),
                    },
                    excluded_io_accounts: Vec::new(),
                    minimum_redemption_io_e8s: 2,
                    expected_io_fee_e8s: 1,
                    expected_icp_fee_e8s: 1,
                    maximum_request_lifetime_nanos: 100,
                    retry_delay_nanos: 1,
                    ledger_deduplication_window_nanos: 200,
                },
                lifecycle: Lifecycle::Paused,
                active_operation: None,
                active_reward_cohort: None,
                pending_reward_cohort: None,
                pending_maturity_prepared: false,
                latest_cohort_generation: 0,
                next_nns_receipt_sequence: 7,
                next_cohort_timestamp_seconds: 0,
                next_operation_sequence: OperationSequence(0),
                control_epoch: 0,
                last_completed_receipt: None,
                last_consumed_reward_event: None,
                reward_work_due: false,
            },
            principal,
        )
        .unwrap();
        set_caller_state(
            principal,
            CallerRedemptionState {
                next_nonce: 3,
                last_request_fingerprint: None,
                last_result: None,
            },
        );
        reopen(principal);
        assert_eq!(read().next_nns_receipt_sequence, 7);
        assert_eq!(caller_state(principal).next_nonce, 3);

        let user = Principal::from_slice(&[9; 29]);
        let request = crate::redemption::CanonicalRedeemRequestV1 {
            effective_subaccount: [0; 32],
            io_amount_e8s: 2,
            min_icp_out_e8s: 1,
            max_io_fee_e8s: 1,
            max_icp_fee_e8s: 1,
            expires_at_nanos: 50,
            nonce: 0,
        };
        let preparation = crate::redemption::RedemptionPreparation {
            sequence: OperationSequence(0),
            captured_control_epoch: 4,
            request_fingerprint: crate::redemption::request_fingerprint(user, &request),
            account: request.account(user),
            request,
            caller: user,
            prepared_at_nanos: 1,
        };
        let mut with_preparation = read();
        with_preparation.lifecycle = Lifecycle::Ready;
        with_preparation.control_epoch = 4;
        with_preparation.next_operation_sequence = OperationSequence(1);
        with_preparation.active_operation = Some(StreamOperation::Redemption(Box::new(
            RedemptionStreamOperation::Preparing(Box::new(preparation.clone())),
        )));
        write(with_preparation);
        crate::lifecycle::set_paused();
        assert_eq!(read().lifecycle, Lifecycle::Paused);
        assert!(read().active_operation.is_none());

        let mut before_upgrade = read();
        before_upgrade.lifecycle = Lifecycle::Ready;
        before_upgrade.active_operation = Some(StreamOperation::Redemption(Box::new(
            RedemptionStreamOperation::Preparing(Box::new(preparation)),
        )));
        write(before_upgrade);
        reopen(principal);
        assert_eq!(read().lifecycle, Lifecycle::Paused);
        assert!(read().active_operation.is_none());
        assert_eq!(caller_state(principal).next_nonce, 3);

        let receipt_request = io_receipt_types::PrepareLiquidReceiptArgs {
            receipt_sequence: 7,
            receipt_kind: io_receipt_types::ReceiptKind::Jupiter,
            source_operation_id: vec![8],
            liquid_amount_e8s: 10,
            cohort_generation: None,
        };
        let receipt_preparation = crate::receipt::ReceiptPreparation {
            request_fingerprint: crate::receipt::request_fingerprint(&receipt_request),
            request: receipt_request,
            authority: manager,
            captured_control_epoch: 4,
            prepared_at_nanos: 1,
        };
        let mut before_receipt_upgrade = read();
        before_receipt_upgrade.lifecycle = Lifecycle::Ready;
        before_receipt_upgrade.control_epoch = 4;
        before_receipt_upgrade.active_operation = Some(StreamOperation::LiquidReceipt(Box::new(
            LiquidReceiptStreamOperation::Preparing(Box::new(receipt_preparation)),
        )));
        write(before_receipt_upgrade);
        reopen(principal);
        assert_eq!(read().lifecycle, Lifecycle::Paused);
        assert!(read().active_operation.is_none());

        let config = read().config;
        let mut unsafe_jupiter = config.clone();
        unsafe_jupiter.jupiter_io_account.owner = Principal::anonymous();
        assert!(unsafe_jupiter.validate(principal).is_err());
        let mut excluded_jupiter = config.clone();
        excluded_jupiter
            .excluded_io_accounts
            .push(excluded_jupiter.jupiter_io_account.clone());
        assert!(excluded_jupiter.validate(principal).is_err());

        let member = RewardMember {
            sns_neuron_id: vec![7; 32],
            account: Account {
                owner: config.sns_governance,
                subaccount: Some(vec![7; 32]),
            },
            frozen_stake_e8s: 1,
            observed_stake_e8s: 1,
            reward_shares: None,
            destination_is_currently_eligible: true,
        };
        let cohort = RewardCohort {
            generation: 1,
            captured_at_timestamp_seconds: 1,
            closes_at_timestamp_seconds: 1 + io_core_model::TWO_WEEK_SECONDS,
            target_icp_e8s: 1,
            reward_event_at_capture: Some(RewardEventId {
                end_timestamp_seconds: 1,
                round: 1,
            }),
            reward_share_snapshot: None,
            members: vec![member.clone()],
        };
        assert_eq!(cohort.validate(&config), Ok(()));
        let mut duplicate = cohort.clone();
        duplicate.members.push(member.clone());
        assert!(duplicate.validate(&config).is_err());
        let mut wrong_destination = cohort;
        wrong_destination.members[0].account.subaccount = Some(vec![8; 32]);
        assert!(wrong_destination.validate(&config).is_err());
        let mut excluded = config.clone();
        excluded.excluded_io_accounts.push(member.account);
        duplicate.members.truncate(1);
        assert!(duplicate.validate(&excluded).is_err());

        let mut one_day = config.clone();
        one_day.approved_reward_event_duration_seconds = 86_400;
        assert!(one_day.validate(principal).is_err());
        let mut wrong_interval = config.clone();
        wrong_interval.approved_reward_event_duration_seconds = io_core_model::TWO_WEEK_SECONDS + 1;
        assert!(wrong_interval.validate(principal).is_err());
        let mut installed = io_sns_reward_boundary::InstalledGovernance {
            canister: config.sns_governance,
            module_hash: config.expected_sns_governance_module_hash.clone(),
            initial_reward_rate_basis_points: 0,
            final_reward_rate_basis_points: 0,
            round_duration_seconds: io_core_model::TWO_WEEK_SECONDS,
        };
        assert_eq!(
            crate::lifecycle::validate_installed_governance(&config, &installed),
            Ok(())
        );
        installed.round_duration_seconds = 86_400;
        assert!(crate::lifecycle::validate_installed_governance(&config, &installed).is_err());
        installed.round_duration_seconds = io_core_model::TWO_WEEK_SECONDS + 1;
        assert!(crate::lifecycle::validate_installed_governance(&config, &installed).is_err());

        let prepared = CohortCaptureOperation::Prepared {
            generation: 1,
            captured_at_timestamp_seconds: 10,
        };
        let mut prepared_state = read();
        prepared_state.lifecycle = Lifecycle::Ready;
        prepared_state.active_operation =
            Some(StreamOperation::CohortCapture(Box::new(prepared.clone())));
        write(prepared_state.clone());
        crate::lifecycle::set_paused();
        assert!(read().active_operation.is_none());

        prepared_state.lifecycle = Lifecycle::Ready;
        write(prepared_state);
        reopen(principal);
        assert_eq!(read().lifecycle, Lifecycle::Paused);
        assert!(read().active_operation.is_none());

        let mut submitted_state = read();
        submitted_state.lifecycle = Lifecycle::Ready;
        submitted_state.active_operation = Some(StreamOperation::CohortCapture(Box::new(
            CohortCaptureOperation::TargetSubmitted {
                cohort: duplicate.clone(),
            },
        )));
        write(submitted_state.clone());
        reopen(principal);
        assert_eq!(read().lifecycle, Lifecycle::Paused);
        assert_eq!(read().active_operation, submitted_state.active_operation);

        let mut under_target = read();
        under_target.lifecycle = Lifecycle::Ready;
        write(under_target.clone());
        assert!(matches!(
            crate::rewards::reject_under_target(under_target),
            crate::api::ApiError::Pending(_)
        ));
        assert!(read().active_operation.is_none());
        assert_eq!(read().latest_cohort_generation, 0);
        assert_eq!(crate::api::require_ready(&read()), Ok(()));

        let fresh = CohortCaptureOperation::Prepared {
            generation: 1,
            captured_at_timestamp_seconds: 11,
        };
        let mut fresh_state = read();
        fresh_state.active_operation =
            Some(StreamOperation::CohortCapture(Box::new(fresh.clone())));
        write(fresh_state.clone());
        crate::rewards::clear_matching_capture_preparation(&prepared);
        assert_eq!(read(), fresh_state);

        let mut failed_capture = read();
        failed_capture.lifecycle = Lifecycle::Ready;
        failed_capture.active_operation = None;
        write(failed_capture);
        assert!(poll_ready(crate::rewards::capture(12)).is_err());
        assert!(read().active_operation.is_none());
    }

    #[test]
    fn maximum_v1_state_has_one_operation_and_reopens() {
        use crate::receipt::{
            LiquidReceiptOperation, ReceiptContext, ReceiptPhase, RewardRecipient,
            TwoWeekReceiptOperation, TwoWeekSettlement,
        };
        use io_receipt_types::{LiquidReceiptPermit, PrepareLiquidReceiptArgs, ReceiptKind};

        let canister = Principal::from_slice(&[42; 29]);
        let manager = Principal::from_slice(&[3; 29]);
        let governance = Principal::from_slice(&[4; 29]);
        let account = |owner, byte| Account {
            owner,
            subaccount: Some(vec![byte; 32]),
        };
        let config = StreamConfig {
            io_ledger: Principal::from_slice(&[1; 29]),
            icp_ledger: Principal::from_slice(&[2; 29]),
            nns_manager: manager,
            jupiter_receipt_source: account(manager, 1),
            two_week_receipt_source: account(manager, 2),
            jupiter_io_account: account(manager, 3),
            sns_governance: governance,
            sns_root: Principal::from_slice(&[5; 29]),
            expected_sns_governance_module_hash: vec![6; 32],
            approved_reward_event_duration_seconds: io_core_model::TWO_WEEK_SECONDS,
            io_reserve: account(canister, 4),
            liquid_icp: account(canister, 5),
            excluded_io_accounts: Vec::new(),
            minimum_redemption_io_e8s: 2,
            expected_io_fee_e8s: 1,
            expected_icp_fee_e8s: 1,
            maximum_request_lifetime_nanos: 100,
            retry_delay_nanos: 1,
            ledger_deduplication_window_nanos: 200,
        };
        let members = (0..RewardCohort::MAX_MEMBERS)
            .map(|index| {
                let mut id = vec![7; 32];
                id[..8].copy_from_slice(&(index as u64).to_be_bytes());
                RewardMember {
                    sns_neuron_id: id.clone(),
                    account: Account {
                        owner: governance,
                        subaccount: Some(id),
                    },
                    frozen_stake_e8s: 1,
                    observed_stake_e8s: 1,
                    reward_shares: None,
                    destination_is_currently_eligible: true,
                }
            })
            .collect::<Vec<_>>();
        let active = RewardCohort {
            generation: 1,
            captured_at_timestamp_seconds: 1,
            closes_at_timestamp_seconds: 1 + io_core_model::TWO_WEEK_SECONDS,
            target_icp_e8s: 1,
            reward_event_at_capture: Some(RewardEventId {
                end_timestamp_seconds: 1,
                round: 1,
            }),
            reward_share_snapshot: None,
            members,
        };
        let mut settled = active.clone();
        for member in &mut settled.members {
            member.reward_shares = Some(1);
        }
        settled.reward_share_snapshot = Some(RewardShareSnapshot {
            event: RewardEventId {
                end_timestamp_seconds: 2,
                round: 2,
            },
            settled_proposal_count: 1,
            total_eligible_reward_shares: RewardCohort::MAX_MEMBERS as u128,
            captured_at_nanos: 1,
        });
        let base = StreamStateV1 {
            config: config.clone(),
            lifecycle: Lifecycle::Paused,
            active_operation: None,
            active_reward_cohort: None,
            pending_reward_cohort: None,
            pending_maturity_prepared: false,
            latest_cohort_generation: 1,
            next_nns_receipt_sequence: 0,
            next_cohort_timestamp_seconds: 0,
            next_operation_sequence: OperationSequence(0),
            control_epoch: 0,
            last_completed_receipt: None,
            last_consumed_reward_event: None,
            reward_work_due: false,
        };
        let mut active_state = base.clone();
        active_state.active_reward_cohort = Some(active.clone());
        active_state.next_cohort_timestamp_seconds = active.closes_at_timestamp_seconds;
        active_state.validate(canister).unwrap();

        let mut capture_state = base.clone();
        capture_state.latest_cohort_generation = 0;
        capture_state.active_operation = Some(StreamOperation::CohortCapture(Box::new(
            CohortCaptureOperation::TargetSubmitted {
                cohort: active.clone(),
            },
        )));
        capture_state.validate(canister).unwrap();

        let mut close_state = base.clone();
        close_state.reward_work_due = true;
        close_state.active_operation = Some(StreamOperation::CohortClose(Box::new(active.clone())));
        close_state.validate(canister).unwrap();

        let mut pending_state = base.clone();
        pending_state.pending_reward_cohort = Some(settled.clone());
        pending_state.validate(canister).unwrap();
        pending_state.lifecycle = Lifecycle::Ready;
        assert_eq!(crate::api::require_ready(&pending_state), Ok(()));
        assert!(pending_state.active_operation.is_none());
        assert!(crate::rewards::require_capture_slot(&pending_state, 2).is_err());
        let caller = Principal::from_slice(&[9; 29]);
        let redeem_request = crate::redemption::CanonicalRedeemRequestV1 {
            effective_subaccount: [0; 32],
            io_amount_e8s: 2,
            min_icp_out_e8s: 1,
            max_io_fee_e8s: 1,
            max_icp_fee_e8s: 1,
            expires_at_nanos: 50,
            nonce: 0,
        };
        let mut redemption_available = pending_state.clone();
        redemption_available.next_operation_sequence = OperationSequence(1);
        redemption_available.active_operation = Some(StreamOperation::Redemption(Box::new(
            RedemptionStreamOperation::Preparing(Box::new(
                crate::redemption::RedemptionPreparation {
                    sequence: OperationSequence(0),
                    captured_control_epoch: 0,
                    request_fingerprint: crate::redemption::request_fingerprint(
                        caller,
                        &redeem_request,
                    ),
                    account: redeem_request.account(caller),
                    request: redeem_request,
                    caller,
                    prepared_at_nanos: 1,
                },
            )),
        )));
        redemption_available.validate(canister).unwrap();
        initialize(pending_state.clone(), canister).unwrap();
        for error in [
            crate::reward_nns::CallError::Waiting("below threshold".into()),
            crate::reward_nns::CallError::Paused,
            crate::reward_nns::CallError::Pending("ambiguous".into()),
        ] {
            let expected = read();
            assert!(crate::rewards::complete_pending_maturity(
                expected.clone(),
                settled.clone(),
                Err(error),
            )
            .is_err());
            assert_eq!(read(), expected);
            assert!(read().active_operation.is_none());
            assert_eq!(crate::api::require_ready(&read()), Ok(()));
        }
        let expected = read();
        assert_eq!(
            crate::rewards::complete_pending_maturity(expected, settled.clone(), Ok(()),).unwrap(),
            settled
        );
        assert!(read().pending_maturity_prepared);
        write(pending_state.clone());
        pending_state.pending_maturity_prepared = true;
        pending_state.validate(canister).unwrap();
        assert_eq!(
            crate::rewards::require_capture_slot(&pending_state, 2),
            Ok(())
        );
        let mut orphan_prepared = base.clone();
        orphan_prepared.pending_maturity_prepared = true;
        assert!(orphan_prepared.validate(canister).is_err());
        let mut second_pending = pending_state.clone();
        second_pending.active_operation =
            Some(StreamOperation::CohortClose(Box::new(active.clone())));
        assert!(second_pending.validate(canister).is_err());
        pending_state.lifecycle = Lifecycle::Paused;

        let pending_unprepared = {
            let mut state = pending_state.clone();
            state.pending_maturity_prepared = false;
            state
        };
        initialize(pending_unprepared.clone(), canister).unwrap();
        write(pending_unprepared.clone());
        reopen(canister);
        assert_eq!(read(), pending_unprepared);

        let request = PrepareLiquidReceiptArgs {
            receipt_sequence: 0,
            receipt_kind: ReceiptKind::TwoWeekMaturity,
            source_operation_id: vec![8; 64],
            liquid_amount_e8s: 1_000,
            cohort_generation: Some(1),
        };
        let permit = LiquidReceiptPermit {
            sequence: 0,
            destination: config.liquid_icp.clone(),
            memo: crate::receipt::receipt_memo(manager, 0),
        };
        let context = ReceiptContext {
            request: request.clone(),
            request_fingerprint: crate::receipt::request_fingerprint(&request),
            source: config.two_week_receipt_source.clone(),
            permit,
            backing_snapshot: crate::receipt::BackingSnapshot {
                total_io_supply_e8s: 2_000,
                reserve_io_e8s: 1_000,
                excluded_io_balances: Vec::new(),
                liquid_icp_e8s: 1_000,
                io_fee_e8s: 1,
                observed_at_nanos: 1,
            },
        };
        let recipients = settled
            .members
            .iter()
            .map(|member| RewardRecipient {
                sns_neuron_id: member.sns_neuron_id.clone(),
                destination: member.account.clone(),
                before_stake_e8s: 1,
                io_e8s: 1,
                eligibility_checked: false,
                forfeited: false,
                transfer: None,
                refresh_submitted: false,
            })
            .collect();
        let receipt = LiquidReceiptOperation::TwoWeek(Box::new(TwoWeekReceiptOperation {
            context,
            phase: ReceiptPhase::Settling,
            receipt_block: Some(1),
            settlement: Some(TwoWeekSettlement {
                backed_io_pool_e8s: 1_000,
                recipients,
                recipient_index: 0,
                distributed_io_e8s: 0,
                forfeited_io_e8s: 0,
                rounding_dust_io_e8s: 0,
                total_dust_io_e8s: 0,
            }),
        }));
        let mut receipt_state = pending_state;
        receipt_state.active_operation = Some(StreamOperation::LiquidReceipt(Box::new(
            LiquidReceiptStreamOperation::Active(Box::new(receipt)),
        )));
        receipt_state.validate(canister).unwrap();

        let states = [
            active_state,
            capture_state,
            close_state,
            pending_unprepared,
            receipt_state,
        ];
        let encoded = states
            .iter()
            .map(|state| candid::encode_one(StableStreamState::V1(state.clone())).unwrap())
            .collect::<Vec<_>>();
        let largest = encoded.iter().map(Vec::len).max().unwrap();
        for bytes in &encoded {
            let decoded: StableStreamState = candid::decode_one(bytes).unwrap();
            assert!(matches!(decoded, StableStreamState::V1(_)));
        }
        assert!(largest < 2_000_000);
        eprintln!(
            "maximum StableStreamState::V1 encoded bytes={largest} margin={}",
            2_000_000 - largest
        );

        let reopen_state = states.last().unwrap().clone();
        initialize(reopen_state.clone(), canister).unwrap();
        write(reopen_state.clone());
        reopen(canister);
        assert_eq!(read(), reopen_state);
    }
}
