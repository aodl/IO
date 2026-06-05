use candid::CandidType;
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Deserialize)]
pub struct SchedulerTickOutcome {
    pub scanned_jupiter_faucet_deposits: u64,
    pub scanned_nns_maturity_deposits: u64,
    pub scanned_redemption_transfers: u64,
    pub processed_authorized_streams: u64,
    pub planned_steps: Vec<String>,
}

impl SchedulerTickOutcome {
    fn no_work_configured() -> Self {
        Self {
            scanned_jupiter_faucet_deposits: 0,
            scanned_nns_maturity_deposits: 0,
            scanned_redemption_transfers: 0,
            processed_authorized_streams: 0,
            planned_steps: vec![
                "scan ICP ledger/index for Jupiter Faucet deposits".to_string(),
                "scan ICP ledger/index for NNS maturity deposits".to_string(),
                "scan IO ledger/index for user redemption transfers".to_string(),
                "classify observed flows before internal processing".to_string(),
            ],
        }
    }
}

pub fn scheduler_tick_once() -> SchedulerTickOutcome {
    SchedulerTickOutcome::no_work_configured()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_is_idempotent_without_configured_work() {
        assert_eq!(scheduler_tick_once(), scheduler_tick_once());
    }

    #[test]
    fn outcome_is_debuggable_and_candid_serializable() {
        let outcome = scheduler_tick_once();
        assert!(format!("{outcome:?}").contains("planned_steps"));
        candid::encode_one(outcome).unwrap();
    }
}
