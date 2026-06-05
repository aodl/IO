use io_core_model::E8S_PER_TOKEN;
use io_nns_neuron_manager::{
    NnsNeuronManagerModel, RebalanceAction, TwoWeekPoolState, SECONDS_PER_DAY,
    TWO_WEEK_DISSOLVE_SECONDS,
};
use io_reward_policy::NeuronSnapshot;
use io_stream_manager::state::{
    IO_NNS_NEURON_MANAGER_SOURCE, JUPITER_FAUCET_SOURCE, TWO_WEEK_MATURITY_MEMO,
    TWO_YEAR_MATURITY_MEMO,
};
use io_stream_manager::StreamManager;

fn t(n: u128) -> u128 {
    n * E8S_PER_TOKEN
}

fn neuron(id: u64, stake: u128, voted: u64, total: u64) -> NeuronSnapshot {
    NeuronSnapshot {
        neuron_id: id,
        staked_io_e8s: stake,
        eligible_seconds: 14 * 24 * 60 * 60,
        eligible_closed_proposals: total,
        voted_closed_proposals: voted,
        is_genesis_governance_neuron: false,
        is_protocol_owned: false,
        is_dissolving: false,
    }
}

#[test]
fn e2e_jupiter_to_staking_to_maturity_to_redemption() {
    let mut stream = StreamManager::default_for_tests();
    let mut nns = NnsNeuronManagerModel::new(0, 0);

    // 1. Jupiter Faucet sends 100 ICP. IO stream manager issues 60 backed IO.
    let faucet = stream
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet-1")
        .unwrap();
    assert_eq!(faucet.io_issued_e8s, t(60));
    assert_eq!(faucet.split.stake_e8s, t(40));
    assert_eq!(faucet.split.liquid_e8s, t(60));

    // 2. The stream manager's 40% stake instruction funds the 2-year NNS neuron.
    nns.two_year_neuron.principal_e8s += faucet.split.stake_e8s;

    // 3. Users stake some IO. The target 2-week pool follows active eligible stake.
    let alice = neuron(1, t(20), 2, 2);
    let bob = neuron(2, t(10), 1, 2);
    stream.refresh_active_staked_io_from_neurons(&[alice.clone(), bob.clone()]);
    let target = stream.target_two_week_pool_e8s().unwrap();
    assert_eq!(target, t(30));
    let rebalance = TwoWeekPoolState {
        target_staked_e8s: target,
        active_staked_e8s: nns.two_week_pool.principal_e8s,
        pending_unwind_e8s: 0,
        pending_restake_e8s: 0,
    }
    .plan_rebalance();
    assert_eq!(rebalance, RebalanceAction::StakeMore { amount_e8s: t(30) });
    nns.stake_more_two_week(t(30));

    // 4. Fast-forward enough time that both NNS neurons accrue test maturity.
    nns.advance_time(30 * SECONDS_PER_DAY, 12_000);
    let two_year_maturity = nns.disburse_two_year_maturity();
    let two_week_maturity = nns.disburse_two_week_maturity();
    assert!(two_year_maturity > 0);
    assert!(two_week_maturity > 0);

    // 5. 2-year maturity strengthens all IO backing and issues no IO.
    let two_year = stream
        .process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_YEAR_MATURITY_MEMO,
            two_year_maturity,
            "2y-maturity-1",
        )
        .unwrap();
    assert_eq!(two_year.io_issued_e8s, 0);

    // 6. 2-week maturity creates backed IO for active voting stakers.
    let two_week = stream
        .process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_WEEK_MATURITY_MEMO,
            two_week_maturity,
            "2w-maturity-1",
        )
        .unwrap();
    assert!(two_week.io_issued_e8s > 0);
    let alloc = stream.allocate_two_week_maturity_io(two_week.io_issued_e8s, &[alice, bob]);
    assert_eq!(alloc.allocations.len(), 2);
    assert!(alloc.allocations[0].io_e8s > alloc.allocations[1].io_e8s);

    // 7. A user starts dissolving. The pooled 2-week neuron is split and becomes liquid after two weeks.
    let unwind_amount = t(10);
    let child = nns.split_and_start_unwind(unwind_amount).unwrap();
    nns.advance_time(TWO_WEEK_DISSOLVE_SECONDS, 0);
    let returned_liquidity = nns.disburse_ready_unwind(child).unwrap();
    assert_eq!(returned_liquidity, unwind_amount);
    stream.state.liquid_icp_e8s += returned_liquidity;

    // 8. Redemption pays ICP and returns IO to the protocol reserve.
    let before_rate = stream.state.redemption_rate().unwrap();
    let redemption = stream.redeem(t(5)).unwrap();
    assert_eq!(
        redemption.icp_paid_e8s,
        before_rate.icp_for_io(t(5)).unwrap()
    );
    assert_eq!(
        stream
            .state
            .redemption_rate()
            .unwrap()
            .icp_for_io(t(1))
            .unwrap(),
        redemption.rate_after.icp_for_io(t(1)).unwrap()
    );
}

