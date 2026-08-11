#[cfg(target_family = "wasm")]
mod adapters;
mod model;

pub use model::*;

use candid::{CandidType, Decode, Encode};
use io_stable_schema::IO_HISTORIAN_SCHEMA_VERSION;
use serde::Deserialize;
use std::cell::{Cell, RefCell};

pub const HISTORIAN_SCHEMA_VERSION: u32 = IO_HISTORIAN_SCHEMA_VERSION;
const SOURCE_NAMES: &[&str] = &[
    "protocol",
    "stream",
    "nns-manager",
    "nns-governance",
    "sns-root",
    "sns-governance",
    "sns-index",
];

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct StableState {
    pub schema_version: u32,
    pub config: Option<ObservationConfig>,
    pub protocol: ProtocolSnapshot,
    pub source_health: Vec<SourceHealth>,
    pub canisters: Vec<CanisterObservation>,
    pub stream: Option<StreamStatus>,
    pub nns_manager: Option<NnsManagerStatus>,
    pub nns_governance: Option<NnsGovernanceStatus>,
    pub sns: Option<SnsStatus>,
    pub index: Option<IndexStatus>,
    pub refresh_generation: u64,
    pub last_attempt_timestamp_nanos: Option<u64>,
    pub last_success_timestamp_nanos: Option<u64>,
}

impl Default for StableState {
    fn default() -> Self {
        Self {
            schema_version: HISTORIAN_SCHEMA_VERSION,
            config: None,
            protocol: ProtocolSnapshot::default(),
            source_health: SOURCE_NAMES
                .iter()
                .map(|name| SourceHealth::prelaunch(name))
                .collect(),
            canisters: Vec::new(),
            stream: None,
            nns_manager: None,
            nns_governance: None,
            sns: None,
            index: None,
            refresh_generation: 0,
            last_attempt_timestamp_nanos: None,
            last_success_timestamp_nanos: None,
        }
    }
}

