pub mod clients;
pub mod scheduler;

use candid::{CandidType, Principal};
use serde::Deserialize;
use std::cell::RefCell;

pub const TWO_YEAR_NNS_NEURON_ID: u64 = 6_345_890_886_899_317_159;
pub const CONTROLLER_CANISTER_PRINCIPAL_TEXT: &str = "oae4c-3iaaa-aaaar-qb5qq-cai";
pub const SECONDS_PER_DAY: u64 = 86_400;
pub const TWO_WEEK_DISSOLVE_SECONDS: u64 = 14 * SECONDS_PER_DAY;
pub const MAX_MODEL_ANNUAL_BPS: u128 = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ManagedNeuronKind {
    TwoYearProtocol,
    TwoWeekPooled,
    TwoWeekUnwind,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SimulatedNnsNeuron {
    pub neuron_id: u64,
    pub kind: ManagedNeuronKind,
    pub principal_e8s: u128,
    pub maturity_e8s: u128,
    pub dissolve_delay_seconds: u64,
    pub is_dissolving: bool,
    pub dissolve_started_at_seconds: Option<u64>,
}

impl SimulatedNnsNeuron {
    pub fn new(
        neuron_id: u64,
        kind: ManagedNeuronKind,
        principal_e8s: u128,
        dissolve_delay_seconds: u64,
    ) -> Self {
        Self {
            neuron_id,
            kind,
            principal_e8s,
            maturity_e8s: 0,
            dissolve_delay_seconds,
            is_dissolving: false,
            dissolve_started_at_seconds: None,
        }
    }

    /// Deterministic maturity accrual used by the PocketIC-shaped tests.
    /// `annual_bps` is deliberately explicit so tests can use high rates and fast-forward time.
    pub fn accrue_maturity(&mut self, elapsed_seconds: u64, annual_bps: u128) {
        const YEAR_SECONDS: u128 = 365 * 24 * 60 * 60;
        let accrued = self
            .principal_e8s
            .saturating_mul(annual_bps)
            .saturating_mul(u128::from(elapsed_seconds))
            / 10_000
            / YEAR_SECONDS;
        self.maturity_e8s = self.maturity_e8s.saturating_add(accrued);
    }

    pub fn disburse_maturity(&mut self) -> u128 {
        let maturity = self.maturity_e8s;
        self.maturity_e8s = 0;
        maturity
    }

    pub fn start_dissolving(&mut self, now_seconds: u64) {
        self.is_dissolving = true;
        self.dissolve_started_at_seconds = Some(now_seconds);
    }

    pub fn stop_dissolving(&mut self) {
        self.is_dissolving = false;
        self.dissolve_started_at_seconds = None;
    }

    pub fn is_ready_to_disburse(&self, now_seconds: u64) -> bool {
        self.is_dissolving
            && self
                .dissolve_started_at_seconds
                .map(|started| now_seconds.saturating_sub(started) >= self.dissolve_delay_seconds)
                .unwrap_or(false)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct TwoWeekPoolState {
    pub target_staked_e8s: u128,
    pub active_staked_e8s: u128,
    pub pending_unwind_e8s: u128,
    pub pending_restake_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum RebalanceAction {
    None,
    StakeMore { amount_e8s: u128 },
    SplitAndDissolve { amount_e8s: u128 },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum TwoWeekPoolLifecyclePlan {
    None,
    TwoWeekPoolRestake { amount_e8s: u128 },
    TwoWeekPoolSplit { amount_e8s: u128 },
    TwoWeekPoolStartDissolving { neuron_id: u64 },
    TwoWeekPoolStopDissolving { neuron_id: u64 },
    TwoWeekPoolMergeBack { neuron_id: u64, amount_e8s: u128 },
    TwoWeekUnwindPrincipalDisbursement { neuron_id: u64, amount_e8s: u128 },
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum TwoWeekPoolLifecycleResult {
    Succeeded,
    Retryable { message: String },
    Terminal { message: String },
}

impl TwoWeekPoolLifecycleResult {
    pub fn from_governance_error(err: &io_governance_types::NnsGovernanceError) -> Self {
        if err.is_retryable() {
            Self::Retryable {
                message: format!("{err:?}"),
            }
        } else {
            Self::Terminal {
                message: format!("{err:?}"),
            }
        }
    }
}

impl TwoWeekPoolState {
    /// Plans a target-based rebalance. Pending unwind/restake is intentionally
    /// included so callers can batch cancel-dissolve and dissolve events before
    /// issuing expensive NNS split/merge operations.
    pub fn plan_rebalance(&self) -> RebalanceAction {
        let effective_active = self
            .active_staked_e8s
            .saturating_add(self.pending_restake_e8s)
            .saturating_sub(self.pending_unwind_e8s);
        if self.target_staked_e8s > effective_active {
            RebalanceAction::StakeMore {
                amount_e8s: self.target_staked_e8s - effective_active,
            }
        } else if effective_active > self.target_staked_e8s {
            RebalanceAction::SplitAndDissolve {
                amount_e8s: effective_active - self.target_staked_e8s,
            }
        } else {
            RebalanceAction::None
        }
    }

    pub fn plan_lifecycle(&self) -> TwoWeekPoolLifecyclePlan {
        match self.plan_rebalance() {
            RebalanceAction::None => TwoWeekPoolLifecyclePlan::None,
            RebalanceAction::StakeMore { amount_e8s } => {
                TwoWeekPoolLifecyclePlan::TwoWeekPoolRestake { amount_e8s }
            }
            RebalanceAction::SplitAndDissolve { amount_e8s } => {
                TwoWeekPoolLifecyclePlan::TwoWeekPoolSplit { amount_e8s }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ManagerError {
    SplitExceedsMainPool,
    UnknownUnwindNeuron,
    NeuronNotReady,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsNeuronManagerModel {
    pub now_seconds: u64,
    pub next_neuron_id: u64,
    pub two_year_neuron: SimulatedNnsNeuron,
    pub two_week_pool: SimulatedNnsNeuron,
    pub unwind_neurons: Vec<SimulatedNnsNeuron>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct InitArgs {
    pub controller_canister_principal_text: String,
    pub two_year_nns_neuron_id: u64,
    pub two_week_dissolve_seconds: u64,
    pub initial_two_year_principal_e8s: u128,
    pub initial_two_week_principal_e8s: u128,
    pub model_annual_bps: u128,
    pub io_stream_manager_principal_text: Option<String>,
    pub two_year_maturity_memo: Option<u64>,
    pub two_week_maturity_memo: Option<u64>,
    pub principal_unwind_memo: Option<u64>,
    pub nns_governance_principal_text: Option<String>,
    pub icp_ledger_principal_text: Option<String>,
    pub icp_index_principal_text: Option<String>,
}

impl Default for InitArgs {
    fn default() -> Self {
        Self {
            controller_canister_principal_text: CONTROLLER_CANISTER_PRINCIPAL_TEXT.to_string(),
            two_year_nns_neuron_id: TWO_YEAR_NNS_NEURON_ID,
            two_week_dissolve_seconds: TWO_WEEK_DISSOLVE_SECONDS,
            initial_two_year_principal_e8s: 0,
            initial_two_week_principal_e8s: 0,
            model_annual_bps: 0,
            io_stream_manager_principal_text: None,
            two_year_maturity_memo: None,
            two_week_maturity_memo: None,
            principal_unwind_memo: None,
            nns_governance_principal_text: None,
            icp_ledger_principal_text: None,
            icp_index_principal_text: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsNeuronManagerConfig {
    pub controller_canister_principal_text: String,
    pub two_year_nns_neuron_id: u64,
    pub two_week_dissolve_seconds: u64,
    pub initial_two_year_principal_e8s: u128,
    pub initial_two_week_principal_e8s: u128,
    pub model_annual_bps: u128,
    pub io_stream_manager_principal_text: Option<String>,
    pub two_year_maturity_memo: Option<u64>,
    pub two_week_maturity_memo: Option<u64>,
    pub principal_unwind_memo: Option<u64>,
    pub nns_governance_principal_text: Option<String>,
    pub icp_ledger_principal_text: Option<String>,
    pub icp_index_principal_text: Option<String>,
}

impl Default for NnsNeuronManagerConfig {
    fn default() -> Self {
        InitArgs::default()
            .try_into()
            .expect("default nns-neuron-manager config must be valid")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitArgsError {
    EmptyControllerPrincipal,
    InvalidControllerPrincipal { value: String },
    InvalidStreamManagerPrincipal { value: String },
    InvalidNnsGovernancePrincipal { value: String },
    InvalidIcpLedgerPrincipal { value: String },
    InvalidIcpIndexPrincipal { value: String },
    ZeroTwoYearNeuronId,
    ZeroTwoWeekDissolveSeconds,
    ModelAnnualBpsTooHigh { bps: u128, max_bps: u128 },
}

impl TryFrom<InitArgs> for NnsNeuronManagerConfig {
    type Error = InitArgsError;

    fn try_from(args: InitArgs) -> Result<Self, Self::Error> {
        if args.controller_canister_principal_text.trim().is_empty() {
            return Err(InitArgsError::EmptyControllerPrincipal);
        }
        if Principal::from_text(&args.controller_canister_principal_text).is_err() {
            return Err(InitArgsError::InvalidControllerPrincipal {
                value: args.controller_canister_principal_text,
            });
        }
        if args.two_year_nns_neuron_id == 0 {
            return Err(InitArgsError::ZeroTwoYearNeuronId);
        }
        if args.two_week_dissolve_seconds == 0 {
            return Err(InitArgsError::ZeroTwoWeekDissolveSeconds);
        }
        if args.model_annual_bps > MAX_MODEL_ANNUAL_BPS {
            return Err(InitArgsError::ModelAnnualBpsTooHigh {
                bps: args.model_annual_bps,
                max_bps: MAX_MODEL_ANNUAL_BPS,
            });
        }
        if let Some(text) = &args.io_stream_manager_principal_text {
            if text.trim().is_empty() || Principal::from_text(text).is_err() {
                return Err(InitArgsError::InvalidStreamManagerPrincipal {
                    value: text.clone(),
                });
            }
        }
        if let Some(text) = &args.nns_governance_principal_text {
            if text.trim().is_empty() || Principal::from_text(text).is_err() {
                return Err(InitArgsError::InvalidNnsGovernancePrincipal {
                    value: text.clone(),
                });
            }
        }
        if let Some(text) = &args.icp_ledger_principal_text {
            if text.trim().is_empty() || Principal::from_text(text).is_err() {
                return Err(InitArgsError::InvalidIcpLedgerPrincipal {
                    value: text.clone(),
                });
            }
        }
        if let Some(text) = &args.icp_index_principal_text {
            if text.trim().is_empty() || Principal::from_text(text).is_err() {
                return Err(InitArgsError::InvalidIcpIndexPrincipal {
                    value: text.clone(),
                });
            }
        }

        Ok(Self {
            controller_canister_principal_text: args.controller_canister_principal_text,
            two_year_nns_neuron_id: args.two_year_nns_neuron_id,
            two_week_dissolve_seconds: args.two_week_dissolve_seconds,
            initial_two_year_principal_e8s: args.initial_two_year_principal_e8s,
            initial_two_week_principal_e8s: args.initial_two_week_principal_e8s,
            model_annual_bps: args.model_annual_bps,
            io_stream_manager_principal_text: args.io_stream_manager_principal_text,
            two_year_maturity_memo: args.two_year_maturity_memo,
            two_week_maturity_memo: args.two_week_maturity_memo,
            principal_unwind_memo: args.principal_unwind_memo,
            nns_governance_principal_text: args.nns_governance_principal_text,
            icp_ledger_principal_text: args.icp_ledger_principal_text,
            icp_index_principal_text: args.icp_index_principal_text,
        })
    }
}

#[cfg_attr(not(any(test, debug_assertions)), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct CanisterState {
    config: NnsNeuronManagerConfig,
    model: NnsNeuronManagerModel,
    two_week_pool_state: TwoWeekPoolState,
    operation_journal: Vec<NnsOperation>,
    scheduler_cursors: NnsSchedulerCursors,
}

impl CanisterState {
    fn new(config: NnsNeuronManagerConfig) -> Self {
        let model = NnsNeuronManagerModel::new_with_config(&config);
        Self {
            config,
            two_week_pool_state: TwoWeekPoolState {
                target_staked_e8s: 0,
                active_staked_e8s: model.two_week_pool.principal_e8s,
                pending_unwind_e8s: 0,
                pending_restake_e8s: 0,
            },
            operation_journal: Vec::new(),
            scheduler_cursors: NnsSchedulerCursors::default(),
            model,
        }
    }
}

impl Default for CanisterState {
    fn default() -> Self {
        Self::new(NnsNeuronManagerConfig::default())
    }
}

thread_local! {
    static CANISTER_STATE: RefCell<CanisterState> = RefCell::new(CanisterState::default());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ApiManagedNeuronKind {
    TwoYearProtocol,
    TwoWeekPooled,
    TwoWeekUnwind,
}

impl From<ManagedNeuronKind> for ApiManagedNeuronKind {
    fn from(value: ManagedNeuronKind) -> Self {
        match value {
            ManagedNeuronKind::TwoYearProtocol => ApiManagedNeuronKind::TwoYearProtocol,
            ManagedNeuronKind::TwoWeekPooled => ApiManagedNeuronKind::TwoWeekPooled,
            ManagedNeuronKind::TwoWeekUnwind => ApiManagedNeuronKind::TwoWeekUnwind,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiSimulatedNnsNeuron {
    pub neuron_id: u64,
    pub kind: ApiManagedNeuronKind,
    pub principal_e8s: u128,
    pub maturity_e8s: u128,
    pub dissolve_delay_seconds: u64,
    pub is_dissolving: bool,
    pub dissolve_started_at_seconds: Option<u64>,
}

impl From<&SimulatedNnsNeuron> for ApiSimulatedNnsNeuron {
    fn from(value: &SimulatedNnsNeuron) -> Self {
        Self {
            neuron_id: value.neuron_id,
            kind: value.kind.into(),
            principal_e8s: value.principal_e8s,
            maturity_e8s: value.maturity_e8s,
            dissolve_delay_seconds: value.dissolve_delay_seconds,
            is_dissolving: value.is_dissolving,
            dissolve_started_at_seconds: value.dissolve_started_at_seconds,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiTwoWeekPoolState {
    pub target_staked_e8s: u128,
    pub active_staked_e8s: u128,
    pub pending_unwind_e8s: u128,
    pub pending_restake_e8s: u128,
}

impl From<TwoWeekPoolState> for ApiTwoWeekPoolState {
    fn from(value: TwoWeekPoolState) -> Self {
        Self {
            target_staked_e8s: value.target_staked_e8s,
            active_staked_e8s: value.active_staked_e8s,
            pending_unwind_e8s: value.pending_unwind_e8s,
            pending_restake_e8s: value.pending_restake_e8s,
        }
    }
}

impl From<ApiTwoWeekPoolState> for TwoWeekPoolState {
    fn from(value: ApiTwoWeekPoolState) -> Self {
        Self {
            target_staked_e8s: value.target_staked_e8s,
            active_staked_e8s: value.active_staked_e8s,
            pending_unwind_e8s: value.pending_unwind_e8s,
            pending_restake_e8s: value.pending_restake_e8s,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ApiRebalanceAction {
    None,
    StakeMore { amount_e8s: u128 },
    SplitAndDissolve { amount_e8s: u128 },
}

impl From<RebalanceAction> for ApiRebalanceAction {
    fn from(value: RebalanceAction) -> Self {
        match value {
            RebalanceAction::None => ApiRebalanceAction::None,
            RebalanceAction::StakeMore { amount_e8s } => {
                ApiRebalanceAction::StakeMore { amount_e8s }
            }
            RebalanceAction::SplitAndDissolve { amount_e8s } => {
                ApiRebalanceAction::SplitAndDissolve { amount_e8s }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiState {
    pub now_seconds: u64,
    pub next_neuron_id: u64,
    pub two_year_neuron: ApiSimulatedNnsNeuron,
    pub two_week_pool: ApiSimulatedNnsNeuron,
    pub unwind_neurons: Vec<ApiSimulatedNnsNeuron>,
    pub two_week_pool_state: ApiTwoWeekPoolState,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct AdvanceModelTimeRequest {
    pub elapsed_seconds: u64,
    pub annual_bps: Option<u128>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct DebugTickOutcome {
    pub disbursed_two_year_maturity_e8s: u128,
    pub disbursed_two_week_maturity_e8s: u128,
    pub disbursed_unwind_principal_e8s: u128,
    pub planned_pool_rebalances: u64,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PendingIcpTransfer {
    pub amount_e8s: u128,
    pub memo: String,
    pub post_model: Option<NnsNeuronManagerModel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsOperationKind {
    TwoYearMaturityDisbursement,
    TwoWeekMaturityDisbursement,
    TwoWeekUnwindPrincipalDisbursement,
    TwoWeekPoolSplit,
    TwoWeekPoolMergeBack,
    TwoWeekPoolRestake,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsOperationPhase {
    Observed,
    AwaitingIcpTransfer,
    Completed,
    FailedRetryable,
    FailedTerminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsTransferStatus {
    Pending,
    Succeeded,
    FailedRetryable,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsOperation {
    pub operation_id: String,
    pub kind: NnsOperationKind,
    pub phase: NnsOperationPhase,
    pub amount_e8s: u128,
    pub memo: String,
    pub created_at: u64,
    pub last_updated: u64,
    pub retry_count: u32,
    pub last_error: Option<String>,
    pub icp_transfer_status: NnsTransferStatus,
    pub icp_transfer_block: Option<u64>,
    pub post_model: Option<NnsNeuronManagerModel>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsSchedulerCursors {
    pub last_two_year_maturity_check_time: Option<u64>,
    pub last_two_week_maturity_check_time: Option<u64>,
    pub last_unwind_check_time: Option<u64>,
}

#[cfg(target_family = "wasm")]
fn canister_time() -> u64 {
    ic_cdk::api::time()
}

#[cfg(not(target_family = "wasm"))]
fn canister_time() -> u64 {
    0
}

impl NnsOperation {
    pub fn new(
        operation_id: String,
        kind: NnsOperationKind,
        amount_e8s: u128,
        memo: String,
        post_model: Option<NnsNeuronManagerModel>,
    ) -> Self {
        let now = canister_time();
        Self {
            operation_id,
            kind,
            phase: NnsOperationPhase::AwaitingIcpTransfer,
            amount_e8s,
            memo,
            created_at: now,
            last_updated: now,
            retry_count: 0,
            last_error: None,
            icp_transfer_status: NnsTransferStatus::Pending,
            icp_transfer_block: None,
            post_model,
        }
    }

    #[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
    fn mark_retryable_error(&mut self, err: String) {
        self.phase = NnsOperationPhase::FailedRetryable;
        self.icp_transfer_status = NnsTransferStatus::FailedRetryable;
        self.retry_count = self.retry_count.saturating_add(1);
        self.last_error = Some(err);
        self.last_updated = canister_time();
    }

    #[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
    fn mark_completed(&mut self, block: u64) {
        self.phase = NnsOperationPhase::Completed;
        self.icp_transfer_status = NnsTransferStatus::Succeeded;
        self.icp_transfer_block = Some(block);
        self.last_error = None;
        self.last_updated = canister_time();
    }
}

impl NnsNeuronManagerModel {
    pub fn new(two_year_principal_e8s: u128, two_week_principal_e8s: u128) -> Self {
        Self::new_with_params(
            TWO_YEAR_NNS_NEURON_ID,
            two_year_principal_e8s,
            two_week_principal_e8s,
            TWO_WEEK_DISSOLVE_SECONDS,
        )
    }

    pub fn new_with_config(config: &NnsNeuronManagerConfig) -> Self {
        Self::new_with_params(
            config.two_year_nns_neuron_id,
            config.initial_two_year_principal_e8s,
            config.initial_two_week_principal_e8s,
            config.two_week_dissolve_seconds,
        )
    }

    fn new_with_params(
        two_year_nns_neuron_id: u64,
        two_year_principal_e8s: u128,
        two_week_principal_e8s: u128,
        two_week_dissolve_seconds: u64,
    ) -> Self {
        Self {
            now_seconds: 0,
            next_neuron_id: 10_000,
            two_year_neuron: SimulatedNnsNeuron::new(
                two_year_nns_neuron_id,
                ManagedNeuronKind::TwoYearProtocol,
                two_year_principal_e8s,
                2 * 365 * SECONDS_PER_DAY,
            ),
            two_week_pool: SimulatedNnsNeuron::new(
                2,
                ManagedNeuronKind::TwoWeekPooled,
                two_week_principal_e8s,
                two_week_dissolve_seconds,
            ),
            unwind_neurons: vec![],
        }
    }

    pub fn advance_time(&mut self, elapsed_seconds: u64, annual_bps: u128) {
        self.now_seconds = self.now_seconds.saturating_add(elapsed_seconds);
        self.two_year_neuron
            .accrue_maturity(elapsed_seconds, annual_bps);
        self.two_week_pool
            .accrue_maturity(elapsed_seconds, annual_bps);
        for neuron in &mut self.unwind_neurons {
            // Dissolving unwind neurons are not part of the active yield pool in the model.
            if !neuron.is_dissolving {
                neuron.accrue_maturity(elapsed_seconds, annual_bps);
            }
        }
    }

    pub fn disburse_two_year_maturity(&mut self) -> u128 {
        self.two_year_neuron.disburse_maturity()
    }

    pub fn disburse_two_week_maturity(&mut self) -> u128 {
        self.two_week_pool.disburse_maturity()
    }

    pub fn stake_more_two_week(&mut self, amount_e8s: u128) {
        self.two_week_pool.principal_e8s =
            self.two_week_pool.principal_e8s.saturating_add(amount_e8s);
    }

    pub fn split_and_start_unwind(&mut self, amount_e8s: u128) -> Result<u64, ManagerError> {
        if amount_e8s > self.two_week_pool.principal_e8s {
            return Err(ManagerError::SplitExceedsMainPool);
        }
        self.two_week_pool.principal_e8s -= amount_e8s;
        let id = self.next_neuron_id;
        self.next_neuron_id += 1;
        let mut child = SimulatedNnsNeuron::new(
            id,
            ManagedNeuronKind::TwoWeekUnwind,
            amount_e8s,
            TWO_WEEK_DISSOLVE_SECONDS,
        );
        child.start_dissolving(self.now_seconds);
        self.unwind_neurons.push(child);
        Ok(id)
    }

    pub fn cancel_unwind_and_merge_back(&mut self, neuron_id: u64) -> Result<u128, ManagerError> {
        let index = self
            .unwind_neurons
            .iter()
            .position(|n| n.neuron_id == neuron_id)
            .ok_or(ManagerError::UnknownUnwindNeuron)?;
        let mut neuron = self.unwind_neurons.remove(index);
        neuron.stop_dissolving();
        let amount = neuron.principal_e8s;
        self.two_week_pool.principal_e8s = self.two_week_pool.principal_e8s.saturating_add(amount);
        Ok(amount)
    }

    pub fn cancel_unwind_plan(
        &self,
        neuron_id: u64,
    ) -> Result<TwoWeekPoolLifecyclePlan, ManagerError> {
        let neuron = self
            .unwind_neurons
            .iter()
            .find(|n| n.neuron_id == neuron_id)
            .ok_or(ManagerError::UnknownUnwindNeuron)?;
        Ok(TwoWeekPoolLifecyclePlan::TwoWeekPoolStopDissolving {
            neuron_id: neuron.neuron_id,
        })
    }

    pub fn merge_back_plan(
        &self,
        neuron_id: u64,
    ) -> Result<TwoWeekPoolLifecyclePlan, ManagerError> {
        let neuron = self
            .unwind_neurons
            .iter()
            .find(|n| n.neuron_id == neuron_id)
            .ok_or(ManagerError::UnknownUnwindNeuron)?;
        Ok(TwoWeekPoolLifecyclePlan::TwoWeekPoolMergeBack {
            neuron_id: neuron.neuron_id,
            amount_e8s: neuron.principal_e8s,
        })
    }

    pub fn ready_unwind_disbursement_plan(
        &self,
        neuron_id: u64,
    ) -> Result<TwoWeekPoolLifecyclePlan, ManagerError> {
        let neuron = self
            .unwind_neurons
            .iter()
            .find(|n| n.neuron_id == neuron_id)
            .ok_or(ManagerError::UnknownUnwindNeuron)?;
        if !neuron.is_ready_to_disburse(self.now_seconds) {
            return Err(ManagerError::NeuronNotReady);
        }
        Ok(
            TwoWeekPoolLifecyclePlan::TwoWeekUnwindPrincipalDisbursement {
                neuron_id: neuron.neuron_id,
                amount_e8s: neuron.principal_e8s,
            },
        )
    }

    pub fn disburse_ready_unwind(&mut self, neuron_id: u64) -> Result<u128, ManagerError> {
        let index = self
            .unwind_neurons
            .iter()
            .position(|n| n.neuron_id == neuron_id)
            .ok_or(ManagerError::UnknownUnwindNeuron)?;
        if !self.unwind_neurons[index].is_ready_to_disburse(self.now_seconds) {
            return Err(ManagerError::NeuronNotReady);
        }
        let neuron = self.unwind_neurons.remove(index);
        Ok(neuron.principal_e8s)
    }
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg_attr(target_family = "wasm", ic_cdk::init)]
pub fn init(args: InitArgs) {
    let config =
        NnsNeuronManagerConfig::try_from(args).expect("invalid io_nns_neuron_manager init args");
    CANISTER_STATE.with(|cell| {
        *cell.borrow_mut() = CanisterState::new(config);
    });
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StableState {
    pub config: NnsNeuronManagerConfig,
    pub model: NnsNeuronManagerModel,
    pub two_week_pool_state: TwoWeekPoolState,
    pub operation_journal: Vec<NnsOperation>,
    pub scheduler_cursors: NnsSchedulerCursors,
}

fn export_stable_state() -> StableState {
    CANISTER_STATE.with(|cell| {
        let state = cell.borrow();
        StableState {
            config: state.config.clone(),
            model: state.model.clone(),
            two_week_pool_state: state.two_week_pool_state.clone(),
            operation_journal: state.operation_journal.clone(),
            scheduler_cursors: state.scheduler_cursors,
        }
    })
}

fn import_stable_state(state: StableState) {
    CANISTER_STATE.with(|cell| {
        *cell.borrow_mut() = CanisterState {
            config: state.config,
            model: state.model,
            two_week_pool_state: state.two_week_pool_state,
            operation_journal: state.operation_journal,
            scheduler_cursors: state.scheduler_cursors,
        };
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::pre_upgrade)]
pub fn pre_upgrade() {
    ic_cdk::storage::stable_save((export_stable_state(),))
        .expect("failed to save io_nns_neuron_manager stable state");
}

#[cfg_attr(target_family = "wasm", ic_cdk::post_upgrade)]
pub fn post_upgrade() {
    if let Ok((state,)) = ic_cdk::storage::stable_restore::<(StableState,)>() {
        import_stable_state(state);
    }
}

#[cfg(any(test, debug_assertions))]
pub fn export_stable_state_for_tests() -> StableState {
    export_stable_state()
}

#[cfg(any(test, debug_assertions))]
pub fn import_stable_state_for_tests(state: StableState) {
    import_stable_state(state);
}

#[cfg(any(test, debug_assertions))]
fn config_snapshot() -> NnsNeuronManagerConfig {
    CANISTER_STATE.with(|cell| cell.borrow().config.clone())
}

#[cfg(any(test, debug_assertions))]
fn state_snapshot() -> ApiState {
    CANISTER_STATE.with(|cell| {
        let state = cell.borrow();
        ApiState {
            now_seconds: state.model.now_seconds,
            next_neuron_id: state.model.next_neuron_id,
            two_year_neuron: (&state.model.two_year_neuron).into(),
            two_week_pool: (&state.model.two_week_pool).into(),
            unwind_neurons: state.model.unwind_neurons.iter().map(Into::into).collect(),
            two_week_pool_state: state.two_week_pool_state.clone().into(),
        }
    })
}

#[cfg(any(test, debug_assertions))]
fn plan_rebalance_impl(pool_state: ApiTwoWeekPoolState) -> ApiRebalanceAction {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.two_week_pool_state = pool_state.into();
        state.two_week_pool_state.plan_rebalance().into()
    })
}

#[cfg(any(test, debug_assertions))]
fn advance_model_time_impl(request: AdvanceModelTimeRequest) -> ApiState {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let annual_bps = request.annual_bps.unwrap_or(state.config.model_annual_bps);
        state
            .model
            .advance_time(request.elapsed_seconds, annual_bps);
    });
    state_snapshot()
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_config() -> NnsNeuronManagerConfig {
    config_snapshot()
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_state() -> ApiState {
    state_snapshot()
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_plan_rebalance(pool_state: ApiTwoWeekPoolState) -> ApiRebalanceAction {
    plan_rebalance_impl(pool_state)
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_advance_model_time(request: AdvanceModelTimeRequest) -> ApiState {
    advance_model_time_impl(request)
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn debug_tick() -> DebugTickOutcome {
    scheduler::scheduler_tick_once().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_live_neuron_configuration() {
        assert_eq!(TWO_YEAR_NNS_NEURON_ID, 6_345_890_886_899_317_159);
        assert_eq!(
            CONTROLLER_CANISTER_PRINCIPAL_TEXT,
            "oae4c-3iaaa-aaaar-qb5qq-cai"
        );
    }

    #[test]
    fn rebalancer_stakes_more_when_target_increases() {
        let s = TwoWeekPoolState {
            target_staked_e8s: 150,
            active_staked_e8s: 100,
            pending_unwind_e8s: 0,
            pending_restake_e8s: 0,
        };
        assert_eq!(
            s.plan_rebalance(),
            RebalanceAction::StakeMore { amount_e8s: 50 }
        );
    }

    #[test]
    fn rebalancer_splits_when_target_decreases() {
        let s = TwoWeekPoolState {
            target_staked_e8s: 50,
            active_staked_e8s: 100,
            pending_unwind_e8s: 0,
            pending_restake_e8s: 0,
        };
        assert_eq!(
            s.plan_rebalance(),
            RebalanceAction::SplitAndDissolve { amount_e8s: 50 }
        );
    }

    #[test]
    fn pending_unwind_reduces_effective_active_stake() {
        let s = TwoWeekPoolState {
            target_staked_e8s: 100,
            active_staked_e8s: 150,
            pending_unwind_e8s: 50,
            pending_restake_e8s: 0,
        };
        assert_eq!(s.plan_rebalance(), RebalanceAction::None);
    }

    #[test]
    fn pending_restake_increases_effective_active_stake() {
        let s = TwoWeekPoolState {
            target_staked_e8s: 100,
            active_staked_e8s: 50,
            pending_unwind_e8s: 0,
            pending_restake_e8s: 50,
        };
        assert_eq!(s.plan_rebalance(), RebalanceAction::None);
    }

    #[test]
    fn simulated_fast_forward_accrues_and_disburses_maturity() {
        let mut m = NnsNeuronManagerModel::new(1_000_000_000, 500_000_000);
        m.advance_time(365 * SECONDS_PER_DAY, 1_000); // 10% APY in test model.
        assert_eq!(m.two_year_neuron.maturity_e8s, 100_000_000);
        assert_eq!(m.two_week_pool.maturity_e8s, 50_000_000);
        assert_eq!(m.disburse_two_year_maturity(), 100_000_000);
        assert_eq!(m.two_year_neuron.maturity_e8s, 0);
    }

    #[test]
    fn split_unwind_becomes_disbursable_after_two_weeks() {
        let mut m = NnsNeuronManagerModel::new(0, 1_000);
        let child = m.split_and_start_unwind(250).unwrap();
        assert_eq!(m.two_week_pool.principal_e8s, 750);
        assert_eq!(
            m.disburse_ready_unwind(child),
            Err(ManagerError::NeuronNotReady)
        );
        m.advance_time(TWO_WEEK_DISSOLVE_SECONDS, 0);
        assert_eq!(m.disburse_ready_unwind(child).unwrap(), 250);
        assert!(m.unwind_neurons.is_empty());
    }

    #[test]
    fn cancel_unwind_merges_principal_back_before_disbursement() {
        let mut m = NnsNeuronManagerModel::new(0, 1_000);
        let child = m.split_and_start_unwind(250).unwrap();
        m.advance_time(SECONDS_PER_DAY, 0);
        assert_eq!(m.cancel_unwind_and_merge_back(child).unwrap(), 250);
        assert_eq!(m.two_week_pool.principal_e8s, 1_000);
        assert!(m.unwind_neurons.is_empty());
    }

    #[test]
    fn cannot_split_more_than_pool_principal() {
        let mut m = NnsNeuronManagerModel::new(0, 100);
        assert_eq!(
            m.split_and_start_unwind(101),
            Err(ManagerError::SplitExceedsMainPool)
        );
    }

    #[test]
    fn canister_api_reports_known_config_and_state() {
        init(InitArgs::default());
        let config = debug_get_config();
        assert_eq!(config.two_year_nns_neuron_id, 6_345_890_886_899_317_159);
        assert_eq!(
            config.controller_canister_principal_text,
            "oae4c-3iaaa-aaaar-qb5qq-cai"
        );

        let state = debug_get_state();
        assert_eq!(state.two_year_neuron.neuron_id, TWO_YEAR_NNS_NEURON_ID);
        assert_eq!(state.two_week_pool_state.active_staked_e8s, 0);
    }

    #[test]
    fn canister_api_advances_model_time_and_plans_rebalance() {
        init(InitArgs::default());
        let state = debug_advance_model_time(AdvanceModelTimeRequest {
            elapsed_seconds: SECONDS_PER_DAY,
            annual_bps: Some(1_000),
        });
        assert_eq!(state.now_seconds, SECONDS_PER_DAY);

        let action = debug_plan_rebalance(ApiTwoWeekPoolState {
            target_staked_e8s: 150,
            active_staked_e8s: 100,
            pending_unwind_e8s: 0,
            pending_restake_e8s: 0,
        });
        assert_eq!(action, ApiRebalanceAction::StakeMore { amount_e8s: 50 });
        assert_eq!(debug_get_state().two_week_pool_state.target_staked_e8s, 150);
    }

    #[test]
    fn init_rejects_invalid_config() {
        assert_eq!(
            NnsNeuronManagerConfig::try_from(InitArgs {
                two_year_nns_neuron_id: 0,
                ..InitArgs::default()
            })
            .unwrap_err(),
            InitArgsError::ZeroTwoYearNeuronId
        );
        assert_eq!(
            NnsNeuronManagerConfig::try_from(InitArgs {
                controller_canister_principal_text: " ".to_string(),
                ..InitArgs::default()
            })
            .unwrap_err(),
            InitArgsError::EmptyControllerPrincipal
        );
        assert_eq!(
            NnsNeuronManagerConfig::try_from(InitArgs {
                model_annual_bps: MAX_MODEL_ANNUAL_BPS + 1,
                ..InitArgs::default()
            })
            .unwrap_err(),
            InitArgsError::ModelAnnualBpsTooHigh {
                bps: MAX_MODEL_ANNUAL_BPS + 1,
                max_bps: MAX_MODEL_ANNUAL_BPS
            }
        );
        assert_eq!(
            NnsNeuronManagerConfig::try_from(InitArgs {
                icp_index_principal_text: Some("not a principal".to_string()),
                ..InitArgs::default()
            })
            .unwrap_err(),
            InitArgsError::InvalidIcpIndexPrincipal {
                value: "not a principal".to_string()
            }
        );
    }

    #[test]
    fn init_accepts_local_sns_topology_principals() {
        let config = NnsNeuronManagerConfig::try_from(InitArgs {
            controller_canister_principal_text: "aaaaa-aa".to_string(),
            two_year_nns_neuron_id: 42,
            io_stream_manager_principal_text: Some("oae4c-3iaaa-aaaar-qb5qq-cai".to_string()),
            nns_governance_principal_text: Some("rrkah-fqaaa-aaaaa-aaaaq-cai".to_string()),
            icp_ledger_principal_text: Some("ryjl3-tyaaa-aaaaa-aaaba-cai".to_string()),
            icp_index_principal_text: Some("qhbym-qaaaa-aaaaa-aaafq-cai".to_string()),
            ..InitArgs::default()
        })
        .unwrap();

        assert_eq!(
            config.nns_governance_principal_text.as_deref(),
            Some("rrkah-fqaaa-aaaaa-aaaaq-cai")
        );
    }

    #[test]
    fn stable_state_round_trip_preserves_config_model_and_pool_state() {
        init(InitArgs {
            two_year_nns_neuron_id: 42,
            two_week_dissolve_seconds: 7 * SECONDS_PER_DAY,
            initial_two_year_principal_e8s: 1_000_000_000,
            initial_two_week_principal_e8s: 500_000_000,
            model_annual_bps: 5_000,
            io_stream_manager_principal_text: Some("oae4c-3iaaa-aaaar-qb5qq-cai".to_string()),
            two_year_maturity_memo: Some(100),
            two_week_maturity_memo: Some(200),
            principal_unwind_memo: Some(300),
            ..InitArgs::default()
        });
        debug_advance_model_time(AdvanceModelTimeRequest {
            elapsed_seconds: 30 * SECONDS_PER_DAY,
            annual_bps: None,
        });
        let action = debug_plan_rebalance(ApiTwoWeekPoolState {
            target_staked_e8s: 700_000_000,
            active_staked_e8s: 500_000_000,
            pending_unwind_e8s: 0,
            pending_restake_e8s: 0,
        });
        assert_eq!(
            action,
            ApiRebalanceAction::StakeMore {
                amount_e8s: 200_000_000
            }
        );
        let before_config = debug_get_config();
        let before_state = debug_get_state();
        let stable = export_stable_state_for_tests();

        init(InitArgs::default());
        assert_ne!(debug_get_state(), before_state);

        import_stable_state_for_tests(stable);
        assert_eq!(debug_get_config(), before_config);
        assert_eq!(debug_get_state(), before_state);
        assert!(debug_get_state().two_year_neuron.maturity_e8s > 0);
        assert_eq!(
            debug_get_state().two_week_pool_state.target_staked_e8s,
            700_000_000
        );
    }

    #[test]
    fn stable_state_round_trip_preserves_journal_and_scheduler_cursors() {
        init(InitArgs::default());
        debug_advance_model_time(AdvanceModelTimeRequest {
            elapsed_seconds: SECONDS_PER_DAY,
            annual_bps: Some(1_000),
        });
        let post_model = CANISTER_STATE.with(|cell| cell.borrow().model.clone());
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.operation_journal.push(NnsOperation::new(
                "two-year:1".to_string(),
                NnsOperationKind::TwoYearMaturityDisbursement,
                123,
                "two_year_maturity".to_string(),
                Some(post_model),
            ));
            state.scheduler_cursors.last_two_year_maturity_check_time = Some(10);
            state.scheduler_cursors.last_two_week_maturity_check_time = Some(20);
            state.scheduler_cursors.last_unwind_check_time = Some(30);
        });

        let stable = export_stable_state_for_tests();
        init(InitArgs::default());
        import_stable_state_for_tests(stable.clone());
        assert_eq!(
            export_stable_state_for_tests().operation_journal,
            stable.operation_journal
        );
        assert_eq!(
            export_stable_state_for_tests().scheduler_cursors,
            stable.scheduler_cursors
        );
    }

    #[test]
    fn scheduler_tick_does_not_mutate_model_state() {
        init(InitArgs::default());
        let before = export_stable_state_for_tests();
        let outcome = crate::scheduler::scheduler_tick_plan_only();
        assert_eq!(outcome.planned_pool_rebalances, 0);
        assert_eq!(export_stable_state_for_tests(), before);
    }
}

#[cfg(test)]
mod additional_manager_tests {
    use super::*;

    #[test]
    fn split_children_receive_unique_monotonic_ids() {
        let mut m = NnsNeuronManagerModel::new(0, 1_000);
        let a = m.split_and_start_unwind(100).unwrap();
        let b = m.split_and_start_unwind(200).unwrap();
        assert_ne!(a, b);
        assert!(b > a);
        assert_eq!(m.unwind_neurons.len(), 2);
        assert_eq!(m.two_week_pool.principal_e8s, 700);
    }

    #[test]
    fn disbursing_one_ready_unwind_leaves_other_children_intact() {
        let mut m = NnsNeuronManagerModel::new(0, 1_000);
        let a = m.split_and_start_unwind(100).unwrap();
        let b = m.split_and_start_unwind(200).unwrap();
        m.advance_time(TWO_WEEK_DISSOLVE_SECONDS, 0);
        assert_eq!(m.disburse_ready_unwind(a).unwrap(), 100);
        assert_eq!(
            m.unwind_neurons
                .iter()
                .map(|n| n.neuron_id)
                .collect::<Vec<_>>(),
            vec![b]
        );
        assert_eq!(m.disburse_ready_unwind(b).unwrap(), 200);
        assert!(m.unwind_neurons.is_empty());
    }

    #[test]
    fn unknown_unwind_ids_are_rejected() {
        let mut m = NnsNeuronManagerModel::new(0, 1_000);
        assert_eq!(
            m.cancel_unwind_and_merge_back(999),
            Err(ManagerError::UnknownUnwindNeuron)
        );
        assert_eq!(
            m.disburse_ready_unwind(999),
            Err(ManagerError::UnknownUnwindNeuron)
        );
    }

    #[test]
    fn dissolving_unwind_neurons_do_not_accrue_maturity() {
        let mut m = NnsNeuronManagerModel::new(0, 1_000_000);
        let child = m.split_and_start_unwind(500_000).unwrap();
        m.advance_time(365 * SECONDS_PER_DAY, 10_000);
        let unwind = m
            .unwind_neurons
            .iter()
            .find(|n| n.neuron_id == child)
            .unwrap();
        assert_eq!(unwind.maturity_e8s, 0);
        assert!(m.two_week_pool.maturity_e8s > 0);
    }

    #[test]
    fn cancel_after_ready_but_before_disburse_still_merges_back() {
        let mut m = NnsNeuronManagerModel::new(0, 1_000);
        let child = m.split_and_start_unwind(250).unwrap();
        m.advance_time(TWO_WEEK_DISSOLVE_SECONDS, 0);
        assert_eq!(m.cancel_unwind_and_merge_back(child).unwrap(), 250);
        assert_eq!(m.two_week_pool.principal_e8s, 1_000);
        assert!(m.unwind_neurons.is_empty());
    }

    #[test]
    fn stake_more_two_week_is_saturating() {
        let mut m = NnsNeuronManagerModel::new(0, u128::MAX - 1);
        m.stake_more_two_week(10);
        assert_eq!(m.two_week_pool.principal_e8s, u128::MAX);
    }

    #[test]
    fn pending_unwind_larger_than_active_saturates_to_zero_effective_active() {
        let s = TwoWeekPoolState {
            target_staked_e8s: 50,
            active_staked_e8s: 10,
            pending_unwind_e8s: 100,
            pending_restake_e8s: 0,
        };
        assert_eq!(
            s.plan_rebalance(),
            RebalanceAction::StakeMore { amount_e8s: 50 }
        );
    }

    #[test]
    fn lifecycle_plan_tracks_target_increase_and_decrease() {
        assert_eq!(
            TwoWeekPoolState {
                target_staked_e8s: 200,
                active_staked_e8s: 100,
                pending_unwind_e8s: 0,
                pending_restake_e8s: 0,
            }
            .plan_lifecycle(),
            TwoWeekPoolLifecyclePlan::TwoWeekPoolRestake { amount_e8s: 100 }
        );
        assert_eq!(
            TwoWeekPoolState {
                target_staked_e8s: 50,
                active_staked_e8s: 100,
                pending_unwind_e8s: 0,
                pending_restake_e8s: 0,
            }
            .plan_lifecycle(),
            TwoWeekPoolLifecyclePlan::TwoWeekPoolSplit { amount_e8s: 50 }
        );
    }

    #[test]
    fn cancel_before_readiness_plans_stop_then_merge_back() {
        let mut m = NnsNeuronManagerModel::new(0, 1_000);
        let child = m.split_and_start_unwind(250).unwrap();
        assert_eq!(
            m.cancel_unwind_plan(child).unwrap(),
            TwoWeekPoolLifecyclePlan::TwoWeekPoolStopDissolving { neuron_id: child }
        );
        assert_eq!(
            m.merge_back_plan(child).unwrap(),
            TwoWeekPoolLifecyclePlan::TwoWeekPoolMergeBack {
                neuron_id: child,
                amount_e8s: 250
            }
        );
    }

    #[test]
    fn ready_child_plans_principal_disbursement() {
        let mut m = NnsNeuronManagerModel::new(0, 1_000);
        let child = m.split_and_start_unwind(250).unwrap();
        assert_eq!(
            m.ready_unwind_disbursement_plan(child),
            Err(ManagerError::NeuronNotReady)
        );
        m.advance_time(TWO_WEEK_DISSOLVE_SECONDS, 0);
        assert_eq!(
            m.ready_unwind_disbursement_plan(child).unwrap(),
            TwoWeekPoolLifecyclePlan::TwoWeekUnwindPrincipalDisbursement {
                neuron_id: child,
                amount_e8s: 250
            }
        );
    }

    #[test]
    fn governance_errors_map_to_retryable_or_terminal_lifecycle_results() {
        assert!(matches!(
            TwoWeekPoolLifecycleResult::from_governance_error(
                &io_governance_types::NnsGovernanceError::TemporarilyUnavailable
            ),
            TwoWeekPoolLifecycleResult::Retryable { .. }
        ));
        assert!(matches!(
            TwoWeekPoolLifecycleResult::from_governance_error(
                &io_governance_types::NnsGovernanceError::NotAuthorized
            ),
            TwoWeekPoolLifecycleResult::Terminal { .. }
        ));
    }
}
