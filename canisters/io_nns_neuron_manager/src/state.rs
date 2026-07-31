use candid::{CandidType, Principal};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableCell, Storable,
};
use serde::Deserialize;
use std::{borrow::Cow, cell::RefCell};

use crate::{maturity::PendingMaturity, pool::PendingUnwind, transfer::NnsTransferAttempt};
pub use io_accounts::Account;

type Memory = VirtualMemory<DefaultMemoryImpl>;

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
    pub jupiter_staging: Account,
    pub two_year_maturity_staging: Account,
    pub two_week_maturity_staging: Account,
    pub unwind_staging: Account,
    pub operational_fee_account: Account,
    pub stream_liquid_account: Account,
    pub expected_icp_fee_e8s: u128,
    pub minimum_staging_fee_float_e8s: u128,
    pub seeded_two_week_principal_e8s: u128,
}

impl NnsConfig {
    pub fn validate(&self, canister_self: Principal) -> Result<(), String> {
        let management = Principal::management_canister();
        for (name, principal) in [
            ("canister self", canister_self),
            ("SNS governance", self.sns_governance),
            ("stream manager", self.stream_manager),
            ("Jupiter", self.jupiter),
            ("ICP ledger", self.icp_ledger),
            ("NNS governance", self.nns_governance),
        ] {
            if principal == Principal::anonymous() || principal == management {
                return Err(format!("{name} principal is forbidden"));
            }
        }
        if self.two_year_neuron_id == 0
            || self.two_week_neuron_id == 0
            || self.two_year_neuron_id == self.two_week_neuron_id
        {
            return Err("protected neuron ids must be distinct and non-zero".into());
        }
        if self.stream_liquid_account.owner != self.stream_manager {
            return Err("stream liquid account must be owned by stream manager".into());
        }
        let staging = [
            &self.jupiter_staging,
            &self.two_year_maturity_staging,
            &self.two_week_maturity_staging,
            &self.unwind_staging,
            &self.operational_fee_account,
        ];
        let mut canonical = std::collections::BTreeSet::new();
        for account in staging {
            account.validate()?;
            if account.owner != canister_self {
                return Err("every staging/fee account must be owned by this canister".into());
            }
            if !canonical.insert(account.canonical()?) {
                return Err("NNS staging and fee accounts must be distinct".into());
            }
        }
        self.jupiter_account.validate()?;
        self.stream_liquid_account.validate()?;
        if self.expected_icp_fee_e8s == 0
            || self.minimum_staging_fee_float_e8s < self.expected_icp_fee_e8s
        {
            return Err("explicit staging fee float must cover at least one ICP fee".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum Lifecycle {
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
        active_transfer: Option<Box<NnsTransferAttempt>>,
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

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum StableNnsState {
    V1(NnsStateV1),
}

impl NnsStateV1 {
    fn decode_placeholder() -> Self {
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
                jupiter_staging: account.clone(),
                two_year_maturity_staging: account.clone(),
                two_week_maturity_staging: account.clone(),
                unwind_staging: account.clone(),
                operational_fee_account: account.clone(),
                stream_liquid_account: account,
                expected_icp_fee_e8s: 0,
                minimum_staging_fee_float_e8s: 0,
                seeded_two_week_principal_e8s: 0,
            },
            lifecycle: Lifecycle::Paused,
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

impl Storable for StableNnsState {
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
    static STATE: RefCell<Option<StableCell<StableNnsState, Memory>>> =
        const { RefCell::new(None) };
}

pub fn initialize(state: NnsStateV1, canister_self: Principal) -> Result<(), String> {
    state.config.validate(canister_self)?;
    STATE.with(|slot| {
        let memory = MEMORY_MANAGER.with(|manager| manager.borrow().get(MemoryId::new(0)));
        *slot.borrow_mut() = Some(StableCell::init(memory, StableNnsState::V1(state)));
    });
    Ok(())
}

pub fn reopen(canister_self: Principal) {
    STATE.with(|slot| {
        let memory = MEMORY_MANAGER.with(|manager| manager.borrow().get(MemoryId::new(0)));
        *slot.borrow_mut() = Some(StableCell::init(
            memory,
            StableNnsState::V1(NnsStateV1::decode_placeholder()),
        ));
    });
    read()
        .config
        .validate(canister_self)
        .unwrap_or_else(|error| panic!("invalid stable NNS V1 state: {error}"));
}

pub fn read() -> NnsStateV1 {
    STATE.with(|slot| {
        slot.borrow()
            .as_ref()
            .expect("NNS state is not initialized")
            .get()
            .clone()
            .into_v1()
    })
}

pub fn write(state: NnsStateV1) {
    STATE.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .expect("NNS state is not initialized")
            .set(StableNnsState::V1(state));
    });
}

impl StableNnsState {
    fn into_v1(self) -> NnsStateV1 {
        match self {
            Self::V1(state) => state,
        }
    }
}
