use io_core_model::{StreamKind, E8S_PER_TOKEN};
use io_stream_manager::state::{
    IO_NNS_NEURON_MANAGER_SOURCE, JUPITER_FAUCET_SOURCE, TWO_WEEK_MATURITY_MEMO,
    TWO_YEAR_MATURITY_MEMO,
};
use io_stream_manager::{StreamManager, StreamManagerError};

fn t(n: u128) -> u128 {
    n * E8S_PER_TOKEN
}

#[test]
fn cli_model_duplicate_transactions_are_rejected() {
    let mut manager = StreamManager::default_for_tests();
    manager
        .process_authorized_stream(StreamKind::JupiterFaucet, t(1), "block-1")
        .unwrap();
    assert!(matches!(
        manager.process_authorized_stream(StreamKind::JupiterFaucet, t(1), "block-1"),
        Err(StreamManagerError::DuplicateTransaction)
    ));
}

#[test]
fn cli_model_authorized_scanner_flow_matches_expected_io_issuance() {
    let mut manager = StreamManager::default_for_tests();
    assert_eq!(
        manager
            .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "1")
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
                "2"
            )
            .unwrap()
            .io_issued_e8s,
        0
    );
    assert_eq!(
        manager
            .state
            .redemption_rate()
            .unwrap()
            .icp_for_io(t(1))
            .unwrap(),
        t(2)
    );
    assert_eq!(
        manager
            .process_scanned_icp(
                IO_NNS_NEURON_MANAGER_SOURCE,
                TWO_WEEK_MATURITY_MEMO,
                t(100),
                "3"
            )
            .unwrap()
            .io_issued_e8s,
        t(30)
    );
}

#[test]
fn cli_model_unknown_stream_is_rejected_before_processing() {
    let mut manager = StreamManager::default_for_tests();
    let err = manager
        .process_scanned_icp("someone_else", "", t(100), "bad")
        .unwrap_err();
    assert!(matches!(
        err,
        StreamManagerError::UnknownOrUnauthorizedStream { .. }
    ));
    assert!(manager.processed_transactions.is_empty());
}

#[test]
fn cli_model_bad_nns_memo_is_rejected_before_state_mutation() {
    let mut manager = StreamManager::default_for_tests();
    let before = manager.state;
    let err = manager
        .process_scanned_icp(IO_NNS_NEURON_MANAGER_SOURCE, "wrong", t(100), "bad-nns")
        .unwrap_err();
    assert!(matches!(
        err,
        StreamManagerError::UnknownOrUnauthorizedStream { .. }
    ));
    assert_eq!(manager.state, before);
    assert!(manager.processed_transactions.is_empty());
}

#[test]
fn cli_model_zero_amount_transaction_is_idempotent_noop() {
    let mut manager = StreamManager::default_for_tests();
    let before = manager.state;
    let out = manager
        .process_scanned_icp(JUPITER_FAUCET_SOURCE, "", 0, "zero")
        .unwrap();
    assert_eq!(out.io_issued_e8s, 0);
    assert_eq!(manager.state, before);
    assert!(matches!(
        manager.process_scanned_icp(JUPITER_FAUCET_SOURCE, "", 0, "zero"),
        Err(StreamManagerError::DuplicateTransaction)
    ));
}
