use candid::{CandidType, Principal};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
use serde::Deserialize;
use std::{borrow::Cow, cell::RefCell};

use crate::{
    jupiter::{JupiterCompleted, JupiterOperation},
    maturity::{CompletedMaturity, MaturityCommandOperation, PendingMaturityDisbursement},
    pool::UnwindOperation,
};
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
    pub expected_io_fee_e8s: u128,
    pub expected_icp_fee_e8s: u128,
    pub jupiter_fee_float_e8s: u128,
    pub two_week_fee_float_e8s: u128,
    pub seeded_two_week_principal_e8s: u128,
    pub transfer_retry_delay_nanos: u64,
    pub ledger_deduplication_window_nanos: u64,
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
        if self.jupiter_staging.canonical()?.subaccount != [0; 32] {
            return Err("Jupiter raw-ICP staging must be the NNS manager default Account".into());
        }
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
        let two_fees = self
            .expected_icp_fee_e8s
            .checked_mul(2)
            .ok_or("Jupiter staging fee requirement overflow")?;
        if self.expected_io_fee_e8s == 0
            || self.expected_io_fee_e8s > Self::MAX_STAGING_FEE_FLOAT_E8S
            || self.expected_icp_fee_e8s == 0
            || self.jupiter_fee_float_e8s < two_fees
            || self.two_week_fee_float_e8s < self.expected_icp_fee_e8s
            || self.jupiter_fee_float_e8s > Self::MAX_STAGING_FEE_FLOAT_E8S
            || self.two_week_fee_float_e8s > Self::MAX_STAGING_FEE_FLOAT_E8S
            || self.transfer_retry_delay_nanos == 0
            || self.transfer_retry_delay_nanos >= self.ledger_deduplication_window_nanos
        {
            return Err(
                "explicit staging fee floats do not cover their required ICP effects".into(),
            );
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
    Jupiter(Box<JupiterOperation>),
    Maturity(Box<MaturityCommandOperation>),
    Unwind(UnwindOperation),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TwoWeekTarget {
    pub generation: u64,
    pub target_e8s: u128,
    pub active_parent_principal_e8s: u128,
    pub unwinding_child_principal_e8s: u128,
    pub status: TwoWeekTargetStatus,
}

pub use io_receipt_types::BackingTargetStatus as TwoWeekTargetStatus;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsStateV1 {
    pub config: NnsConfig,
    pub lifecycle: Lifecycle,
    pub active_operation: Option<NnsOperation>,
    pub latest_two_week_target: Option<TwoWeekTarget>,
    pub latest_target_generation: u64,
    pub two_week_maturity_baseline_reconciled: bool,
    pub latest_started_two_week_generation: u64,
    pub latest_completed_two_week_generation: u64,
    pub pending_two_year_maturity: Option<PendingMaturityDisbursement>,
    pub pending_two_week_maturity: Option<PendingMaturityDisbursement>,
    #[serde(default)]
    pub last_two_year_maturity: Option<CompletedMaturity>,
    #[serde(default)]
    pub last_two_week_maturity: Option<CompletedMaturity>,
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
                expected_io_fee_e8s: 0,
                expected_icp_fee_e8s: 0,
                jupiter_fee_float_e8s: 0,
                two_week_fee_float_e8s: 0,
                seeded_two_week_principal_e8s: 0,
                transfer_retry_delay_nanos: 1,
                ledger_deduplication_window_nanos: 2,
            },
            lifecycle: Lifecycle::Paused,
            active_operation: None,
            latest_two_week_target: None,
            latest_target_generation: 0,
            two_week_maturity_baseline_reconciled: false,
            latest_started_two_week_generation: 0,
            latest_completed_two_week_generation: 0,
            pending_two_year_maturity: None,
            pending_two_week_maturity: None,
            last_two_year_maturity: None,
            last_two_week_maturity: None,
            next_operation_sequence: 1,
            control_epoch: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_placeholder() -> Self {
        Self::decode_placeholder()
    }
}