thread_local! {
    static STATE: RefCell<StableState> = RefCell::new(StableState::default());
    static REFRESH_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_public_status() -> PublicStatus {
    STATE.with(|cell| {
        let state = cell.borrow();
        PublicStatus {
            version: version().into(),
            schema_version: state.schema_version,
            configured: state.config.is_some(),
            refresh_active: REFRESH_ACTIVE.with(Cell::get),
            refresh_generation: state.refresh_generation,
            last_attempt_timestamp_nanos: state.last_attempt_timestamp_nanos,
            last_success_timestamp_nanos: state.last_success_timestamp_nanos,
        }
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_dashboard_state() -> Dashboard {
    STATE.with(|cell| {
        let state = cell.borrow();
        #[cfg(target_family = "wasm")]
        let now = ic_cdk::api::time();
        #[cfg(not(target_family = "wasm"))]
        let now = state.last_attempt_timestamp_nanos.unwrap_or_default();
        Dashboard {
            status: get_public_status(),
            protocol: state.protocol.clone(),
            source_health: visible_source_health(&state, now),
            canisters: state.canisters.clone(),
            stream: state.stream.clone(),
            nns_manager: state.nns_manager.clone(),
            nns_governance: state.nns_governance.clone(),
            sns: state.sns.clone(),
            index: state.index.clone(),
        }
    })
}

fn visible_source_health(state: &StableState, now: u64) -> Vec<SourceHealth> {
    let mut health = state.source_health.clone();
    let Some(config) = &state.config else {
        return health;
    };
    let stale_after = config
        .refresh_interval_seconds
        .saturating_mul(2)
        .saturating_mul(1_000_000_000);
    health.iter_mut().for_each(|source| {
        if source.freshness == ObservationFreshness::Fresh
            && source
                .last_success_timestamp_nanos
                .is_none_or(|success| now.saturating_sub(success) > stale_after)
        {
            source.freshness = ObservationFreshness::Stale;
        }
    });
    health
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_protocol_snapshot() -> ProtocolSnapshot {
    STATE.with(|cell| cell.borrow().protocol.clone())
}

#[cfg_attr(target_family = "wasm", ic_cdk::query)]
pub fn get_redemption_rate() -> Option<RedemptionRateSnapshot> {
    STATE.with(|cell| cell.borrow().protocol.redemption_rate.clone())
}

fn install_config(config: Option<ObservationConfig>) {
    let Some(config) = config else { return };
    #[cfg(target_family = "wasm")]
    let self_id = Some(ic_cdk::api::canister_self());
    #[cfg(not(target_family = "wasm"))]
    let self_id = None;
    validate_config(&config, self_id).expect("invalid historian observation configuration");
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.config.as_ref() != Some(&config) {
            *state = StableState {
                config: Some(config),
                source_health: SOURCE_NAMES
                    .iter()
                    .map(|name| SourceHealth {
                        source: (*name).into(),
                        freshness: ObservationFreshness::Missing,
                        last_attempt_timestamp_nanos: None,
                        last_success_timestamp_nanos: None,
                        error: None,
                    })
                    .collect(),
                ..StableState::default()
            };
        }
    });
}

#[cfg_attr(target_family = "wasm", ic_cdk::init)]
pub fn init(config: Option<ObservationConfig>) {
    install_config(config);
    arm_refresh();
}

fn export_state() -> StableState {
    STATE.with(|cell| cell.borrow().clone())
}

#[cfg_attr(target_family = "wasm", ic_cdk::pre_upgrade)]
pub fn pre_upgrade() {
    let bytes = Encode!(&export_state()).expect("failed to encode historian stable state");
    ic_cdk::storage::stable_save((bytes,)).expect("failed to save historian stable state");
}

#[derive(CandidType, Deserialize)]
struct LegacyState {
    schema_version: u32,
    last_ingested_timestamp_nanos: Option<u64>,
}

fn restore_state(bytes: &[u8]) -> Result<StableState, String> {
    if let Ok(state) = Decode!(bytes, StableState) {
        if state.schema_version != HISTORIAN_SCHEMA_VERSION {
            return Err(format!(
                "unsupported historian schema {}",
                state.schema_version
            ));
        }
        return Ok(state);
    }
    let legacy = Decode!(bytes, LegacyState)
        .map_err(|err| format!("historian stable state is corrupt: {err}"))?;
    if legacy.schema_version > 2 {
        return Err(format!(
            "unsupported historian schema {}",
            legacy.schema_version
        ));
    }
    Ok(StableState {
        last_success_timestamp_nanos: legacy.last_ingested_timestamp_nanos,
        ..StableState::default()
    })
}

#[cfg_attr(target_family = "wasm", ic_cdk::post_upgrade)]
pub fn post_upgrade(config: Option<ObservationConfig>) {
    let mut state = match ic_cdk::storage::stable_restore::<(Vec<u8>,)>() {
        Ok((bytes,)) => restore_state(&bytes).expect("historian stable schema migration failed"),
        Err(current_error) => {
            let (legacy,) = ic_cdk::storage::stable_restore::<(LegacyState,)>().unwrap_or_else(
                |legacy_error| {
                    panic!(
                        "historian stable state is missing or corrupt: current={current_error}; legacy={legacy_error}"
                    )
                },
            );
            if legacy.schema_version > 2 {
                panic!("unsupported historian schema {}", legacy.schema_version);
            }
            StableState {
                last_success_timestamp_nanos: legacy.last_ingested_timestamp_nanos,
                ..StableState::default()
            }
        }
    };
    state.source_health.iter_mut().for_each(|health| {
        if health.freshness == ObservationFreshness::Fresh {
            health.freshness = ObservationFreshness::Stale;
        }
    });
    STATE.with(|cell| *cell.borrow_mut() = state);
    REFRESH_ACTIVE.with(|active| active.set(false));
    install_config(config);
    arm_refresh();
}

#[cfg(target_family = "wasm")]
fn arm_refresh() {
    use std::time::Duration;
    let delay = STATE.with(|cell| {
        cell.borrow()
            .config
            .as_ref()
            .map(|config| config.refresh_interval_seconds)
    });
    if let Some(delay) = delay {
        ic_cdk_timers::set_timer(Duration::from_secs(delay), refresh_once());
    }
}

#[cfg(not(target_family = "wasm"))]
fn arm_refresh() {}

#[cfg(target_family = "wasm")]
fn update_health(source: &str, now: u64, result: &Result<(), String>) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let health = state
            .source_health
            .iter_mut()
            .find(|health| health.source == source)
            .expect("known historian source");
        health.last_attempt_timestamp_nanos = Some(now);
        match result {
            Ok(()) => {
                health.freshness = ObservationFreshness::Fresh;
                health.last_success_timestamp_nanos = Some(now);
                health.error = None;
            }
            Err(error) => {
                health.freshness = ObservationFreshness::ErrorRetryable;
                health.error = Some(error.chars().take(512).collect());
            }
        }
    });
}

