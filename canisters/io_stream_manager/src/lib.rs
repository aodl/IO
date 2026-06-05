pub mod logic;
pub mod state;

pub use io_core_model::{
    ModelError, ProtocolState, RedemptionOutcome, StreamKind, StreamOutcome, E8S_PER_TOKEN,
};
pub use logic::StreamManagerError;
pub use state::StreamManager;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{
        IO_NNS_NEURON_MANAGER_SOURCE, JUPITER_FAUCET_SOURCE, TWO_YEAR_MATURITY_MEMO,
    };
    fn t(n: u128) -> u128 {
        n * E8S_PER_TOKEN
    }

    #[test]
    fn manager_accepts_faucet_stream() {
        let mut m = StreamManager::default_for_tests();
        let out = m
            .process_authorized_stream(StreamKind::JupiterFaucet, t(100), "tx-1")
            .unwrap();
        assert_eq!(out.io_issued_e8s, t(60));
        assert!(matches!(
            m.process_authorized_stream(StreamKind::JupiterFaucet, t(100), "tx-1"),
            Err(StreamManagerError::DuplicateTransaction)
        ));
    }

    #[test]
    fn manager_redeems_to_reserve() {
        let mut m = StreamManager::default_for_tests();
        m.process_authorized_stream(StreamKind::JupiterFaucet, t(100), "tx-1")
            .unwrap();
        let out = m.redeem(t(10)).unwrap();
        assert_eq!(out.icp_paid_e8s, t(10));
        assert_eq!(m.state.protocol_reserve_io_e8s, t(899_950));
    }

    #[test]
    fn scanned_source_and_memo_classify_streams() {
        assert_eq!(
            StreamManager::classify_stream(JUPITER_FAUCET_SOURCE, "").unwrap(),
            StreamKind::JupiterFaucet
        );
        assert_eq!(
            StreamManager::classify_stream(IO_NNS_NEURON_MANAGER_SOURCE, TWO_YEAR_MATURITY_MEMO)
                .unwrap(),
            StreamKind::TwoYearMaturity
        );
        assert!(matches!(
            StreamManager::classify_stream("unknown", ""),
            Err(StreamManagerError::UnknownOrUnauthorizedStream { .. })
        ));
    }

    #[test]
    fn failed_stream_does_not_mark_transaction_processed() {
        let mut m = StreamManager::default_for_tests();
        m.state.protocol_reserve_io_e8s = t(1);
        let err = m
            .process_authorized_stream(StreamKind::JupiterFaucet, t(100), "bad-tx")
            .unwrap_err();
        assert!(matches!(
            err,
            StreamManagerError::Model(ModelError::InsufficientProtocolReserve { .. })
        ));
        assert!(!m.processed_transactions.contains("bad-tx"));
    }
}

#[cfg(test)]
mod additional_stream_manager_tests {
    use super::*;
    use crate::state::{
        IO_NNS_NEURON_MANAGER_SOURCE, JUPITER_FAUCET_SOURCE, TWO_WEEK_MATURITY_MEMO,
        TWO_YEAR_MATURITY_MEMO,
    };
    use io_reward_policy::NeuronSnapshot;

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
    fn unknown_memo_from_authorized_nns_manager_is_rejected() {
        let mut m = StreamManager::default_for_tests();
        let err = m
            .process_scanned_icp(
                IO_NNS_NEURON_MANAGER_SOURCE,
                "unexpected",
                t(100),
                "bad-memo",
            )
            .unwrap_err();
        assert!(matches!(
            err,
            StreamManagerError::UnknownOrUnauthorizedStream { .. }
        ));
        assert!(!m.processed_transactions.contains("bad-memo"));
    }

