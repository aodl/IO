use io_core_model::{StreamKind, E8S_PER_TOKEN};
use io_reward_policy::NeuronSnapshot;
use io_stream_manager::state::{
    IO_NNS_NEURON_MANAGER_SOURCE, JUPITER_FAUCET_SOURCE, TWO_WEEK_MATURITY_MEMO,
    TWO_YEAR_MATURITY_MEMO,
};
use io_stream_manager::{ModelError, StreamManager, StreamManagerError};

fn t(n: u128) -> u128 {
    n * E8S_PER_TOKEN
}

fn neuron(id: u64, stake: u128, voted: u64, total: u64) -> NeuronSnapshot {
    NeuronSnapshot {
        neuron_id: id,
        staked_io_e8s: stake,
        eligible_seconds: 100,
        eligible_closed_proposals: total,
        voted_closed_proposals: voted,
        is_genesis_governance_neuron: false,
        is_protocol_owned: false,
        is_dissolving: false,
    }
}

#[test]
fn pocketic_model_full_stream_and_redemption_flow() {
    let mut manager = StreamManager::default_for_tests();
    let faucet = manager
        .process_authorized_stream(StreamKind::JupiterFaucet, t(100), "faucet-1")
        .unwrap();
    assert_eq!(faucet.io_issued_e8s, t(60));

    let two_year = manager
        .process_authorized_stream(StreamKind::TwoYearMaturity, t(100), "2y-1")
        .unwrap();
    assert_eq!(two_year.io_issued_e8s, 0);
    assert_eq!(
        manager
            .state
            .redemption_rate()
            .unwrap()
            .icp_for_io(t(1))
            .unwrap(),
        t(2)
    );

    let two_week = manager
        .process_authorized_stream(StreamKind::TwoWeekMaturity, t(100), "2w-1")
        .unwrap();
    assert_eq!(two_week.io_issued_e8s, t(30));

    let neurons = vec![neuron(10, t(10), 2, 2), neuron(11, t(10), 1, 2)];
    let alloc = manager.allocate_two_week_maturity_io(two_week.io_issued_e8s, &neurons);
    assert_eq!(alloc.allocations[0].io_e8s, t(20));
    assert_eq!(alloc.allocations[1].io_e8s, t(10));

    let redemption = manager.redeem(t(5)).unwrap();
    assert_eq!(redemption.icp_paid_e8s, t(10));
}

#[test]
fn pocketic_scanner_classifies_sources_and_memos() {
    let mut manager = StreamManager::default_for_tests();
    assert_eq!(
        manager
            .process_scanned_icp(JUPITER_FAUCET_SOURCE, "faucet", t(100), "faucet-block")
            .unwrap()
            .io_issued_e8s,
        t(60)
    );
    assert_eq!(
        manager
            .process_scanned_icp(
                IO_NNS_NEURON_MANAGER_SOURCE,
                TWO_YEAR_MATURITY_MEMO,
                t(100),
                "2y-block"
            )
            .unwrap()
            .io_issued_e8s,
        0
    );
    assert_eq!(
        manager
            .process_scanned_icp(
                IO_NNS_NEURON_MANAGER_SOURCE,
                TWO_WEEK_MATURITY_MEMO,
                t(100),
                "2w-block"
            )
            .unwrap()
            .recipient_policy,
        io_core_model::IoRecipientPolicy::EligibleIoSnsNeurons
    );
}

#[test]
fn pocketic_unknown_sender_cannot_issue_io_and_does_not_mark_tx() {
    let mut manager = StreamManager::default_for_tests();
    let err = manager
        .process_scanned_icp("attacker", "faucet", t(100), "attack-block")
        .unwrap_err();
    assert!(matches!(
        err,
        StreamManagerError::UnknownOrUnauthorizedStream { .. }
    ));
    assert!(!manager.processed_transactions.contains("attack-block"));
    assert_eq!(manager.state.redeemable_io_supply_e8s().unwrap(), 0);
}

#[test]
fn pocketic_duplicate_ledger_event_is_idempotently_rejected() {
    let mut manager = StreamManager::default_for_tests();
    manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "block-1")
        .unwrap();
    let before = manager.state;
    let err = manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "block-1")
        .unwrap_err();
    assert_eq!(err, StreamManagerError::DuplicateTransaction);
    assert_eq!(manager.state, before);
}