#[cfg(target_family = "wasm")]
async fn refresh_once() {
    if REFRESH_ACTIVE.with(|active| active.replace(true)) {
        return;
    }
    let Some(config) = STATE.with(|cell| cell.borrow().config.clone()) else {
        REFRESH_ACTIVE.with(|active| active.set(false));
        return;
    };
    let now = ic_cdk::api::time();
    let generation = STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.last_attempt_timestamp_nanos = Some(now);
        state.refresh_generation.saturating_add(1)
    });

    let protocol = adapters::protocol(&config, generation, now).await;
    let protocol_health = protocol.as_ref().map(|_| ()).map_err(Clone::clone);
    if let Ok(value) = protocol {
        STATE.with(|cell| cell.borrow_mut().protocol = value);
    }
    update_health("protocol", now, &protocol_health);

    let stream = adapters::stream(&config, now).await;
    let stream_health = stream.as_ref().map(|_| ()).map_err(Clone::clone);
    if let Ok(value) = stream {
        STATE.with(|cell| cell.borrow_mut().stream = Some(value));
    }
    update_health("stream", now, &stream_health);

    let nns = adapters::nns(&config, now).await;
    let nns_health = nns.as_ref().map(|_| ()).map_err(Clone::clone);
    if let Ok(value) = nns {
        STATE.with(|cell| cell.borrow_mut().nns_manager = Some(value));
    }
    update_health("nns-manager", now, &nns_health);

    let nns_governance = adapters::nns_governance(&config, now).await;
    let nns_governance_health = nns_governance.as_ref().map(|_| ()).map_err(Clone::clone);
    if let Ok(value) = nns_governance {
        STATE.with(|cell| cell.borrow_mut().nns_governance = Some(value));
    }
    update_health("nns-governance", now, &nns_governance_health);

    let topology = adapters::topology(&config, now).await;
    let topology_health = topology.as_ref().map(|_| ()).map_err(Clone::clone);
    if let Ok(value) = &topology {
        STATE.with(|cell| cell.borrow_mut().canisters = value.canisters.clone());
    }
    update_health("sns-root", now, &topology_health);

    let sns = adapters::sns(&config, topology.as_ref().ok(), now).await;
    let sns_health = sns.as_ref().map(|_| ()).map_err(Clone::clone);
    if let Ok(value) = sns {
        STATE.with(|cell| cell.borrow_mut().sns = Some(value));
    }
    update_health("sns-governance", now, &sns_health);

    let index = adapters::index(&config, now).await;
    let index_health = index.as_ref().map(|_| ()).map_err(Clone::clone);
    if let Ok(value) = index {
        STATE.with(|cell| cell.borrow_mut().index = Some(value));
    }
    update_health("sns-index", now, &index_health);

    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.refresh_generation = generation;
        if state
            .source_health
            .iter()
            .all(|health| health.freshness == ObservationFreshness::Fresh)
        {
            state.last_success_timestamp_nanos = Some(now);
        }
    });
    REFRESH_ACTIVE.with(|active| active.set(false));
    arm_refresh();
}

#[cfg(any(test, debug_assertions))]
pub fn import_state_for_tests(state: StableState) {
    STATE.with(|cell| *cell.borrow_mut() = state);
}

#[cfg(any(test, debug_assertions))]
pub fn export_state_for_tests() -> StableState {
    export_state()
}

#[cfg(any(test, debug_assertions))]
#[cfg_attr(target_family = "wasm", ic_cdk::update)]
pub async fn debug_refresh_now() {
    #[cfg(target_family = "wasm")]
    refresh_once().await;
}

