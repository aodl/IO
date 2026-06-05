pub mod logic;
pub mod scheduler;
pub mod state;

use candid::{CandidType, Principal};
use serde::Deserialize;
use std::cell::RefCell;

pub use io_core_model::{
    IoRecipientPolicy, ModelError, ProtocolState, RedemptionOutcome, RedemptionRate, Split,
    StreamKind, StreamOutcome, E8S_PER_TOKEN,
};
pub use logic::StreamManagerError;
pub use state::StreamManager;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct InitArgs {
    pub initial_total_io_supply_e8s: u128,
    pub initial_protocol_reserve_io_e8s: u128,
    pub non_redeemable_governance_io_e8s: u128,
    pub two_week_pool_backing_bps: u128,
    pub jupiter_faucet_principal_text: Option<String>,
    pub io_nns_neuron_manager_principal_text: Option<String>,
    pub icp_ledger_principal_text: Option<String>,
    pub icp_index_principal_text: Option<String>,
    pub io_ledger_principal_text: Option<String>,
    pub io_index_principal_text: Option<String>,
}

impl Default for InitArgs {
    fn default() -> Self {
        Self {
            initial_total_io_supply_e8s: 1_000_000 * E8S_PER_TOKEN,
            initial_protocol_reserve_io_e8s: 900_000 * E8S_PER_TOKEN,
            non_redeemable_governance_io_e8s: 100_000 * E8S_PER_TOKEN,
            two_week_pool_backing_bps: 10_000,
            jupiter_faucet_principal_text: None,
            io_nns_neuron_manager_principal_text: None,
            icp_ledger_principal_text: None,
            icp_index_principal_text: None,
            io_ledger_principal_text: None,
            io_index_principal_text: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StreamManagerConfig {
    pub initial_total_io_supply_e8s: u128,
    pub initial_protocol_reserve_io_e8s: u128,
    pub non_redeemable_governance_io_e8s: u128,
    pub two_week_pool_backing_bps: u128,
    pub jupiter_faucet_principal_text: Option<String>,
    pub io_nns_neuron_manager_principal_text: Option<String>,
    pub icp_ledger_principal_text: Option<String>,
    pub icp_index_principal_text: Option<String>,
    pub io_ledger_principal_text: Option<String>,
    pub io_index_principal_text: Option<String>,
}

impl Default for StreamManagerConfig {
    fn default() -> Self {
        InitArgs::default()
            .try_into()
            .expect("default stream-manager config must be valid")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitArgsError {
    ExcludedSupplyExceedsTotal,
    InvalidBasisPoints { bps: u128 },
    InvalidPrincipalText { field: &'static str, value: String },
}

impl TryFrom<InitArgs> for StreamManagerConfig {
    type Error = InitArgsError;

    fn try_from(args: InitArgs) -> Result<Self, Self::Error> {
        let excluded_supply = args
            .initial_protocol_reserve_io_e8s
            .checked_add(args.non_redeemable_governance_io_e8s)
            .ok_or(InitArgsError::ExcludedSupplyExceedsTotal)?;
        if args.initial_total_io_supply_e8s < excluded_supply {
            return Err(InitArgsError::ExcludedSupplyExceedsTotal);
        }
        if args.two_week_pool_backing_bps > 10_000 {
            return Err(InitArgsError::InvalidBasisPoints {
                bps: args.two_week_pool_backing_bps,
            });
        }

        validate_optional_principal(
            "jupiter_faucet_principal_text",
            &args.jupiter_faucet_principal_text,
        )?;
        validate_optional_principal(
            "io_nns_neuron_manager_principal_text",
            &args.io_nns_neuron_manager_principal_text,
        )?;
        validate_optional_principal("icp_ledger_principal_text", &args.icp_ledger_principal_text)?;
        validate_optional_principal("icp_index_principal_text", &args.icp_index_principal_text)?;
        validate_optional_principal("io_ledger_principal_text", &args.io_ledger_principal_text)?;
        validate_optional_principal("io_index_principal_text", &args.io_index_principal_text)?;

        Ok(Self {
            initial_total_io_supply_e8s: args.initial_total_io_supply_e8s,
            initial_protocol_reserve_io_e8s: args.initial_protocol_reserve_io_e8s,
            non_redeemable_governance_io_e8s: args.non_redeemable_governance_io_e8s,
            two_week_pool_backing_bps: args.two_week_pool_backing_bps,
            jupiter_faucet_principal_text: args.jupiter_faucet_principal_text,
            io_nns_neuron_manager_principal_text: args.io_nns_neuron_manager_principal_text,
            icp_ledger_principal_text: args.icp_ledger_principal_text,
            icp_index_principal_text: args.icp_index_principal_text,
            io_ledger_principal_text: args.io_ledger_principal_text,
            io_index_principal_text: args.io_index_principal_text,
        })
    }
}

fn validate_optional_principal(
    field: &'static str,
    value: &Option<String>,
) -> Result<(), InitArgsError> {
    if let Some(text) = value {
        if text.trim().is_empty() || Principal::from_text(text).is_err() {
            return Err(InitArgsError::InvalidPrincipalText {
                field,
                value: text.clone(),
            });
        }
    }
    Ok(())
}

#[cfg_attr(not(any(test, debug_assertions)), allow(dead_code))]
#[derive(Clone, Debug)]
struct CanisterState {
    config: StreamManagerConfig,
    manager: StreamManager,
}

impl CanisterState {
    fn new(config: StreamManagerConfig) -> Self {
        let manager = StreamManager {
            state: ProtocolState::new(
                config.initial_total_io_supply_e8s,
                config.initial_protocol_reserve_io_e8s,
                config.non_redeemable_governance_io_e8s,
            ),
            processed_transactions: Default::default(),
            active_staked_io_e8s: 0,
            two_week_pool_backing_bps: config.two_week_pool_backing_bps,
        };
        Self { config, manager }
    }
}

impl Default for CanisterState {
    fn default() -> Self {
        Self::new(StreamManagerConfig::default())
    }
}

thread_local! {
    static CANISTER_STATE: RefCell<CanisterState> = RefCell::new(CanisterState::default());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StableProtocolState {
    pub liquid_icp_e8s: u128,
    pub two_year_staked_icp_e8s: u128,
    pub two_week_staked_icp_e8s: u128,
    pub total_io_supply_e8s: u128,
    pub protocol_reserve_io_e8s: u128,
    pub non_redeemable_governance_io_e8s: u128,
}

impl From<ProtocolState> for StableProtocolState {
    fn from(value: ProtocolState) -> Self {
        Self {
            liquid_icp_e8s: value.liquid_icp_e8s,
            two_year_staked_icp_e8s: value.two_year_staked_icp_e8s,
            two_week_staked_icp_e8s: value.two_week_staked_icp_e8s,
            total_io_supply_e8s: value.total_io_supply_e8s,
            protocol_reserve_io_e8s: value.protocol_reserve_io_e8s,
            non_redeemable_governance_io_e8s: value.non_redeemable_governance_io_e8s,
        }
    }
}

impl From<StableProtocolState> for ProtocolState {
    fn from(value: StableProtocolState) -> Self {
        Self {
            liquid_icp_e8s: value.liquid_icp_e8s,
            two_year_staked_icp_e8s: value.two_year_staked_icp_e8s,
            two_week_staked_icp_e8s: value.two_week_staked_icp_e8s,
            total_io_supply_e8s: value.total_io_supply_e8s,
            protocol_reserve_io_e8s: value.protocol_reserve_io_e8s,
            non_redeemable_governance_io_e8s: value.non_redeemable_governance_io_e8s,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StableState {
    pub config: StreamManagerConfig,
    pub protocol: StableProtocolState,
    pub processed_transactions: Vec<String>,
    pub active_staked_io_e8s: u128,
    pub two_week_pool_backing_bps: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ApiStreamKind {
    JupiterFaucet,
    TwoYearMaturity,
    TwoWeekMaturity,
}

impl From<ApiStreamKind> for StreamKind {
    fn from(value: ApiStreamKind) -> Self {
        match value {
            ApiStreamKind::JupiterFaucet => StreamKind::JupiterFaucet,
            ApiStreamKind::TwoYearMaturity => StreamKind::TwoYearMaturity,
            ApiStreamKind::TwoWeekMaturity => StreamKind::TwoWeekMaturity,
        }
    }
}

impl From<StreamKind> for ApiStreamKind {
    fn from(value: StreamKind) -> Self {
        match value {
            StreamKind::JupiterFaucet => ApiStreamKind::JupiterFaucet,
            StreamKind::TwoYearMaturity => ApiStreamKind::TwoYearMaturity,
            StreamKind::TwoWeekMaturity => ApiStreamKind::TwoWeekMaturity,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub enum ApiIoRecipientPolicy {
    JupiterFaucet,
    EligibleIoSnsNeurons,
    None,
}

impl From<IoRecipientPolicy> for ApiIoRecipientPolicy {
    fn from(value: IoRecipientPolicy) -> Self {
        match value {
            IoRecipientPolicy::JupiterFaucet => ApiIoRecipientPolicy::JupiterFaucet,
            IoRecipientPolicy::EligibleIoSnsNeurons => ApiIoRecipientPolicy::EligibleIoSnsNeurons,
            IoRecipientPolicy::None => ApiIoRecipientPolicy::None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ProcessStreamEventRequest {
    pub kind: ApiStreamKind,
    pub amount_e8s: u128,
    pub transaction_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiProtocolState {
    pub liquid_icp_e8s: u128,
    pub two_year_staked_icp_e8s: u128,
    pub two_week_staked_icp_e8s: u128,
    pub total_io_supply_e8s: u128,
    pub protocol_reserve_io_e8s: u128,
    pub non_redeemable_governance_io_e8s: u128,
}

impl From<ProtocolState> for ApiProtocolState {
    fn from(value: ProtocolState) -> Self {
        Self {
            liquid_icp_e8s: value.liquid_icp_e8s,
            two_year_staked_icp_e8s: value.two_year_staked_icp_e8s,
            two_week_staked_icp_e8s: value.two_week_staked_icp_e8s,
            total_io_supply_e8s: value.total_io_supply_e8s,
            protocol_reserve_io_e8s: value.protocol_reserve_io_e8s,
            non_redeemable_governance_io_e8s: value.non_redeemable_governance_io_e8s,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiRedemptionRate {
    pub liquid_icp_e8s: u128,
    pub redeemable_io_e8s: u128,
}

impl From<RedemptionRate> for ApiRedemptionRate {
    fn from(value: RedemptionRate) -> Self {
        Self {
            liquid_icp_e8s: value.liquid_icp_e8s,
            redeemable_io_e8s: value.redeemable_io_e8s,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiSplit {
    pub stake_e8s: u128,
    pub liquid_e8s: u128,
}

impl From<Split> for ApiSplit {
    fn from(value: Split) -> Self {
        Self {
            stake_e8s: value.stake_e8s,
            liquid_e8s: value.liquid_e8s,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiStreamOutcome {
    pub kind: ApiStreamKind,
    pub split: ApiSplit,
    pub recipient_policy: ApiIoRecipientPolicy,
    pub io_issued_e8s: u128,
    pub rate_before: ApiRedemptionRate,
    pub rate_after: ApiRedemptionRate,
}

impl From<StreamOutcome> for ApiStreamOutcome {
    fn from(value: StreamOutcome) -> Self {
        Self {
            kind: value.kind.into(),
            split: value.split.into(),
            recipient_policy: value.recipient_policy.into(),
            io_issued_e8s: value.io_issued_e8s,
            rate_before: value.rate_before.into(),
            rate_after: value.rate_after.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiRedemptionOutcome {
    pub io_redeemed_e8s: u128,
    pub icp_paid_e8s: u128,
    pub rate_before: ApiRedemptionRate,
    pub rate_after: ApiRedemptionRate,
}

impl From<RedemptionOutcome> for ApiRedemptionOutcome {
    fn from(value: RedemptionOutcome) -> Self {
        Self {
            io_redeemed_e8s: value.io_redeemed_e8s,
            icp_paid_e8s: value.icp_paid_e8s,
            rate_before: value.rate_before.into(),
            rate_after: value.rate_after.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiState {
    pub config: StreamManagerConfig,
    pub protocol: ApiProtocolState,
    pub processed_transaction_count: u64,
    pub active_staked_io_e8s: u128,
    pub two_week_pool_backing_bps: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl ApiError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<ModelError> for ApiError {
    fn from(value: ModelError) -> Self {
        Self::new("model_error", format!("{value:?}"))
    }
}

impl From<StreamManagerError> for ApiError {
    fn from(value: StreamManagerError) -> Self {
        match value {
            StreamManagerError::DuplicateTransaction => {
                Self::new("duplicate_transaction", "transaction was already processed")
            }
            StreamManagerError::InvalidTransactionId => {
                Self::new("invalid_transaction_id", "transaction id must be non-empty")
            }
            StreamManagerError::UnknownOrUnauthorizedStream { source, memo } => Self::new(
                "unknown_or_unauthorized_stream",
                format!("stream source {source:?} with memo {memo:?} is not authorized"),
            ),
            StreamManagerError::Model(err) => err.into(),
        }
    }
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg_attr(target_family = "wasm", ic_cdk::init)]
pub fn init(args: InitArgs) {
    let config = StreamManagerConfig::try_from(args).expect("invalid io_stream_manager init args");
    CANISTER_STATE.with(|cell| {
        *cell.borrow_mut() = CanisterState::new(config);
    });
}

fn export_stable_state() -> StableState {
    CANISTER_STATE.with(|cell| {
        let state = cell.borrow();
        StableState {
            config: state.config.clone(),
            protocol: state.manager.state.into(),
            processed_transactions: state
                .manager
                .processed_transactions
                .iter()
                .cloned()
                .collect(),
            active_staked_io_e8s: state.manager.active_staked_io_e8s,
            two_week_pool_backing_bps: state.manager.two_week_pool_backing_bps,
        }
    })
}

fn import_stable_state(state: StableState) {
    CANISTER_STATE.with(|cell| {
        *cell.borrow_mut() = CanisterState {
            config: state.config,
            manager: StreamManager {
                state: state.protocol.into(),
                processed_transactions: state.processed_transactions.into_iter().collect(),
                active_staked_io_e8s: state.active_staked_io_e8s,
                two_week_pool_backing_bps: state.two_week_pool_backing_bps,
            },
        };
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::pre_upgrade)]
pub fn pre_upgrade() {
    ic_cdk::storage::stable_save((export_stable_state(),))
        .expect("failed to save io_stream_manager stable state");
}

#[cfg_attr(target_family = "wasm", ic_cdk::post_upgrade)]
pub fn post_upgrade() {
    if let Ok((state,)) = ic_cdk::storage::stable_restore::<(StableState,)>() {
        import_stable_state(state);
    }
}

#[cfg(any(test, debug_assertions))]
pub fn export_stable_state_for_tests() -> StableState {
    export_stable_state()
}

#[cfg(any(test, debug_assertions))]
pub fn import_stable_state_for_tests(state: StableState) {
    import_stable_state(state);
}

#[cfg(any(test, debug_assertions))]
fn state_snapshot() -> ApiState {
    CANISTER_STATE.with(|cell| {
        let state = cell.borrow();
        ApiState {
            config: state.config.clone(),
            protocol: state.manager.state.into(),
            processed_transaction_count: state.manager.processed_transactions.len() as u64,
            active_staked_io_e8s: state.manager.active_staked_io_e8s,
            two_week_pool_backing_bps: state.manager.two_week_pool_backing_bps,
        }
    })
}

#[cfg(any(test, debug_assertions))]
fn redemption_rate() -> Result<ApiRedemptionRate, ApiError> {
    CANISTER_STATE.with(|cell| {
        cell.borrow()
            .manager
            .state
            .redemption_rate()
            .map(ApiRedemptionRate::from)
            .map_err(ApiError::from)
    })
}

#[cfg(any(test, debug_assertions))]
fn process_stream_event_impl(
    request: ProcessStreamEventRequest,
) -> Result<ApiStreamOutcome, ApiError> {
    CANISTER_STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state
            .manager
            .process_authorized_stream(
                request.kind.into(),
                request.amount_e8s,
                request.transaction_id,
            )
            .map(ApiStreamOutcome::from)
            .map_err(ApiError::from)
    })
}

#[cfg(any(test, debug_assertions))]
fn redeem_impl(io_e8s: u128) -> Result<ApiRedemptionOutcome, ApiError> {
    CANISTER_STATE.with(|cell| {
        cell.borrow_mut()
            .manager
            .redeem(io_e8s)
            .map(ApiRedemptionOutcome::from)
            .map_err(ApiError::from)
    })
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_state() -> ApiState {
    state_snapshot()
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn debug_get_redemption_rate() -> Result<ApiRedemptionRate, ApiError> {
    redemption_rate()
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_process_stream_event(
    request: ProcessStreamEventRequest,
) -> Result<ApiStreamOutcome, ApiError> {
    process_stream_event_impl(request)
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub fn debug_redeem(io_e8s: u128) -> Result<ApiRedemptionOutcome, ApiError> {
    redeem_impl(io_e8s)
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

    #[test]
    fn canister_api_initializes_and_reports_state() {
        init(InitArgs::default());
        let state = debug_get_state();
        assert_eq!(
            state.protocol.total_io_supply_e8s,
            1_000_000 * E8S_PER_TOKEN
        );
        assert_eq!(state.processed_transaction_count, 0);
        assert_eq!(
            debug_get_redemption_rate().unwrap(),
            RedemptionRate::one_to_one().into()
        );
    }

    #[test]
    fn canister_api_processes_stream_and_redeems() {
        init(InitArgs::default());
        let outcome = debug_process_stream_event(ProcessStreamEventRequest {
            kind: ApiStreamKind::JupiterFaucet,
            amount_e8s: t(100),
            transaction_id: "api-tx-1".to_string(),
        })
        .unwrap();
        assert_eq!(outcome.io_issued_e8s, t(60));
        assert_eq!(
            debug_process_stream_event(ProcessStreamEventRequest {
                kind: ApiStreamKind::JupiterFaucet,
                amount_e8s: t(100),
                transaction_id: "api-tx-1".to_string(),
            })
            .unwrap_err()
            .code,
            "duplicate_transaction"
        );

        let redemption = debug_redeem(t(10)).unwrap();
        assert_eq!(redemption.icp_paid_e8s, t(10));
        assert_eq!(debug_get_state().processed_transaction_count, 1);
    }

    #[test]
    fn init_rejects_supply_and_bps_config_that_cannot_be_valid() {
        let args = InitArgs {
            initial_total_io_supply_e8s: 10,
            initial_protocol_reserve_io_e8s: 9,
            non_redeemable_governance_io_e8s: 2,
            ..InitArgs::default()
        };
        assert_eq!(
            StreamManagerConfig::try_from(args).unwrap_err(),
            InitArgsError::ExcludedSupplyExceedsTotal
        );

        let args = InitArgs {
            two_week_pool_backing_bps: 10_001,
            ..InitArgs::default()
        };
        assert_eq!(
            StreamManagerConfig::try_from(args).unwrap_err(),
            InitArgsError::InvalidBasisPoints { bps: 10_001 }
        );
    }

    #[test]
    fn init_rejects_invalid_optional_principal_text() {
        let args = InitArgs {
            jupiter_faucet_principal_text: Some("not a principal".to_string()),
            ..InitArgs::default()
        };
        assert_eq!(
            StreamManagerConfig::try_from(args).unwrap_err(),
            InitArgsError::InvalidPrincipalText {
                field: "jupiter_faucet_principal_text",
                value: "not a principal".to_string()
            }
        );
    }

    #[test]
    fn stable_state_round_trip_preserves_config_accounting_and_processed_txs() {
        init(InitArgs {
            initial_total_io_supply_e8s: t(2_000),
            initial_protocol_reserve_io_e8s: t(1_200),
            non_redeemable_governance_io_e8s: t(300),
            two_week_pool_backing_bps: 7_500,
            jupiter_faucet_principal_text: Some("oae4c-3iaaa-aaaar-qb5qq-cai".to_string()),
            ..InitArgs::default()
        });
        debug_process_stream_event(ProcessStreamEventRequest {
            kind: ApiStreamKind::JupiterFaucet,
            amount_e8s: t(100),
            transaction_id: "stable-tx-1".to_string(),
        })
        .unwrap();
        debug_redeem(t(10)).unwrap();
        let before_state = debug_get_state();
        let before_rate = debug_get_redemption_rate().unwrap();
        let stable = export_stable_state_for_tests();

        init(InitArgs::default());
        assert_ne!(debug_get_state(), before_state);

        import_stable_state_for_tests(stable);
        assert_eq!(debug_get_state(), before_state);
        assert_eq!(debug_get_redemption_rate().unwrap(), before_rate);
        assert_eq!(debug_get_state().processed_transaction_count, 1);
    }

    #[test]
    fn scheduler_tick_does_not_mutate_value_moving_state() {
        init(InitArgs::default());
        let before = export_stable_state_for_tests();
        let outcome = crate::scheduler::scheduler_tick_once();
        assert_eq!(outcome.processed_authorized_streams, 0);
        assert_eq!(export_stable_state_for_tests(), before);
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
