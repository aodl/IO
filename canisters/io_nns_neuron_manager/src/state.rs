use candid::{CandidType, Principal};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableCell, Storable,
};
use serde::Deserialize;
use std::{borrow::Cow, cell::RefCell};

use crate::{maturity::PendingMaturity, pool::PendingUnwind, transfer::NnsTransferAttempt};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct Account {
    pub owner: Principal,
    pub subaccount: Option<Vec<u8>>,
}

impl Account {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .subaccount
            .as_ref()
            .is_some_and(|value| value.len() != 32)
        {
            return Err("subaccount must contain exactly 32 bytes".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsConfig {
    pub sns_governance: Principal,
    pub stream_manager: Principal,
    pub jupiter: Principal,
    pub icp_ledger: Principal,
    pub nns_governance: Principal,
    pub two_year_neuron_id: u64,
    pub two_week_neuron_id: u64,
    pub jupiter_account: Account,
    pub staging_account: Account,
    pub operational_fee_account: Account,
    pub stream_liquid_account: Account,
    pub expected_icp_fee_e8s: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum Lifecycle {
    Inert,
    Paused,
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsOperation {
    JupiterDeposit {
        sequence: u64,
        deposit_block: u128,
        gross_e8s: u128,
        stake_e8s: u128,
        liquid_e8s: u128,
        active_transfer: Option<NnsTransferAttempt>,
    },
    PoolTopUp {
        generation: u64,
        amount_e8s: u128,
    },
    PoolMergeBack {
        generation: u64,
        child_neuron_id: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TwoWeekTarget {
    pub generation: u64,
    pub target_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsStateV1 {
    pub config: NnsConfig,
    pub lifecycle: Lifecycle,
    pub active_operation: Option<NnsOperation>,
    pub next_jupiter_sequence: u64,
    pub latest_two_week_target: Option<TwoWeekTarget>,
    pub latest_target_generation: u64,
    pub pending_two_year_maturity: Option<PendingMaturity>,
    pub pending_two_week_maturity: Option<PendingMaturity>,
    pub pending_unwind: Option<PendingUnwind>,
}

impl NnsStateV1 {
    fn inert_placeholder() -> Self {
        let principal = Principal::anonymous();
        let account = Account {
            owner: principal,
            subaccount: None,
        };
        Self {
            config: NnsConfig {
                sns_governance: principal,
                stream_manager: principal,
                jupiter: principal,
                icp_ledger: principal,
                nns_governance: principal,
                two_year_neuron_id: 0,
                two_week_neuron_id: 0,
                jupiter_account: account.clone(),
                staging_account: account.clone(),
                operational_fee_account: account.clone(),
                stream_liquid_account: account,
                expected_icp_fee_e8s: 0,
            },
            lifecycle: Lifecycle::Inert,
            active_operation: None,
            next_jupiter_sequence: 0,
            latest_two_week_target: None,
            latest_target_generation: 0,
            pending_two_year_maturity: None,
            pending_two_week_maturity: None,
            pending_unwind: None,
        }
    }
}

impl Storable for NnsStateV1 {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(candid::encode_one(self).expect("NNS V1 state must encode"))
    }
    fn into_bytes(self) -> Vec<u8> {
        candid::encode_one(self).expect("NNS V1 state must encode")
    }
    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        candid::decode_one(bytes.as_ref()).expect("NNS V1 state must decode")
    }
    const BOUND: Bound = Bound::Bounded {
        max_size: 1_000_000,
        is_fixed_size: false,
    };
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static STATE: RefCell<Option<StableCell<NnsStateV1, Memory>>> =
        const { RefCell::new(None) };
}

pub fn initialize(state: NnsStateV1) -> Result<(), String> {
    state.config.jupiter_account.validate()?;
    state.config.staging_account.validate()?;
    state.config.operational_fee_account.validate()?;
    state.config.stream_liquid_account.validate()?;
    if state.config.two_year_neuron_id == 0 || state.config.two_week_neuron_id == 0 {
        return Err("protected neuron ids must be non-zero".into());
    }
    STATE.with(|slot| {
        let memory = MEMORY_MANAGER.with(|manager| manager.borrow().get(MemoryId::new(0)));
        *slot.borrow_mut() = Some(StableCell::init(memory, state));
    });
    Ok(())
}

pub fn reopen() {
    STATE.with(|slot| {
        let memory = MEMORY_MANAGER.with(|manager| manager.borrow().get(MemoryId::new(0)));
        *slot.borrow_mut() = Some(StableCell::init(memory, NnsStateV1::inert_placeholder()));
    });
}

pub fn read() -> NnsStateV1 {
    STATE.with(|slot| {
        slot.borrow()
            .as_ref()
            .expect("NNS state is not initialized")
            .get()
            .clone()
    })
}

pub fn write(state: NnsStateV1) {
    STATE.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .expect("NNS state is not initialized")
            .set(state);
    });
}
