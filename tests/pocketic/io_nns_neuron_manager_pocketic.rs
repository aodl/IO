use io_nns_neuron_manager::{
    ManagerError, NnsNeuronManagerModel, RebalanceAction, TwoWeekPoolState,
    CONTROLLER_CANISTER_PRINCIPAL_TEXT, SECONDS_PER_DAY, TWO_WEEK_DISSOLVE_SECONDS,
    TWO_YEAR_NNS_NEURON_ID,
};

#[test]
fn pocketic_model_nns_manager_constants_and_rebalance() {
    assert_eq!(TWO_YEAR_NNS_NEURON_ID, 6_345_890_886_899_317_159);
    assert_eq!(
        CONTROLLER_CANISTER_PRINCIPAL_TEXT,
        "oae4c-3iaaa-aaaar-qb5qq-cai"
    );
    let pool = TwoWeekPoolState {
        target_staked_e8s: 1_000,
        active_staked_e8s: 1_500,
        pending_unwind_e8s: 0,
        pending_restake_e8s: 0,
    };
    assert_eq!(
        pool.plan_rebalance(),
        RebalanceAction::SplitAndDissolve { amount_e8s: 500 }
    );
}

#[test]
fn pocketic_fast_forward_maturity_can_feed_downstream_streams() {
    let mut manager = NnsNeuronManagerModel::new(1_000_000_000, 500_000_000);
    manager.advance_time(30 * SECONDS_PER_DAY, 12_000); // exaggerated 120% APY for deterministic fast-forward testing.
    assert!(manager.two_year_neuron.maturity_e8s > 0);
    assert!(manager.two_week_pool.maturity_e8s > 0);

    let two_year_maturity = manager.disburse_two_year_maturity();
    let two_week_maturity = manager.disburse_two_week_maturity();
    assert!(two_year_maturity > two_week_maturity);
    assert_eq!(manager.two_year_neuron.maturity_e8s, 0);
    assert_eq!(manager.two_week_pool.maturity_e8s, 0);
}

#[test]
fn pocketic_fast_forward_unwind_principal_after_two_weeks() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000);
    let child = manager.split_and_start_unwind(250_000).unwrap();
    assert_eq!(
        manager.disburse_ready_unwind(child),
        Err(ManagerError::NeuronNotReady)
    );
    manager.advance_time(TWO_WEEK_DISSOLVE_SECONDS - 1, 0);
    assert_eq!(
        manager.disburse_ready_unwind(child),
        Err(ManagerError::NeuronNotReady)
    );
    manager.advance_time(1, 0);
    assert_eq!(manager.disburse_ready_unwind(child).unwrap(), 250_000);
}

#[test]
fn pocketic_cancel_dissolve_merges_unwind_back_before_it_becomes_liquid() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000);
    let child = manager.split_and_start_unwind(400_000).unwrap();
    manager.advance_time(7 * SECONDS_PER_DAY, 0);
    let merged = manager.cancel_unwind_and_merge_back(child).unwrap();
    assert_eq!(merged, 400_000);
    assert_eq!(manager.two_week_pool.principal_e8s, 1_000_000);
    assert!(manager.unwind_neurons.is_empty());
}

#[test]
fn pocketic_multiple_unwind_children_can_mature_and_disburse_independently() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000);
    let first = manager.split_and_start_unwind(100_000).unwrap();
    manager.advance_time(SECONDS_PER_DAY, 0);
    let second = manager.split_and_start_unwind(200_000).unwrap();

    manager.advance_time(TWO_WEEK_DISSOLVE_SECONDS - SECONDS_PER_DAY, 0);
    assert_eq!(manager.disburse_ready_unwind(first).unwrap(), 100_000);
    assert_eq!(
        manager.disburse_ready_unwind(second),
        Err(ManagerError::NeuronNotReady)
    );

    manager.advance_time(SECONDS_PER_DAY, 0);
    assert_eq!(manager.disburse_ready_unwind(second).unwrap(), 200_000);
    assert_eq!(manager.two_week_pool.principal_e8s, 700_000);
}

#[test]
fn pocketic_dissolving_child_does_not_receive_fast_forward_maturity() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000_000);
    let child = manager.split_and_start_unwind(500_000_000).unwrap();
    manager.advance_time(365 * SECONDS_PER_DAY, 10_000);
    let unwind = manager
        .unwind_neurons
        .iter()
        .find(|n| n.neuron_id == child)
        .unwrap();
    assert_eq!(unwind.maturity_e8s, 0);
    assert_eq!(manager.two_week_pool.maturity_e8s, 500_000_000);
}

#[test]
fn pocketic_rebalance_plan_handles_cancel_dissolve_batching() {
    let after_user_started_dissolving = TwoWeekPoolState {
        target_staked_e8s: 600_000,
        active_staked_e8s: 1_000_000,
        pending_unwind_e8s: 400_000,
        pending_restake_e8s: 0,
    };
    assert_eq!(
        after_user_started_dissolving.plan_rebalance(),
        RebalanceAction::None
    );

    let after_cancel_before_execution = TwoWeekPoolState {
        target_staked_e8s: 1_000_000,
        active_staked_e8s: 1_000_000,
        pending_unwind_e8s: 400_000,
        pending_restake_e8s: 400_000,
    };
    assert_eq!(
        after_cancel_before_execution.plan_rebalance(),
        RebalanceAction::None
    );
}

#[test]
fn pocketic_maturity_disbursement_is_idempotent_until_more_time_passes() {
    let mut manager = NnsNeuronManagerModel::new(1_000_000_000, 0);
    manager.advance_time(30 * SECONDS_PER_DAY, 12_000);
    let first = manager.disburse_two_year_maturity();
    assert!(first > 0);
    assert_eq!(manager.disburse_two_year_maturity(), 0);
    manager.advance_time(SECONDS_PER_DAY, 12_000);
    assert!(manager.disburse_two_year_maturity() > 0);
}

#[test]
fn pocketic_can_split_entire_two_week_pool_but_not_more() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000);
    let child = manager.split_and_start_unwind(1_000_000).unwrap();
    assert_eq!(manager.two_week_pool.principal_e8s, 0);
    assert_eq!(
        manager.split_and_start_unwind(1),
        Err(ManagerError::SplitExceedsMainPool)
    );
    manager.advance_time(TWO_WEEK_DISSOLVE_SECONDS, 0);
    assert_eq!(manager.disburse_ready_unwind(child).unwrap(), 1_000_000);
}

#[test]
fn pocketic_cancel_after_child_disbursed_is_unknown_and_requires_restake_path() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000);
    let child = manager.split_and_start_unwind(250_000).unwrap();
    manager.advance_time(TWO_WEEK_DISSOLVE_SECONDS, 0);
    assert_eq!(manager.disburse_ready_unwind(child).unwrap(), 250_000);
    assert_eq!(
        manager.cancel_unwind_and_merge_back(child),
        Err(ManagerError::UnknownUnwindNeuron)
    );
}

#[test]
fn pocketic_zero_time_advance_does_not_create_maturity() {
    let mut manager = NnsNeuronManagerModel::new(1_000_000_000, 1_000_000_000);
    manager.advance_time(0, 100_000);
    assert_eq!(manager.two_year_neuron.maturity_e8s, 0);
    assert_eq!(manager.two_week_pool.maturity_e8s, 0);
}
