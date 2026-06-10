//! Opt-in harness for real SNS framework canisters.
//!
//! This crate must not fetch Wasms, call mainnet, or treat IO-owned mocks as
//! real SNS framework canisters. Non-required tests skip when no local pinned
//! artifacts are configured; required xtask gates fail in that case.

pub mod artifacts;
pub mod icrc;
pub mod pocketic_env;
pub mod sns_ledger_index;

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
    #[ignore = "requires pinned real SNS governance/root Wasms and init driver"]
    fn real_sns_governance_staking_smoke() {
        eprintln!(
            "skipping real SNS governance staking smoke: governance/root install and normal staking driver are not implemented yet"
        );
    }

    #[test]
    #[ignore = "requires pinned real ICP/SNS/NNS framework Wasms"]
    fn real_canister_e2e_icp_to_io_stake_reward_redemption() {
        eprintln!(
            "skipping all-real IO E2E: ledger/index smoke is the current executable real-framework layer"
        );
    }
}