#[test]
fn pocketic_failed_issuance_is_atomic_and_retryable() {
    let mut manager = StreamManager::default_for_tests();
    manager.state.protocol_reserve_io_e8s = t(1);
    let before = manager.state;
    let err = manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "reserve-fail")
        .unwrap_err();
    assert!(matches!(
        err,
        StreamManagerError::Model(ModelError::InsufficientProtocolReserve { .. })
    ));
    assert_eq!(manager.state, before);
    assert!(!manager.processed_transactions.contains("reserve-fail"));
}

#[test]
fn pocketic_active_stake_snapshot_drives_two_week_target() {
    let mut manager = StreamManager::default_for_tests();
    manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet")
        .unwrap();
    manager
        .process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_YEAR_MATURITY_MEMO,
            t(100),
            "2y",
        )
        .unwrap(); // rate = 2
    let mut dissolving = neuron(12, t(10), 1, 1);
    dissolving.is_dissolving = true;
    let mut genesis = neuron(13, t(10), 1, 1);
    genesis.is_genesis_governance_neuron = true;
    manager.refresh_active_staked_io_from_neurons(&[neuron(10, t(10), 1, 1), dissolving, genesis]);
    assert_eq!(manager.active_staked_io_e8s, t(10));
    assert_eq!(manager.target_two_week_pool_e8s().unwrap(), t(20));
}

#[test]
fn pocketic_two_week_maturity_fails_atomically_when_reward_reserve_is_exhausted() {
    let mut manager = StreamManager::default_for_tests();
    manager
        .process_authorized_stream(StreamKind::JupiterFaucet, t(100), "faucet")
        .unwrap();
    manager.state.protocol_reserve_io_e8s = 1;
    let before = manager.state;
    let err = manager
        .process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_WEEK_MATURITY_MEMO,
            t(100),
            "2w-reserve-fail",
        )
        .unwrap_err();
    assert!(matches!(
        err,
        StreamManagerError::Model(ModelError::InsufficientProtocolReserve { .. })
    ));
    assert_eq!(manager.state, before);
    assert!(!manager.processed_transactions.contains("2w-reserve-fail"));
}

#[test]
fn pocketic_small_amount_streams_preserve_e8s_totals_and_do_not_panic() {
    let mut manager = StreamManager::default_for_tests();
    for amount in 1..100u128 {
        let tx = format!("tiny-{amount}");
        let out = manager
            .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", amount, tx)
            .unwrap();
        assert_eq!(out.split.stake_e8s + out.split.liquid_e8s, amount);
    }
}

#[test]
fn pocketic_later_faucet_stream_after_two_year_maturity_is_not_dilutive() {
    let mut manager = StreamManager::default_for_tests();
    manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet-1")
        .unwrap();
    manager
        .process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_YEAR_MATURITY_MEMO,
            t(100),
            "2y-1",
        )
        .unwrap();
    let rate_before = manager.state.redemption_rate().unwrap();
    let out = manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet-2")
        .unwrap();
    assert_eq!(out.io_issued_e8s, t(30));
    assert_eq!(manager.state.redemption_rate().unwrap(), rate_before);
}

#[test]
fn pocketic_participation_snapshot_penalizes_non_voters_in_two_week_distribution() {
    let mut manager = StreamManager::default_for_tests();
    manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet")
        .unwrap();
    let two_week = manager
        .process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_WEEK_MATURITY_MEMO,
            t(30),
            "2w",
        )
        .unwrap();
    let neurons = vec![
        neuron(1, t(10), 3, 3),
        neuron(2, t(10), 0, 3),
        neuron(3, t(10), 1, 3),
    ];
    let out = manager.allocate_two_week_maturity_io(two_week.io_issued_e8s, &neurons);
    assert_eq!(
        out.allocations
            .iter()
            .map(|a| a.neuron_id)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
    assert!(out.allocations[0].io_e8s > out.allocations[1].io_e8s);
}

#[test]
fn pocketic_blank_transaction_id_is_rejected_and_not_recorded() {
    let mut manager = StreamManager::default_for_tests();
    let before = manager.state;
    assert_eq!(
        manager
            .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "")
            .unwrap_err(),
        StreamManagerError::InvalidTransactionId
    );
    assert_eq!(manager.state, before);
    assert!(manager.processed_transactions.is_empty());
}

#[test]
fn pocketic_backing_fraction_above_one_hundred_percent_is_rejected() {
    let mut manager = StreamManager::default_for_tests();
    manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet")
        .unwrap();
    manager.two_week_pool_backing_bps = 20_000;
    assert_eq!(
        manager.target_two_week_pool_e8s().unwrap_err(),
        StreamManagerError::Model(ModelError::InvalidBasisPoints { bps: 20_000 })
    );
}
