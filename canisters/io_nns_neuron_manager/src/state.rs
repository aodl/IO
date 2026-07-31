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
    pub two_week_maturity_staging: Account,
    pub stream_liquid_account: Account,
    pub expected_icp_fee_e8s: u128,
    pub jupiter_fee_float_e8s: u128,
    pub two_week_fee_float_e8s: u128,
    pub seeded_two_week_principal_e8s: u128,
}

impl NnsConfig {
    pub const MAX_STAGING_FEE_FLOAT_E8S: u128 = 100_000_000;

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
        if self.jupiter_account.owner != self.jupiter {
            return Err("Jupiter account must be owned by configured Jupiter".into());
        }
        let staging = [&self.jupiter_staging, &self.two_week_maturity_staging];
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
        if staging.iter().try_fold(false, |matched, account| {
            account
                .effective_eq(&self.stream_liquid_account)
                .map(|same| matched || same)
        })? || self
            .jupiter_account
            .effective_eq(&self.stream_liquid_account)?
        {
            return Err("staging, Jupiter and stream liquid accounts must be distinct".into());
        }
        if self.expected_icp_fee_e8s == 0
            || self.jupiter_fee_float_e8s < self.expected_icp_fee_e8s
            || self.two_week_fee_float_e8s < self.expected_icp_fee_e8s
            || self.jupiter_fee_float_e8s > Self::MAX_STAGING_FEE_FLOAT_E8S
            || self.two_week_fee_float_e8s > Self::MAX_STAGING_FEE_FLOAT_E8S
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
        operation_sequence: u64,
        sequence: u64,
        deposit_block: u128,
        gross_e8s: u128,
        stake_e8s: u128,
        liquid_e8s: u128,
        active_transfer: Option<Box<NnsTransferAttempt>>,
    },
    PoolMergeBack {
        operation_sequence: u64,
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
    pub next_operation_sequence: u64,
    pub control_epoch: u64,
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
                two_week_maturity_staging: account.clone(),
                stream_liquid_account: account,
                expected_icp_fee_e8s: 0,
                jupiter_fee_float_e8s: 0,
                two_week_fee_float_e8s: 0,
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
            next_operation_sequence: 1,
            control_epoch: 0,
        }
    }
}

