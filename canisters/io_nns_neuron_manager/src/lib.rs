use candid::CandidType;
use serde::Deserialize;
use std::cell::RefCell;

pub const TWO_YEAR_NNS_NEURON_ID: u64 = 6_345_890_886_899_317_159;
pub const CONTROLLER_CANISTER_PRINCIPAL_TEXT: &str = "oae4c-3iaaa-aaaar-qb5qq-cai";
pub const SECONDS_PER_DAY: u64 = 86_400;
pub const TWO_WEEK_DISSOLVE_SECONDS: u64 = 14 * SECONDS_PER_DAY;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagedNeuronKind {
    TwoYearProtocol,
    TwoWeekPooled,
    TwoWeekUnwind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwoWeekPoolState {
    pub target_staked_e8s: u128,
    pub active_staked_e8s: u128,
    pub pending_unwind_e8s: u128,
    pub pending_restake_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RebalanceAction {
    None,
    StakeMore { amount_e8s: u128 },
    SplitAndDissolve { amount_e8s: u128 },
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManagerError {
    SplitExceedsMainPool,
    UnknownUnwindNeuron,
    NeuronNotReady,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NnsNeuronManagerModel {
    pub now_seconds: u64,
    pub next_neuron_id: u64,
    pub two_year_neuron: SimulatedNnsNeuron,
    pub two_week_pool: SimulatedNnsNeuron,
    pub unwind_neurons: Vec<SimulatedNnsNeuron>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsNeuronManagerConfig {
    pub controller_canister_principal_text: String,
    pub two_year_nns_neuron_id: u64,
    pub two_week_dissolve_seconds: u64,
    pub initial_two_year_principal_e8s: u128,
    pub initial_two_week_principal_e8s: u128,
    pub model_annual_bps: u128,
}

impl Default for NnsNeuronManagerConfig {
    fn default() -> Self {
        Self {
            controller_canister_principal_text: CONTROLLER_CANISTER_PRINCIPAL_TEXT.to_string(),
            two_year_nns_neuron_id: TWO_YEAR_NNS_NEURON_ID,
            two_week_dissolve_seconds: TWO_WEEK_DISSOLVE_SECONDS,
            initial_two_year_principal_e8s: 0,
            initial_two_week_principal_e8s: 0,
            model_annual_bps: 0,
        }
    }
}

#[cfg_attr(not(any(test, debug_assertions)), allow(dead_code))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct CanisterState {
    config: NnsNeuronManagerConfig,
    model: NnsNeuronManagerModel,
    two_week_pool_state: TwoWeekPoolState,
}

impl CanisterState {
    fn new(config: NnsNeuronManagerConfig) -> Self {
        let model = NnsNeuronManagerModel::new(
            config.initial_two_year_principal_e8s,
            config.initial_two_week_principal_e8s,
        );
        Self {
            config,
            two_week_pool_state: TwoWeekPoolState {
                target_staked_e8s: 0,
                active_staked_e8s: model.two_week_pool.principal_e8s,
                pending_unwind_e8s: 0,
                pending_restake_e8s: 0,
            },
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

impl NnsNeuronManagerModel {
    pub fn new(two_year_principal_e8s: u128, two_week_principal_e8s: u128) -> Self {
        Self {
            now_seconds: 0,
            next_neuron_id: 10_000,
            two_year_neuron: SimulatedNnsNeuron::new(
                TWO_YEAR_NNS_NEURON_ID,
                ManagedNeuronKind::TwoYearProtocol,
                two_year_principal_e8s,
                2 * 365 * SECONDS_PER_DAY,
            ),
            two_week_pool: SimulatedNnsNeuron::new(
                2,
                ManagedNeuronKind::TwoWeekPooled,
                two_week_principal_e8s,
                TWO_WEEK_DISSOLVE_SECONDS,
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
pub fn init() {
    CANISTER_STATE.with(|cell| {
        *cell.borrow_mut() = CanisterState::default();
    });
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
        init();
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
        init();
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
}
