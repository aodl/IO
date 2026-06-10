pub mod clients;
pub mod scheduler;

use candid::{CandidType, Principal};
use io_production_wiring::ProductionWiringConfig;
use io_stable_schema::IO_NNS_NEURON_MANAGER_SCHEMA_VERSION;
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
    pub production_wiring: Option<ProductionWiringConfig>,
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
            production_wiring: None,
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
    pub production_wiring: Option<ProductionWiringConfig>,
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
    InvalidProductionWiring { message: String },
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
        if let Some(production_wiring) = &args.production_wiring {
            production_wiring
                .validate()
                .map_err(|err| InitArgsError::InvalidProductionWiring {
                    message: format!("{err:?}"),
                })?;
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
            production_wiring: args.production_wiring,
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

pub const NNS_NEURON_MANAGER_STABLE_SCHEMA_VERSION: u32 = IO_NNS_NEURON_MANAGER_SCHEMA_VERSION;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct VersionedStableState {
    pub schema_version: u32,
    pub state: StableState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StableMigrationError {
    UnsupportedFutureVersion {
        canister: &'static str,
        version: u32,
    },
    UnsupportedOldVersion {
        canister: &'static str,
        version: u32,
    },
    CorruptSnapshot {
        canister: &'static str,
        message: String,
    },
}

pub fn migrate_stable_state(
    snapshot: VersionedStableState,
) -> Result<StableState, StableMigrationError> {
    match snapshot.schema_version {
        0 => Ok(snapshot.state),
        NNS_NEURON_MANAGER_STABLE_SCHEMA_VERSION => Ok(snapshot.state),
        version if version > NNS_NEURON_MANAGER_STABLE_SCHEMA_VERSION => {
            Err(StableMigrationError::UnsupportedFutureVersion {
                canister: "io_nns_neuron_manager",
                version,
            })
        }
        version => Err(StableMigrationError::UnsupportedOldVersion {
            canister: "io_nns_neuron_manager",
            version,
        }),
    }
}

fn decode_stable_state_bytes(bytes: &[u8]) -> Result<StableState, StableMigrationError> {
    let versioned_err = match candid::decode_args::<(VersionedStableState,)>(bytes) {
        Ok((snapshot,)) => return migrate_stable_state(snapshot),
        Err(err) => err,
    };

    match candid::decode_args::<(StableState,)>(bytes) {
        Ok((state,)) => migrate_stable_state(VersionedStableState {
            schema_version: 0,
            state,
        }),
        Err(unversioned_err) => Err(StableMigrationError::CorruptSnapshot {
            canister: "io_nns_neuron_manager",
            message: format!(
                "failed to decode versioned stable state: {versioned_err}; failed to decode legacy unversioned stable state: {unversioned_err}"
            ),
        }),
    }
}

pub fn default_first_install_stable_state() -> StableState {
    CanisterState::default().into()
}

impl From<CanisterState> for StableState {
    fn from(state: CanisterState) -> Self {
        Self {
            config: state.config,
            model: state.model,
            two_week_pool_state: state.two_week_pool_state,
            operation_journal: state.operation_journal,
            scheduler_cursors: state.scheduler_cursors,
        }
    }
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

fn export_versioned_stable_state() -> VersionedStableState {
    VersionedStableState {
        schema_version: NNS_NEURON_MANAGER_STABLE_SCHEMA_VERSION,
        state: export_stable_state(),
    }
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
    ic_cdk::storage::stable_save((export_versioned_stable_state(),))
        .expect("failed to save io_nns_neuron_manager stable state");
}

#[cfg_attr(target_family = "wasm", ic_cdk::post_upgrade)]
pub fn post_upgrade() {
    let bytes = ic_cdk::stable::stable_bytes();
    let state = decode_stable_state_bytes(&bytes).expect(
        "io_nns_neuron_manager stable state is missing, corrupt, or unsupported during upgrade",
    );
    import_stable_state(state);
}

#[cfg(any(test, debug_assertions))]
pub fn export_stable_state_for_tests() -> StableState {
    export_stable_state()
}

#[cfg(any(test, debug_assertions))]
pub fn export_versioned_stable_state_for_tests() -> VersionedStableState {
    export_versioned_stable_state()
}

#[cfg(any(test, debug_assertions))]
pub fn import_stable_state_for_tests(state: StableState) {
    import_stable_state(state);
}

#[cfg(any(test, debug_assertions))]
pub fn migrate_stable_state_for_tests(
    snapshot: VersionedStableState,
) -> Result<StableState, StableMigrationError> {
    migrate_stable_state(snapshot)
}

#[cfg(any(test, debug_assertions))]
pub fn decode_stable_state_bytes_for_tests(
    bytes: &[u8],
) -> Result<StableState, StableMigrationError> {
    decode_stable_state_bytes(bytes)
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

    fn dry_run_wiring() -> ProductionWiringConfig {
        use io_production_wiring::{
            DeploymentTargets, FeePolicyWiring, IoLedgerRole, PrincipalWiring, ProtectedReferences,
            WiringMode, ICP_INDEX_PRINCIPAL, ICP_LEDGER_PRINCIPAL, ICP_TRANSFER_FEE_E8S,
            NNS_GOVERNANCE_PRINCIPAL, PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID,
            PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID, PROTECTED_IO_NEURON_OWNER_CANISTER,
            PROTECTED_IO_NNS_NEURON_ID,
        };

        ProductionWiringConfig {
            mode: WiringMode::DryRun,
            io_ledger_role: IoLedgerRole::FutureCanonicalSnsIo,
            fixture_marked: false,
            principals: PrincipalWiring {
                icp_ledger_principal_text: Some(ICP_LEDGER_PRINCIPAL.to_string()),
                icp_index_principal_text: Some(ICP_INDEX_PRINCIPAL.to_string()),
                nns_governance_principal_text: Some(NNS_GOVERNANCE_PRINCIPAL.to_string()),
                nns_ledger_principal_text: Some(ICP_LEDGER_PRINCIPAL.to_string()),
                nns_index_principal_text: Some(ICP_INDEX_PRINCIPAL.to_string()),
                sns_root_principal_text: Some("qaa6y-5yaaa-aaaaa-aaafa-cai".to_string()),
                sns_governance_principal_text: Some("r7inp-6aaaa-aaaaa-aaabq-cai".to_string()),
                sns_ledger_principal_text: Some("qjdve-lqaaa-aaaaa-aaaeq-cai".to_string()),
                sns_index_principal_text: Some("renrk-eyaaa-aaaaa-aaada-cai".to_string()),
                io_ledger_principal_text: Some("qjdve-lqaaa-aaaaa-aaaeq-cai".to_string()),
                io_index_principal_text: Some("renrk-eyaaa-aaaaa-aaada-cai".to_string()),
            },
            fee_policy: FeePolicyWiring {
                icp_transfer_fee_e8s: Some(ICP_TRANSFER_FEE_E8S),
                io_ledger_transfer_fee_e8s: Some(10_000),
                tiny_value_policy_max_fee_e8s: Some(1_000_000),
                allow_zero_fees_for_mock_or_local: false,
            },
            protected: ProtectedReferences {
                neuron_owner_canister_principal_text: Some(
                    PROTECTED_IO_NEURON_OWNER_CANISTER.to_string(),
                ),
                io_nns_neuron_id: Some(PROTECTED_IO_NNS_NEURON_ID),
            },
            deployment_targets: DeploymentTargets {
                io_stream_manager_principal_text: Some(
                    PRODUCTION_IO_STREAM_MANAGER_CANISTER_ID.to_string(),
                ),
                io_nns_neuron_manager_principal_text: Some(
                    PRODUCTION_IO_NNS_NEURON_MANAGER_CANISTER_ID.to_string(),
                ),
                mutation_target_principal_texts: Vec::new(),
                mutation_target_nns_neuron_ids: Vec::new(),
            },
        }
    }

    #[test]
    fn install_args_accept_valid_dry_run_wiring() {
        let config = NnsNeuronManagerConfig::try_from(InitArgs {
            production_wiring: Some(dry_run_wiring()),
            ..InitArgs::default()
        })
        .unwrap();

        assert!(config.production_wiring.is_some());
    }

    #[test]
    fn install_args_reject_invalid_production_planned_wiring() {
        let mut wiring = dry_run_wiring();
        wiring.mode = io_production_wiring::WiringMode::ProductionPlanned;
        wiring.fee_policy.io_ledger_transfer_fee_e8s = Some(0);

        assert!(matches!(
            NnsNeuronManagerConfig::try_from(InitArgs {
                production_wiring: Some(wiring),
                ..InitArgs::default()
            })
            .unwrap_err(),
            InitArgsError::InvalidProductionWiring { .. }
        ));
    }

    #[test]
    fn default_install_args_do_not_enable_production_wiring() {
        let config = NnsNeuronManagerConfig::try_from(InitArgs::default()).unwrap();

        assert!(config.production_wiring.is_none());
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

    fn pending_lifecycle_fixture() -> StableState {
        init(InitArgs::default());
        debug_advance_model_time(AdvanceModelTimeRequest {
            elapsed_seconds: SECONDS_PER_DAY,
            annual_bps: Some(1_000),
        });
        let post_model = CANISTER_STATE.with(|cell| cell.borrow().model.clone());
        CANISTER_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            let mut op = NnsOperation::new(
                "two-week-unwind:42".to_string(),
                NnsOperationKind::TwoWeekUnwindPrincipalDisbursement,
                42_000,
                "principal_unwind".to_string(),
                Some(post_model),
            );
            op.phase = NnsOperationPhase::FailedRetryable;
            op.retry_count = 3;
            op.last_error = Some("transient governance error".to_string());
            op.icp_transfer_status = NnsTransferStatus::FailedRetryable;
            state.operation_journal.push(op);
            state.scheduler_cursors.last_two_year_maturity_check_time = Some(11);
            state.scheduler_cursors.last_two_week_maturity_check_time = Some(12);
            state.scheduler_cursors.last_unwind_check_time = Some(13);
        });
        export_stable_state_for_tests()
    }

    #[test]
    fn nns_neuron_manager_migrates_previous_stable_fixture() {
        let fixture = pending_lifecycle_fixture();
        let migrated = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: 0,
            state: fixture.clone(),
        })
        .unwrap();

        assert_eq!(migrated, fixture);
        assert!(migrated.config.production_wiring.is_none());
    }

    #[test]
    fn nns_neuron_manager_decodes_legacy_unversioned_stable_root() {
        let fixture = pending_lifecycle_fixture();
        let bytes = candid::encode_args((fixture.clone(),)).unwrap();
        let migrated = decode_stable_state_bytes_for_tests(&bytes).unwrap();

        assert_eq!(migrated, fixture);
        assert!(migrated.config.production_wiring.is_none());
    }

    #[test]
    fn nns_neuron_manager_current_fixture_round_trips_unchanged() {
        let fixture = pending_lifecycle_fixture();

        assert_eq!(
            migrate_stable_state_for_tests(VersionedStableState {
                schema_version: NNS_NEURON_MANAGER_STABLE_SCHEMA_VERSION,
                state: fixture.clone(),
            })
            .unwrap(),
            fixture
        );
    }

    #[test]
    fn nns_neuron_manager_rejects_future_schema_version() {
        let err = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: NNS_NEURON_MANAGER_STABLE_SCHEMA_VERSION + 1,
            state: default_first_install_stable_state(),
        })
        .unwrap_err();

        assert!(matches!(
            err,
            StableMigrationError::UnsupportedFutureVersion {
                canister: "io_nns_neuron_manager",
                ..
            }
        ));
    }

    #[test]
    fn nns_neuron_manager_rejects_corrupt_stable_fixture() {
        let decoded = decode_stable_state_bytes_for_tests(b"not candid stable state");

        assert!(decoded.is_err());
    }

    #[test]
    fn nns_neuron_manager_empty_first_install_state_defaults_safely() {
        let stable = default_first_install_stable_state();

        assert!(stable.config.production_wiring.is_none());
        assert_eq!(
            stable.config.controller_canister_principal_text,
            CONTROLLER_CANISTER_PRINCIPAL_TEXT
        );
        assert_eq!(stable.config.two_year_nns_neuron_id, TWO_YEAR_NNS_NEURON_ID);
        assert!(stable.operation_journal.is_empty());
    }

    #[test]
    fn nns_neuron_manager_preserves_pending_lifecycle_journal() {
        let migrated = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: 0,
            state: pending_lifecycle_fixture(),
        })
        .unwrap();
        let op = migrated.operation_journal.first().unwrap();

        assert_eq!(
            op.kind,
            NnsOperationKind::TwoWeekUnwindPrincipalDisbursement
        );
        assert_eq!(op.phase, NnsOperationPhase::FailedRetryable);
        assert_eq!(op.retry_count, 3);
        assert_eq!(op.icp_transfer_status, NnsTransferStatus::FailedRetryable);
        assert_eq!(
            migrated.scheduler_cursors.last_two_year_maturity_check_time,
            Some(11)
        );
        assert_eq!(
            migrated.scheduler_cursors.last_two_week_maturity_check_time,
            Some(12)
        );
        assert_eq!(migrated.scheduler_cursors.last_unwind_check_time, Some(13));
    }

    #[test]
    fn nns_neuron_manager_protected_references_remain_protected_only() {
        let migrated = migrate_stable_state_for_tests(VersionedStableState {
            schema_version: 0,
            state: pending_lifecycle_fixture(),
        })
        .unwrap();

        assert_eq!(
            migrated.config.controller_canister_principal_text,
            CONTROLLER_CANISTER_PRINCIPAL_TEXT
        );
        assert_eq!(
            migrated.config.two_year_nns_neuron_id,
            TWO_YEAR_NNS_NEURON_ID
        );
        assert!(migrated
            .config
            .production_wiring
            .as_ref()
            .is_none_or(|wiring| wiring
                .deployment_targets
                .mutation_target_principal_texts
                .is_empty()
                && wiring
                    .deployment_targets
                    .mutation_target_nns_neuron_ids
                    .is_empty()));
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