impl NnsStateV1 {
    pub fn validate(&self, canister_self: Principal) -> Result<(), String> {
        self.config.validate(canister_self)?;
        if let Some(target) = &self.latest_two_week_target {
            if target.generation == 0 || target.generation != self.latest_target_generation {
                return Err("latest two-week target generation is inconsistent".into());
            }
        } else if self.latest_target_generation != 0 {
            return Err("target generation exists without a target".into());
        }
        if let Some(operation) = &self.active_operation {
            match operation {
                NnsOperation::JupiterDeposit {
                    operation_sequence,
                    sequence,
                    gross_e8s,
                    stake_e8s,
                    liquid_e8s,
                    active_transfer,
                    ..
                } => {
                    let expected_stake =
                        gross_e8s.checked_mul(40).ok_or("Jupiter split overflow")? / 100;
                    if *operation_sequence >= self.next_operation_sequence
                        || *sequence != self.next_jupiter_sequence
                        || *gross_e8s == 0
                        || *stake_e8s != expected_stake
                        || gross_e8s.checked_sub(*stake_e8s) != Some(*liquid_e8s)
                    {
                        return Err("active Jupiter operation is inconsistent".into());
                    }
                    if let Some(transfer) = active_transfer {
                        transfer.validate()?;
                        if transfer.ledger != self.config.icp_ledger {
                            return Err("Jupiter transfer uses the wrong ledger".into());
                        }
                    }
                }
                NnsOperation::PoolMergeBack {
                    operation_sequence,
                    generation,
                    child_neuron_id,
                } if *operation_sequence < self.next_operation_sequence
                    && *generation > 0
                    && *child_neuron_id > 0
                    && *child_neuron_id != self.config.two_week_neuron_id => {}
                NnsOperation::PoolMergeBack { .. } => {
                    return Err("pool merge-back operation is inconsistent".into())
                }
            }
        }
        for (pending, kind, neuron_id, destination) in [
            (
                self.pending_two_year_maturity.as_ref(),
                crate::maturity::MaturityKind::TwoYear,
                self.config.two_year_neuron_id,
                &self.config.stream_liquid_account,
            ),
            (
                self.pending_two_week_maturity.as_ref(),
                crate::maturity::MaturityKind::TwoWeek,
                self.config.two_week_neuron_id,
                &self.config.two_week_maturity_staging,
            ),
        ] {
            let Some(pending) = pending else { continue };
            if pending.kind != kind
                || pending.neuron_id != neuron_id
                || pending.requested_at_seconds == 0
                || pending.original_maturity_e8s == 0
                || pending.staked_maturity_e8s > pending.original_maturity_e8s
                || pending.amount_disbursed_e8s == Some(0)
                || !pending.destination.effective_eq(destination)?
            {
                return Err("pending maturity contains an invalid identity".into());
            }
            pending.destination.validate()?;
        }
        if let Some(unwind) = &self.pending_unwind {
            if unwind.generation == 0
                || unwind.child_neuron_id == 0
                || unwind.child_neuron_id == self.config.two_week_neuron_id
                || unwind.principal_e8s == 0
            {
                return Err("pending unwind is inconsistent".into());
            }
        }
        Ok(())
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
    state.validate(canister_self)?;
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
    let mut reopened = read();
    reopened
        .validate(canister_self)
        .unwrap_or_else(|error| panic!("invalid stable NNS V1 state: {error}"));
    reopened.lifecycle = Lifecycle::Paused;
    write(reopened);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(value: u8) -> Principal {
        Principal::from_slice(&[value; 29])
    }

    fn valid_state() -> (Principal, NnsStateV1) {
        let canister_self = principal(1);
        let stream = principal(2);
        let jupiter = principal(3);
        let account = |owner, byte| Account {
            owner,
            subaccount: Some(vec![byte; 32]),
        };
        (
            canister_self,
            NnsStateV1 {
                config: NnsConfig {
                    sns_governance: principal(4),
                    stream_manager: stream,
                    jupiter,
                    icp_ledger: principal(5),
                    nns_governance: principal(6),
                    two_year_neuron_id: 1,
                    two_week_neuron_id: 2,
                    jupiter_account: account(jupiter, 4),
                    jupiter_staging: account(canister_self, 1),
                    two_week_maturity_staging: account(canister_self, 2),
                    stream_liquid_account: account(stream, 3),
                    expected_icp_fee_e8s: 10_000,
                    jupiter_fee_float_e8s: 20_000,
                    two_week_fee_float_e8s: 20_000,
                    seeded_two_week_principal_e8s: 1,
                },
                lifecycle: Lifecycle::Paused,
                active_operation: None,
                next_jupiter_sequence: 0,
                latest_two_week_target: None,
                latest_target_generation: 0,
                pending_two_year_maturity: None,
                pending_two_week_maturity: None,
                pending_unwind: None,
                next_operation_sequence: 1,
                control_epoch: 0,
            },
        )
    }

    #[test]
    fn semantic_validation_rejects_corrupt_active_and_pending_state() {
        let (canister_self, mut state) = valid_state();
        assert_eq!(state.validate(canister_self), Ok(()));
        state.active_operation = Some(NnsOperation::JupiterDeposit {
            operation_sequence: 0,
            sequence: 0,
            deposit_block: 0,
            gross_e8s: 100,
            stake_e8s: 39,
            liquid_e8s: 61,
            active_transfer: None,
        });
        assert!(state.validate(canister_self).is_err());
        state.active_operation = None;
        state.pending_two_year_maturity = Some(crate::maturity::PendingMaturity {
            kind: crate::maturity::MaturityKind::TwoYear,
            neuron_id: 1,
            original_maturity_e8s: 100,
            staked_maturity_e8s: 40,
            remaining_maturity_e8s: 60,
            amount_disbursed_e8s: None,
            destination: state.config.two_week_maturity_staging.clone(),
            requested_at_seconds: 1,
        });
        assert!(state.validate(canister_self).is_err());
    }

    #[test]
    fn semantic_validation_rejects_orphan_target_generation() {
        let (canister_self, mut state) = valid_state();
        state.latest_target_generation = 1;
        assert!(state.validate(canister_self).is_err());
    }
}
