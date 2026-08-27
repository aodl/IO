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

pub(crate) const LAUNCH_SCHEMA_MARKER: u8 = 11;

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
        let two_week_staging = io_accounts::two_week_maturity_staging(canister_self);
        let two_year_staging = io_accounts::two_year_maturity_staging(canister_self);
        let staging = [&self.jupiter_staging, &two_week_staging, &two_year_staging];
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
    pub reconciliation_request_fingerprint: Vec<u8>,
    pub target_e8s: u128,
    pub principal_e8s: u128,
    pub snapshot_fingerprint: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CompletedUnwindReconciliation {
    pub generation: u64,
    pub reconciliation_request_fingerprint: Vec<u8>,
    pub child_neuron_id: u64,
    pub physical_principal_e8s: u128,
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
    pub last_completed_unwind: Option<CompletedUnwindReconciliation>,
    pub last_held_reconciliation: Option<HeldReconciliation>,
    pub latest_reconciliation_generation: u64,
    pub latest_pooled_target: Option<PooledTarget>,
    pub two_year_maturity_baseline_reconciled: bool,
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
            last_completed_unwind: None,
            last_held_reconciliation: None,
            latest_reconciliation_generation: 0,
            latest_pooled_target: None,
            two_year_maturity_baseline_reconciled: false,
            pending_two_year_maturity: None,
            pending_two_week_maturity: None,
            last_two_year_maturity: None,
            last_two_week_maturity: None,
            next_operation_sequence: 1,
            control_epoch: 0,
        }
    }
}

fn validate_maturity_delivery(
    kind: crate::maturity::MaturityKind,
    operation_sequence: u64,
    delivery: &crate::maturity::MaturityDeliveryOperation,
    config: &NnsConfig,
    canister_self: Principal,
) -> Result<(), String> {
    use io_nns_types::{receipt::receipt_memo, transfer::NnsTransferAttempt};

    let captured_e8s = delivery
        .pending
        .captured_e8s
        .ok_or("maturity delivery lacks frozen capture")?;
    let split = io_nns_types::maturity::capture_40_60(
        captured_e8s,
        config.expected_icp_fee_e8s,
        config.expected_icp_fee_e8s,
    )
    .map_err(|error| format!("maturity capture split failed: {error:?}"))?;
    let source = crate::maturity_flow::staging_account(canister_self, kind)
        .canonical()?
        .subaccount;
    let validate_transfer = |attempt: &NnsTransferAttempt| -> Result<(), String> {
        attempt.validate()?;
        if attempt.intent.ledger != config.icp_ledger
            || attempt.intent.source_subaccount != source
            || attempt.intent.fee_e8s != config.expected_icp_fee_e8s
        {
            return Err("maturity transfer differs from canonical staging policy".into());
        }
        Ok(())
    };
    if let Some(credit) = &delivery.permanent_credit {
        match credit {
            crate::maturity::PermanentCreditState::Prepared { before, transfer } => {
                validate_transfer(transfer)?;
                if before.neuron_id != config.two_year_neuron_id
                    || transfer.intent.amount_e8s != split.permanent_credit
                    || transfer.intent.destination.owner != config.nns_governance
                    || transfer.intent.destination.canonical()?.subaccount
                        != before.staking_subaccount
                {
                    return Err("permanent-leg transfer is inconsistent".into());
                }
            }
            crate::maturity::PermanentCreditState::RefreshSubmitted { before, .. } => {
                if before.neuron_id != config.two_year_neuron_id || before.cached_stake_e8s == 0 {
                    return Err("permanent-leg refresh evidence is inconsistent".into());
                }
            }
            crate::maturity::PermanentCreditState::Proved(proof) => {
                proof.validate()?;
                if proof.neuron_id != config.two_year_neuron_id
                    || proof.protocol_credit_e8s != split.permanent_credit
                {
                    return Err("permanent-leg effect proof is inconsistent".into());
                }
            }
        }
    }
    if !matches!(
        delivery.permanent_credit,
        Some(crate::maturity::PermanentCreditState::Proved(_))
    ) && (delivery.permit.is_some() || delivery.claim_transfer.is_some())
    {
        return Err("maturity claim transfer precedes permanent transfer".into());
    }
    match (kind, &delivery.permit) {
        (crate::maturity::MaturityKind::TwoYear, Some(_)) => {
            return Err("two-year maturity contains paired issuance state".into())
        }
        (crate::maturity::MaturityKind::TwoWeek, Some(permit))
            if permit.stream_operation_sequence > 0
                && permit.amount_e8s == split.claim_credit
                && permit
                    .destination
                    .effective_eq(&config.stream_liquid_account)?
                && permit.memo == receipt_memo(operation_sequence) => {}
        (crate::maturity::MaturityKind::TwoWeek, Some(_)) => {
            return Err("two-week receipt permit differs from frozen capture".into())
        }
        _ => {}
    }
    if let Some(transfer) = &delivery.claim_transfer {
        validate_transfer(transfer)?;
        let expected_memo = match &delivery.permit {
            Some(permit) => permit.memo.clone(),
            None => crate::maturity_flow::maturity_transfer_memo(
                b"io-two-year-maturity-claim-v1",
                operation_sequence,
            ),
        };
        if !transfer
            .intent
            .destination
            .effective_eq(&config.stream_liquid_account)?
            || transfer.intent.amount_e8s != split.claim_credit
            || transfer.intent.memo != expected_memo
        {
            return Err("claim transfer differs from frozen maturity economics".into());
        }
    }
    if kind == crate::maturity::MaturityKind::TwoWeek
        && delivery.claim_transfer.is_some()
        && delivery.permit.is_none()
    {
        return Err("two-week claim transfer precedes paired receipt".into());
    }
    Ok(())
}