ic_cdk::export_candid!();

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;
    use io_ledger_types::Subaccount;

    fn account(seed: u8) -> io_ledger_types::Account {
        io_ledger_types::Account::new(Principal::from_slice(&[seed]), Some(Subaccount([seed; 32])))
    }

    fn config() -> ObservationConfig {
        let principals = (10..=17)
            .map(|seed| Principal::from_slice(&[seed]))
            .collect::<Vec<_>>();
        let roles = [
            CanisterRole::StreamManager,
            CanisterRole::NnsManager,
            CanisterRole::SnsRoot,
            CanisterRole::SnsGovernance,
            CanisterRole::SnsLedger,
            CanisterRole::SnsIndex,
            CanisterRole::Historian,
        ];
        ObservationConfig {
            stream_manager: principals[0],
            nns_manager: principals[1],
            sns_root: principals[2],
            sns_governance: principals[3],
            sns_ledger: principals[4],
            sns_index: principals[5],
            icp_ledger: principals[6],
            nns_governance: principals[7],
            reward_backing_neuron_id: 1,
            two_year_neuron_id: 2,
            protocol_io_reserve: account(20),
            liquid_icp_reserve: account(21),
            excluded_io_accounts: vec![NamedAccount {
                name: "governance".into(),
                account: account(22),
            }],
            history_accounts: vec![NamedAccount {
                name: "reserve".into(),
                account: account(23),
            }],
            expected_modules: roles
                .into_iter()
                .enumerate()
                .map(|(index, role)| ExpectedModule {
                    role,
                    canister_id: if role == CanisterRole::Historian {
                        Principal::from_slice(&[42])
                    } else {
                        principals[index]
                    },
                    wasm_sha256: vec![index as u8; 32],
                })
                .collect(),
            reward_share_capable_governance_sha256: Some(vec![3; 32]),
            refresh_interval_seconds: 60,
        }
    }

    #[test]
    fn coherent_snapshot_rejects_inverted_supply() {
        let error = coherent_protocol_snapshot(1, 10, 8, &[3], 5, 99).unwrap_err();
        assert!(error.contains("less than"));
    }

    #[test]
    fn coherent_snapshot_never_mixes_missing_values_or_infers_zero_rate() {
        let zero = coherent_protocol_snapshot(1, 10, 4, &[6], 5, 99).unwrap();
        assert_eq!(zero.redeemable_io_supply_e8s, Some(0));
        assert_eq!(zero.redemption_rate, None);
        assert!(!zero.completeness.redemption_rate);
    }

    #[test]
    fn configuration_is_bounded_and_rejects_duplicates() {
        let mut value = config();
        assert!(validate_config(&value, Some(Principal::from_slice(&[42]))).is_ok());
        value
            .history_accounts
            .push(value.history_accounts[0].clone());
        assert!(validate_config(&value, None)
            .unwrap_err()
            .contains("duplicate"));
    }

    #[test]
    fn successful_observations_age_to_stale_without_erasing_last_success() {
        let mut state = StableState {
            config: Some(config()),
            ..StableState::default()
        };
        state.source_health[0].freshness = ObservationFreshness::Fresh;
        state.source_health[0].last_success_timestamp_nanos = Some(10);
        let visible = visible_source_health(&state, 120_000_000_011);
        assert_eq!(visible[0].freshness, ObservationFreshness::Stale);
        assert_eq!(visible[0].last_success_timestamp_nanos, Some(10));
    }

    #[test]
    fn replacing_config_clears_old_observations() {
        import_state_for_tests(StableState {
            protocol: coherent_protocol_snapshot(9, 100, 10, &[20], 70, 1).unwrap(),
            ..StableState::default()
        });
        install_config(Some(config()));
        let state = export_state_for_tests();
        assert!(state.config.is_some());
        assert_eq!(state.protocol, ProtocolSnapshot::default());
        assert!(state
            .source_health
            .iter()
            .all(|item| item.freshness == ObservationFreshness::Missing));
    }

    #[test]
    fn same_config_preserves_observations() {
        let config = config();
        install_config(Some(config.clone()));
        let protocol = coherent_protocol_snapshot(2, 100, 10, &[20], 70, 1).unwrap();
        STATE.with(|cell| cell.borrow_mut().protocol = protocol.clone());
        install_config(Some(config));
        assert_eq!(export_state_for_tests().protocol, protocol);
    }

    #[test]
    fn upgrade_encoding_preserves_error_and_does_not_preserve_active_refresh() {
        let mut state = StableState::default();
        state.source_health[0].freshness = ObservationFreshness::ErrorRetryable;
        state.source_health[0].error = Some("transport".into());
        let bytes = Encode!(&state).unwrap();
        let restored = restore_state(&bytes).unwrap();
        assert_eq!(
            restored.source_health[0].error.as_deref(),
            Some("transport")
        );
        assert!(!get_public_status().refresh_active);
    }
}
