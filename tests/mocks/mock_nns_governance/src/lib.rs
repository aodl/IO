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
}

thread_local! {
    static STATE: RefCell<GovernanceState> = const { RefCell::new(GovernanceState { now_seconds: 0, next_neuron_id: 10_000, neurons: Vec::new() }) };
}

fn neuron_mut(state: &mut GovernanceState, id: u64) -> Result<&mut MockNeuron, String> {
    state
        .neurons
        .iter_mut()
        .find(|n| n.neuron_id == id)
        .ok_or_else(|| "unknown neuron".to_string())
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
pub fn debug_split(args: NeuronAmountArgs) -> Result<u64, String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let child_id = state.next_neuron_id;
        state.next_neuron_id = state.next_neuron_id.saturating_add(1);
        let dissolve_delay_seconds = {
            let source = neuron_mut(&mut state, args.neuron_id)?;
            if source.principal_e8s < args.amount_e8s {
                return Err("split exceeds principal".to_string());
            }
            source.principal_e8s -= args.amount_e8s;
            source.dissolve_delay_seconds
        };
        state.neurons.push(MockNeuron {
            neuron_id: child_id,
            principal_e8s: args.amount_e8s,
            maturity_e8s: 0,
            dissolve_delay_seconds,
            is_dissolving: false,
            dissolve_started_at_seconds: None,
        });
        Ok(child_id)
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_start_dissolving(args: NeuronIdArgs) -> Result<(), String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let now = state.now_seconds;
        let neuron = neuron_mut(&mut state, args.neuron_id)?;
        neuron.is_dissolving = true;
        neuron.dissolve_started_at_seconds = Some(now);
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_stop_dissolving(args: NeuronIdArgs) -> Result<(), String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let neuron = neuron_mut(&mut state, args.neuron_id)?;
        neuron.is_dissolving = false;
        neuron.dissolve_started_at_seconds = None;
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_merge(args: NeuronAmountArgs) -> Result<(), String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let amount = {
            let source = neuron_mut(&mut state, args.neuron_id)?;
            source.principal_e8s.min(args.amount_e8s)
        };
        let target = state
            .neurons
            .first_mut()
            .ok_or_else(|| "missing target neuron".to_string())?;
        target.principal_e8s = target.principal_e8s.saturating_add(amount);
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_disburse_principal(args: NeuronIdArgs) -> Result<u128, String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let now = state.now_seconds;
        let index = state
            .neurons
            .iter()
            .position(|n| n.neuron_id == args.neuron_id)
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