impl NnsStateV1 {
    pub fn validate(&self, canister_self: Principal) -> Result<(), String> {
        if self.launch_schema_marker != LAUNCH_SCHEMA_MARKER {
            return Err("invalid NNS launch schema marker".into());
        }
        self.config.validate(canister_self)?;
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
        if self
            .live_cohorts
            .iter()
            .any(|cohort| cohort.principal_e8s <= self.config.expected_icp_fee_e8s)
        {
            return Err("live child principal cannot cover its committed disbursement fee".into());
        }
        if let Some(completed) = &self.last_completed_pool {
            completed.validate(self.next_operation_sequence)?;
        }
        if let Some(held) = &self.last_held_reconciliation {
            if held.generation == 0
                || held.generation != self.latest_reconciliation_generation
                || held.reconciliation_request_fingerprint.len() != 32
                || held.snapshot_fingerprint.len() != 32
            {
                return Err("held reconciliation checkpoint is inconsistent".into());
            }
        }
        if self
            .last_completed_unwind
            .as_ref()
            .is_some_and(|completed| {
                completed.generation == 0
                    || completed.generation > self.latest_reconciliation_generation
                    || completed.reconciliation_request_fingerprint.len() != 32
                    || completed.child_neuron_id == 0
                    || completed.physical_principal_e8s == 0
            })
        {
            return Err("completed unwind replay evidence is inconsistent".into());
        }
        let completed_two_week_generation = self.completed_two_week_generation();
        let latest_two_week_generation = self.latest_two_week_generation();
        if latest_two_week_generation > completed_two_week_generation.saturating_add(1)
            || (latest_two_week_generation > 0 && self.latest_pooled_target.is_none())
        {
            return Err("two-week maturity generation tracking is inconsistent".into());
        }
        if latest_two_week_generation > completed_two_week_generation {
            let active_generation = match &self.active_operation {
                Some(NnsOperation::Maturity(operation))
                    if operation.kind == crate::maturity::MaturityKind::TwoWeek =>
                {
                    operation.intent().entitlement_batch_generation
                }
                _ => None,
            };
            let pending_generation = self
                .pending_two_week_maturity
                .as_ref()
                .and_then(|pending| pending.entitlement_batch_generation);
            if active_generation != Some(latest_two_week_generation)
                && pending_generation != Some(latest_two_week_generation)
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
                    operation.validate(self.next_operation_sequence)?;
                    if let crate::maturity::MaturityCommandPhase::Delivery(delivery) =
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
                            return Err("maturity delivery lost its frozen capture".into());
                        }
                        validate_maturity_delivery(
                            operation.kind,
                            operation.operation_sequence,
                            delivery,
                            &self.config,
                            canister_self,
                        )?;
                    }
                }
                NnsOperation::Pool(operation) => {
                    operation.validate(self.next_operation_sequence)?;
                }
                NnsOperation::Unwind(operation) => {
                    operation.validate(self.next_operation_sequence)?;
                    if !matches!(
                        operation.phase,
                        crate::pool::UnwindPhase::SplitPrepared
                            | crate::pool::UnwindPhase::SplitSubmitted
                    ) && operation.principal_e8s <= operation.committed_disbursement_fee_e8s
                    {
                        return Err(
                            "committed unwind principal cannot cover its disbursement fee".into(),
                        );
                    }
                }
            }
        }
        for (pending, kind) in [
            (
                self.pending_two_year_maturity.as_ref(),
                crate::maturity::MaturityKind::TwoYear,
            ),
            (
                self.pending_two_week_maturity.as_ref(),
                crate::maturity::MaturityKind::TwoWeek,
            ),
        ] {
            let Some(pending) = pending else { continue };
            pending.validate(kind)?;
        }
        for (completed, kind) in [
            (
                self.last_two_year_maturity.as_ref(),
                crate::maturity::MaturityKind::TwoYear,
            ),
            (
                self.last_two_week_maturity.as_ref(),
                crate::maturity::MaturityKind::TwoWeek,
            ),
        ] {
            let Some(completed) = completed else { continue };
            if completed.kind != kind
                || completed.captured_e8s == 0
                || completed.permanent_credit_e8s == 0
                || completed.claim_credit_e8s == 0
                || completed.completed_at_nanos == 0
                || (kind == crate::maturity::MaturityKind::TwoWeek)
                    != completed.entitlement_batch_generation.is_some()
                || (kind == crate::maturity::MaturityKind::TwoWeek)
                    != completed.two_week_target_e8s.is_some()
            {
                return Err("completed maturity result is inconsistent".into());
            }
        }
        Ok(())
    }

    pub fn completed_two_week_generation(&self) -> u64 {
        self.last_two_week_maturity
            .as_ref()
            .and_then(|completed| completed.entitlement_batch_generation)
            .unwrap_or(0)
    }

    pub fn latest_two_week_generation(&self) -> u64 {
        let active = match &self.active_operation {
            Some(NnsOperation::Maturity(operation))
                if operation.kind == crate::maturity::MaturityKind::TwoWeek =>
            {
                operation.intent().entitlement_batch_generation
            }
            _ => None,
        };
        let pending = self
            .pending_two_week_maturity
            .as_ref()
            .and_then(|pending| pending.entitlement_batch_generation);
        [Some(self.completed_two_week_generation()), active, pending]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(0)
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
pub(crate) mod tests {
    use super::*;

    #[derive(candid::CandidType)]
    enum FutureStableNnsState {
        V2(NnsStateV1),
    }

    #[derive(candid::CandidType)]
    struct PriorPooledNnsStateV1 {
        launch_schema_marker: u8,
        config: NnsConfig,
        lifecycle: Lifecycle,
        active_operation: Option<NnsOperation>,
        pooled_parent_id: Option<u64>,
        pooled_parent_staking_account: Option<Account>,
        live_cohorts: Vec<PassiveCohort>,
        last_completed_pool: Option<CompletedPoolCommand>,
        last_held_reconciliation: Option<HeldReconciliation>,
        latest_reconciliation_generation: u64,
        latest_pooled_target: Option<PooledTarget>,
        two_year_maturity_baseline_reconciled: bool,
        latest_started_two_week_generation: u64,
        latest_completed_two_week_generation: u64,
        pending_two_year_maturity: Option<PendingMaturityDisbursement>,
        pending_two_week_maturity: Option<PendingMaturityDisbursement>,
        last_two_year_maturity: Option<CompletedMaturity>,
        last_two_week_maturity: Option<CompletedMaturity>,
        next_operation_sequence: u64,
        control_epoch: u64,
    }

    #[derive(candid::CandidType)]
    enum PriorPooledStableNnsState {
        V1(PriorPooledNnsStateV1),
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

    pub(crate) fn valid_state() -> (Principal, NnsStateV1) {
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
                last_completed_unwind: None,
                last_held_reconciliation: None,
                latest_reconciliation_generation: 0,
                latest_pooled_target: None,
                two_year_maturity_baseline_reconciled: false,
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
            reconciliation_request_fingerprint: vec![1; 32],
            child_neuron_id: 3,
            principal_e8s: 10_001,
            committed_fee_e8s: 10_000,
            child_staking_subaccount: vec![3; 32],
            ready_at_seconds: 4,
            proof: io_nns_types::backing::CohortProofState::Dissolving,
            disbursement_block: None,
        }
    }

    fn pending_two_week_maturity(
        _state: &NnsStateV1,
        captured_e8s: Option<u128>,
    ) -> crate::maturity::PendingMaturityDisbursement {
        crate::maturity::PendingMaturityDisbursement {
            nominal_disbursed_e8s: 200_000_000,
            initiated_at_seconds: 1,
            scheduled_finalization_timestamp_seconds: 604_801,
            entitlement_batch_generation: Some(1),
            two_week_target_e8s: Some(100_000_000),
            captured_e8s,
        }
    }

    fn pending_two_year_maturity(
        _state: &NnsStateV1,
    ) -> crate::maturity::PendingMaturityDisbursement {
        crate::maturity::PendingMaturityDisbursement {
            nominal_disbursed_e8s: 120_000_000,
            initiated_at_seconds: 1,
            scheduled_finalization_timestamp_seconds: 604_801,
            entitlement_batch_generation: None,
            two_week_target_e8s: None,
            captured_e8s: Some(120_000_000),
        }
    }

    fn configure_pooled_maturity(state: &mut NnsStateV1) {
        state.pooled_parent_id = Some(2);
        state.pooled_parent_staking_account = Some(Account {
            owner: state.config.nns_governance,
            subaccount: Some(vec![9; 32]),
        });
        state.latest_pooled_target = Some(PooledTarget {
            target_e8s: 100_000_000,
            status: PooledTargetStatus::AtTarget,
        });
    }

    fn permanent_transfer(
        state: &NnsStateV1,
        before: &crate::jupiter::NeuronSnapshot,
    ) -> io_nns_types::transfer::NnsTransferAttempt {
        use io_nns_types::transfer::{NnsTransferIntent, TransferState};

        let mut transfer =
            io_nns_types::transfer::NnsTransferAttempt::prepared(NnsTransferIntent {
                ledger: state.config.icp_ledger,
                source_subaccount: io_accounts::TWO_WEEK_MATURITY_SUBACCOUNT,
                destination: Account {
                    owner: state.config.nns_governance,
                    subaccount: Some(before.staking_subaccount.to_vec()),
                },
                amount_e8s: 79_990_000,
                fee_e8s: state.config.expected_icp_fee_e8s,
                memo: vec![1; 32],
                created_at_time_nanos: 1,
            })
            .unwrap();
        transfer.state = TransferState::Succeeded { block: 12 };
        transfer
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
        state.pending_two_year_maturity = Some(crate::maturity::PendingMaturityDisbursement {
            nominal_disbursed_e8s: 120_000_000,
            initiated_at_seconds: 1,
            scheduled_finalization_timestamp_seconds: 1,
            entitlement_batch_generation: None,
            two_week_target_e8s: None,
            captured_e8s: None,
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
    fn two_year_delivery_rejects_paired_receipt_state() {
        let (canister_self, state) = valid_state();
        let pending = pending_two_year_maturity(&state);
        let delivery = crate::maturity::MaturityDeliveryOperation {
            pending,
            permit: Some(io_receipt_types::ClaimBackingReceiptPermit {
                stream_operation_sequence: 1,
                destination: state.config.stream_liquid_account.clone(),
                amount_e8s: 71_990_000,
                memo: io_nns_types::receipt::receipt_memo(1),
            }),
            permanent_credit: Some(crate::maturity::PermanentCreditState::Proved(
                crate::jupiter::PermanentNeuronCreditProof {
                    neuron_id: state.config.two_year_neuron_id,
                    staking_subaccount: [1; 32],
                    before_cached_stake_e8s: 100_000_000,
                    protocol_credit_e8s: 47_990_000,
                    transfer_block: 1,
                    observed_after_cached_stake_e8s: 147_990_000,
                },
            )),
            claim_transfer: None,
        };
        assert!(validate_maturity_delivery(
            crate::maturity::MaturityKind::TwoYear,
            1,
            &delivery,
            &state.config,
            canister_self,
        )
        .unwrap_err()
        .contains("paired issuance"));
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

        let (canister_self, state) = valid_state();
        assert_ne!(
            io_accounts::two_week_maturity_staging(canister_self),
            io_accounts::two_year_maturity_staging(canister_self)
        );
        assert!(state.validate(canister_self).is_ok());
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
    fn ambiguous_and_identified_split_survive_same_schema_upgrade() {
        for phase in [
            crate::pool::UnwindPhase::SplitSubmitted,
            crate::pool::UnwindPhase::ChildIdentified,
        ] {
            let (canister_self, mut state) = valid_state();
            state.lifecycle = Lifecycle::Ready;
            state.two_year_maturity_baseline_reconciled = true;
            state.pending_two_year_maturity = Some(pending_two_year_maturity(&state));
            state.latest_reconciliation_generation = 1;
            state.pooled_parent_id = Some(2);
            state.pooled_parent_staking_account = Some(Account {
                owner: state.config.nns_governance,
                subaccount: Some(vec![9; 32]),
            });
            state.latest_pooled_target = Some(PooledTarget {
                target_e8s: 90,
                status: PooledTargetStatus::OverTarget,
            });
            state.next_operation_sequence = 2;
            let identified = phase == crate::pool::UnwindPhase::ChildIdentified;
            state.active_operation = Some(NnsOperation::Unwind(UnwindOperation {
                operation_sequence: 1,
                generation: 1,
                reconciliation_request_fingerprint: vec![1; 32],
                target_e8s: 90,
                gross_e8s: 30_000,
                split_fee_e8s: 10_000,
                committed_disbursement_fee_e8s: 10_000,
                parent_principal_before_split_e8s: 100_000,
                child_neuron_id: if identified { 3 } else { 0 },
                principal_e8s: if identified { 20_000 } else { 0 },
                child_staking_subaccount: Vec::new(),
                submitted_at_seconds: 0,
                expected_block_index: None,
                child_maturity_e8s: 0,
                parent_maturity_e8s: 0,
                parent_principal_e8s: 0,
                phase,
            }));
            initialize(state.clone(), canister_self).unwrap();
            write(state.clone());
            reopen(canister_self);
            let reopened = read();
            assert_eq!(reopened.lifecycle, Lifecycle::Paused);
            assert_eq!(reopened.active_operation, state.active_operation);
        }
    }

    #[test]
    fn permanent_credit_checkpoints_survive_same_schema_upgrade() {
        for refresh_submitted in [false, true] {
            let (canister_self, mut state) = valid_state();
            configure_pooled_maturity(&mut state);
            state.lifecycle = Lifecycle::Ready;
            state.next_operation_sequence = 2;
            let pending = pending_two_week_maturity(&state, Some(200_000_000));
            state.pending_two_week_maturity = Some(pending.clone());
            let before = crate::jupiter::NeuronSnapshot {
                neuron_id: state.config.two_year_neuron_id,
                staking_subaccount: [1; 32],
                cached_stake_e8s: 1_000_000,
            };
            let permanent_credit = if refresh_submitted {
                crate::maturity::PermanentCreditState::RefreshSubmitted {
                    before,
                    transfer_block: 12,
                }
            } else {
                crate::maturity::PermanentCreditState::Prepared {
                    transfer: Box::new(permanent_transfer(&state, &before)),
                    before,
                }
            };
            state.active_operation = Some(NnsOperation::Maturity(Box::new(
                crate::maturity::MaturityCommandOperation {
                    operation_sequence: 1,
                    dispatch_epoch: 1,
                    kind: crate::maturity::MaturityKind::TwoWeek,
                    phase: crate::maturity::MaturityCommandPhase::Delivery(
                        crate::maturity::MaturityDeliveryOperation {
                            pending,
                            permit: None,
                            permanent_credit: Some(permanent_credit),
                            claim_transfer: None,
                        },
                    ),
                },
            )));
            initialize(state.clone(), canister_self).unwrap();
            write(state.clone());
            reopen(canister_self);
            let reopened = read();
            assert_eq!(reopened.lifecycle, Lifecycle::Paused);
            assert_eq!(reopened.active_operation, state.active_operation);
        }
    }

    #[test]
    fn compact_active_maturity_command_phases_survive_same_schema_upgrade() {
        let intent = crate::maturity::MaturityIntent {
            entitlement_batch_generation: None,
            two_week_target_e8s: None,
        };
        for phase in [
            crate::maturity::MaturityCommandPhase::Observed(intent),
            crate::maturity::MaturityCommandPhase::DisburseMaturitySubmitted {
                intent,
                submitted_at_seconds: 1,
            },
            crate::maturity::MaturityCommandPhase::DisburseMaturitySucceeded {
                intent,
                submitted_at_seconds: 1,
                amount_disbursed_e8s: 120_000_000,
            },
        ] {
            let (canister_self, mut state) = valid_state();
            state.lifecycle = Lifecycle::Ready;
            state.two_year_maturity_baseline_reconciled = true;
            state.next_operation_sequence = 2;
            state.active_operation = Some(NnsOperation::Maturity(Box::new(
                crate::maturity::MaturityCommandOperation {
                    operation_sequence: 1,
                    dispatch_epoch: 1,
                    kind: crate::maturity::MaturityKind::TwoYear,
                    phase,
                },
            )));
            initialize(state.clone(), canister_self).unwrap();
            write(state.clone());
            reopen(canister_self);
            let reopened = read();
            assert_eq!(reopened.lifecycle, Lifecycle::Paused);
            assert_eq!(reopened.active_operation, state.active_operation);
        }
    }

    #[test]
    fn jupiter_refresh_checkpoint_survives_same_schema_upgrade() {
        let (canister_self, mut state) = valid_state();
        state.lifecycle = Lifecycle::Ready;
        state.two_year_maturity_baseline_reconciled = true;
        state.pending_two_year_maturity = Some(pending_two_year_maturity(&state));
        state.next_operation_sequence = 2;
        state.active_operation = Some(NnsOperation::Jupiter(Box::new(
            crate::jupiter::JupiterOperation {
                operation_sequence: 1,
                dispatch_epoch: 1,
                captured_control_epoch: 0,
                deposit: crate::jupiter::JupiterDeposit {
                    block_index: 1,
                    gross_e8s: 100_000,
                    stake_e8s: 30_000,
                    liquid_e8s: 50_000,
                    fee_e8s: 10_000,
                    created_at_time_nanos: 1,
                },
                phase: crate::jupiter::JupiterPhase::RefreshSubmitted(
                    crate::jupiter::StakeTransferSucceeded {
                        before: crate::jupiter::NeuronSnapshot {
                            neuron_id: 1,
                            staking_subaccount: [1; 32],
                            cached_stake_e8s: 1_000_000,
                        },
                        block_index: 2,
                    },
                ),
            },
        )));
        initialize(state.clone(), canister_self).unwrap();
        reopen(canister_self);
        let reopened = read();
        assert_eq!(reopened.lifecycle, Lifecycle::Paused);
        assert_eq!(reopened.active_operation, state.active_operation);
    }

    #[test]
    fn both_pending_maturity_slots_and_ready_cohort_survive_same_schema_upgrade() {
        let (canister_self, mut state) = valid_state();
        configure_pooled_maturity(&mut state);
        state.lifecycle = Lifecycle::Ready;
        state.two_year_maturity_baseline_reconciled = true;
        state.pending_two_year_maturity = Some(pending_two_year_maturity(&state));
        state.pending_two_week_maturity = Some(pending_two_week_maturity(&state, None));
        state.live_cohorts = vec![passive_unwind()];
        initialize(state.clone(), canister_self).unwrap();
        reopen(canister_self);
        let reopened = read();
        assert_eq!(reopened.lifecycle, Lifecycle::Paused);
        assert_eq!(
            reopened.pending_two_year_maturity,
            state.pending_two_year_maturity
        );
        assert_eq!(
            reopened.pending_two_week_maturity,
            state.pending_two_week_maturity
        );
        assert_eq!(reopened.live_cohorts, state.live_cohorts);
    }

    #[test]
    fn pending_permanent_yield_and_active_pool_survive_same_schema_upgrade() {
        let (canister_self, mut state) = valid_state();
        state.lifecycle = Lifecycle::Ready;
        state.two_year_maturity_baseline_reconciled = true;
        state.pending_two_year_maturity = Some(pending_two_year_maturity(&state));
        state.pooled_parent_id = Some(2);
        state.pooled_parent_staking_account = Some(Account {
            owner: state.config.nns_governance,
            subaccount: Some(vec![9; 32]),
        });
        state.latest_reconciliation_generation = 1;
        state.latest_pooled_target = Some(PooledTarget {
            target_e8s: 110_000_000,
            status: PooledTargetStatus::UnderTarget,
        });
        state.next_operation_sequence = 2;
        state.active_operation = Some(NnsOperation::Pool(io_nns_types::backing::PoolCommand {
            kind: io_nns_types::backing::PoolCommandKind::TopUp,
            permit: io_nns_types::backing::TopUpPermit {
                generation: 1,
                operation_sequence: 1,
                expected_parent_principal_e8s: 100_000_000,
                destination: state.pooled_parent_staking_account.clone().unwrap(),
                expected_credit_e8s: 10_000_000,
                fee_e8s: state.config.expected_icp_fee_e8s,
                memo: vec![1],
                prepared_at_nanos: 1,
                snapshot_fingerprint: vec![2; 32],
            },
            transfer_block_index: None,
            parent_neuron_id: Some(2),
            phase: io_nns_types::backing::PoolCommandPhase::AwaitingTransfer,
        }));
        initialize(state.clone(), canister_self).unwrap();
        write(state.clone());
        reopen(canister_self);
        assert_eq!(read().active_operation, state.active_operation);
        assert_eq!(
            read().pending_two_year_maturity,
            state.pending_two_year_maturity
        );
    }

    #[test]
    fn maximum_cohort_collection_fits_the_stable_cell_bound() {
        let (canister_self, mut state) = valid_state();
        state.latest_pooled_target = Some(PooledTarget {
            target_e8s: 1,
            status: PooledTargetStatus::AtTarget,
        });
        state.live_cohorts = (1..=io_nns_types::backing::MAX_LIVE_UNWIND_COHORTS as u64)
            .map(|generation| PassiveCohort {
                generation,
                reconciliation_request_fingerprint: vec![generation as u8; 32],
                child_neuron_id: generation,
                principal_e8s: u128::from(u64::MAX),
                committed_fee_e8s: 10_000,
                child_staking_subaccount: vec![generation as u8; 32],
                ready_at_seconds: u64::MAX,
                proof: io_nns_types::backing::CohortProofState::Dissolving,
                disbursement_block: None,
            })
            .collect();
        state.validate(canister_self).unwrap();
        let stable = StableNnsState::V1(state);
        let encoded = stable.to_bytes();
        let Bound::Bounded { max_size, .. } = <StableNnsState as Storable>::BOUND else {
            panic!("NNS state must remain bounded");
        };
        eprintln!("maximum encoded NNS state: {} bytes", encoded.len());
        assert!(encoded.len() <= max_size as usize);
    }

    #[test]
    fn processed_jupiter_block_replays_exact_typed_result() {
        let (canister_self, state) = valid_state();
        initialize(state, canister_self).unwrap();
        let result = JupiterCompleted {
            deposit_block: 9_223_372_036_854_000_001,
            gross_e8s: 100,
            stake_e8s: 30,
            observed_after_cached_stake_e8s: 31,
            liquid_e8s: 50,
            stake_transfer_block: 2,
            liquid_transfer_block: 3,
            stream_receipt_sequence: 4,
            backed_io_e8s: 5,
            io_transfer_block: 6,
            io_fee_e8s: 10_000,
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

        let mut prior_checkpoint = state.clone();
        prior_checkpoint.launch_schema_marker = LAUNCH_SCHEMA_MARKER - 1;
        let prior = candid::encode_one(StableNnsState::V1(prior_checkpoint)).unwrap();
        let prior = candid::decode_one::<StableNnsState>(&prior)
            .unwrap()
            .into_v1();
        assert!(prior
            .validate(canister_self)
            .unwrap_err()
            .contains("launch schema marker"));

        let checkpoint = PriorPooledStableNnsState::V1(PriorPooledNnsStateV1 {
            launch_schema_marker: LAUNCH_SCHEMA_MARKER - 1,
            config: state.config.clone(),
            lifecycle: state.lifecycle,
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
        });
        let checkpoint = candid::encode_one(checkpoint).unwrap();
        let rejected = match candid::decode_one::<StableNnsState>(&checkpoint) {
            Err(_) => true,
            Ok(decoded) => decoded.into_v1().validate(canister_self).is_err(),
        };
        assert!(rejected, "the 0e7299e pooled NNS state must be rejected");

        let mut bad_marker = state.clone();
        bad_marker.launch_schema_marker = 2;
        assert!(bad_marker
            .validate(canister_self)
            .unwrap_err()
            .contains("launch schema marker"));

        let future = candid::encode_one(FutureStableNnsState::V2(state.clone())).unwrap();
        assert!(candid::decode_one::<StableNnsState>(&future).is_err());

        let latest_two_week_generation = state.latest_two_week_generation();
        let completed_two_week_generation = state.completed_two_week_generation();
        let checkpoint = candid::encode_one(CheckpointStableNnsState::V1(CheckpointNnsStateV1 {
            config: state.config,
            lifecycle: state.lifecycle,
            active_operation: state.active_operation,
            latest_pooled_target: state.latest_pooled_target,
            two_year_maturity_baseline_reconciled: state.two_year_maturity_baseline_reconciled,
            two_week_maturity_baseline_reconciled: false,
            latest_started_two_week_generation: latest_two_week_generation,
            latest_completed_two_week_generation: completed_two_week_generation,
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