impl NnsStateV1 {
    pub fn validate(&self, canister_self: Principal) -> Result<(), String> {
        self.config.validate(canister_self)?;
        if let Some(target) = &self.latest_two_week_target {
            let tracked_child = match &self.active_operation {
                Some(NnsOperation::Unwind(operation)) => operation.principal_e8s,
                _ => 0,
            };
            if target.generation == 0
                || target.generation != self.latest_target_generation
                || target.unwinding_child_principal_e8s != tracked_child
                || target.status
                    != target_status(
                        target.active_parent_principal_e8s,
                        target.target_e8s,
                        self.config
                            .expected_icp_fee_e8s
                            .checked_mul(2)
                            .ok_or("unwind tolerance overflow")?,
                    )
            {
                return Err("latest two-week target generation is inconsistent".into());
            }
        } else if self.latest_target_generation != 0 {
            return Err("target generation exists without a target".into());
        }
        if self.latest_completed_two_week_generation > self.latest_started_two_week_generation
            || (self.latest_started_two_week_generation > 0
                && !self.two_week_maturity_baseline_reconciled)
            || (self.latest_started_two_week_generation > 0
                && self.latest_two_week_target.is_none())
            || (self.latest_completed_two_week_generation > 0
                && self.last_two_week_maturity.is_none())
        {
            return Err("two-week maturity generation tracking is inconsistent".into());
        }
        if self.latest_started_two_week_generation > self.latest_completed_two_week_generation {
            let active_generation = match &self.active_operation {
                Some(NnsOperation::Maturity(operation))
                    if operation.kind == crate::maturity::MaturityKind::TwoWeek =>
                {
                    operation.plan().entitlement_batch_generation
                }
                _ => None,
            };
            let pending_generation = self
                .pending_two_week_maturity
                .as_ref()
                .and_then(|pending| pending.stake_evidence.plan.entitlement_batch_generation);
            if active_generation != Some(self.latest_started_two_week_generation)
                && pending_generation != Some(self.latest_started_two_week_generation)
            {
                return Err("started two-week generation lacks exact work evidence".into());
            }
        }
        if let Some(operation) = &self.active_operation {
            match operation {
                NnsOperation::Jupiter(operation) => {
                    if operation.operation_sequence >= self.next_operation_sequence {
                        return Err("active Jupiter sequence is inconsistent".into());
                    }
                    operation.validate(self.config.icp_ledger, self.config.nns_governance)?;
                }
                NnsOperation::Maturity(operation) => {
                    let (neuron_id, destination) = match operation.kind {
                        crate::maturity::MaturityKind::TwoYear => (
                            self.config.two_year_neuron_id,
                            &self.config.stream_liquid_account,
                        ),
                        crate::maturity::MaturityKind::TwoWeek => (
                            self.config.two_week_neuron_id,
                            &self.config.two_week_maturity_staging,
                        ),
                    };
                    operation.validate(self.next_operation_sequence, neuron_id, destination)?;
                    if let crate::maturity::MaturityCommandPhase::TwoWeekDelivery(delivery) =
                        &operation.phase
                    {
                        if self.pending_two_week_maturity.as_ref() != Some(&delivery.pending) {
                            return Err(
                                "two-week delivery lost its passive maturity evidence".into()
                            );
                        }
                        let crate::maturity::MintProofState::Delivering(mint) =
                            &delivery.pending.mint_proof
                        else {
                            return Err("two-week delivery lacks an exact Mint".into());
                        };
                        if let Some(permit) = &delivery.permit {
                            if permit.memo.is_empty()
                                || !permit
                                    .destination
                                    .effective_eq(&self.config.stream_liquid_account)?
                            {
                                return Err("two-week stream permit is inconsistent".into());
                            }
                        }
                        if let Some(transfer) = &delivery.transfer {
                            transfer.validate()?;
                            let permit = delivery
                                .permit
                                .as_ref()
                                .ok_or("two-week transfer lacks its stream permit")?;
                            if transfer.intent.ledger != self.config.icp_ledger
                                || transfer.intent.source_subaccount
                                    != self
                                        .config
                                        .two_week_maturity_staging
                                        .canonical()?
                                        .subaccount
                                || !transfer
                                    .intent
                                    .destination
                                    .effective_eq(&permit.destination)?
                                || transfer.intent.amount_e8s != mint.actual_minted_icp_e8s
                                || transfer.intent.fee_e8s != self.config.expected_icp_fee_e8s
                                || transfer.intent.memo != permit.memo
                            {
                                return Err(
                                    "two-week transfer does not match exact Mint receipt".into()
                                );
                            }
                        }
                        if delivery.receipt_completed
                            && !matches!(
                                delivery.transfer.as_ref().map(|value| &value.state),
                                Some(crate::transfer::TransferState::Succeeded { .. })
                            )
                        {
                            return Err("completed two-week receipt lacks transfer proof".into());
                        }
                    }
                }
                NnsOperation::Unwind(operation) => {
                    operation.validate(self.next_operation_sequence)?
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
            pending.validate(kind, neuron_id, destination)?;
        }
        for (completed, kind, neuron_id, destination) in [
            (
                self.last_two_year_maturity.as_ref(),
                crate::maturity::MaturityKind::TwoYear,
                self.config.two_year_neuron_id,
                &self.config.stream_liquid_account,
            ),
            (
                self.last_two_week_maturity.as_ref(),
                crate::maturity::MaturityKind::TwoWeek,
                self.config.two_week_neuron_id,
                &self.config.two_week_maturity_staging,
            ),
        ] {
            let Some(completed) = completed else { continue };
            if completed.kind != kind
                || completed.neuron_id != neuron_id
                || completed.nominal_disbursed_maturity_e8s == 0
                || completed.actual_minted_icp_e8s == 0
                || completed.completed_at_nanos == 0
                || !completed.destination.effective_eq(destination)?
            {
                return Err("completed maturity result is inconsistent".into());
            }
        }
        Ok(())
    }
}

pub fn target_status(actual: u128, target: u128, tolerance: u128) -> TwoWeekTargetStatus {
    match actual.cmp(&target) {
        std::cmp::Ordering::Less => TwoWeekTargetStatus::UnderTarget,
        std::cmp::Ordering::Equal => TwoWeekTargetStatus::AtTarget,
        std::cmp::Ordering::Greater if actual - target <= tolerance => {
            TwoWeekTargetStatus::AtTargetWithinUnwindTolerance
        }
        std::cmp::Ordering::Greater => TwoWeekTargetStatus::OverTarget,
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
    static PROCESSED_JUPITER: RefCell<Option<StableBTreeMap<u64, JupiterCompleted, Memory>>> =
        const { RefCell::new(None) };
}

pub fn initialize(state: NnsStateV1, canister_self: Principal) -> Result<(), String> {
    state.validate(canister_self)?;
    STATE.with(|slot| {
        let memory = MEMORY_MANAGER.with(|manager| manager.borrow().get(MemoryId::new(0)));
        *slot.borrow_mut() = Some(StableCell::init(memory, StableNnsState::V1(state)));
    });
    reopen_processed_jupiter();
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
    reopen_processed_jupiter();
    let mut reopened = read();
    reopened
        .validate(canister_self)
        .unwrap_or_else(|error| panic!("invalid stable NNS V1 state: {error}"));
    reopened.lifecycle = Lifecycle::Paused;
    write(reopened);
}

fn reopen_processed_jupiter() {
    PROCESSED_JUPITER.with(|slot| {
        let memory = MEMORY_MANAGER.with(|manager| manager.borrow().get(MemoryId::new(1)));
        *slot.borrow_mut() = Some(StableBTreeMap::init(memory));
    });
}

pub fn processed_jupiter(block_index: u128) -> Result<Option<JupiterCompleted>, String> {
    let block_index: u64 = block_index
        .try_into()
        .map_err(|_| "Jupiter block index does not fit u64")?;
    Ok(PROCESSED_JUPITER.with(|slot| {
        slot.borrow()
            .as_ref()
            .expect("processed Jupiter set is not initialized")
            .get(&block_index)
    }))
}

pub fn record_processed_jupiter(result: JupiterCompleted) -> Result<(), String> {
    let block_index: u64 = result
        .deposit_block
        .try_into()
        .map_err(|_| "Jupiter block index does not fit u64")?;
    PROCESSED_JUPITER.with(|slot| {
        let previous = slot
            .borrow_mut()
            .as_mut()
            .expect("processed Jupiter set is not initialized")
            .insert(block_index, result.clone());
        if previous
            .as_ref()
            .is_some_and(|previous| previous != &result)
        {
            panic!("processed Jupiter block conflicts with its durable result");
        }
    });
    Ok(())
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
                    jupiter_staging: Account {
                        owner: canister_self,
                        subaccount: None,
                    },
                    two_week_maturity_staging: account(canister_self, 2),
                    stream_liquid_account: account(stream, 3),
                    expected_io_fee_e8s: 10_000,
                    expected_icp_fee_e8s: 10_000,
                    jupiter_fee_float_e8s: 20_000,
                    two_week_fee_float_e8s: 20_000,
                    seeded_two_week_principal_e8s: 1,
                    transfer_retry_delay_nanos: 1_000_000_000,
                    ledger_deduplication_window_nanos: 86_400_000_000_000,
                },
                lifecycle: Lifecycle::Paused,
                active_operation: None,
                latest_two_week_target: None,
                latest_target_generation: 0,
                two_week_maturity_baseline_reconciled: false,
                latest_started_two_week_generation: 0,
                latest_completed_two_week_generation: 0,
                pending_two_year_maturity: None,
                pending_two_week_maturity: None,
                last_two_year_maturity: None,
                last_two_week_maturity: None,
                next_operation_sequence: 1,
                control_epoch: 0,
            },
        )
    }

    #[test]
    fn semantic_validation_rejects_corrupt_active_and_pending_state() {
        let (canister_self, mut state) = valid_state();
        assert_eq!(state.validate(canister_self), Ok(()));
        state.active_operation = Some(NnsOperation::Jupiter(Box::new(
            crate::jupiter::JupiterOperation {
                operation_sequence: 0,
                dispatch_epoch: 0,
                captured_control_epoch: 0,
                deposit: crate::jupiter::JupiterDeposit {
                    block_index: 0,
                    gross_e8s: 100,
                    stake_e8s: 39,
                    liquid_e8s: 61,
                    created_at_time_nanos: 1,
                },
                phase: crate::jupiter::JupiterPhase::DepositProved,
            },
        )));
        assert!(state.validate(canister_self).is_err());
        state.active_operation = None;
        let plan = crate::maturity::MaturityPlan {
            neuron: crate::jupiter::NeuronSnapshot {
                neuron_id: 1,
                staking_subaccount: [1; 32],
                cached_stake_e8s: 1,
            },
            original_maturity_e8s: 200_000_000,
            original_staked_maturity_e8s: 0,
            stake_maturity_e8s: 80_000_000,
            remaining_maturity_e8s: 120_000_000,
            destination: state.config.two_week_maturity_staging.clone(),
            requested_at_seconds: 1,
            entitlement_batch_generation: None,
        };
        let stake = crate::maturity::StakeMaturitySucceeded {
            plan,
            remaining_maturity_e8s: 120_000_000,
            staked_maturity_e8s: 80_000_000,
            evidence_source: crate::maturity::MaturityEvidenceSource::CommandResponse,
        };
        let submission = crate::maturity::DisburseMaturitySubmission {
            stake: stake.clone(),
            submitted_at_seconds: 1,
        };
        state.pending_two_year_maturity = Some(crate::maturity::PendingMaturityDisbursement {
            kind: crate::maturity::MaturityKind::TwoYear,
            neuron_id: 1,
            nominal_disbursed_maturity_e8s: 120_000_000,
            destination: state.config.two_week_maturity_staging.clone(),
            initiation_timestamp_seconds: 1,
            scheduled_finalization_timestamp_seconds: 604_801,
            stake_evidence: stake,
            disburse_evidence: crate::maturity::DisburseMaturitySucceeded {
                submission,
                amount_disbursed_e8s: 120_000_000,
                evidence_source: crate::maturity::MaturityEvidenceSource::CommandResponse,
            },
            mint_proof: crate::maturity::MintProofState::Awaiting,
        });
        assert!(state.validate(canister_self).is_err());
    }

    #[test]
    fn semantic_validation_rejects_orphan_target_generation() {
        let (canister_self, mut state) = valid_state();
        state.latest_target_generation = 1;
        assert!(state.validate(canister_self).is_err());
    }

    #[test]
    fn config_requires_default_jupiter_staging_and_two_fees() {
        let (canister_self, mut state) = valid_state();
        state.config.jupiter_fee_float_e8s = state.config.expected_icp_fee_e8s;
        assert!(state.validate(canister_self).is_err());
        state.config.jupiter_fee_float_e8s = state.config.expected_icp_fee_e8s * 2;
        state.config.jupiter_staging.subaccount = Some(vec![9; 32]);
        assert!(state.validate(canister_self).is_err());
    }

    #[test]
    fn reopen_always_repauses_valid_v1() {
        let (canister_self, mut state) = valid_state();
        state.lifecycle = Lifecycle::Ready;
        state.two_week_maturity_baseline_reconciled = true;
        initialize(state, canister_self).unwrap();
        reopen(canister_self);
        let reopened = read();
        assert_eq!(reopened.lifecycle, Lifecycle::Paused);
        assert!(reopened.two_week_maturity_baseline_reconciled);
    }

    #[test]
    fn processed_jupiter_block_replays_exact_typed_result() {
        let (canister_self, state) = valid_state();
        initialize(state, canister_self).unwrap();
        let result = JupiterCompleted {
            deposit_block: 9_223_372_036_854_000_001,
            gross_e8s: 100,
            stake_e8s: 40,
            liquid_e8s: 60,
            stake_transfer_block: 2,
            liquid_transfer_block: 3,
            stream_receipt_sequence: 4,
            backed_io_e8s: 5,
            io_transfer_block: 6,
            io_fee_e8s: 10_000,
            stream_receipt_fingerprint: vec![7; 32],
            completed_at_nanos: 5,
        };
        record_processed_jupiter(result.clone()).unwrap();
        assert_eq!(
            processed_jupiter(result.deposit_block).unwrap(),
            Some(result)
        );
    }
}