#[test]
fn e2e_cancel_dissolve_before_two_weeks_restores_pool_without_liquid_unwind() {
    let mut nns = NnsNeuronManagerModel::new(0, t(30));
    let child = nns.split_and_start_unwind(t(10)).unwrap();
    assert_eq!(nns.two_week_pool.principal_e8s, t(20));
    nns.advance_time(7 * SECONDS_PER_DAY, 0);
    assert_eq!(nns.cancel_unwind_and_merge_back(child).unwrap(), t(10));
    assert_eq!(nns.two_week_pool.principal_e8s, t(30));
}

#[test]
fn e2e_multi_epoch_faucet_yield_staker_rewards_and_late_entry() {
    let mut stream = StreamManager::default_for_tests();
    let mut nns = NnsNeuronManagerModel::new(0, 0);

    let first_faucet = stream
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet-epoch-1")
        .unwrap();
    nns.two_year_neuron.principal_e8s += first_faucet.split.stake_e8s;
    assert_eq!(first_faucet.io_issued_e8s, t(60));

    let alice = neuron(1, t(30), 3, 3);
    stream.refresh_active_staked_io_from_neurons(std::slice::from_ref(&alice));
    let target = stream.target_two_week_pool_e8s().unwrap();
    nns.stake_more_two_week(target);

    nns.advance_time(60 * SECONDS_PER_DAY, 12_000);
    let two_year_maturity = nns.disburse_two_year_maturity();
    let two_week_maturity = nns.disburse_two_week_maturity();

    let two_year = stream
        .process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_YEAR_MATURITY_MEMO,
            two_year_maturity,
            "2y-epoch-1",
        )
        .unwrap();
    assert_eq!(two_year.io_issued_e8s, 0);
    let rate_after_protocol_yield = stream.state.redemption_rate().unwrap();
    assert!(rate_after_protocol_yield.icp_for_io(t(1)).unwrap() > t(1));

    let two_week = stream
        .process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_WEEK_MATURITY_MEMO,
            two_week_maturity,
            "2w-epoch-1",
        )
        .unwrap();
    let alloc =
        stream.allocate_two_week_maturity_io(two_week.io_issued_e8s, std::slice::from_ref(&alice));
    assert_eq!(alloc.allocations.len(), 1);
    assert_eq!(alloc.allocations[0].neuron_id, 1);
    let rate_before_late_faucet = stream.state.redemption_rate().unwrap();
    assert!(
        rate_before_late_faucet.liquid_icp_e8s * rate_after_protocol_yield.redeemable_io_e8s
            >= rate_after_protocol_yield.liquid_icp_e8s * rate_before_late_faucet.redeemable_io_e8s
    );

    let late_faucet = stream
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet-epoch-2")
        .unwrap();
    assert!(late_faucet.io_issued_e8s < t(60));
    assert_eq!(late_faucet.rate_before, rate_before_late_faucet);
}

#[test]
fn e2e_dissolve_unwind_then_cancel_after_liquid_return_restakes_on_next_rebalance() {
    let mut stream = StreamManager::default_for_tests();
    let mut nns = NnsNeuronManagerModel::new(0, t(50));

    let child = nns.split_and_start_unwind(t(20)).unwrap();
    nns.advance_time(TWO_WEEK_DISSOLVE_SECONDS, 0);
    let liquid_returned = nns.disburse_ready_unwind(child).unwrap();
    stream.state.liquid_icp_e8s += liquid_returned;

    // The user cannot merge the already-disbursed split, so cancel-dissolve is represented
    // as a higher target and a later restake from available liquidity.
    let rebalance = TwoWeekPoolState {
        target_staked_e8s: t(50),
        active_staked_e8s: nns.two_week_pool.principal_e8s,
        pending_unwind_e8s: 0,
        pending_restake_e8s: 0,
    }
    .plan_rebalance();
    assert_eq!(rebalance, RebalanceAction::StakeMore { amount_e8s: t(20) });
    nns.stake_more_two_week(liquid_returned);
    stream.state.liquid_icp_e8s -= liquid_returned;
    assert_eq!(nns.two_week_pool.principal_e8s, t(50));
}

#[test]
fn e2e_malicious_and_duplicate_inputs_cannot_change_protocol_state() {
    let mut stream = StreamManager::default_for_tests();
    let before = stream.state;
    assert!(stream
        .process_scanned_icp("attacker", "faucet", t(100), "attack-1")
        .is_err());
    assert_eq!(stream.state, before);
    assert!(!stream.processed_transactions.contains("attack-1"));

    stream
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "block-1")
        .unwrap();
    let after_first = stream.state;
    assert!(stream
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "block-1")
        .is_err());
    assert_eq!(stream.state, after_first);
}
