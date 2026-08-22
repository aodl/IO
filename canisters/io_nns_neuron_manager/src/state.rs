use crate::{
    jupiter::{JupiterCompleted, JupiterOperation},
    maturity::{CompletedMaturity, MaturityCommandOperation, PendingMaturityDisbursement},
    pool::{PassiveCohort, UnwindOperation},
};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
pub use io_accounts::Account;
use io_nns_types::backing::CompletedPoolCommand;
use {
    candid::{CandidType, Principal},
    serde::Deserialize,
    std::{borrow::Cow, cell::RefCell},
};

type Memory = VirtualMemory<DefaultMemoryImpl>;

pub(crate) const LAUNCH_SCHEMA_MARKER: u8 = 3;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsConfig {
    pub sns_governance: Principal,
    pub stream_manager: Principal,
    pub jupiter: Principal,
    pub icp_ledger: Principal,
    pub nns_governance: Principal,
    pub two_year_neuron_id: u64,
    pub pooled_parent_memo: u64,
    pub pooled_parent_followee_id: u64,
    pub minimum_parent_stake_e8s: u128,
    pub jupiter_account: Account,
    pub jupiter_staging: Account,
    pub maturity_staging: Account,
    pub stream_liquid_account: Account,
    pub expected_io_fee_e8s: u128,
    pub expected_icp_fee_e8s: u128,
    pub jupiter_activation_block_floor: u128,
    pub audited_permanent_principal_e8s: u128,
    pub transfer_retry_delay_nanos: u64,
    pub ledger_deduplication_window_nanos: u64,
}

impl NnsConfig {
    pub const MAX_LEDGER_FEE_E8S: u128 = 100_000_000;