    #[test]
    fn same_transaction_id_cannot_be_reused_across_stream_kinds() {
        let mut m = StreamManager::default_for_tests();
        m.process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "ledger-block-1")
            .unwrap();
        assert_eq!(
            m.process_scanned_icp(
                IO_NNS_NEURON_MANAGER_SOURCE,
                TWO_YEAR_MATURITY_MEMO,
                t(100),
                "ledger-block-1"
            )
            .unwrap_err(),
            StreamManagerError::DuplicateTransaction
        );
    }

    #[test]
    fn two_year_stream_does_not_consume_io_reserve() {
        let mut m = StreamManager::default_for_tests();
        let before_reserve = m.state.protocol_reserve_io_e8s;
        m.process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_YEAR_MATURITY_MEMO,
            t(100),
            "2y",
        )
        .unwrap();
        assert_eq!(m.state.protocol_reserve_io_e8s, before_reserve);
    }

    #[test]
    fn two_week_stream_consumes_io_reserve_but_preserves_rate() {
        let mut m = StreamManager::default_for_tests();
        m.process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "faucet")
            .unwrap();
        m.process_scanned_icp(
            IO_NNS_NEURON_MANAGER_SOURCE,
            TWO_YEAR_MATURITY_MEMO,
            t(100),
            "2y",
        )
        .unwrap();
        let rate_before = m.state.redemption_rate().unwrap();
        let reserve_before = m.state.protocol_reserve_io_e8s;
        let out = m
            .process_scanned_icp(
                IO_NNS_NEURON_MANAGER_SOURCE,
                TWO_WEEK_MATURITY_MEMO,
                t(100),
                "2w",
            )
            .unwrap();
        assert!(out.io_issued_e8s > 0);
        assert_eq!(
            m.state.protocol_reserve_io_e8s,
            reserve_before - out.io_issued_e8s
        );
        assert_eq!(m.state.redemption_rate().unwrap(), rate_before);
    }

    #[test]
    fn half_backing_fraction_halves_two_week_target() {
        let mut m = StreamManager::default_for_tests();
        m.process_authorized_stream(StreamKind::JupiterFaucet, t(100), "faucet")
            .unwrap();
        m.two_week_pool_backing_bps = 5_000;
        m.refresh_active_staked_io_from_neurons(&[neuron(1, t(20), 1, 1)]);
        assert_eq!(m.target_two_week_pool_e8s().unwrap(), t(10));
    }

    #[test]
    fn reward_allocation_with_no_eligible_neurons_keeps_pool_as_dust() {
        let m = StreamManager::default_for_tests();
        let mut genesis = neuron(1, t(10), 1, 1);
        genesis.is_genesis_governance_neuron = true;
        let out = m.allocate_two_week_maturity_io(t(5), &[genesis]);
        assert!(out.allocations.is_empty());
        assert_eq!(out.dust_e8s, t(5));
    }

    #[test]
    fn redemption_failure_is_retryable_with_same_user_intent() {
        let mut m = StreamManager::default_for_tests();
        m.process_authorized_stream(StreamKind::JupiterFaucet, t(100), "faucet")
            .unwrap();
        let before = m.state;
        let err = m.redeem(t(100)).unwrap_err();
        assert!(matches!(
            err,
            StreamManagerError::Model(ModelError::InsufficientLiquidReserve { .. })
        ));
        assert_eq!(m.state, before);
        let ok = m.redeem(t(10)).unwrap();
        assert_eq!(ok.icp_paid_e8s, t(10));
    }

    #[test]
    fn empty_or_whitespace_transaction_ids_are_rejected_before_state_changes() {
        let mut m = StreamManager::default_for_tests();
        let before = m.state;
        assert_eq!(
            m.process_scanned_icp(JUPITER_FAUCET_SOURCE, "", t(100), "   ")
                .unwrap_err(),
            StreamManagerError::InvalidTransactionId
        );
        assert_eq!(m.state, before);
        assert!(m.processed_transactions.is_empty());
    }

    #[test]
    fn invalid_two_week_backing_fraction_surfaces_as_model_error() {
        let mut m = StreamManager::default_for_tests();
        m.process_authorized_stream(StreamKind::JupiterFaucet, t(100), "faucet")
            .unwrap();
        m.two_week_pool_backing_bps = 10_001;
        let err = m.target_two_week_pool_e8s().unwrap_err();
        assert_eq!(
            err,
            StreamManagerError::Model(ModelError::InvalidBasisPoints { bps: 10_001 })
        );
    }

    #[test]
    fn source_classification_is_case_sensitive_and_strict() {
        assert!(StreamManager::classify_stream("JUPITER_FAUCET", "").is_err());
        assert!(
            StreamManager::classify_stream(JUPITER_FAUCET_SOURCE, TWO_YEAR_MATURITY_MEMO).is_err()
        );
        assert!(StreamManager::classify_stream(IO_NNS_NEURON_MANAGER_SOURCE, "").is_err());
    }
}
