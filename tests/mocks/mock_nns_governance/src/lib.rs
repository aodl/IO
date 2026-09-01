use candid::CandidType;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::cell::RefCell;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MockNeuron {
    pub neuron_id: u64,
    pub account: Vec<u8>,
    pub principal_e8s: u128,
    pub maturity_e8s: u128,
    pub dissolve_delay_seconds: u64,
    pub is_dissolving: bool,
    pub dissolve_started_at_seconds: Option<u64>,
    pub followee_id: Option<u64>,
    pub pending_refresh_credit_e8s: u128,
    pub maturity_disbursements: Vec<MaturityDisbursement>,
    pub voting_power_refreshed_timestamp_seconds: u64,
    pub controller: candid::Principal,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CreateNeuronArgs {
    pub neuron_id: u64,
    pub principal_e8s: u128,
    pub dissolve_delay_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CreateSplitChildArgs {
    pub neuron_id: u64,
    pub principal_e8s: u128,
    pub dissolve_delay_seconds: u64,
    pub memo: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CreateStakingNeuronArgs {
    pub neuron_id: u64,
    pub principal_e8s: u128,
    pub dissolve_delay_seconds: u64,
    pub memo: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NeuronAmountArgs {
    pub neuron_id: u64,
    pub amount_e8s: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SetNextDisburseBlockArgs {
    pub block_index: u64,
}

fn default_neuron_account(neuron_id: u64) -> Vec<u8> {
    let mut account = vec![0_u8; 32];
    account[24..].copy_from_slice(&neuron_id.to_be_bytes());
    account
}

fn split_child_subaccount(controller: candid::Principal, memo: u64) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update([b"split-neuron".len() as u8]);
    hasher.update(b"split-neuron");
    hasher.update(controller.as_slice());
    hasher.update(memo.to_be_bytes());
    hasher.finalize().to_vec()
}

fn staking_subaccount(controller: candid::Principal, memo: u64) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update([b"neuron-stake".len() as u8]);
    hasher.update(b"neuron-stake");
    hasher.update(controller.as_slice());
    hasher.update(memo.to_be_bytes());
    hasher.finalize().to_vec()
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NeuronIdArgs {
    pub neuron_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SetFolloweeArgs {
    pub neuron_id: u64,
    pub followee: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SetVotingPowerTimestampArgs {
    pub neuron_id: u64,
    pub timestamp_seconds: u64,
}

#[derive(Default)]
struct GovernanceState {
    now_seconds: u64,
    next_neuron_id: u64,
    neurons: Vec<MockNeuron>,
    two_week_target: Option<SetTargetArgs>,
    maturity_preparation: Option<PrepareTwoWeekMaturityArgs>,
    reconcile_calls: u64,
    reconcile_rejections_remaining: u64,
    get_full_neuron_calls: u64,
    pooled_principal_e8s: u128,
    anchor_available_e8s: u128,
    live_cohort: Option<io_nns_types::backing::CohortObservation>,
    claim_asset_observation_calls: u64,
    pool_policy_observation_calls: u64,
    pool_policy_valid: bool,
    command_controls: Vec<CommandControl>,
    command_calls: GovernanceCommandCounters,
    split_trap_before_effect: bool,
    transaction_fee_e8s: u64,
    next_disburse_block: Option<u64>,
}

thread_local! {
    static STATE: RefCell<GovernanceState> = const { RefCell::new(GovernanceState { now_seconds: 0, next_neuron_id: 10_000, neurons: Vec::new(), two_week_target: None, maturity_preparation: None, reconcile_calls: 0, reconcile_rejections_remaining: 0, get_full_neuron_calls: 0, pooled_principal_e8s: 0, anchor_available_e8s: io_nns_types::backing::DYNAMIC_ANCHOR_TARGET_E8S, live_cohort: None, claim_asset_observation_calls: 0, pool_policy_observation_calls: 0, pool_policy_valid: true, command_controls: Vec::new(), command_calls: GovernanceCommandCounters::ZERO, split_trap_before_effect: false, transaction_fee_e8s: 10_000, next_disburse_block: None }) };
}

fn canonical_now_seconds(state: &GovernanceState) -> u64 {
    if state.now_seconds == 0 {
        ic_cdk::api::time() / 1_000_000_000
    } else {
        state.now_seconds
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ControlledCommand {
    Split,
    Disburse,
    DisburseMaturity,
    ClaimOrRefresh,
    IncreaseDissolveDelay,
    SetFollowing,
    StartDissolving,
    Merge,
    RefreshVotingPower,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct CommandControl {
    pub command: ControlledCommand,
    pub reject_before_effect: u64,
    pub malformed_after_effect: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, CandidType, Deserialize)]
pub struct GovernanceCommandCounters {
    pub split: u64,
    pub disburse: u64,
    pub disburse_maturity: u64,
    pub claim_or_refresh: u64,
    pub increase_dissolve_delay: u64,
    pub set_following: u64,
    pub start_dissolving: u64,
    pub merge: u64,
    pub refresh_voting_power: u64,
}

impl GovernanceCommandCounters {
    const ZERO: Self = Self {
        split: 0,
        disburse: 0,
        disburse_maturity: 0,
        claim_or_refresh: 0,
        increase_dissolve_delay: 0,
        set_following: 0,
        start_dissolving: 0,
        merge: 0,
        refresh_voting_power: 0,
    };

    fn increment(&mut self, command: ControlledCommand) {
        let counter = match command {
            ControlledCommand::Split => &mut self.split,
            ControlledCommand::Disburse => &mut self.disburse,
            ControlledCommand::DisburseMaturity => &mut self.disburse_maturity,
            ControlledCommand::ClaimOrRefresh => &mut self.claim_or_refresh,
            ControlledCommand::IncreaseDissolveDelay => &mut self.increase_dissolve_delay,
            ControlledCommand::SetFollowing => &mut self.set_following,
            ControlledCommand::StartDissolving => &mut self.start_dissolving,
            ControlledCommand::Merge => &mut self.merge,
            ControlledCommand::RefreshVotingPower => &mut self.refresh_voting_power,
        };
        *counter = counter.saturating_add(1);
    }
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
    let (pooled_principal_e8s, anchor_available_e8s, live_cohort) = STATE.with(|cell| {
        let state = cell.borrow();
        (
            state.pooled_principal_e8s,
            state.anchor_available_e8s,
            state.live_cohort.clone(),
        )
    });
    let physical_principal_e8s = pooled_principal_e8s
        .checked_add(anchor_available_e8s)
        .ok_or_else(|| NnsError::Invalid("mock Dynamic parent principal overflow".into()))?;
    Ok(ClaimAssetObservation {
        parent: Some(ParentAssetObservation {
            neuron_id: 1,
            staking_account: pool_staking_account.clone(),
            physical_principal_e8s,
        }),
        pool_staking_account,
        claim_bearing_dynamic_principal_e8s: pooled_principal_e8s,
        anchor_target_e8s: io_nns_types::backing::DYNAMIC_ANCHOR_TARGET_E8S,
        anchor_available_e8s,
        excluded_dynamic_surplus_e8s: 0,
        permanent_fee_shortfall_e8s: 0,
        live_cohorts: live_cohort.iter().cloned().collect(),
        live_child_physical_principal_e8s: live_cohort
            .as_ref()
            .map_or(0, |cohort| cohort.physical_principal_e8s),
        live_child_net_backing_e8s: live_cohort
            .as_ref()
            .map_or(0, |cohort| cohort.net_backing_e8s),
        live_child_committed_fee_liability_e8s: live_cohort
            .as_ref()
            .map_or(0, |cohort| cohort.committed_fee_e8s),
        transit_components: Vec::new(),
        transit_backing_e8s: 0,
        active_operation_sequence: 0,
        last_completed_pool_operation_sequence: None,
        control_epoch: 1,
        fingerprint: vec![42; 32],
        oldest_ready_at_seconds: live_cohort.as_ref().map(|cohort| cohort.ready_at_seconds),
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn observe_pool_policy() -> Result<io_nns_types::backing::PoolPolicyObservation, NnsError> {
    use io_nns_types::backing::{
        FollowPolicy, ParentPolicyObservation, PoolPolicyObservation,
        NNS_DYNAMIC_DISSOLVE_DELAY_SECONDS,
    };
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.pool_policy_observation_calls = state.pool_policy_observation_calls.saturating_add(1);
        if !state.pool_policy_valid {
            return Err(NnsError::Invalid("mock pooled-parent policy drift".into()));
        }
        Ok(())
    })?;
    Ok(PoolPolicyObservation {
        parent: Some(ParentPolicyObservation {
            neuron_id: 1,
            dissolve_delay_seconds: NNS_DYNAMIC_DISSOLVE_DELAY_SECONDS,
            auto_stake_maturity: false,
            follow_policy: FollowPolicy {
                followee_neuron_id: 2,
            },
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
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.pooled_principal_e8s = pooled_principal_e8s;
        state.anchor_available_e8s = io_nns_types::backing::DYNAMIC_ANCHOR_TARGET_E8S;
        state.live_cohort = None;
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_reject_next_reconciliations(count: u64) {
    STATE.with(|cell| cell.borrow_mut().reconcile_rejections_remaining = count);
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn prepare_pool_reconciliation(
    args: io_nns_types::backing::PreparePoolReconciliationArgs,
) -> Result<io_nns_types::backing::PoolProgress, NnsError> {
    use io_nns_types::backing::{PoolProgress, PoolReconciliationAction};
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.reconcile_calls = state.reconcile_calls.saturating_add(1);
        if state.reconcile_rejections_remaining > 0 {
            state.reconcile_rejections_remaining -= 1;
            return Err(NnsError::Invalid(
                "controlled reconciliation contention".into(),
            ));
        }
        Ok(())
    })?;
    if args.generation == 0 || args.snapshot_fingerprint != vec![42; 32] {
        return Err(NnsError::Invalid(
            "mock reconciliation identity is invalid".into(),
        ));
    }
    let result = STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.two_week_target = Some(SetTargetArgs {
            target_e8s: args.target_e8s,
            generation: args.generation,
        });
        match args.action {
            PoolReconciliationAction::Hold if args.target_e8s == state.pooled_principal_e8s => {
                Ok(PoolProgress::Held {
                    principal_e8s: state.pooled_principal_e8s,
                })
            }
            PoolReconciliationAction::Unwind { expected_gross_e8s } => {
                let claim_return = state
                    .pooled_principal_e8s
                    .checked_sub(args.target_e8s)
                    .ok_or_else(|| {
                        NnsError::Invalid("mock unwind target exceeds principal".into())
                    })?;
                let required_anchor = args
                    .fee_e8s
                    .checked_mul(2)
                    .ok_or_else(|| NnsError::Invalid("mock unwind fee overflow".into()))?;
                if expected_gross_e8s != claim_return.saturating_add(required_anchor)
                    || state.anchor_available_e8s < required_anchor
                    || state.live_cohort.is_some()
                {
                    return Err(NnsError::Invalid(
                        "mock unwind amount or anchor capacity is invalid".into(),
                    ));
                }
                let child_physical = claim_return
                    .checked_add(args.fee_e8s)
                    .ok_or_else(|| NnsError::Invalid("mock child principal overflow".into()))?;
                state.pooled_principal_e8s = args.target_e8s;
                state.anchor_available_e8s -= required_anchor;
                state.live_cohort = Some(io_nns_types::backing::CohortObservation {
                    generation: args.generation,
                    child_neuron_id: 10_000 + args.generation,
                    physical_principal_e8s: child_physical,
                    net_backing_e8s: claim_return,
                    committed_fee_e8s: args.fee_e8s,
                    ready_at_seconds: state
                        .now_seconds
                        .saturating_add(io_nns_types::backing::NNS_DYNAMIC_DISSOLVE_DELAY_SECONDS),
                    proof: io_nns_types::backing::CohortProofState::Dissolving,
                });
                Ok(PoolProgress::UnwindCommitted {
                    generation: args.generation,
                    principal_e8s: child_physical,
                })
            }
            _ => Err(NnsError::Invalid(
                "mock reconciliation action is unsupported".into(),
            )),
        }
    })?;
    Ok(result)
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn prepare_two_week_maturity(args: PrepareTwoWeekMaturityArgs) -> Result<(), NnsError> {
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
                return Ok(());
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
        Ok(())
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

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct NnsAccount {
    pub owner: Option<candid::Principal>,
    pub subaccount: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct MaturityDisbursement {
    pub amount_e8s: Option<u64>,
    pub timestamp_of_disbursement_seconds: Option<u64>,
    pub finalize_disbursement_timestamp_seconds: Option<u64>,
    pub account_to_disburse_to: Option<NnsAccount>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct FullNeuron {
    pub id: Option<NnsNeuronId>,
    pub controller: Option<candid::Principal>,
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

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum NnsNeuronIdOrSubaccount {
    NeuronId(NnsNeuronId),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Empty {}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ClaimOrRefresh {
    pub by: Option<ClaimBy>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ClaimBy {
    NeuronIdOrSubaccount(Empty),
    MemoAndController(ClaimFromAccount),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ClaimFromAccount {
    pub controller: Option<candid::Principal>,
    pub memo: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Configure {
    pub operation: Option<ConfigureOperation>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ConfigureOperation {
    StopDissolving(Empty),
    StartDissolving(Empty),
    IncreaseDissolveDelay(IncreaseDissolveDelay),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct IncreaseDissolveDelay {
    pub additional_dissolve_delay_seconds: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SetFollowing {
    pub topic_following: Option<Vec<FolloweesForTopic>>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct FolloweesForTopic {
    pub followees: Option<Vec<NnsNeuronId>>,
    pub topic: Option<i32>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Merge {
    pub source_neuron_id: Option<NnsNeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Split {
    pub amount_e8s: u64,
    pub memo: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct AccountIdentifier {
    pub hash: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Disburse {
    pub to_account: Option<AccountIdentifier>,
    pub amount: Option<Amount>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct Amount {
    pub e8s: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct DisburseMaturity {
    pub percentage_to_disburse: u32,
    pub to_account: Option<NnsAccount>,
    pub to_account_identifier: Option<AccountIdentifier>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ManageCommand {
    Split(Split),
    Disburse(Disburse),
    DisburseMaturity(DisburseMaturity),
    ClaimOrRefresh(ClaimOrRefresh),
    Configure(Configure),
    Merge(Merge),
    SetFollowing(SetFollowing),
    RefreshVotingPower(Empty),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ManageNeuron {
    pub id: Option<NnsNeuronId>,
    pub neuron_id_or_subaccount: Option<NnsNeuronIdOrSubaccount>,
    pub command: Option<ManageCommand>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ClaimOrRefreshResponse {
    pub refreshed_neuron_id: Option<NnsNeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SplitResponse {
    pub created_neuron_id: Option<NnsNeuronId>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct DisburseResponse {
    pub transfer_block_height: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct DisburseMaturityResponse {
    pub amount_disbursed_e8s: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ManageCommandResponse {
    Error(GovernanceError),
    ClaimOrRefresh(ClaimOrRefreshResponse),
    Configure(candid::Reserved),
    Split(SplitResponse),
    Disburse(DisburseResponse),
    DisburseMaturity(DisburseMaturityResponse),
    Merge(candid::Reserved),
    SetFollowing(candid::Reserved),
    RefreshVotingPower(candid::Reserved),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ManageNeuronResponse {
    pub command: Option<ManageCommandResponse>,
}

fn managed_neuron_id(request: &ManageNeuron) -> Option<u64> {
    request
        .neuron_id_or_subaccount
        .as_ref()
        .map(|NnsNeuronIdOrSubaccount::NeuronId(id)| id.id)
        .or_else(|| request.id.as_ref().map(|id| id.id))
}

fn controlled_attempt(state: &mut GovernanceState, command: ControlledCommand) -> (bool, bool) {
    state.command_calls.increment(command);
    let Some(control) = state
        .command_controls
        .iter_mut()
        .find(|control| control.command == command)
    else {
        return (false, false);
    };
    if control.reject_before_effect > 0 {
        control.reject_before_effect -= 1;
        return (true, false);
    }
    if control.malformed_after_effect > 0 {
        control.malformed_after_effect -= 1;
        return (false, true);
    }
    (false, false)
}

fn rejected(command: ControlledCommand) -> ManageNeuronResponse {
    ManageNeuronResponse {
        command: Some(ManageCommandResponse::Error(GovernanceError {
            error_type: 5,
            error_message: format!("controlled {command:?} rejection"),
        })),
    }
}

fn malformed_after_effect() -> ManageNeuronResponse {
    ManageNeuronResponse {
        command: Some(ManageCommandResponse::RefreshVotingPower(candid::Reserved)),
    }
}

fn split_response(
    state: &mut GovernanceState,
    parent_id: u64,
    split: Split,
    controller: candid::Principal,
) -> ManageCommandResponse {
    let gross = u128::from(split.amount_e8s);
    let delay = match neuron_mut(state, parent_id) {
        Ok(parent) if parent.principal_e8s >= gross => {
            parent.principal_e8s -= gross;
            parent.dissolve_delay_seconds
        }
        _ => {
            return ManageCommandResponse::Error(GovernanceError {
                error_type: 5,
                error_message: "controlled Split rejection".into(),
            })
        }
    };
    let child_id = state.next_neuron_id;
    state.next_neuron_id = state.next_neuron_id.saturating_add(1);
    state.neurons.push(MockNeuron {
        neuron_id: child_id,
        account: split
            .memo
            .map(|memo| split_child_subaccount(controller, memo))
            .unwrap_or_else(|| default_neuron_account(child_id)),
        principal_e8s: gross.saturating_sub(10_000),
        maturity_e8s: 0,
        dissolve_delay_seconds: delay,
        is_dissolving: false,
        dissolve_started_at_seconds: None,
        followee_id: Some(43),
        pending_refresh_credit_e8s: 0,
        maturity_disbursements: Vec::new(),
        voting_power_refreshed_timestamp_seconds: 1,
        controller,
    });
    ManageCommandResponse::Split(SplitResponse {
        created_neuron_id: Some(NnsNeuronId { id: child_id }),
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn manage_neuron(request: ManageNeuron) -> ManageNeuronResponse {
    let caller = ic_cdk::api::msg_caller();
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let neuron_id = managed_neuron_id(&request).unwrap_or_default();
        let Some(command) = request.command else {
            return rejected(ControlledCommand::ClaimOrRefresh);
        };
        let kind = match &command {
            ManageCommand::Split(_) => ControlledCommand::Split,
            ManageCommand::Disburse(_) => ControlledCommand::Disburse,
            ManageCommand::DisburseMaturity(_) => ControlledCommand::DisburseMaturity,
            ManageCommand::ClaimOrRefresh(_) => ControlledCommand::ClaimOrRefresh,
            ManageCommand::Configure(Configure {
                operation: Some(ConfigureOperation::IncreaseDissolveDelay(_)),
            }) => ControlledCommand::IncreaseDissolveDelay,
            ManageCommand::Configure(Configure {
                operation: Some(ConfigureOperation::StartDissolving(_)),
            }) => ControlledCommand::StartDissolving,
            ManageCommand::Configure(_) => ControlledCommand::StartDissolving,
            ManageCommand::Merge(_) => ControlledCommand::Merge,
            ManageCommand::SetFollowing(_) => ControlledCommand::SetFollowing,
            ManageCommand::RefreshVotingPower(_) => ControlledCommand::RefreshVotingPower,
        };
        let (reject, malformed) = controlled_attempt(&mut state, kind);
        if kind == ControlledCommand::Split && state.split_trap_before_effect {
            ic_cdk::trap("controlled Split transport rejection before effect");
        }
        if reject {
            return rejected(kind);
        }
        let response = match command {
            ManageCommand::Split(split) => split_response(&mut state, neuron_id, split, caller),
            ManageCommand::Disburse(_) => {
                let block = state.next_disburse_block.take().unwrap_or(777);
                if let Ok(neuron) = neuron_mut(&mut state, neuron_id) {
                    neuron.principal_e8s = 0;
                }
                ManageCommandResponse::Disburse(DisburseResponse {
                    transfer_block_height: block,
                })
            }
            ManageCommand::DisburseMaturity(command) => {
                let now = (ic_cdk::api::time() / 1_000_000_000).max(1);
                let response = match neuron_mut(&mut state, neuron_id) {
                    Ok(neuron) if command.percentage_to_disburse == 100 => {
                        let amount = u64::try_from(std::mem::take(&mut neuron.maturity_e8s))
                            .unwrap_or(u64::MAX);
                        neuron.maturity_disbursements.push(MaturityDisbursement {
                            amount_e8s: Some(amount),
                            timestamp_of_disbursement_seconds: Some(now),
                            finalize_disbursement_timestamp_seconds: Some(
                                now.saturating_add(604_800),
                            ),
                            account_to_disburse_to: command.to_account,
                        });
                        ManageCommandResponse::DisburseMaturity(DisburseMaturityResponse {
                            amount_disbursed_e8s: Some(amount),
                        })
                    }
                    _ => ManageCommandResponse::Error(GovernanceError {
                        error_type: 5,
                        error_message: "controlled DisburseMaturity rejection".into(),
                    }),
                };
                response
            }
            ManageCommand::ClaimOrRefresh(claim) => {
                let id = match claim.by {
                    Some(ClaimBy::MemoAndController(from)) => {
                        let controller = from.controller.unwrap_or(caller);
                        let account = staking_subaccount(controller, from.memo);
                        match state
                            .neurons
                            .iter()
                            .position(|neuron| neuron.account == account)
                        {
                            Some(index) => state.neurons[index].neuron_id,
                            None => {
                                let id = state.next_neuron_id;
                                state.next_neuron_id = state.next_neuron_id.saturating_add(1);
                                state.neurons.push(MockNeuron {
                                    neuron_id: id,
                                    account,
                                    principal_e8s: io_nns_types::backing::DYNAMIC_ANCHOR_TARGET_E8S,
                                    maturity_e8s: 0,
                                    dissolve_delay_seconds: 604_800,
                                    is_dissolving: false,
                                    dissolve_started_at_seconds: None,
                                    followee_id: None,
                                    pending_refresh_credit_e8s: 0,
                                    maturity_disbursements: Vec::new(),
                                    voting_power_refreshed_timestamp_seconds: 1,
                                    controller,
                                });
                                id
                            }
                        }
                    }
                    _ => neuron_id,
                };
                if let Ok(neuron) = neuron_mut(&mut state, id) {
                    neuron.principal_e8s = neuron
                        .principal_e8s
                        .saturating_add(neuron.pending_refresh_credit_e8s);
                    neuron.pending_refresh_credit_e8s = 0;
                }
                ManageCommandResponse::ClaimOrRefresh(ClaimOrRefreshResponse {
                    refreshed_neuron_id: Some(NnsNeuronId { id }),
                })
            }
            ManageCommand::Configure(Configure { operation }) => {
                if let Some(operation) = operation {
                    let now = canonical_now_seconds(&state);
                    if let Ok(neuron) = neuron_mut(&mut state, neuron_id) {
                        match operation {
                            ConfigureOperation::IncreaseDissolveDelay(increase) => {
                                neuron.dissolve_delay_seconds =
                                    neuron.dissolve_delay_seconds.saturating_add(u64::from(
                                        increase.additional_dissolve_delay_seconds,
                                    ));
                            }
                            ConfigureOperation::StartDissolving(_) => {
                                neuron.is_dissolving = true;
                                neuron.dissolve_started_at_seconds = Some(now);
                            }
                            ConfigureOperation::StopDissolving(_) => {
                                neuron.is_dissolving = false;
                                neuron.dissolve_started_at_seconds = None;
                            }
                        }
                    }
                }
                ManageCommandResponse::Configure(candid::Reserved)
            }
            ManageCommand::SetFollowing(following) => {
                let followee = following
                    .topic_following
                    .unwrap_or_default()
                    .into_iter()
                    .find_map(|entry| entry.followees?.first().map(|id| id.id));
                if let Ok(neuron) = neuron_mut(&mut state, neuron_id) {
                    neuron.followee_id = followee;
                }
                ManageCommandResponse::SetFollowing(candid::Reserved)
            }
            ManageCommand::RefreshVotingPower(_) => {
                let now = canonical_now_seconds(&state);
                if let Ok(neuron) = neuron_mut(&mut state, neuron_id) {
                    neuron.voting_power_refreshed_timestamp_seconds = now;
                }
                ManageCommandResponse::RefreshVotingPower(candid::Reserved)
            }
            ManageCommand::Merge(merge) => {
                let source = merge.source_neuron_id.map(|id| id.id).unwrap_or_default();
                let maturity = neuron_mut(&mut state, source)
                    .map(|child| std::mem::take(&mut child.maturity_e8s))
                    .unwrap_or_default();
                if let Ok(parent) = neuron_mut(&mut state, neuron_id) {
                    parent.maturity_e8s = parent.maturity_e8s.saturating_add(maturity);
                }
                ManageCommandResponse::Merge(candid::Reserved)
            }
        };
        if malformed {
            malformed_after_effect()
        } else {
            ManageNeuronResponse {
                command: Some(response),
            }
        }
    })
}

fn full_neuron(neuron: &MockNeuron) -> FullNeuron {
    FullNeuron {
        id: Some(NnsNeuronId {
            id: neuron.neuron_id,
        }),
        controller: Some(neuron.controller),
        account: neuron.account.clone(),
        cached_neuron_stake_e8s: neuron.principal_e8s.try_into().unwrap_or(u64::MAX),
        maturity_e8s_equivalent: neuron.maturity_e8s.try_into().unwrap_or(u64::MAX),
        staked_maturity_e8s_equivalent: Some(0),
        auto_stake_maturity: Some(false),
        maturity_disbursements_in_progress: Some(neuron.maturity_disbursements.clone()),
        dissolve_state: Some(if neuron.is_dissolving {
            NnsDissolveState::WhenDissolvedTimestampSeconds(
                neuron
                    .dissolve_started_at_seconds
                    .unwrap_or_default()
                    .saturating_add(neuron.dissolve_delay_seconds),
            )
        } else {
            NnsDissolveState::DissolveDelaySeconds(neuron.dissolve_delay_seconds)
        }),
        followees: neuron
            .followee_id
            .map(|followee| {
                [0, 4, 14]
                    .into_iter()
                    .map(|topic| {
                        (
                            topic,
                            NnsFollowees {
                                followees: vec![NnsNeuronId { id: followee }],
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        voting_power_refreshed_timestamp_seconds: Some(
            neuron.voting_power_refreshed_timestamp_seconds,
        ),
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn get_full_neuron(neuron_id: u64) -> Result<FullNeuron, GovernanceError> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.get_full_neuron_calls += 1;
        state
            .neurons
            .iter()
            .find(|neuron| neuron.neuron_id == neuron_id)
            .map(full_neuron)
            .ok_or_else(|| GovernanceError {
                error_type: 1,
                error_message: "mock neuron not found".into(),
            })
    })
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ListNeuronsRequest {
    pub neuron_ids: Vec<u64>,
    pub include_neurons_readable_by_caller: bool,
    pub include_empty_neurons_readable_by_caller: Option<bool>,
    pub include_public_neurons_in_full_neurons: Option<bool>,
    pub page_number: Option<u64>,
    pub page_size: Option<u64>,
    pub neuron_subaccounts: Option<Vec<NeuronSubaccount>>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct NeuronSubaccount {
    pub subaccount: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ListNeuronsResponse {
    pub neuron_infos: Vec<(u64, candid::Reserved)>,
    pub full_neurons: Vec<FullNeuron>,
    pub total_pages_available: Option<u64>,
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn list_neurons(request: ListNeuronsRequest) -> ListNeuronsResponse {
    let caller = ic_cdk::api::msg_caller();
    let requested_subaccounts = request
        .neuron_subaccounts
        .unwrap_or_default()
        .into_iter()
        .map(|entry| entry.subaccount)
        .collect::<Vec<_>>();
    let requested_ids = request.neuron_ids;
    ListNeuronsResponse {
        neuron_infos: Vec::new(),
        full_neurons: STATE.with(|cell| {
            cell.borrow()
                .neurons
                .iter()
                .filter(|neuron| {
                    requested_ids.contains(&neuron.neuron_id)
                        || requested_subaccounts.contains(&neuron.account)
                        || (request.include_neurons_readable_by_caller
                            && neuron.controller == caller)
                })
                .filter(|neuron| neuron.controller == caller)
                .map(full_neuron)
                .collect()
        }),
        total_pages_available: Some(1),
    }
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct NetworkEconomics {
    pub transaction_fee_e8s: u64,
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn get_network_economics_parameters() -> NetworkEconomics {
    NetworkEconomics {
        transaction_fee_e8s: STATE.with(|cell| cell.borrow().transaction_fee_e8s),
    }
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_full_neuron_call_count() -> u64 {
    STATE.with(|cell| cell.borrow().get_full_neuron_calls)
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_command_counters() -> GovernanceCommandCounters {
    STATE.with(|cell| cell.borrow().command_calls)
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_command_control(control: CommandControl) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if let Some(existing) = state
            .command_controls
            .iter_mut()
            .find(|existing| existing.command == control.command)
        {
            *existing = control;
        } else {
            state.command_controls.push(control);
        }
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_split_trap_before_effect(enabled: bool) {
    STATE.with(|cell| cell.borrow_mut().split_trap_before_effect = enabled);
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_transaction_fee_e8s(fee_e8s: u64) {
    STATE.with(|cell| cell.borrow_mut().transaction_fee_e8s = fee_e8s);
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_refresh_credit(args: NeuronAmountArgs) -> Result<(), String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        neuron_mut(&mut state, args.neuron_id)?.pending_refresh_credit_e8s = args.amount_e8s;
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_followee(args: SetFolloweeArgs) -> Result<(), String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        neuron_mut(&mut state, args.neuron_id)?.followee_id = args.followee;
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_voting_power_timestamp(args: SetVotingPowerTimestampArgs) -> Result<(), String> {
    STATE.with(|cell| {
        neuron_mut(&mut cell.borrow_mut(), args.neuron_id)?
            .voting_power_refreshed_timestamp_seconds = args.timestamp_seconds;
        Ok(())
    })
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
            account: default_neuron_account(args.neuron_id),
            principal_e8s: args.principal_e8s,
            maturity_e8s: 0,
            dissolve_delay_seconds: args.dissolve_delay_seconds,
            is_dissolving: false,
            dissolve_started_at_seconds: None,
            followee_id: Some(43),
            pending_refresh_credit_e8s: 0,
            maturity_disbursements: Vec::new(),
            voting_power_refreshed_timestamp_seconds: 1,
            controller: ic_cdk::api::msg_caller(),
        });
        args.neuron_id
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_create_staking_neuron(args: CreateStakingNeuronArgs) -> u64 {
    let controller = ic_cdk::api::msg_caller();
    STATE.with(|cell| {
        cell.borrow_mut().neurons.push(MockNeuron {
            neuron_id: args.neuron_id,
            account: staking_subaccount(controller, args.memo),
            principal_e8s: args.principal_e8s,
            maturity_e8s: 0,
            dissolve_delay_seconds: args.dissolve_delay_seconds,
            is_dissolving: false,
            dissolve_started_at_seconds: None,
            followee_id: Some(43),
            pending_refresh_credit_e8s: 0,
            maturity_disbursements: Vec::new(),
            voting_power_refreshed_timestamp_seconds: 1,
            controller,
        });
        args.neuron_id
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_create_split_child(args: CreateSplitChildArgs) -> u64 {
    let controller = ic_cdk::api::msg_caller();
    STATE.with(|cell| {
        cell.borrow_mut().neurons.push(MockNeuron {
            neuron_id: args.neuron_id,
            account: split_child_subaccount(controller, args.memo),
            principal_e8s: args.principal_e8s,
            maturity_e8s: 0,
            dissolve_delay_seconds: args.dissolve_delay_seconds,
            is_dissolving: false,
            dissolve_started_at_seconds: None,
            followee_id: Some(43),
            pending_refresh_credit_e8s: 0,
            maturity_disbursements: Vec::new(),
            voting_power_refreshed_timestamp_seconds: 1,
            controller,
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
pub fn debug_set_principal(args: NeuronAmountArgs) -> Result<(), String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        neuron_mut(&mut state, args.neuron_id)?.principal_e8s = args.amount_e8s;
        Ok(())
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_set_next_disburse_block(args: SetNextDisburseBlockArgs) {
    STATE.with(|cell| cell.borrow_mut().next_disburse_block = Some(args.block_index));
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
            account: default_neuron_account(child_id),
            principal_e8s: amount_e8s,
            maturity_e8s: 0,
            dissolve_delay_seconds,
            is_dissolving: false,
            dissolve_started_at_seconds: None,
            followee_id: Some(43),
            pending_refresh_credit_e8s: 0,
            maturity_disbursements: Vec::new(),
            voting_power_refreshed_timestamp_seconds: 1,
            controller: ic_cdk::api::msg_caller(),
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