    pub fn validate(&self, canister_self: Principal) -> Result<(), String> {
        let management = Principal::management_canister();
        let principals = [
            ("canister self", canister_self),
            ("SNS governance", self.sns_governance),
            ("stream manager", self.stream_manager),
            ("Jupiter", self.jupiter),
            ("ICP ledger", self.icp_ledger),
            ("NNS governance", self.nns_governance),
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
        if self.two_year_neuron_id == 0
            || self.pooled_parent_memo == 0
            || self.pooled_parent_followee_id == 0
            || self.minimum_parent_stake_e8s <= self.expected_icp_fee_e8s
        {
            return Err("protected neuron and pooled-parent policy must be non-zero".into());
        }
        if self.stream_liquid_account.owner != self.stream_manager {
            return Err("stream liquid account must be owned by stream manager".into());
        }
        if self.jupiter_account.owner != self.jupiter {
            return Err("Jupiter account must be owned by configured Jupiter".into());
        }
        let staging = [&self.jupiter_staging, &self.maturity_staging];
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
        self.stream_liquid_account.validate()?;
        if self.jupiter_account.canonical()?.subaccount != [0; 32]
            || self.jupiter_staging.canonical()?.subaccount != [0; 32]
        {
            return Err("Jupiter source and staging must be default Accounts".into());
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
        if self.expected_io_fee_e8s == 0
            || self.expected_io_fee_e8s > Self::MAX_LEDGER_FEE_E8S
            || self.expected_icp_fee_e8s == 0
            || self.expected_icp_fee_e8s > Self::MAX_LEDGER_FEE_E8S
            || self.jupiter_activation_block_floor == 0
            || self.audited_permanent_principal_e8s == 0
            || self.transfer_retry_delay_nanos == 0
            || self.transfer_retry_delay_nanos >= self.ledger_deduplication_window_nanos
        {
            return Err("launch fees, floors, principals or retry windows are invalid".into());
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
    Pool(io_nns_types::backing::PoolCommand),
    Unwind(UnwindOperation),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PooledTarget {
    pub target_e8s: u128,
    pub status: PooledTargetStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum PooledTargetStatus {
    UnderTarget,
    AtTarget,
    AtTargetWithinUnwindTolerance,
    OverTarget,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct HeldReconciliation {
    pub generation: u64,
    pub target_e8s: u128,
    pub principal_e8s: u128,
    pub snapshot_fingerprint: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsStateV1 {
    pub launch_schema_marker: u8,
    pub config: NnsConfig,
    pub lifecycle: Lifecycle,
    pub active_operation: Option<NnsOperation>,
    pub pooled_parent_id: Option<u64>,
    pub pooled_parent_staking_account: Option<Account>,
    pub live_cohorts: Vec<PassiveCohort>,
    pub last_completed_pool: Option<CompletedPoolCommand>,
    pub last_held_reconciliation: Option<HeldReconciliation>,
    pub latest_reconciliation_generation: u64,
    pub latest_pooled_target: Option<PooledTarget>,
    pub two_year_maturity_baseline_reconciled: bool,
    pub latest_started_two_week_generation: u64,
    pub latest_completed_two_week_generation: u64,
    pub pending_two_year_maturity: Option<PendingMaturityDisbursement>,
    pub pending_two_week_maturity: Option<PendingMaturityDisbursement>,
    pub last_two_year_maturity: Option<CompletedMaturity>,
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
            launch_schema_marker: LAUNCH_SCHEMA_MARKER,
            config: NnsConfig {
                sns_governance: principal,
                stream_manager: principal,
                jupiter: principal,
                icp_ledger: principal,
                nns_governance: principal,
                two_year_neuron_id: 0,
                pooled_parent_memo: 0,
                pooled_parent_followee_id: 0,
                minimum_parent_stake_e8s: 0,
                jupiter_account: account.clone(),
                jupiter_staging: account.clone(),
                maturity_staging: account.clone(),
                stream_liquid_account: account,
                expected_io_fee_e8s: 0,
                expected_icp_fee_e8s: 0,
                jupiter_activation_block_floor: 0,
                audited_permanent_principal_e8s: 0,
                transfer_retry_delay_nanos: 1,
                ledger_deduplication_window_nanos: 2,
            },
            lifecycle: Lifecycle::Paused,
            active_operation: None,
            pooled_parent_id: None,
            pooled_parent_staking_account: None,
            live_cohorts: Vec::new(),
            last_completed_pool: None,
            last_held_reconciliation: None,
            latest_reconciliation_generation: 0,
            latest_pooled_target: None,
            two_year_maturity_baseline_reconciled: false,
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
}

fn validate_backing_inflow_delivery(
    kind: crate::maturity::MaturityKind,
    delivery: &crate::maturity::BackingInflowDeliveryOperation,
    config: &NnsConfig,
) -> Result<(), String> {
    use io_nns_types::{
        inflow::{effect_memo, BackingEffect, FrozenInflowEconomics},
        transfer::{NnsTransferAttempt, TransferState},
    };
    use sha2::Digest;

    let Some(permit) = &delivery.permit else {
        if delivery.permanent_transfer.is_some()
            || delivery.claim_transfer.is_some()
            || delivery.stream_pooled_block.is_some()
            || delivery.parent_credit_phase != crate::maturity::ParentCreditPhase::NotRequired
        {
            return Err("unpermitted backing inflow contains effect evidence".into());
        }
        return Ok(());
    };
    permit.validate()?;
    let crate::maturity::MintProofState::Delivering(mint) = &delivery.pending.mint_proof else {
        return Err("backing inflow permit lacks a delivering Mint".into());
    };
    let economics_match = matches!(
        (kind, &permit.economics),
        (
            crate::maturity::MaturityKind::TwoYear,
            FrozenInflowEconomics::Permanent { .. }
        ) | (
            crate::maturity::MaturityKind::TwoWeek,
            FrozenInflowEconomics::Pooled { .. }
        )
    );
    let generation = delivery
        .pending
        .stake_evidence
        .plan
        .entitlement_batch_generation
        .unwrap_or(delivery.pending.initiation_timestamp_seconds);
    let source_operation_id = sha2::Sha256::digest(
        candid::encode_one(&delivery.pending)
            .map_err(|error| format!("maturity source evidence encode failed: {error}"))?,
    )
    .to_vec();
    if !economics_match
        || permit.actual_mint_e8s != mint.actual_minted_icp_e8s
        || permit.mint_block != mint.mint_block
        || permit.maturity_generation != generation
        || permit.source_operation_id != source_operation_id
        || !permit
            .staging_account
            .effective_eq(&config.maturity_staging)?
        || permit.permanent_transfer_fee_e8s != config.expected_icp_fee_e8s
        || permit.claim_transfer_fee_e8s != config.expected_icp_fee_e8s
    {
        return Err("backing-inflow permit differs from exact maturity evidence".into());
    }

    let validate_transfer = |attempt: &NnsTransferAttempt,
                             effect: BackingEffect,
                             destination: &Account,
                             amount: u128,
                             fee: u128|
     -> Result<(), String> {
        attempt.validate()?;
        if attempt.intent.ledger != config.icp_ledger
            || attempt.intent.source_subaccount != config.maturity_staging.canonical()?.subaccount
            || !attempt.intent.destination.effective_eq(destination)?
            || attempt.intent.amount_e8s != amount
            || attempt.intent.fee_e8s != fee
            || attempt.intent.memo != effect_memo(&permit.source_operation_id, effect)
        {
            return Err("backing-inflow transfer differs from its permit".into());
        }
        Ok(())
    };
    if let Some(transfer) = &delivery.permanent_transfer {
        if permit.permanent_credit() == 0 {
            return Err("zero permanent route contains a transfer".into());
        }
        validate_transfer(
            transfer,
            BackingEffect::PermanentCredit,
            &permit.permanent_destination,
            permit.permanent_credit(),
            permit.permanent_transfer_fee_e8s,
        )?;
    }
    if let Some(transfer) = &delivery.claim_transfer {
        let route = permit.route();
        validate_transfer(
            transfer,
            BackingEffect::FirstClaimCredit,
            if route.route == io_reward_policy::ClaimRoute::AllPool {
                &permit.pool_destination
            } else {
                &permit.liquid_destination
            },
            permit
                .first_claim_credit()
                .ok_or("claim transfer credit overflow")?,
            permit.claim_transfer_fee_e8s,
        )?;
    }
    let route = permit.route().route;
    let claim_succeeded = delivery
        .claim_transfer
        .as_ref()
        .is_some_and(|attempt| matches!(attempt.state, TransferState::Succeeded { .. }));
    let permanent_succeeded = delivery
        .permanent_transfer
        .as_ref()
        .is_some_and(|attempt| matches!(attempt.state, TransferState::Succeeded { .. }));
    if permit.permanent_credit() > 0 && delivery.claim_transfer.is_some() && !permanent_succeeded
        || delivery.stream_pooled_block.is_some()
            && (route != io_reward_policy::ClaimRoute::Mixed || !claim_succeeded)
        || route == io_reward_policy::ClaimRoute::AllLiquid
            && delivery.parent_credit_phase != crate::maturity::ParentCreditPhase::NotRequired
        || delivery.parent_credit_phase != crate::maturity::ParentCreditPhase::NotRequired
            && (!claim_succeeded
                || route == io_reward_policy::ClaimRoute::Mixed
                    && delivery.stream_pooled_block.is_none())
    {
        return Err("backing-inflow pooled effect contradicts its route".into());
    }
    Ok(())
}

impl NnsStateV1 {
    pub fn validate(&self, canister_self: Principal) -> Result<(), String> {
        if self.launch_schema_marker != LAUNCH_SCHEMA_MARKER {
            return Err("invalid NNS launch schema marker".into());
        }
        self.config.validate(canister_self)?;
        let pooled_parent_id = self.pooled_parent_id.unwrap_or_default();
        if self.pooled_parent_id.is_some() != self.pooled_parent_staking_account.is_some()
            || self
                .pooled_parent_staking_account
                .as_ref()
                .is_some_and(|account| account.owner != self.config.nns_governance)
        {
            return Err("pooled parent identity evidence is inconsistent".into());
        }
        if (self.pending_two_year_maturity.is_some()
            || self.last_two_year_maturity.is_some()
            || matches!(
                self.active_operation,
                Some(NnsOperation::Maturity(ref operation))
                    if operation.kind == crate::maturity::MaturityKind::TwoYear
            ))
            && !self.two_year_maturity_baseline_reconciled
        {
            return Err(
                "two-year maturity work exists before launch baseline reconciliation".into(),
            );
        }
        crate::pool::validate_cohorts(&self.live_cohorts)?;
        if let Some(completed) = &self.last_completed_pool {
            completed.validate(self.next_operation_sequence)?;
        }
        if let Some(held) = &self.last_held_reconciliation {
            if held.generation == 0
                || held.generation != self.latest_reconciliation_generation
                || held.snapshot_fingerprint.len() != 32
            {
                return Err("held reconciliation checkpoint is inconsistent".into());
            }
        }
        if self.latest_completed_two_week_generation > self.latest_started_two_week_generation
            || (self.latest_started_two_week_generation > 0 && self.latest_pooled_target.is_none())
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
                            &self.config.maturity_staging,
                        ),
                        crate::maturity::MaturityKind::TwoWeek => {
                            (pooled_parent_id, &self.config.maturity_staging)
                        }
                    };
                    operation.validate(self.next_operation_sequence, neuron_id, destination)?;
                    if let crate::maturity::MaturityCommandPhase::BackingInflowDelivery(delivery) =
                        &operation.phase
                    {
                        let pending = match operation.kind {
                            crate::maturity::MaturityKind::TwoYear => {
                                self.pending_two_year_maturity.as_ref()
                            }
                            crate::maturity::MaturityKind::TwoWeek => {
                                self.pending_two_week_maturity.as_ref()
                            }
                        };
                        if pending != Some(&delivery.pending) {
                            return Err("backing inflow lost its passive maturity evidence".into());
                        }
                        let crate::maturity::MintProofState::Delivering(_) =
                            &delivery.pending.mint_proof
                        else {
                            return Err("backing inflow lacks an exact Mint".into());
                        };
                        validate_backing_inflow_delivery(operation.kind, delivery, &self.config)?;
                    }
                }
                NnsOperation::Pool(operation) => {
                    operation.validate(self.next_operation_sequence)?;
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
                &self.config.maturity_staging,
            ),
            (
                self.pending_two_week_maturity.as_ref(),
                crate::maturity::MaturityKind::TwoWeek,
                pooled_parent_id,
                &self.config.maturity_staging,
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
                &self.config.maturity_staging,
            ),
            (
                self.last_two_week_maturity.as_ref(),
                crate::maturity::MaturityKind::TwoWeek,
                pooled_parent_id,
                &self.config.maturity_staging,
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

pub fn target_status(actual: u128, target: u128, tolerance: u128) -> PooledTargetStatus {
    match actual.cmp(&target) {
        std::cmp::Ordering::Less => PooledTargetStatus::UnderTarget,
        std::cmp::Ordering::Equal => PooledTargetStatus::AtTarget,
        std::cmp::Ordering::Greater if actual - target <= tolerance => {
            PooledTargetStatus::AtTargetWithinUnwindTolerance
        }
        std::cmp::Ordering::Greater => PooledTargetStatus::OverTarget,
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

    #[derive(candid::CandidType)]
    enum FutureStableNnsState {
        V2(NnsStateV1),
    }

    #[derive(candid::CandidType)]
    struct CheckpointJupiterLookupLease {
        block_index: u128,
        started_at_nanos: u64,
    }

    #[derive(candid::CandidType)]
    struct CheckpointNnsStateV1 {
        config: NnsConfig,
        lifecycle: Lifecycle,
        active_operation: Option<NnsOperation>,
        latest_pooled_target: Option<PooledTarget>,
        two_year_maturity_baseline_reconciled: bool,
        two_week_maturity_baseline_reconciled: bool,
        latest_started_two_week_generation: u64,
        latest_completed_two_week_generation: u64,
        pending_two_year_maturity: Option<PendingMaturityDisbursement>,
        pending_two_week_maturity: Option<PendingMaturityDisbursement>,
        pending_unwind: Option<UnwindOperation>,
        last_two_year_maturity: Option<CompletedMaturity>,
        last_two_week_maturity: Option<CompletedMaturity>,
        next_operation_sequence: u64,
        control_epoch: u64,
        last_passive_reconciliation_attempt_nanos: Option<u64>,
        last_public_jupiter_lookup_attempt_nanos: Option<u64>,
        jupiter_lookup_lease: Option<CheckpointJupiterLookupLease>,
    }

    #[derive(candid::CandidType)]
    enum CheckpointStableNnsState {
        V1(CheckpointNnsStateV1),
    }

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
                launch_schema_marker: LAUNCH_SCHEMA_MARKER,
                config: NnsConfig {
                    sns_governance: principal(4),
                    stream_manager: stream,
                    jupiter,
                    icp_ledger: principal(5),
                    nns_governance: principal(6),
                    two_year_neuron_id: 1,
                    pooled_parent_memo: 2,
                    pooled_parent_followee_id: 3,
                    minimum_parent_stake_e8s: 100_000_000,
                    jupiter_account: Account {
                        owner: jupiter,
                        subaccount: None,
                    },
                    jupiter_staging: Account {
                        owner: canister_self,
                        subaccount: None,
                    },
                    maturity_staging: account(canister_self, 2),
                    stream_liquid_account: account(stream, 3),
                    expected_io_fee_e8s: 10_000,
                    expected_icp_fee_e8s: 10_000,
                    jupiter_activation_block_floor: 1,
                    audited_permanent_principal_e8s: 1,
                    transfer_retry_delay_nanos: 1_000_000_000,
                    ledger_deduplication_window_nanos: 86_400_000_000_000,
                },
                lifecycle: Lifecycle::Paused,
                active_operation: None,
                pooled_parent_id: None,
                pooled_parent_staking_account: None,
                live_cohorts: Vec::new(),
                last_completed_pool: None,
                last_held_reconciliation: None,
                latest_reconciliation_generation: 0,
                latest_pooled_target: None,
                two_year_maturity_baseline_reconciled: false,
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

    fn passive_unwind() -> PassiveCohort {
        PassiveCohort {
            generation: 1,
            child_neuron_id: 3,
            principal_e8s: 1,
            child_staking_subaccount: vec![3; 32],
            ready_at_seconds: 4,
            proof: io_nns_types::backing::CohortProofState::Dissolving,
            disbursement_block: None,
        }
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
                    fee_e8s: 0,
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
            destination: state.config.maturity_staging.clone(),
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
            destination: state.config.maturity_staging.clone(),
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
    fn config_requires_default_jupiter_accounts() {
        let (canister_self, mut state) = valid_state();
        state.config.jupiter_account.subaccount = Some(vec![9; 32]);
        assert!(state.validate(canister_self).is_err());
        state.config.jupiter_account.subaccount = None;
        state.config.jupiter_staging.subaccount = Some(vec![9; 32]);
        assert!(state.validate(canister_self).is_err());
    }

    #[test]
    fn every_control_principal_pair_must_be_distinct() {
        let roles = ["self", "sns", "stream", "jupiter", "icp", "nns"];
        for left in 0..roles.len() {
            for right in left + 1..roles.len() {
                let (canister_self, mut state) = valid_state();
                let value = match roles[left] {
                    "self" => canister_self,
                    "sns" => state.config.sns_governance,
                    "stream" => state.config.stream_manager,
                    "jupiter" => state.config.jupiter,
                    "icp" => state.config.icp_ledger,
                    "nns" => state.config.nns_governance,
                    _ => unreachable!(),
                };
                match roles[right] {
                    "sns" => state.config.sns_governance = value,
                    "stream" => state.config.stream_manager = value,
                    "jupiter" => state.config.jupiter = value,
                    "icp" => state.config.icp_ledger = value,
                    "nns" => state.config.nns_governance = value,
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
    fn launch_boundaries_and_account_roles_are_strict() {
        let (canister_self, mut state) = valid_state();
        state.config.jupiter_activation_block_floor = 0;
        assert!(state.validate(canister_self).is_err());

        let (canister_self, mut state) = valid_state();
        state.config.audited_permanent_principal_e8s = 0;
        assert!(state.validate(canister_self).is_err());

        let (canister_self, mut state) = valid_state();
        state.config.maturity_staging = state.config.jupiter_staging.clone();
        assert!(state.validate(canister_self).is_err());
    }

    #[test]
    fn reopen_always_repauses_valid_v1() {
        let (canister_self, mut state) = valid_state();
        state.lifecycle = Lifecycle::Ready;
        state.two_year_maturity_baseline_reconciled = true;
        initialize(state, canister_self).unwrap();
        reopen(canister_self);
        let reopened = read();
        assert_eq!(reopened.lifecycle, Lifecycle::Paused);
        assert_eq!(reopened.config.jupiter_activation_block_floor, 1);
        assert!(reopened.two_year_maturity_baseline_reconciled);

        let mut unreconciled = reopened;
        unreconciled.two_year_maturity_baseline_reconciled = false;
        write(unreconciled);
        reopen(canister_self);
        assert!(!read().two_year_maturity_baseline_reconciled);
    }

    #[test]
    fn passive_unwind_survives_upgrade_and_cannot_duplicate_active_child() {
        let (canister_self, mut state) = valid_state();
        state.latest_pooled_target = Some(PooledTarget {
            target_e8s: 1,
            status: PooledTargetStatus::AtTarget,
        });
        state.live_cohorts = vec![passive_unwind()];
        initialize(state, canister_self).unwrap();
        reopen(canister_self);
        let reopened = read();
        assert_eq!(reopened.live_cohorts, vec![passive_unwind()]);
        let mut duplicate = reopened;
        duplicate.live_cohorts.push(passive_unwind());
        assert!(duplicate.validate(canister_self).is_err());
    }

    #[test]
    fn maximum_fixed_nns_v1_slots_fit_the_stable_cell_bound() {
        use crate::maturity::{
            BackingInflowDeliveryOperation, CompletedMaturity, DisburseMaturitySubmission,
            DisburseMaturitySucceeded, MaturityCommandOperation, MaturityCommandPhase,
            MaturityEvidenceSource, MaturityKind, MaturityPlan, MintEvidence, MintProofState,
            ParentCreditPhase, PendingMaturityDisbursement, StakeMaturitySucceeded,
        };

        let (canister_self, mut state) = valid_state();
        let pending = |kind: MaturityKind, neuron_id: u64, destination: Account, generation| {
            let stake_maturity_e8s = if kind == MaturityKind::TwoYear {
                400_000_000
            } else {
                0
            };
            let remaining_maturity_e8s = 1_000_000_000 - stake_maturity_e8s;
            let plan = MaturityPlan {
                neuron: crate::jupiter::NeuronSnapshot {
                    neuron_id,
                    staking_subaccount: [7; 32],
                    cached_stake_e8s: u128::MAX,
                },
                original_maturity_e8s: 1_000_000_000,
                original_staked_maturity_e8s: u64::MAX,
                stake_maturity_e8s,
                remaining_maturity_e8s,
                destination: destination.clone(),
                requested_at_seconds: 1,
                entitlement_batch_generation: generation,
            };
            let stake = StakeMaturitySucceeded {
                plan,
                remaining_maturity_e8s,
                staked_maturity_e8s: u64::MAX,
                evidence_source: MaturityEvidenceSource::CanonicalNeuronObservation,
            };
            PendingMaturityDisbursement {
                kind,
                neuron_id,
                nominal_disbursed_maturity_e8s: remaining_maturity_e8s,
                destination,
                initiation_timestamp_seconds: 1,
                scheduled_finalization_timestamp_seconds: 604_801,
                stake_evidence: stake.clone(),
                disburse_evidence: DisburseMaturitySucceeded {
                    submission: DisburseMaturitySubmission {
                        stake,
                        submitted_at_seconds: 1,
                    },
                    amount_disbursed_e8s: remaining_maturity_e8s,
                    evidence_source: MaturityEvidenceSource::CanonicalNeuronObservation,
                },
                mint_proof: MintProofState::Awaiting,
            }
        };
        let mut two_year = pending(
            MaturityKind::TwoYear,
            state.config.two_year_neuron_id,
            state.config.maturity_staging.clone(),
            None,
        );
        two_year.mint_proof = MintProofState::Proved(MintEvidence {
            mint_block: 7,
            actual_minted_icp_e8s: 500_000_000,
            native_memo_u64: 604_801,
            created_at_time_nanos: 604_801_000_000_000,
        });
        two_year
            .validate(
                MaturityKind::TwoYear,
                state.config.two_year_neuron_id,
                &state.config.maturity_staging,
            )
            .expect("valid adverse modulation may Mint less than nominal maturity");
        let mut two_week = pending(
            MaturityKind::TwoWeek,
            2,
            state.config.maturity_staging.clone(),
            Some(1),
        );
        two_week.mint_proof = MintProofState::Delivering(MintEvidence {
            mint_block: u128::MAX,
            actual_minted_icp_e8s: u128::MAX,
            native_memo_u64: 604_801,
            created_at_time_nanos: 604_801_000_000_000,
        });
        state.lifecycle = Lifecycle::Paused;
        state.latest_pooled_target = Some(PooledTarget {
            target_e8s: 1,
            status: PooledTargetStatus::AtTarget,
        });
        state.pooled_parent_id = Some(2);
        state.pooled_parent_staking_account = Some(Account {
            owner: state.config.nns_governance,
            subaccount: Some(vec![2; 32]),
        });
        state.latest_started_two_week_generation = 1;
        state.pending_two_year_maturity = Some(two_year);
        state.pending_two_week_maturity = Some(two_week.clone());
        state.live_cohorts = (1..=io_nns_types::backing::MAX_LIVE_UNWIND_COHORTS as u64)
            .map(|generation| PassiveCohort {
                generation,
                child_neuron_id: generation,
                principal_e8s: u128::MAX,
                child_staking_subaccount: vec![generation as u8; 32],
                ready_at_seconds: u64::MAX,
                proof: io_nns_types::backing::CohortProofState::Dissolving,
                disbursement_block: None,
            })
            .collect();
        state.last_two_year_maturity = Some(CompletedMaturity {
            kind: MaturityKind::TwoYear,
            neuron_id: state.config.two_year_neuron_id,
            mint_block: u128::MAX,
            nominal_disbursed_maturity_e8s: u64::MAX,
            actual_minted_icp_e8s: u128::MAX,
            destination: state.config.maturity_staging.clone(),
            completed_at_nanos: u64::MAX,
        });
        state.last_two_week_maturity = Some(CompletedMaturity {
            kind: MaturityKind::TwoWeek,
            neuron_id: 2,
            mint_block: u128::MAX,
            nominal_disbursed_maturity_e8s: u64::MAX,
            actual_minted_icp_e8s: u128::MAX,
            destination: state.config.maturity_staging.clone(),
            completed_at_nanos: u64::MAX,
        });
        state.active_operation = Some(NnsOperation::Maturity(Box::new(MaturityCommandOperation {
            operation_sequence: 1,
            dispatch_epoch: u64::MAX,
            kind: MaturityKind::TwoWeek,
            phase: MaturityCommandPhase::BackingInflowDelivery(BackingInflowDeliveryOperation {
                pending: two_week,
                permit: None,
                permanent_transfer: None,
                claim_transfer: None,
                parent_credit_phase: ParentCreditPhase::NotRequired,
                stream_pooled_block: None,
            }),
        })));
        state.two_year_maturity_baseline_reconciled = true;
        state.next_operation_sequence = 2;
        state.validate(canister_self).unwrap();
        let stable = StableNnsState::V1(state);
        let encoded = stable.to_bytes();
        let Bound::Bounded { max_size, .. } = <StableNnsState as Storable>::BOUND else {
            panic!("NNS state must remain bounded");
        };
        eprintln!(
            "maximum fixed NNS V1 slots encode to {} bytes of the {}-byte stable bound",
            encoded.len(),
            max_size
        );
        assert!(encoded.len() <= max_size as usize);
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

    #[test]
    fn strict_launch_v1_rejects_corrupt_and_future_state() {
        assert!(candid::decode_one::<StableNnsState>(b"not candid").is_err());
        let (canister_self, state) = valid_state();
        let current = candid::encode_one(StableNnsState::V1(state.clone())).unwrap();
        let decoded = candid::decode_one::<StableNnsState>(&current).unwrap();
        assert_eq!(decoded, StableNnsState::V1(state.clone()));

        let mut bad_marker = state.clone();
        bad_marker.launch_schema_marker = 2;
        assert!(bad_marker
            .validate(canister_self)
            .unwrap_err()
            .contains("launch schema marker"));

        let future = candid::encode_one(FutureStableNnsState::V2(state.clone())).unwrap();
        assert!(candid::decode_one::<StableNnsState>(&future).is_err());

        let checkpoint = candid::encode_one(CheckpointStableNnsState::V1(CheckpointNnsStateV1 {
            config: state.config,
            lifecycle: state.lifecycle,
            active_operation: state.active_operation,
            latest_pooled_target: state.latest_pooled_target,
            two_year_maturity_baseline_reconciled: state.two_year_maturity_baseline_reconciled,
            two_week_maturity_baseline_reconciled: false,
            latest_started_two_week_generation: state.latest_started_two_week_generation,
            latest_completed_two_week_generation: state.latest_completed_two_week_generation,
            pending_two_year_maturity: state.pending_two_year_maturity,
            pending_two_week_maturity: state.pending_two_week_maturity,
            pending_unwind: None,
            last_two_year_maturity: state.last_two_year_maturity,
            last_two_week_maturity: state.last_two_week_maturity,
            next_operation_sequence: state.next_operation_sequence,
            control_epoch: state.control_epoch,
            last_passive_reconciliation_attempt_nanos: Some(1),
            last_public_jupiter_lookup_attempt_nanos: Some(2),
            jupiter_lookup_lease: Some(CheckpointJupiterLookupLease {
                block_index: 3,
                started_at_nanos: 4,
            }),
        }))
        .unwrap();
        assert!(candid::decode_one::<StableNnsState>(&checkpoint).is_err());
    }
}
