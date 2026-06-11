//! Opt-in harness for real SNS framework canisters.
//!
//! This crate must not fetch Wasms, call mainnet, or treat IO-owned mocks as
//! real SNS framework canisters. Non-required tests skip when no local pinned
//! artifacts are configured; required xtask gates fail in that case.

pub mod artifacts;
pub mod brief_blockers;
pub mod exact_economics;
pub mod framework;
pub mod icrc;
pub mod nns_setup;
pub mod pocketic_env;
pub mod sns_governance_setup;
pub mod sns_ledger_index;
pub mod sns_lifecycle;
pub mod sns_root_setup;
pub mod sns_wasm_setup;

#[cfg(test)]
pub static TEST_ENV_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

#[cfg(test)]
pub fn lock_test_env() -> std::sync::MutexGuard<'static, ()> {
    TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires pinned real SNS ledger/index Wasms and POCKET_IC_BIN"]
    fn real_sns_ledger_index_smoke() {
        crate::sns_ledger_index::run_ledger_index_smoke(false);
    }

    #[test]
    #[ignore = "requires pinned real SNS ledger/index Wasms and POCKET_IC_BIN"]
    fn real_sns_ledger_index_same_wasm_upgrade_preserves_balances_history_and_duplicates() {
        crate::sns_ledger_index::run_ledger_index_same_wasm_upgrade(false);
    }

    #[test]
    #[ignore = "requires pinned real SNS governance/ledger Wasms and POCKET_IC_BIN"]
    fn real_sns_governance_staking_smoke() {
        crate::sns_governance_setup::install_real_sns_governance_and_stake_neuron(true).unwrap();
        crate::sns_governance_setup::install_real_sns_governance_and_topup_neuron(true).unwrap();
        crate::sns_governance_setup::install_real_sns_governance_and_reject_below_minimum_stake(
            true,
        )
        .unwrap();
        crate::sns_governance_setup::install_real_sns_governance_and_observe_dissolve_delay_boundaries(true).unwrap();
    }

    #[test]
    #[ignore = "requires pinned real SNS ledger/index Wasms and POCKET_IC_BIN"]
    fn real_canister_e2e_icp_to_io_stake_reward_redemption() {
        crate::exact_economics::run_exact_economics(false);
    }
}
