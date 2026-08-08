use candid::CandidType;
use serde::Deserialize;
use std::cell::RefCell;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MockNeuron {
    pub neuron_id: u64,
    pub principal_e8s: u128,
    pub maturity_e8s: u128,
    pub dissolve_delay_seconds: u64,
    pub is_dissolving: bool,
    pub dissolve_started_at_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CreateNeuronArgs {
    pub neuron_id: u64,
    pub principal_e8s: u128,
    pub dissolve_delay_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NeuronAmountArgs {
    pub neuron_id: u64,
    pub amount_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NeuronIdArgs {
    pub neuron_id: u64,
}

#[derive(Default)]
struct GovernanceState {
    now_seconds: u64,
    next_neuron_id: u64,
    neurons: Vec<MockNeuron>,
    two_week_target: Option<SetTargetArgs>,
    maturity_preparation: Option<PrepareTwoWeekMaturityArgs>,
    backing_readiness: Option<io_receipt_types::TwoWeekBackingReadiness>,
}

thread_local! {
    static STATE: RefCell<GovernanceState> = const { RefCell::new(GovernanceState { now_seconds: 0, next_neuron_id: 10_000, neurons: Vec::new(), two_week_target: None, maturity_preparation: None, backing_readiness: None }) };
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SetTargetArgs {
    pub target_e8s: u128,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum TargetStatus {
    UnderTarget,
    AtTarget,
    AtTargetWithinUnwindTolerance,
    OverTarget,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct PrepareTwoWeekMaturityArgs {
    pub entitlement_batch_generation: u64,
    pub target_e8s: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum PreparedMaturityProgress {
    Observed,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum NnsError {
    Invalid(String),
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn prepare_two_week_maturity(
    args: PrepareTwoWeekMaturityArgs,
) -> Result<PreparedMaturityProgress, NnsError> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let target = SetTargetArgs {
            generation: args.entitlement_batch_generation,
            target_e8s: args.target_e8s,
        };
        if let Some(existing) = &state.two_week_target {
            if existing.generation == target.generation && existing != &target {
                return Err(NnsError::Invalid("generation conflicts with target".into()));
            }
            if existing.generation != target.generation
                && existing.generation.checked_add(1) != Some(target.generation)
            {
                return Err(NnsError::Invalid(
                    "target generation is not sequential".into(),
                ));
            }
        }
        state.two_week_target = Some(target);
        if state
            .two_week_target
            .as_ref()
            .map(|target| target.generation)
            != Some(args.entitlement_batch_generation)
        {
            return Err(NnsError::Invalid(
                "maturity preparation lacks the matching target generation".into(),
            ));
        }
        if let Some(existing) = &state.maturity_preparation {
            if existing == &args {
                return Ok(PreparedMaturityProgress::Observed);
            }
            if existing.entitlement_batch_generation.checked_add(1)
                != Some(args.entitlement_batch_generation)
            {
                return Err(NnsError::Invalid(
                    "maturity preparation generation is not sequential".into(),
                ));
            }
        }
        state.maturity_preparation = Some(args);
        Ok(PreparedMaturityProgress::Observed)
    })
}

#[derive(CandidType, Deserialize)]
pub struct ObserveReadinessArgs {
    target_e8s: u128,
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn observe_two_week_backing_readiness(
    args: ObserveReadinessArgs,
) -> Result<io_receipt_types::TwoWeekBackingReadiness, NnsError> {
    let _ = args.target_e8s;
    Ok(STATE.with(|cell| {
        cell.borrow().backing_readiness.clone().unwrap_or(
            io_receipt_types::TwoWeekBackingReadiness::Ready {
                target_status: io_receipt_types::BackingTargetStatus::AtTarget,
                ordinary_maturity_e8s: 200_000_000,
                retained_maturity_e8s: 80_000_000,
                liquid_maturity_e8s: 120_000_000,
                minimum_disbursement_e8s: 100_000_000,
            },
        )
    }))
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_backing_readiness(readiness: io_receipt_types::TwoWeekBackingReadiness) {
    STATE.with(|cell| cell.borrow_mut().backing_readiness = Some(readiness));
}

fn neuron_mut(state: &mut GovernanceState, id: u64) -> Result<&mut MockNeuron, String> {
    state
        .neurons
        .iter_mut()
        .find(|n| n.neuron_id == id)
        .ok_or_else(|| format!("unknown neuron {id}"))
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_create_neuron(args: CreateNeuronArgs) -> u64 {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.neurons.push(MockNeuron {
            neuron_id: args.neuron_id,
            principal_e8s: args.principal_e8s,
            maturity_e8s: 0,
            dissolve_delay_seconds: args.dissolve_delay_seconds,
            is_dissolving: false,
            dissolve_started_at_seconds: None,
        });
        args.neuron_id
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_add_maturity(args: NeuronAmountArgs) -> Result<u128, String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let neuron = neuron_mut(&mut state, args.neuron_id)?;
        neuron.maturity_e8s = neuron.maturity_e8s.saturating_add(args.amount_e8s);
        Ok(neuron.maturity_e8s)
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_advance_time(seconds: u64) -> u64 {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.now_seconds = state.now_seconds.saturating_add(seconds);
        state.now_seconds
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_disburse_maturity(args: NeuronIdArgs) -> Result<u128, String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let neuron = neuron_mut(&mut state, args.neuron_id)?;
        let amount = neuron.maturity_e8s;
        neuron.maturity_e8s = 0;
        Ok(amount)
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_split(neuron_id: u64, amount_e8s: u128) -> Result<u64, String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let child_id = state.next_neuron_id;
        state.next_neuron_id = state.next_neuron_id.saturating_add(1);
        let dissolve_delay_seconds = {
            let source = neuron_mut(&mut state, neuron_id)?;
            if source.principal_e8s < amount_e8s {
                return Err("split exceeds principal".to_string());
            }
            source.principal_e8s -= amount_e8s;
            source.dissolve_delay_seconds
        };
        state.neurons.push(MockNeuron {
            neuron_id: child_id,
            principal_e8s: amount_e8s,
            maturity_e8s: 0,
            dissolve_delay_seconds,
            is_dissolving: false,
            dissolve_started_at_seconds: None,
        });
        Ok(child_id)
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_start_dissolving(neuron_id: u64) -> Result<(), String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let now = state.now_seconds;
        let neuron = neuron_mut(&mut state, neuron_id)?;
        neuron.is_dissolving = true;
        neuron.dissolve_started_at_seconds = Some(now);
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_stop_dissolving(neuron_id: u64) -> Result<(), String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let neuron = neuron_mut(&mut state, neuron_id)?;
        neuron.is_dissolving = false;
        neuron.dissolve_started_at_seconds = None;
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_merge(neuron_id: u64, amount_e8s: u128) -> Result<(), String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let amount = {
            let source = neuron_mut(&mut state, neuron_id)?;
            source.principal_e8s.min(amount_e8s)
        };
        let target = neuron_mut(&mut state, 2)?;
        target.principal_e8s = target.principal_e8s.saturating_add(amount);
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_disburse_principal(neuron_id: u64) -> Result<u128, String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let now = state.now_seconds;
        let index = state
            .neurons
            .iter()
            .position(|n| n.neuron_id == neuron_id)
            .ok_or_else(|| "unknown neuron".to_string())?;
        let neuron = &state.neurons[index];
        let ready = neuron.is_dissolving
            && neuron
                .dissolve_started_at_seconds
                .map(|started| now.saturating_sub(started) >= neuron.dissolve_delay_seconds)
                .unwrap_or(false);
        if !ready {
            return Err("neuron not ready".to_string());
        }
        Ok(state.neurons.remove(index).principal_e8s)
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_neuron(args: NeuronIdArgs) -> Option<MockNeuron> {
    STATE.with(|cell| {
        cell.borrow()
            .neurons
            .iter()
            .find(|n| n.neuron_id == args.neuron_id)
            .cloned()
    })
}
