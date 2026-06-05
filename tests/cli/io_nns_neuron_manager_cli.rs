use io_nns_neuron_manager::{
    NnsNeuronManagerModel, RebalanceAction, TwoWeekPoolState, SECONDS_PER_DAY,
    TWO_WEEK_DISSOLVE_SECONDS,
};

#[test]
fn cli_model_cancel_dissolve_increases_target_and_rebalance_stakes_more() {
    let pool = TwoWeekPoolState {
        target_staked_e8s: 2_000,
        active_staked_e8s: 1_250,
        pending_unwind_e8s: 0,
        pending_restake_e8s: 0,
    };
    assert_eq!(
        pool.plan_rebalance(),
        RebalanceAction::StakeMore { amount_e8s: 750 }
    );
}

#[test]
fn cli_model_batching_pending_unwind_avoids_unnecessary_split() {
    let pool = TwoWeekPoolState {
        target_staked_e8s: 1_000,
        active_staked_e8s: 1_500,
        pending_unwind_e8s: 500,
        pending_restake_e8s: 0,
    };
    assert_eq!(pool.plan_rebalance(), RebalanceAction::None);
}

#[test]
fn cli_model_unwind_then_late_cancel_requires_restake_after_liquidity_returned() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000);
    let child = manager.split_and_start_unwind(100_000).unwrap();
    manager.advance_time(TWO_WEEK_DISSOLVE_SECONDS, 0);
    let liquid = manager.disburse_ready_unwind(child).unwrap();
    assert_eq!(liquid, 100_000);

    let pool = TwoWeekPoolState {
        target_staked_e8s: 1_000_000,
        active_staked_e8s: 900_000,
        pending_unwind_e8s: 0,
        pending_restake_e8s: 0,
    };
    assert_eq!(
        pool.plan_rebalance(),
        RebalanceAction::StakeMore {
            amount_e8s: 100_000
        }
    );
    manager.stake_more_two_week(liquid);
    assert_eq!(manager.two_week_pool.principal_e8s, 1_000_000);
}

#[test]
fn cli_model_maturity_is_zero_after_disbursement_until_time_advances_again() {
    let mut manager = NnsNeuronManagerModel::new(1_000_000_000, 1_000_000_000);
    manager.advance_time(SECONDS_PER_DAY, 36_500); // 365% APY gives easy daily accrual: 1% of principal.
    let maturity = manager.disburse_two_week_maturity();
    assert_eq!(maturity, 10_000_000);
    assert_eq!(manager.disburse_two_week_maturity(), 0);
    manager.advance_time(SECONDS_PER_DAY, 36_500);
    assert_eq!(manager.disburse_two_week_maturity(), 10_000_000);
}

#[test]
fn cli_model_unknown_child_operations_are_rejected() {
    let mut manager = NnsNeuronManagerModel::new(0, 1_000_000);
    assert_eq!(
        manager.cancel_unwind_and_merge_back(123456),
        Err(io_nns_neuron_manager::ManagerError::UnknownUnwindNeuron)
    );
    assert_eq!(
        manager.disburse_ready_unwind(123456),
        Err(io_nns_neuron_manager::ManagerError::UnknownUnwindNeuron)
    );
}

#[test]
fn cli_model_cannot_split_more_than_available_pool() {
    let mut manager = NnsNeuronManagerModel::new(0, 100);
    assert_eq!(
        manager.split_and_start_unwind(101),
        Err(io_nns_neuron_manager::ManagerError::SplitExceedsMainPool)
    );
}
