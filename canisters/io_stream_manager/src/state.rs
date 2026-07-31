use candid::{CandidType, Principal};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
use serde::Deserialize;
use std::{borrow::Cow, cell::RefCell};

use crate::{receipt::LiquidReceiptOperation, redemption::RedemptionOperation};
pub use io_accounts::Account;

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StreamConfig {
    pub io_ledger: Principal,
    pub icp_ledger: Principal,
    pub nns_manager: Principal,
    pub nns_receipt_source: Account,
    pub sns_governance: Principal,
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
        if self.io_ledger == self.icp_ledger {
            return Err("IO and ICP ledgers must be distinct".into());
        }
        if self.nns_manager == self.sns_governance {
            return Err("NNS manager and SNS governance must be distinct".into());
        }
        if self.io_reserve.owner != canister_self || self.liquid_icp.owner != canister_self {
            return Err("reserve and liquid accounts must be owned by this canister".into());
        }
        if self.nns_receipt_source.owner != self.nns_manager {
            return Err("receipt source owner must equal NNS manager".into());
        }
        self.io_reserve.validate()?;
        self.liquid_icp.validate()?;
        self.nns_receipt_source.validate()?;
        if self.io_reserve.effective_eq(&self.liquid_icp)? {
            return Err("reserve and liquid accounts must be distinct".into());
        }
        if self.excluded_io_accounts.len() > Self::MAX_EXCLUDED_ACCOUNTS {
            return Err("too many excluded IO accounts".into());
        }
        let mut canonical_excluded = std::collections::BTreeSet::new();
        for account in &self.excluded_io_accounts {
            account.validate()?;
            if account.effective_eq(&self.io_reserve)? {
                return Err("reserve account cannot be excluded".into());
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
    Redemption(Box<RedemptionOperation>),
    LiquidReceipt(Box<LiquidReceiptOperation>),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardCohort {
    pub generation: u64,
    pub captured_at_timestamp_seconds: u64,
    pub members: Vec<RewardMember>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct RewardMember {
    pub sns_neuron_id: Vec<u8>,
    pub account: Account,
    pub frozen_stake_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StreamStateV1 {
    pub config: StreamConfig,
    pub lifecycle: Lifecycle,
    pub active_operation: Option<StreamOperation>,
    pub active_reward_cohort: Option<RewardCohort>,
    pub pending_reward_cohort: Option<RewardCohort>,
    pub next_nns_receipt_sequence: u64,
    pub next_cohort_timestamp_seconds: u64,
    pub next_operation_sequence: OperationSequence,
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
                nns_receipt_source: account.clone(),
                sns_governance: anonymous,
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
            next_nns_receipt_sequence: 0,
            next_cohort_timestamp_seconds: 0,
            next_operation_sequence: OperationSequence(0),
        }
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
    state.config.validate(canister_self)?;
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
    read()
        .config
        .validate(canister_self)
        .unwrap_or_else(|error| panic!("invalid stable stream V1 state: {error}"));
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
    REDEMPTIONS.with(|slot| {
        slot.borrow()
            .as_ref()
            .expect("redemption map is not initialized")
            .get(&caller)
            .unwrap_or_default()
    })
}

pub fn set_caller_state(caller: Principal, state: CallerRedemptionState) {
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

    #[test]
    fn v1_cell_and_caller_nonce_survive_reopen() {
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
                    nns_receipt_source: Account {
                        owner: manager,
                        subaccount: None,
                    },
                    sns_governance: governance,
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
                next_nns_receipt_sequence: 7,
                next_cohort_timestamp_seconds: 8,
                next_operation_sequence: OperationSequence(0),
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
    }
}
