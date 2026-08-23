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
    reconcile_calls: u64,
    get_full_neuron_calls: u64,
    pooled_principal_e8s: u128,
    claim_asset_observation_calls: u64,
    pool_policy_observation_calls: u64,
    pool_policy_valid: bool,
}

thread_local! {
    static STATE: RefCell<GovernanceState> = const { RefCell::new(GovernanceState { now_seconds: 0, next_neuron_id: 10_000, neurons: Vec::new(), two_week_target: None, maturity_preparation: None, reconcile_calls: 0, get_full_neuron_calls: 0, pooled_principal_e8s: 0, claim_asset_observation_calls: 0, pool_policy_observation_calls: 0, pool_policy_valid: true }) };
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
pub fn observe_claim_assets() -> Result<io_nns_types::backing::ClaimAssetObservation, NnsError> {
    use io_accounts::Account;
    use io_nns_types::backing::{ClaimAssetObservation, ParentAssetObservation};
    let owner = ic_cdk::api::canister_self();
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.claim_asset_observation_calls = state.claim_asset_observation_calls.saturating_add(1);
    });
    let pool_staking_account = Account {
        owner,
        subaccount: Some(vec![2; 32]),
    };
    let pooled_principal_e8s = STATE.with(|cell| cell.borrow().pooled_principal_e8s);
    Ok(ClaimAssetObservation {
        parent: (pooled_principal_e8s > 0).then(|| ParentAssetObservation {
            neuron_id: 1,
            staking_account: pool_staking_account.clone(),
            physical_principal_e8s: pooled_principal_e8s,
        }),
        pool_staking_account,
        minimum_parent_stake_e8s: 1,
        pooled_parent_principal_e8s: pooled_principal_e8s,
        live_cohorts: Vec::new(),
        live_child_physical_principal_e8s: 0,
        live_child_net_backing_e8s: 0,
        live_child_committed_fee_liability_e8s: 0,
        transit_backing_e8s: 0,
        active_operation_sequence: 0,
        last_completed_pool_operation_sequence: None,
        control_epoch: 1,
        fingerprint: vec![42; 32],
        oldest_ready_at_seconds: None,
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn observe_pool_policy() -> Result<io_nns_types::backing::PoolPolicyObservation, NnsError> {
    use io_nns_types::backing::{
        FollowPolicy, ParentPolicyObservation, PoolPolicyObservation, POOLED_PARENT_DELAY_SECONDS,
    };
    let pooled = STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.pool_policy_observation_calls = state.pool_policy_observation_calls.saturating_add(1);
        if !state.pool_policy_valid {
            return Err(NnsError::Invalid("mock pooled-parent policy drift".into()));
        }
        Ok(state.pooled_principal_e8s > 0)
    })?;
    Ok(PoolPolicyObservation {
        parent: pooled.then_some(ParentPolicyObservation {
            neuron_id: 1,
            dissolve_delay_seconds: POOLED_PARENT_DELAY_SECONDS,
            auto_stake_maturity: false,
            follow_policy: FollowPolicy {
                followee_neuron_id: 2,
            },
            voting_power_refreshed_at_seconds: 1,
        }),
        control_epoch: 1,
        active_operation_sequence: 0,
        fingerprint: vec![43; 32],
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ObservationCallCounters {
    pub claim_assets: u64,
    pub pool_policy: u64,
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_observation_call_counters() -> ObservationCallCounters {
    STATE.with(|cell| {
        let state = cell.borrow();
        ObservationCallCounters {
            claim_assets: state.claim_asset_observation_calls,
            pool_policy: state.pool_policy_observation_calls,
        }
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_pool_policy_valid(valid: bool) {
    STATE.with(|cell| cell.borrow_mut().pool_policy_valid = valid);
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_pooled_principal(pooled_principal_e8s: u128) {
    STATE.with(|cell| cell.borrow_mut().pooled_principal_e8s = pooled_principal_e8s);
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn prepare_pool_reconciliation(
    args: io_nns_types::backing::PreparePoolReconciliationArgs,
) -> Result<io_nns_types::backing::PoolProgress, NnsError> {
    use io_nns_types::backing::{PoolProgress, PoolReconciliationAction};
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.reconcile_calls = state.reconcile_calls.saturating_add(1);
    });
    if args.generation == 0
        || args.snapshot_fingerprint != vec![42; 32]
        || !matches!(args.action, PoolReconciliationAction::Hold)
    {
        return Err(NnsError::Invalid(
            "mock accepts only a bounded absent-parent hold".into(),
        ));
    }
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.two_week_target = Some(SetTargetArgs {
            target_e8s: args.target_e8s,
            generation: args.generation,
        });
    });
    Ok(PoolProgress::Held { principal_e8s: 0 })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn prepare_two_week_maturity(
    args: PrepareTwoWeekMaturityArgs,
) -> Result<PreparedMaturityProgress, NnsError> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if state
            .two_week_target
            .as_ref()
            .map(|target| target.target_e8s)
            != Some(args.target_e8s)
        {
            return Err(NnsError::Invalid(
                "maturity preparation lacks the matching reconciled target".into(),
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

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_reconcile_call_count() -> u64 {
    STATE.with(|cell| cell.borrow().reconcile_calls)
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct GovernanceError {
    pub error_type: i32,
    pub error_message: String,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize)]
pub struct NnsNeuronId {
    pub id: u64,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize)]
pub enum NnsDissolveState {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct NnsAccount {
    pub owner: Option<candid::Principal>,
    pub subaccount: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct MaturityDisbursement {
    pub amount_e8s: Option<u64>,
    pub timestamp_of_disbursement_seconds: Option<u64>,
    pub finalize_disbursement_timestamp_seconds: Option<u64>,
    pub account_to_disburse_to: Option<NnsAccount>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct FullNeuron {
    pub id: Option<NnsNeuronId>,
    pub account: Vec<u8>,
    pub cached_neuron_stake_e8s: u64,
    pub maturity_e8s_equivalent: u64,
    pub staked_maturity_e8s_equivalent: Option<u64>,
    pub auto_stake_maturity: Option<bool>,
    pub maturity_disbursements_in_progress: Option<Vec<MaturityDisbursement>>,
    pub dissolve_state: Option<NnsDissolveState>,
    pub followees: Vec<(i32, NnsFollowees)>,
    pub voting_power_refreshed_timestamp_seconds: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct NnsFollowees {
    pub followees: Vec<NnsNeuronId>,
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn get_full_neuron(neuron_id: u64) -> Result<FullNeuron, GovernanceError> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.get_full_neuron_calls += 1;
        let neuron = state
            .neurons
            .iter()
            .find(|neuron| neuron.neuron_id == neuron_id)
            .ok_or_else(|| GovernanceError {
                error_type: 1,
                error_message: "mock neuron not found".into(),
            })?;
        let mut account = [0_u8; 32];
        account[24..].copy_from_slice(&neuron_id.to_be_bytes());
        Ok(FullNeuron {
            id: Some(NnsNeuronId { id: neuron_id }),
            account: account.to_vec(),
            cached_neuron_stake_e8s: neuron.principal_e8s.try_into().unwrap_or(u64::MAX),
            maturity_e8s_equivalent: neuron.maturity_e8s.try_into().unwrap_or(u64::MAX),
            staked_maturity_e8s_equivalent: Some(0),
            auto_stake_maturity: Some(false),
            maturity_disbursements_in_progress: Some(Vec::new()),
            dissolve_state: Some(if neuron.is_dissolving {
                NnsDissolveState::WhenDissolvedTimestampSeconds(
                    neuron.dissolve_started_at_seconds.unwrap_or_default(),
                )
            } else {
                NnsDissolveState::DissolveDelaySeconds(neuron.dissolve_delay_seconds)
            }),
            followees: [0, 4, 14]
                .into_iter()
                .map(|topic| {
                    (
                        topic,
                        NnsFollowees {
                            followees: vec![NnsNeuronId { id: 43 }],
                        },
                    )
                })
                .collect(),
            voting_power_refreshed_timestamp_seconds: Some(1),
        })
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_full_neuron_call_count() -> u64 {
    STATE.with(|cell| cell.borrow().get_full_neuron_calls)
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
